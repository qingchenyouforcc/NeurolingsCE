//! 命令服务层：本地 IPC 与 HTTP 共用的命令分发与执行。
//!
//! 请求在服务线程进入通道，由主循环执行（引擎状态为主线程独占），
//! 经一次性通道回传响应。命令形状与原版 IPC 契约逐一对齐：
//! spawn/alter 使用 request/patch 子对象，标签从 0 起分配，
//! selector 为 JS 表达式并带时间预算。

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, Sender, SyncSender, TrySendError, sync_channel};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};

use neurolings_common::version;
use neurolings_engine::math::Vec2;
use neurolings_platform::Rect;

use crate::runtime::environment::EnvironmentSet;
use crate::runtime::session::Session;
use crate::settings::Settings;
use crate::templates::TemplateStore;

const IPC_MESSAGE_MAX_BYTES: usize = 1024 * 1024;
/// selector 求值的单条超时与总预算。
const SELECTOR_EVAL_TIMEOUT_MS: u64 = 25;
const SELECTOR_BUDGET_MS: u64 = 50;
/// selector 最大长度。
const SELECTOR_MAX_LEN: usize = 1024;
/// 后台慢任务队列容量。单 worker 串行处理包缓存，避免并发解压互相覆盖。
const BACKGROUND_JOB_CAPACITY: usize = 4;
/// 主循环尚未来得及消费时允许积压的完成事件数。
const BACKGROUND_COMPLETION_CAPACITY: usize = 4;
/// 内部控制面令牌字节数。256 位熵足以抵御本机端口探测。
const INTERNAL_CONTROL_TOKEN_BYTES: usize = 32;
/// 服务线程等待运行时主循环响应的最长时间；超时表示请求未被确认受理。
const COMMAND_REPLY_TIMEOUT: Duration = Duration::from_secs(120);
/// 后台操作最终结果在内存中保留的时间，供 Manager 完成轮询。
const OPERATION_RESULT_RETENTION: Duration = Duration::from_secs(300);
/// 同时保留的最终结果数量上限，避免异常客户端耗尽运行时内存。
const OPERATION_RESULT_CAPACITY: usize = 64;

/// runtime 与 Manager 之间传递内部控制面令牌的环境变量名。
pub const INTERNAL_CONTROL_TOKEN_ENV: &str = "NEUROLINGSCE_MANAGER_TOKEN";

static INTERNAL_CONTROL_TOKEN: OnceLock<String> = OnceLock::new();

/// 在启动阶段初始化内部控制面令牌。
///
/// 优先接受 Manager 显式传入的 256 位 URL-safe Base64 令牌；未传入时使用操作系统
/// 密码学随机源生成。格式错误的显式令牌会拒绝启动，避免两端持有不同凭据。令牌仅
/// 保存在进程内存，绝不写入设置、日志或响应。
pub(crate) fn initialize_internal_control_token() -> Result<(), String> {
    if INTERNAL_CONTROL_TOKEN.get().is_some() {
        return Ok(());
    }

    let supplied = match std::env::var(INTERNAL_CONTROL_TOKEN_ENV) {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err("internal control token is not valid Unicode".to_string());
        }
    };
    let token = select_internal_control_token(supplied)?;
    let _ = INTERNAL_CONTROL_TOKEN.set(token);
    Ok(())
}

/// 读取已初始化的内部控制面令牌，供 runtime 拉起 Manager 时显式传递。
pub(crate) fn internal_control_token() -> Option<&'static str> {
    INTERNAL_CONTROL_TOKEN.get().map(String::as_str)
}

fn select_internal_control_token(supplied: Option<String>) -> Result<String, String> {
    match supplied {
        Some(value) if valid_internal_control_token(&value) => Ok(value),
        Some(_) => Err("internal control token has an invalid format".to_string()),
        None => generate_internal_control_token(),
    }
}

fn valid_internal_control_token(value: &str) -> bool {
    URL_SAFE_NO_PAD
        .decode(value)
        .ok()
        .is_some_and(|bytes| bytes.len() == INTERNAL_CONTROL_TOKEN_BYTES)
}

fn generate_internal_control_token() -> Result<String, String> {
    let mut bytes = [0u8; INTERNAL_CONTROL_TOKEN_BYTES];
    fill_secure_random(&mut bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

#[cfg(windows)]
fn fill_secure_random(bytes: &mut [u8]) -> Result<(), String> {
    use std::ffi::c_void;

    #[link(name = "bcrypt")]
    unsafe extern "system" {
        fn BCryptGenRandom(
            algorithm: *mut c_void,
            buffer: *mut u8,
            buffer_len: u32,
            flags: u32,
        ) -> i32;
    }

    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x0000_0002;
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            bytes.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err("could not initialize the Windows secure random source".to_string())
    }
}

#[cfg(not(windows))]
fn fill_secure_random(bytes: &mut [u8]) -> Result<(), String> {
    use std::io::Read as _;

    std::fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(bytes))
        .map_err(|_| "could not initialize the operating system secure random source".to_string())
}

pub struct PendingCommand {
    pub request: Value,
    pub reply: SyncSender<Value>,
    /// 后台任务入队后回传稳定操作 ID；快速命令不会向该通道写入。
    pub operation: SyncSender<u64>,
}

pub struct CommandChannel {
    tx: Sender<PendingCommand>,
    rx: Receiver<PendingCommand>,
}

impl CommandChannel {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Self { tx, rx }
    }

    pub fn sender(&self) -> Sender<PendingCommand> {
        self.tx.clone()
    }

    pub fn try_recv(&self) -> Option<PendingCommand> {
        self.rx.try_recv().ok()
    }
}

/// 主循环持有的有界后台任务调度器。
///
/// worker 只能处理网络、文件和解析；所有 RuntimeView 改动由完成事件回到主循环后执行。
pub(crate) struct BackgroundJobs {
    jobs: SyncSender<BackgroundJob>,
    completed: Receiver<BackgroundCompletion>,
    pending: HashSet<u64>,
    replies: HashMap<u64, SyncSender<Value>>,
    results: HashMap<u64, OperationResult>,
    next_id: u64,
}

struct OperationResult {
    response: Value,
    finished_at: Instant,
}

/// 命令分流结果：快速路径立即回复，慢路径仅提交不持有运行时视图的工作单元。
pub(crate) enum BackgroundCommand {
    NotBackground,
    Reply(Value),
    Job(BackgroundJobKind),
}

/// 可在后台 worker 中执行的慢工作类型。
pub(crate) enum BackgroundJobKind {
    UpdateCheck {
        config: crate::update::UpdateRequestConfig,
    },
    UpdateDownload {
        app_data_dir: PathBuf,
        config: crate::update::UpdateRequestConfig,
    },
    UpdateInstall,
    Import {
        archive: PathBuf,
        storage: PathBuf,
        cache: PathBuf,
    },
    ReloadTemplates {
        storage: PathBuf,
        cache: PathBuf,
    },
    StoreIndex {
        request: Value,
    },
    StoreInstall {
        id: String,
    },
    StoreGitHubStart,
    StoreGitHubStep,
    StoreGitHubStatus,
    StoreGitHubSignout,
    StoreSubmit {
        request: Value,
    },
    AnalyzeArchive {
        request: Value,
    },
    ConvertArchive {
        request: Value,
    },
    #[cfg(test)]
    TestSleep(std::time::Duration),
}

struct BackgroundJob {
    id: u64,
    kind: BackgroundJobKind,
}

/// worker 结束后交由主循环提交的结果。
pub(crate) struct BackgroundCompletion {
    pub(crate) id: u64,
    pub(crate) result: BackgroundResult,
}

/// 完成事件携带的纯数据结果；所有运行时状态改动在主线程完成。
pub(crate) enum BackgroundResult {
    Reply(Value),
    Imported(ImportedTemplates),
    Reloaded(Vec<crate::templates::LoadedTemplate>),
    UpdateDownloaded(Result<crate::update::DownloadedUpdate, String>),
}

/// 导入完成后等待主线程登记的模板快照。
pub(crate) struct ImportedTemplates {
    changed: BTreeSet<String>,
    templates: Vec<crate::templates::LoadedTemplate>,
    store_entry: Option<Value>,
}

impl BackgroundJobs {
    /// 创建单 worker 调度器，串行化共享存储与缓存的解压写入。
    pub(crate) fn new() -> Self {
        let (job_tx, job_rx) = sync_channel::<BackgroundJob>(BACKGROUND_JOB_CAPACITY);
        let (completed_tx, completed_rx) =
            sync_channel::<BackgroundCompletion>(BACKGROUND_COMPLETION_CAPACITY);
        std::thread::spawn(move || {
            while let Ok(job) = job_rx.recv() {
                let result = execute_background_job(job.kind);
                if completed_tx
                    .send(BackgroundCompletion { id: job.id, result })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            jobs: job_tx,
            completed: completed_rx,
            pending: HashSet::new(),
            replies: HashMap::new(),
            results: HashMap::new(),
            next_id: 1,
        }
    }

    /// 提交慢任务；队列满时立即拒绝，避免无限积压请求和等待线程。
    pub(crate) fn submit(
        &mut self,
        kind: BackgroundJobKind,
        reply: &SyncSender<Value>,
    ) -> Result<u64, Value> {
        self.prune_results();
        let id = self.allocate_id();
        match self.jobs.try_send(BackgroundJob { id, kind }) {
            Ok(()) => {
                self.pending.insert(id);
                self.replies.insert(id, reply.clone());
                Ok(id)
            }
            Err(TrySendError::Full(job)) => {
                job.kind.cancel_before_start();
                Err(error_json(
                    "Too many background operations are already in progress",
                    "busy",
                    429,
                ))
            }
            Err(TrySendError::Disconnected(job)) => {
                job.kind.cancel_before_start();
                Err(error_json("runtime is shutting down", "unavailable", 503))
            }
        }
    }

    /// 取得一个已完成的后台任务事件；调用方必须在主循环中应用结果。
    pub(crate) fn try_recv(&self) -> Option<BackgroundCompletion> {
        self.completed.try_recv().ok()
    }

    /// 记录后台操作的最终响应，并使其可被内部控制面查询。
    pub(crate) fn finish(&mut self, id: u64, response: Value) {
        self.pending.remove(&id);
        let response = completed_operation_response(id, response);
        self.results.insert(
            id,
            OperationResult {
                response,
                finished_at: Instant::now(),
            },
        );
        self.prune_results();
    }

    /// 取回首次调用的响应通道；状态结果已独立保存在操作记录中。
    pub(crate) fn take_reply(&mut self, id: u64) -> Option<SyncSender<Value>> {
        self.replies.remove(&id)
    }

    /// 查询后台操作状态；结果只暴露给已通过内部控制面鉴权的调用方。
    pub(crate) fn operation_status(&mut self, request: &Value) -> Value {
        self.prune_results();
        let Some(id) = request.get("operation_id").and_then(Value::as_u64) else {
            return error_json(
                "operation_id must be a positive integer",
                "bad_request",
                400,
            );
        };
        if id == 0 {
            return error_json(
                "operation_id must be a positive integer",
                "bad_request",
                400,
            );
        }
        if self.pending.contains(&id) {
            return accepted_operation_response(id);
        }
        self.results
            .get(&id)
            .map(|result| result.response.clone())
            .unwrap_or_else(|| {
                error_json(
                    "Operation was not found or has expired",
                    "operation_not_found",
                    404,
                )
            })
    }

    fn allocate_id(&mut self) -> u64 {
        loop {
            let id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1).max(1);
            if !self.pending.contains(&id) && !self.results.contains_key(&id) {
                return id;
            }
        }
    }

    fn prune_results(&mut self) {
        self.results
            .retain(|_, result| result.finished_at.elapsed() <= OPERATION_RESULT_RETENTION);
        while self.results.len() > OPERATION_RESULT_CAPACITY {
            let Some(oldest_id) = self
                .results
                .iter()
                .min_by_key(|(_, result)| result.finished_at)
                .map(|(id, _)| *id)
            else {
                break;
            };
            self.results.remove(&oldest_id);
        }
    }
}

/// 返回后台操作已受理响应，调用方随后应查询 `operation_status`。
pub(crate) fn accepted_operation_response(id: u64) -> Value {
    json!({
        "accepted": true,
        "pending": true,
        "operation_id": id,
        "operation_state": "pending",
        "status": 202,
    })
}

fn completed_operation_response(id: u64, mut response: Value) -> Value {
    let Some(object) = response.as_object_mut() else {
        return json!({
            "operation_id": id,
            "pending": false,
            "operation_state": "completed",
            "result": response,
            "status": 200,
        });
    };
    let failed = object
        .get("status")
        .and_then(Value::as_i64)
        .is_some_and(|status| status >= 400)
        || object
            .get("error")
            .and_then(Value::as_str)
            .is_some_and(|error| !error.is_empty());
    object.insert("operation_id".to_string(), json!(id));
    object.insert("pending".to_string(), json!(false));
    object.insert(
        "operation_state".to_string(),
        json!(if failed { "failed" } else { "completed" }),
    );
    response
}

impl BackgroundJobKind {
    /// 撤销尚未入队的预置状态，避免队列拒绝后更新页永久显示“下载中”。
    fn cancel_before_start(&self) {
        if matches!(self, Self::UpdateDownload { .. }) {
            crate::update::cancel_download_before_start();
        }
    }
}

/// 将可能阻塞的命令转换为不持有 RuntimeView 的后台工作单元。
pub(crate) fn prepare_background_command(request: &Value, view: &RuntimeView) -> BackgroundCommand {
    let Some(command) = request.get("command").and_then(Value::as_str) else {
        return BackgroundCommand::NotBackground;
    };
    match command {
        "update_check" => BackgroundCommand::Job(BackgroundJobKind::UpdateCheck {
            config: crate::update::request_config(view.settings),
        }),
        "update_download" => match crate::update::begin_download() {
            Ok(()) => BackgroundCommand::Job(BackgroundJobKind::UpdateDownload {
                app_data_dir: view.app_data_dir(),
                config: crate::update::request_config(view.settings),
            }),
            Err(error) => BackgroundCommand::Reply(error_json(&error, "download_failed", 500)),
        },
        "update_install" => BackgroundCommand::Job(BackgroundJobKind::UpdateInstall),
        "import_mascot_template" => match template_storage_paths() {
            Ok((storage, cache)) => {
                let Some(path) = request.get("archive_path").and_then(Value::as_str) else {
                    return BackgroundCommand::Reply(error_json(
                        "archive_path must be a string",
                        "bad_request",
                        400,
                    ));
                };
                if path.is_empty() {
                    return BackgroundCommand::Reply(error_json(
                        "Archive path is required",
                        "invalid_archive",
                        400,
                    ));
                }
                BackgroundCommand::Job(BackgroundJobKind::Import {
                    archive: PathBuf::from(path),
                    storage,
                    cache,
                })
            }
            Err(error) => BackgroundCommand::Reply(error),
        },
        "reload_templates" => match template_storage_paths() {
            Ok((storage, cache)) => {
                BackgroundCommand::Job(BackgroundJobKind::ReloadTemplates { storage, cache })
            }
            Err(error) => BackgroundCommand::Reply(error),
        },
        "store_index" => BackgroundCommand::Job(BackgroundJobKind::StoreIndex {
            request: request.clone(),
        }),
        "store_install" => {
            let Some(id) = request.get("id").and_then(Value::as_str) else {
                return BackgroundCommand::Reply(error_json("id is required", "bad_request", 400));
            };
            BackgroundCommand::Job(BackgroundJobKind::StoreInstall { id: id.to_string() })
        }
        "store_github_start" => BackgroundCommand::Job(BackgroundJobKind::StoreGitHubStart),
        "store_github_step" => BackgroundCommand::Job(BackgroundJobKind::StoreGitHubStep),
        "store_github_status" => BackgroundCommand::Job(BackgroundJobKind::StoreGitHubStatus),
        "store_github_signout" => BackgroundCommand::Job(BackgroundJobKind::StoreGitHubSignout),
        "store_submit_mascot" => BackgroundCommand::Job(BackgroundJobKind::StoreSubmit {
            request: request.clone(),
        }),
        "analyze_archive" => BackgroundCommand::Job(BackgroundJobKind::AnalyzeArchive {
            request: request.clone(),
        }),
        "convert_archive" => BackgroundCommand::Job(BackgroundJobKind::ConvertArchive {
            request: request.clone(),
        }),
        _ => BackgroundCommand::NotBackground,
    }
}

/// 在主循环应用后台任务的完成事件，并生成原命令的最终响应。
pub(crate) fn complete_background_command(
    result: BackgroundResult,
    view: &mut RuntimeView,
) -> Value {
    match result {
        BackgroundResult::Reply(response) => response,
        BackgroundResult::Imported(imported) => apply_imported_templates(view, imported),
        BackgroundResult::Reloaded(templates) => {
            let names = apply_template_snapshot(view, templates);
            json!({ "reloaded": true, "templates": names, "count": names.len() })
        }
        BackgroundResult::UpdateDownloaded(result) => match result {
            Ok(downloaded) => match crate::update::persist_downloaded(
                view.settings,
                &downloaded.version,
                &downloaded.path,
                &downloaded.sha256,
            ) {
                Ok(()) => json!({
                    "downloaded": true,
                    "version": downloaded.version,
                    "path": downloaded.path,
                }),
                Err(error) => error_json(&error, "settings_failed", 500),
            },
            Err(error) => error_json(&error, "download_failed", 500),
        },
    }
}

fn execute_background_job(kind: BackgroundJobKind) -> BackgroundResult {
    match kind {
        BackgroundJobKind::UpdateCheck { config } => {
            let notify = crate::update::run_check_with_config(&config);
            BackgroundResult::Reply(json!({
                "checked": true,
                "notify": notify,
                "status": crate::update::status_json(),
            }))
        }
        BackgroundJobKind::UpdateDownload {
            app_data_dir,
            config,
        } => BackgroundResult::UpdateDownloaded(crate::update::download_with_config(
            &app_data_dir,
            &config,
        )),
        BackgroundJobKind::UpdateInstall => BackgroundResult::Reply(
            crate::update::install()
                .unwrap_or_else(|error| error_json(&error, "install_failed", 500)),
        ),
        BackgroundJobKind::Import {
            archive,
            storage,
            cache,
        } => match import_mascot_template_job(&archive, &storage, &cache) {
            Ok(imported) => BackgroundResult::Imported(imported),
            Err(error) => BackgroundResult::Reply(error),
        },
        BackgroundJobKind::ReloadTemplates { storage, cache } => {
            BackgroundResult::Reloaded(load_template_snapshot(&storage, &cache))
        }
        BackgroundJobKind::StoreIndex { request } => {
            BackgroundResult::Reply(store_index_command(&request))
        }
        BackgroundJobKind::StoreInstall { id } => match store_install_job(&id) {
            Ok(imported) => BackgroundResult::Imported(imported),
            Err(error) => BackgroundResult::Reply(error),
        },
        BackgroundJobKind::StoreGitHubStart => {
            BackgroundResult::Reply(store_github_start_command())
        }
        BackgroundJobKind::StoreGitHubStep => BackgroundResult::Reply(store_github_step_command()),
        BackgroundJobKind::StoreGitHubStatus => {
            BackgroundResult::Reply(store_github_status_command())
        }
        BackgroundJobKind::StoreGitHubSignout => {
            BackgroundResult::Reply(store_github_signout_command())
        }
        BackgroundJobKind::StoreSubmit { request } => {
            BackgroundResult::Reply(store_submit_mascot_command(&request))
        }
        BackgroundJobKind::AnalyzeArchive { request } => {
            BackgroundResult::Reply(analyze_archive_command(&request))
        }
        BackgroundJobKind::ConvertArchive { request } => {
            BackgroundResult::Reply(convert_archive_command(&request))
        }
        #[cfg(test)]
        BackgroundJobKind::TestSleep(duration) => {
            std::thread::sleep(duration);
            BackgroundResult::Reply(json!({ "ok": true }))
        }
    }
}

/// 从服务线程提交请求并阻塞等待回复。
pub fn call(tx: &Sender<PendingCommand>, request: Value) -> Value {
    let (reply_tx, reply_rx) = sync_channel::<Value>(1);
    let (operation_tx, operation_rx) = sync_channel::<u64>(1);
    if tx
        .send(PendingCommand {
            request,
            reply: reply_tx,
            operation: operation_tx,
        })
        .is_err()
    {
        return error_json("runtime is shutting down", "unavailable", 503);
    }
    wait_for_command_reply(&reply_rx, &operation_rx, COMMAND_REPLY_TIMEOUT)
}

/// 等待主线程完成命令；仅已取得后台操作 ID 的超时请求才会返回已受理状态。
fn wait_for_command_reply(
    reply_rx: &Receiver<Value>,
    operation_rx: &Receiver<u64>,
    timeout: Duration,
) -> Value {
    let deadline = Instant::now() + timeout;
    let mut operation_id = None;
    loop {
        if let Ok(value) = reply_rx.try_recv() {
            return value;
        }
        if operation_id.is_none()
            && let Ok(id) = operation_rx.try_recv()
        {
            operation_id = Some(id);
        }

        let now = Instant::now();
        if now >= deadline {
            return operation_id
                .map(accepted_operation_response)
                .unwrap_or_else(|| {
                    error_json(
                        "Runtime did not respond before the deadline",
                        "timeout",
                        504,
                    )
                });
        }

        let remaining = deadline.saturating_duration_since(now);
        let wait = remaining.min(Duration::from_millis(25));
        match reply_rx.recv_timeout(wait) {
            Ok(value) => return value,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return error_json("runtime is shutting down", "unavailable", 503);
            }
        }
    }
}

/// "更新可用→跳转 About 页"请求标志（update 线程设置，心跳响应消费）。
static UPDATE_NAVIGATE_REQUESTED: std::sync::OnceLock<std::sync::atomic::AtomicBool> =
    std::sync::OnceLock::new();

/// 请求下一次 Manager 心跳跳转到更新页面。
pub fn request_update_navigate() {
    UPDATE_NAVIGATE_REQUESTED
        .get_or_init(|| std::sync::atomic::AtomicBool::new(false))
        .store(true, std::sync::atomic::Ordering::SeqCst);
}

fn take_update_navigate() -> bool {
    UPDATE_NAVIGATE_REQUESTED
        .get_or_init(|| std::sync::atomic::AtomicBool::new(false))
        .swap(false, std::sync::atomic::Ordering::SeqCst)
}

/// "安装更新后退出运行时"请求标志（update::install 设置，主循环消费）。
static EXIT_AFTER_INSTALL: std::sync::OnceLock<std::sync::atomic::AtomicBool> =
    std::sync::OnceLock::new();

/// 请求运行时优雅退出：延迟 500ms 再置位，确保 install 响应先发回调用方。
pub fn request_exit_after_install() {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(500));
        EXIT_AFTER_INSTALL
            .get_or_init(|| std::sync::atomic::AtomicBool::new(false))
            .store(true, std::sync::atomic::Ordering::SeqCst);
    });
}

/// 消费一次“安装器已启动，运行时应退出”的请求。
pub fn take_exit_after_install() -> bool {
    EXIT_AFTER_INSTALL
        .get_or_init(|| std::sync::atomic::AtomicBool::new(false))
        .swap(false, std::sync::atomic::Ordering::SeqCst)
}

pub fn error_json(message: &str, code: &str, status: i32) -> Value {
    json!({ "error": message, "code": code, "status": status })
}

pub const MESSAGE_MAX_BYTES: usize = IPC_MESSAGE_MAX_BYTES;

/// 主循环状态的可变视图。
impl<'a> RuntimeView<'a> {
    /// 应用数据目录（LOCALAPPDATA\NeurolingsCE）。
    pub fn app_data_dir(&self) -> std::path::PathBuf {
        self.app_data_dir.to_path_buf()
    }
}

pub struct RuntimeView<'a> {
    pub sessions: &'a mut Vec<Session>,
    pub factory: &'a mut neurolings_engine::mascot::Factory,
    pub envs: &'a mut EnvironmentSet,
    pub templates: &'a mut TemplateStore,
    pub settings: &'a mut Settings,
    pub labels: &'a mut std::collections::HashMap<i64, u64>,
    pub next_label: &'a mut i64,
    pub next_session_id: &'a mut u64,
    pub combinations: &'a crate::combinations::CombinationStore,
    pub quit: &'a mut bool,
    pub backend:
        &'a Option<std::rc::Rc<std::cell::RefCell<Box<dyn neurolings_platform::MascotBackend>>>>,
    pub app_data_dir: &'a std::path::Path,
    pub windowed: &'a mut bool,
    /// 管理器窗口最新矩形（心跳上报），召唤落点跟随管理器所在屏。
    pub manager_rect: &'a mut Option<neurolings_platform::Rect>,
    /// Codex 通知去重表。
    pub codex_seen: &'a mut std::collections::VecDeque<(String, String, std::time::Instant)>,
    /// 点击 Codex 气泡后请求管理器跳转 Codex 页。
    pub codex_page_requested: &'a mut bool,
    /// 右键「检查」后请求管理器打开检查器。
    pub inspect_requested: &'a mut Option<u64>,
    pub cli_runtime_mode: bool,
    pub startup_mode: bool,
}

#[cfg(any(target_os = "macos", test))]
fn normalize_macos_manager_heartbeat_rect(rect: Rect, reference_screen: Option<Rect>) -> Rect {
    let Some(reference_screen) = reference_screen else {
        return rect;
    };
    Rect {
        left: rect.left.saturating_add(reference_screen.left),
        top: rect.top.saturating_add(reference_screen.top),
        right: rect.right.saturating_add(reference_screen.left),
        bottom: rect.bottom.saturating_add(reference_screen.top),
    }
}

fn normalize_manager_heartbeat_rect(rect: Rect, reference_screen: Option<Rect>) -> Rect {
    #[cfg(target_os = "macos")]
    {
        normalize_macos_manager_heartbeat_rect(rect, reference_screen)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = reference_screen;
        rect
    }
}

/// 在主循环中执行快速命令。
///
/// 调用方必须先通过 [`prepare_background_command`] 分流慢任务，避免网络、解压和
/// 大文件校验占住 RuntimeView。
pub fn dispatch(request: &Value, view: &mut RuntimeView) -> Value {
    let Some(command) = request.get("command").and_then(Value::as_str) else {
        return error_json("Missing command", "bad_request", 400);
    };
    crate::log::debug("ipc", &format!("dispatch command={command}"));
    match command {
        "ping" => json!({
            "ok": true,
            "app": version::APP_NAME,
            "api_version": "v1",
        }),
        "list_mascots" => list_mascots(view, request),
        "list_loaded_mascots" => list_loaded_mascots(view),
        "import_mascot_template" | "reload_templates" => background_dispatch_missed(),
        "remove_mascot_template" => remove_mascot_template(view, request),
        "spawn_mascot" => spawn_mascot(view, request),
        "register_cli_label" => register_cli_label(view, request),
        "get_cli_label" => get_cli_label(view, request),
        "alter_mascot" => alter_mascot(view, request),
        "get_mascot" => get_mascot(view, request),
        "dismiss_mascot" => dismiss_mascot(view, request),
        "dismiss_all_mascots" => dismiss_all_mascots(view, request),
        "stop_runtime" => {
            *view.quit = true;
            json!({ "stopped": true })
        }
        "show_manager" => {
            launch_manager();
            json!({ "shown": true })
        }
        "app_info" => json!({
            "app": version::APP_NAME,
            "version": version::VERSION,
        }),
        "manager_heartbeat" => {
            // 管理器定期上报窗口矩形与可见性：召唤落点跟随管理器所在屏。
            if let Some(is_visible) = request.get("is_visible").and_then(Value::as_bool) {
                neurolings_platform::manager_window::report_visibility(is_visible);
            }
            let x = request.get("x").and_then(Value::as_i64).unwrap_or(0) as i32;
            let y = request.get("y").and_then(Value::as_i64).unwrap_or(0) as i32;
            let width = request.get("width").and_then(Value::as_i64).unwrap_or(0) as i32;
            let height = request.get("height").and_then(Value::as_i64).unwrap_or(0) as i32;
            if width > 0 && height > 0 {
                // Flutter 插件与 macOS 后端都以 NSScreen 数组首项为参考屏；
                // 运行时环境则已归一到虚拟桌面左上角，需补回该屏的偏移量。
                let rect = normalize_manager_heartbeat_rect(
                    Rect {
                        left: x,
                        top: y,
                        right: x + width,
                        bottom: y + height,
                    },
                    view.envs
                        .screens
                        .first()
                        .map(|screen| screen.screen.monitor),
                );
                *view.manager_rect = Some(rect);
            }
            // 携带并消费"跳转 Codex 页"请求（点击 Codex 气泡触发）与
            // "跳转 About 页"请求（启动检查发现新版本触发）。
            let codex_navigate = *view.codex_page_requested;
            *view.codex_page_requested = false;
            let update_navigate = take_update_navigate();
            let inspect_id = view.inspect_requested.take();
            json!({
                "ok": true,
                "codex_navigate": codex_navigate,
                "update_navigate": update_navigate,
                "inspect_id": inspect_id,
                "mascot_count": view.sessions.len(),
                "template_count": view.templates.names_sorted().len(),
            })
        }
        "inspect_mascot" => inspect_mascot_command(view, request),
        "storage_path" => json!({
            "path": neurolings_pack::storage::default_storage_path()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }),
        // 以下为运行时扩展命令（管理器与工具链使用）。
        "update_status" => crate::update::status_json(),
        "update_check" | "update_download" | "update_install" => background_dispatch_missed(),
        "update_ignore" => {
            let version = request.get("version").and_then(Value::as_str).unwrap_or("");
            if version.is_empty() {
                error_json("version is required", "bad_request", 400)
            } else {
                match view.settings.set("update/ignoredVersion", json!(version)) {
                    Ok(()) => json!({ "ignored": version }),
                    Err(e) => error_json(&e, "settings_failed", 500),
                }
            }
        }
        "update_remind" => {
            let version = request.get("version").and_then(Value::as_str).unwrap_or("");
            if version.is_empty() {
                error_json("version is required", "bad_request", 400)
            } else {
                // 稍后提醒：抑制该版本 1 天（对齐原版 addDays(1)）。
                let remind_at = chrono::Utc::now().timestamp() + 86_400;
                let _ = view.settings.set("update/remindVersion", json!(version));
                let _ = view
                    .settings
                    .set("update/remindAt", json!(remind_at.to_string()));
                json!({ "remind": version })
            }
        }
        "codex_server_status" => codex_server_status_command(view),
        "codex_server_connect" => codex_server_connect_command(view, request),
        "codex_server_disconnect" => {
            crate::codex_appserver::disconnect();
            json!({ "stopped": true })
        }
        "codex_server_new_thread" => codex_server_new_thread_command(request),
        "codex_server_resume" => codex_server_resume_command(request),
        "codex_server_turn" => codex_server_turn_command(request),
        "codex_server_steer" => codex_server_steer_command(request),
        "codex_server_interrupt" => {
            let _ = crate::codex_appserver::with_client(|c| c.interrupt_turn());
            json!({ "sent": true })
        }
        "codex_server_resolve" => codex_server_resolve_command(request),
        "codex_server_input" => codex_server_input_command(request),
        "store_github_status"
        | "store_github_start"
        | "store_github_step"
        | "store_github_signout" => background_dispatch_missed(),
        "store_submit_mascot" | "analyze_archive" | "convert_archive" => {
            background_dispatch_missed()
        }
        "preview_png" => preview_png(view, request),
        "show_bubble" => show_bubble(view, request),
        "show_codex_notification" => show_codex_notification(view, request),
        "save_combination" => save_combination(view, request),
        "restore_combination" => restore_combination(view, request),
        "list_combinations" => list_combinations(view),
        "get_combination" => get_combination(view, request),
        "delete_combination" => delete_combination(view, request),
        "set_autostart" => set_autostart_command(request),
        "get_autostart" => get_autostart_command(),
        "codex_setup" => codex_setup_command(request),
        "codex_status" => codex_status_command(),
        "set_window_mode" => set_window_mode(view, request),
        "get_settings" => get_settings_command(view, request),
        "set_settings" => set_settings_command(view, request),
        "store_status" => store_status_command(),
        "store_index" | "store_install" => background_dispatch_missed(),
        _ => error_json("Unknown command", "bad_request", 400),
    }
}

fn background_dispatch_missed() -> Value {
    error_json(
        "background command was not scheduled",
        "internal_error",
        500,
    )
}

/// 拉起管理器进程（与运行时同目录的可执行文件）。
pub fn launch_manager() {
    // 已在运行的管理器只需前置显示，避免拉出第二个实例（对齐原版单窗口语义）。
    if neurolings_platform::manager_window::is_running() {
        neurolings_platform::manager_window::show();
        return;
    }
    let Some(exe) = std::env::current_exe().ok() else {
        return;
    };
    let Some(dir) = exe.parent() else { return };
    let name = if cfg!(windows) {
        "neurolings_manager.exe"
    } else {
        "neurolings_manager"
    };
    let path = dir.join(name);
    if !path.is_file() {
        crate::log::warn(
            "manager",
            &format!("manager executable not found: {}", path.display()),
        );
        return;
    }
    let Some(token) = internal_control_token() else {
        crate::log::error("manager", "internal control token is unavailable");
        return;
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // 完全脱离父进程：不共享控制台、独立进程组，避免拖住父进程管道。
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        let result = std::process::Command::new(&path)
            .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS)
            .env(
                "NEUROLINGSCE_MANAGER_PORT",
                neurolings_common::api::INTERNAL_HTTP_PORT.to_string(),
            )
            .env(INTERNAL_CONTROL_TOKEN_ENV, token)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        match result {
            Ok(_) => crate::log::info("manager", "manager launched"),
            Err(e) => crate::log::error("manager", &format!("manager launch failed: {e}")),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new(&path)
            .env(
                "NEUROLINGSCE_MANAGER_PORT",
                neurolings_common::api::INTERNAL_HTTP_PORT.to_string(),
            )
            .env(INTERNAL_CONTROL_TOKEN_ENV, token)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

// ---- 查询类命令 ----

fn mascot_info(session: &Session) -> Value {
    let state = session.manager.state.borrow();
    let behavior = state
        .behavior
        .as_ref()
        .map(|b| b.dereferenced().name.clone());
    json!({
        "id": session.id,
        "data_id": session.data_id,
        "name": session.name,
        "label": session.label,
        "anchor": { "x": state.anchor.x, "y": state.anchor.y },
        "active_behavior": behavior,
    })
}

/// selector 为 JS 表达式：逐会话求值，带总时间预算。
fn sessions_matching(view: &RuntimeView, selector: Option<&str>) -> Result<Vec<u64>, Value> {
    let selector = selector.unwrap_or("");
    if selector.len() > SELECTOR_MAX_LEN {
        return Err(error_json(
            "Selector must not exceed 1024 characters",
            "selector_too_long",
            400,
        ));
    }
    let mut matched = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(SELECTOR_BUDGET_MS);
    for session in view.sessions.iter() {
        if selector.is_empty() {
            matched.push(session.id);
            continue;
        }
        if std::time::Instant::now() >= deadline {
            return Err(error_json(
                "Selector evaluation exceeded the total time budget",
                "selector_budget_exceeded",
                503,
            ));
        }
        session
            .manager
            .script_ctx
            .set_state(session.manager.state.clone());
        let ok = session.manager.script_ctx.eval_bool_timed(
            selector,
            std::time::Duration::from_millis(SELECTOR_EVAL_TIMEOUT_MS),
        );
        if std::time::Instant::now() >= deadline {
            return Err(error_json(
                "Selector evaluation exceeded the total time budget",
                "selector_budget_exceeded",
                503,
            ));
        }
        if ok {
            matched.push(session.id);
        }
    }
    Ok(matched)
}

fn list_mascots(view: &RuntimeView, request: &Value) -> Value {
    let selector = request.get("selector").and_then(Value::as_str);
    let matched = match sessions_matching(view, selector) {
        Ok(ids) => ids,
        Err(err) => return err,
    };
    let mascots: Vec<Value> = view
        .sessions
        .iter()
        .filter(|s| matched.contains(&s.id))
        .map(mascot_info)
        .collect();
    json!({ "mascots": mascots })
}

fn get_mascot(view: &RuntimeView, request: &Value) -> Value {
    let Some(id) = request.get("mascot_id").and_then(Value::as_i64) else {
        return error_json(
            "mascot_id must be a non-negative integer",
            "bad_request",
            400,
        );
    };
    match view.sessions.iter().find(|s| s.id as i64 == id) {
        Some(session) => json!({ "mascot": mascot_info(session) }),
        None => {
            let mut err = error_json("No such mascot", "mascot_not_found", 404);
            err["mascot"] = Value::Null;
            err
        }
    }
}

fn inspect_mascot_command(view: &RuntimeView, request: &Value) -> Value {
    let id = request
        .get("id")
        .and_then(Value::as_u64)
        .or_else(|| request.get("mascot_id").and_then(Value::as_u64));
    let Some(id) = id else {
        return error_json("id is required", "bad_request", 400);
    };
    match view.sessions.iter().find(|s| s.id == id) {
        Some(session) => json!({
            "id": id,
            "name": session.name,
            "text": crate::runtime::inspector::inspect_text(session),
        }),
        None => error_json("No such mascot", "mascot_not_found", 404),
    }
}

// ---- 模板 data_id 稳定分配 ----
//
// 模板标识契约沿用 ManagerMascotRuntime 的 idCounter：加载时取得递增 id，
// 运行期间不变，新增模板不会挤占旧 id（原先按名称排序下标当 id，
// 导入新模板后既有 id 会错位）。默认模板基于最先加载的事实固定为 0。
struct DataIdRegistry {
    next: i64,
    by_name: std::collections::HashMap<String, i64>,
}

static DATA_IDS: std::sync::OnceLock<std::sync::Mutex<DataIdRegistry>> = std::sync::OnceLock::new();

fn data_id_registry() -> &'static std::sync::Mutex<DataIdRegistry> {
    DATA_IDS.get_or_init(|| {
        std::sync::Mutex::new(DataIdRegistry {
            next: 1,
            by_name: std::collections::HashMap::new(),
        })
    })
}

/// 为当前模板集中尚无 id 的模板分配递增 id（默认模板固定 0）。
/// 只增不删：查询路径调用它时不会改变已有映射，保证并发查询期间 id 稳定。
/// 返回持锁的注册表。
fn sync_data_ids(templates: &TemplateStore) -> std::sync::MutexGuard<'static, DataIdRegistry> {
    let mut registry = data_id_registry().lock().unwrap();
    for name in templates.names_sorted() {
        if crate::templates::is_default_template(&name) {
            registry.by_name.entry(name).or_insert(0);
        } else if !registry.by_name.contains_key(&name) {
            let next = registry.next;
            registry.by_name.insert(name, next);
            registry.next += 1;
        }
    }
    registry
}

/// 移除指定模板的 id（模板卸载后 id 不复用；同名模板重新加载拿新 id）。
/// 只按名字定向移除，不做全量比对，避免误删其它注册表使用者的条目。
fn remove_data_ids<'a>(names: impl IntoIterator<Item = &'a str>) {
    let mut registry = data_id_registry().lock().unwrap();
    for name in names {
        registry.by_name.remove(name);
    }
}

/// 模板名 → 稳定 data_id（未登记时返回 -1）。
pub(crate) fn template_data_id(templates: &TemplateStore, name: &str) -> i64 {
    sync_data_ids(templates)
        .by_name
        .get(name)
        .copied()
        .unwrap_or(-1)
}

/// data_id → 模板名（id 无效或模板已卸载时返回 None）。
fn template_name_for_data_id(templates: &TemplateStore, id: i64) -> Option<String> {
    sync_data_ids(templates)
        .by_name
        .iter()
        .find(|(_, data_id)| **data_id == id)
        .map(|(name, _)| name.clone())
}

fn list_loaded_mascots(view: &RuntimeView) -> Value {
    // 列表按名称排序以保持 QMap 键序契约，id 取自稳定注册表。
    let names = view.templates.names_sorted();
    let registry = sync_data_ids(view.templates);
    let templates: Vec<Value> = names
        .into_iter()
        .map(|name| {
            let meta = view.templates.metadata(&name).cloned().unwrap_or_default();
            let id = registry.by_name.get(&name).copied().unwrap_or(-1);
            json!({
                "id": id,
                "name": name,
                "version": meta.version,
                "description": meta.description,
                "author": meta.author,
            })
        })
        .collect();
    json!({ "loaded_mascots": templates })
}

// ---- 生成与修改 ----

/// 解析 anchor 字段：键缺失或为 null 时返回 Ok(None)，遵循 mascotPatchFromJson
/// 的跳过语义；存在但畸形时返回 bad_request，遵循 parseAnchor 的报错契约。
fn parse_anchor_patch(object: &Value) -> Result<Option<Vec2>, Value> {
    match object.get("anchor") {
        None | Some(Value::Null) => Ok(None),
        Some(anchor) if !anchor.is_object() => {
            Err(error_json("anchor must be an object", "bad_request", 400))
        }
        Some(anchor) => match (
            anchor.get("x").and_then(Value::as_f64),
            anchor.get("y").and_then(Value::as_f64),
        ) {
            (Some(x), Some(y)) => Ok(Some(Vec2::new(x, y))),
            _ => Err(error_json(
                "anchor must contain numeric x and y",
                "bad_request",
                400,
            )),
        },
    }
}

fn spawn_mascot(view: &mut RuntimeView, request: &Value) -> Value {
    let req = request.get("request");
    let req = req.unwrap_or(request);
    let name = req.get("name").and_then(Value::as_str).map(str::to_string);
    let data_id = req.get("data_id").and_then(Value::as_i64);
    let behavior = req
        .get("behavior")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let anchor = match parse_anchor_patch(req) {
        Ok(anchor) => anchor,
        Err(error) => return error,
    };

    let spawn_name = match (&name, data_id) {
        (Some(name), _) => match view.templates.resolve(name) {
            Some(resolved) => resolved.to_string(),
            None => {
                return error_json("Invalid mascot name or data ID", "invalid_mascot", 400);
            }
        },
        (None, Some(id)) => match template_name_for_data_id(view.templates, id) {
            Some(n) => n,
            None => return error_json("Invalid mascot name or data ID", "invalid_mascot", 400),
        },
        (None, None) => {
            return error_json("Invalid mascot name or data ID", "invalid_mascot", 400);
        }
    };

    let env = if *view.windowed {
        view.envs.sandbox.clone()
    } else {
        view.envs.primary().cloned()
    };
    let Some(env) = env else {
        return error_json("Failed to spawn mascot", "spawn_failed", 500);
    };
    let mut guard = view.backend.as_ref().map(|b| b.borrow_mut());
    let mut backend_opt = guard.as_deref_mut();
    let result = crate::runtime::session::create_session(
        view.sessions,
        view.factory,
        &mut backend_opt,
        &env,
        view.templates,
        view.next_session_id,
        &spawn_name,
        anchor,
        &behavior,
    );
    drop(guard);
    match result {
        Ok(id) => {
            let info = view
                .sessions
                .iter()
                .find(|s| s.id == id)
                .map(mascot_info)
                .unwrap_or(Value::Null);
            json!({ "mascot": info })
        }
        Err(_) => error_json("Failed to spawn mascot", "spawn_failed", 500),
    }
}

fn alter_mascot(view: &mut RuntimeView, request: &Value) -> Value {
    let Some(id) = request
        .get("mascot_id")
        .and_then(Value::as_i64)
        .filter(|v| *v >= 0)
    else {
        return error_json(
            "mascot_id must be a non-negative integer",
            "bad_request",
            400,
        );
    };
    let Some(session) = view.sessions.iter_mut().find(|s| s.id as i64 == id) else {
        let mut err = error_json("No such mascot", "mascot_not_found", 404);
        err["mascot"] = Value::Null;
        return err;
    };
    let patch = request.get("patch").unwrap_or(request);
    // anchor 为 null/缺失时跳过该字段，其余 patch 照常应用。
    match parse_anchor_patch(patch) {
        Ok(Some(anchor)) => {
            session.manager.state.borrow_mut().anchor = anchor;
        }
        Ok(None) => {}
        Err(error) => return error,
    }
    if let Some(behavior) = patch.get("behavior").and_then(Value::as_str)
        && session
            .manager
            .initial_behavior_list()
            .find(behavior)
            .is_some()
    {
        // 未知行为静默忽略，与原版一致。
        session.manager.next_behavior(behavior);
    }
    let info = mascot_info(session);
    json!({ "mascot": info })
}

fn dismiss_mascot(view: &mut RuntimeView, request: &Value) -> Value {
    let Some(id) = request
        .get("mascot_id")
        .and_then(Value::as_i64)
        .filter(|v| *v >= 0)
    else {
        return error_json(
            "mascot_id must be a non-negative integer",
            "bad_request",
            400,
        );
    };
    let Some(pos) = view.sessions.iter().position(|s| s.id as i64 == id) else {
        return error_json("No such mascot", "mascot_not_found", 404);
    };
    view.sessions.remove(pos);
    view.labels.retain(|_, mascot_id| *mascot_id != id as u64);
    maybe_show_manager_after_last_mascot(view);
    json!({})
}

fn dismiss_all_mascots(view: &mut RuntimeView, request: &Value) -> Value {
    let selector = request.get("selector").and_then(Value::as_str);
    let selector_empty = selector.is_none_or(str::is_empty);
    let matched = match sessions_matching(view, selector) {
        Ok(ids) => ids,
        Err(err) => return err,
    };
    view.sessions.retain(|s| !matched.contains(&s.id));
    view.labels
        .retain(|_, mascot_id| view.sessions.iter().any(|s| s.id == *mascot_id));
    if selector_empty {
        view.labels.clear();
    }
    maybe_show_manager_after_last_mascot(view);
    json!({})
}

fn maybe_show_manager_after_last_mascot(view: &RuntimeView) {
    if view.sessions.is_empty() && !*view.windowed && !view.cli_runtime_mode && !view.startup_mode {
        launch_manager();
    }
}

// ---- CLI 标签 ----

fn register_cli_label(view: &mut RuntimeView, request: &Value) -> Value {
    let Some(mascot_id) = request
        .get("mascot_id")
        .and_then(Value::as_i64)
        .filter(|v| *v >= 0)
    else {
        return error_json(
            "mascot_id must be a non-negative integer",
            "bad_request",
            400,
        );
    };
    if !view.sessions.iter().any(|s| s.id as i64 == mascot_id) {
        return error_json("No such mascot", "mascot_not_found", 404);
    }
    let preferred = request.get("label").and_then(Value::as_i64);

    // 已有标签：未请求或请求相同则原样返回；请求不同则报错。
    if let Some((existing, _)) = view.labels.iter().find(|(_, id)| **id == mascot_id as u64) {
        let existing = *existing;
        if preferred.is_none_or(|v| v == existing) {
            return json!({ "label": existing, "mascot_id": mascot_id });
        }
        return error_json(
            "Mascot already has a different CLI label",
            "invalid_cli_label",
            400,
        );
    }

    let label = match preferred {
        Some(label) => {
            if label < 0 {
                return error_json(
                    "CLI label must be greater than or equal to 0",
                    "invalid_cli_label",
                    400,
                );
            }
            if view.labels.contains_key(&label) {
                return error_json("CLI label is already in use", "cli_label_conflict", 400);
            }
            label
        }
        None => {
            // 自动分配：从 next_label 起找第一个未用标签。
            let mut label = *view.next_label;
            while view.labels.contains_key(&label) {
                label += 1;
            }
            *view.next_label = label + 1;
            label
        }
    };
    view.labels.insert(label, mascot_id as u64);
    if let Some(session) = view.sessions.iter_mut().find(|s| s.id as i64 == mascot_id) {
        session.label = Some(label);
    }
    json!({ "label": label, "mascot_id": mascot_id })
}

fn get_cli_label(view: &RuntimeView, request: &Value) -> Value {
    let Some(label) = request
        .get("label")
        .and_then(Value::as_i64)
        .filter(|v| *v >= 0)
    else {
        return error_json("label must be a non-negative integer", "bad_request", 400);
    };
    match view.labels.get(&label) {
        Some(id) => json!({ "label": label, "mascot_id": *id as i64 }),
        None => error_json("No such CLI label", "cli_label_not_found", 404),
    }
}

// ---- 模板导入/移除 ----

fn template_storage_paths() -> Result<(PathBuf, PathBuf), Value> {
    let Some(storage) = neurolings_pack::storage::default_storage_path() else {
        return Err(error_json(
            "Could not determine mascot storage directory",
            "storage_unavailable",
            500,
        ));
    };
    let cache = storage
        .parent()
        .map(|path| path.join("mascot-cache"))
        .unwrap_or_else(|| storage.join("mascot-cache"));
    Ok((storage, cache))
}

fn load_template_snapshot(
    storage: &std::path::Path,
    cache: &std::path::Path,
) -> Vec<crate::templates::LoadedTemplate> {
    let _ = std::fs::create_dir_all(cache);
    crate::templates::load_from_storage(storage, cache)
}

fn import_mascot_template_job(
    archive: &std::path::Path,
    storage: &std::path::Path,
    cache: &std::path::Path,
) -> Result<ImportedTemplates, Value> {
    if !archive.is_file() {
        return Err(error_json(
            "Mascot archive does not exist",
            "invalid_arguments",
            400,
        ));
    }
    let changed = match neurolings_pack::import_archive(archive, storage) {
        Ok(changed) if !changed.is_empty() => changed,
        Ok(_) => {
            return Err(error_json(
                "Could not import any mascots from the specified archive",
                "import_failed",
                400,
            ));
        }
        Err(error) => return Err(error_json(&error.to_string(), "import_failed", 400)),
    };
    Ok(ImportedTemplates {
        changed,
        templates: load_template_snapshot(storage, cache),
        store_entry: None,
    })
}

fn apply_imported_templates(view: &mut RuntimeView, imported: ImportedTemplates) -> Value {
    let ImportedTemplates {
        changed,
        templates,
        store_entry,
    } = imported;
    for name in &changed {
        view.sessions.retain(|session| &session.name != name);
        view.labels
            .retain(|_, id| view.sessions.iter().any(|session| session.id == *id));
    }
    apply_template_snapshot(view, templates);

    let mut loaded = Vec::new();
    for name in &changed {
        let resolved = view
            .templates
            .resolve(name)
            .unwrap_or(name.as_str())
            .to_string();
        let Some(meta) = view.templates.metadata(&resolved).cloned() else {
            continue;
        };
        loaded.push(json!({
            "id": template_data_id(view.templates, &resolved),
            "name": resolved,
            "version": meta.version,
            "description": meta.description,
            "author": meta.author,
        }));
    }
    if loaded.is_empty() {
        return error_json(
            "Imported mascot archive but no templates were loaded",
            "import_failed",
            500,
        );
    }
    let mut response = json!({ "loaded_mascots": loaded, "imported": loaded.len() });
    if let Some(entry) = store_entry {
        response["store_entry"] = entry;
    }
    response
}

/// 把后台完成的磁盘模板快照提交到主线程工厂。
///
/// 解压和文件读取已在线程中完成；这里仅做内存注册和必要的会话清理，避免 tick
/// 持有 RuntimeView 时进行慢 I/O。
fn apply_template_snapshot(
    view: &mut RuntimeView,
    loaded_templates: Vec<crate::templates::LoadedTemplate>,
) -> Vec<String> {
    let previous_names = view.templates.names_sorted();
    let names =
        crate::templates::apply_loaded_templates(view.templates, view.factory, &loaded_templates);
    let removed: Vec<String> = previous_names
        .into_iter()
        .filter(|name| {
            !crate::templates::is_default_template(name) && !names.iter().any(|item| item == name)
        })
        .collect();
    if !removed.is_empty() {
        view.sessions
            .retain(|session| !removed.iter().any(|name| name == &session.name));
        view.labels
            .retain(|_, id| view.sessions.iter().any(|session| session.id == *id));
        remove_data_ids(removed.iter().map(String::as_str));
    }

    #[cfg(any(windows, target_os = "macos"))]
    crate::tray::refresh(&names);
    names
}

fn remove_mascot_template(view: &mut RuntimeView, request: &Value) -> Value {
    let Some(name) = request.get("mascot_name").and_then(Value::as_str) else {
        return error_json("mascot_name must be a string", "bad_request", 400);
    };
    let name = name.trim();
    if name.is_empty() {
        return error_json(
            "Mascot template name is required",
            "invalid_mascot_template",
            400,
        );
    }
    if crate::templates::is_default_template(name) {
        return error_json(
            "Mascot template cannot be deleted",
            "mascot_template_not_deletable",
            400,
        );
    }
    if !view.templates.contains(name) {
        return error_json("No such mascot template", "mascot_template_not_found", 404);
    }
    let Some(pack_dir) = view.templates.pack_dir(name) else {
        return error_json("No such mascot template", "mascot_template_not_found", 404);
    };
    // 只允许删存储目录或解压缓存里的模板，禁止任意路径。
    let storage = neurolings_pack::storage::default_storage_path().unwrap_or_default();
    let cache = storage
        .parent()
        .map(|p| p.join("mascot-cache"))
        .unwrap_or_else(|| storage.join("mascot-cache"));
    if let Ok(target_c) = pack_dir.canonicalize() {
        let storage_ok = storage
            .canonicalize()
            .is_ok_and(|root| target_c.starts_with(&root));
        let cache_ok = cache
            .canonicalize()
            .is_ok_and(|root| target_c.starts_with(&root));
        if !storage_ok && !cache_ok {
            return error_json(
                "Refusing to delete a mascot outside the storage directory",
                "invalid_template_path",
                400,
            );
        }
    }

    let removed_cache = if pack_dir.is_dir() {
        std::fs::remove_dir_all(&pack_dir)
    } else {
        std::fs::remove_file(&pack_dir)
    };
    // 缓存目录之外还要删掉存储里的 .mascot，否则刷新会把包再解出来。
    let package_file = neurolings_pack::package::package_path_for_name(&storage, name);
    let removed_package = if package_file.is_file() {
        std::fs::remove_file(&package_file)
    } else {
        Ok(())
    };
    if removed_cache.is_err() && removed_package.is_err() {
        return error_json("Could not remove mascot template", "remove_failed", 400);
    }
    view.sessions.retain(|s| s.name != name);
    view.labels
        .retain(|_, id| view.sessions.iter().any(|s| s.id == *id));
    view.templates.deregister(name);
    let _ = view.factory.deregister_template(name);
    remove_data_ids([name]);
    #[cfg(any(windows, target_os = "macos"))]
    crate::tray::refresh(&view.templates.names_sorted());
    json!({})
}

// ---- 预览与气泡 ----

// ---- Codex app-server 会话命令（对齐原版 ManagerCodexPage 的交互面） ----

fn codex_server_status_command(view: &RuntimeView) -> Value {
    let mut status =
        crate::codex_appserver::AppServerClient::status(crate::codex_appserver::shared_state());
    status["enabled"] = json!(
        view.settings
            .get_bool(crate::settings::KEY_CODEX_APP_SERVER_ENABLED, false)
    );
    status
}

fn codex_server_connect_command(view: &mut RuntimeView, request: &Value) -> Value {
    let enabled = view
        .settings
        .get_bool(crate::settings::KEY_CODEX_APP_SERVER_ENABLED, false);
    if !enabled {
        return error_json(
            "Codex app server is disabled in settings",
            "codex_disabled",
            400,
        );
    }
    let executable = request
        .get("executable")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            view.settings
                .get_string(crate::settings::KEY_CODEX_APP_SERVER_EXECUTABLE, "")
        });
    match crate::codex_appserver::connect(&executable) {
        Ok(()) => json!({ "started": true }),
        Err(e) => error_json(&e, "codex_start_failed", 500),
    }
}

fn codex_server_new_thread_command(request: &Value) -> Value {
    let cwd = request.get("cwd").and_then(Value::as_str).unwrap_or("");
    let sent = crate::codex_appserver::with_client(|client| {
        client.start_new_thread(cwd);
    })
    .is_some();
    if sent {
        json!({ "sent": true })
    } else {
        error_json("Codex app server is not running", "codex_not_running", 400)
    }
}

fn codex_server_resume_command(request: &Value) -> Value {
    let thread_id = request
        .get("thread_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    if thread_id.trim().is_empty() {
        return error_json("thread_id is required", "bad_request", 400);
    }
    crate::codex_appserver::with_client(|client| client.resume_thread(thread_id));
    json!({ "sent": true })
}

fn codex_server_turn_command(request: &Value) -> Value {
    let Some(text) = request.get("text").and_then(Value::as_str) else {
        return error_json("text is required", "bad_request", 400);
    };
    let plan = request
        .get("plan")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    crate::codex_appserver::with_client(|client| client.start_turn(text, plan));
    json!({ "sent": true })
}

fn codex_server_steer_command(request: &Value) -> Value {
    let Some(text) = request.get("text").and_then(Value::as_str) else {
        return error_json("text is required", "bad_request", 400);
    };
    crate::codex_appserver::with_client(|client| client.steer_turn(text));
    json!({ "sent": true })
}

fn codex_server_resolve_command(request: &Value) -> Value {
    let Some(id) = request.get("id").and_then(Value::as_str) else {
        return error_json("id is required", "bad_request", 400);
    };
    let Some(decision) = request.get("decision").and_then(Value::as_str) else {
        return error_json("decision is required", "bad_request", 400);
    };
    match crate::codex_appserver::with_client(|client| client.resolve_approval(id, decision)) {
        Some(true) => json!({ "resolved": true }),
        Some(false) => error_json(
            "This approval is no longer available.",
            "approval_unavailable",
            410,
        ),
        None => error_json("Codex app server is not running", "codex_not_running", 400),
    }
}

fn codex_server_input_command(request: &Value) -> Value {
    let Some(id) = request.get("id").and_then(Value::as_str) else {
        return error_json("id is required", "bad_request", 400);
    };
    let answers = request.get("answers").cloned().unwrap_or(json!({}));
    match crate::codex_appserver::with_client(|client| client.resolve_user_input(id, answers)) {
        Some(true) => json!({ "resolved": true }),
        Some(false) => error_json(
            "This input request is no longer available.",
            "input_unavailable",
            410,
        ),
        None => error_json("Codex app server is not running", "codex_not_running", 400),
    }
}

// ---- 商店 GitHub 登录会话（Device Flow 分步轮询，跨命令保持状态） ----

struct StoreSession {
    auth: neurolings_store::github::GitHubAuth,
    device: Option<neurolings_store::github::DeviceCodeInfo>,
    poll_interval: u64,
    expires_at: Option<std::time::Instant>,
}

static STORE_SESSION: std::sync::OnceLock<std::sync::Mutex<Option<StoreSession>>> =
    std::sync::OnceLock::new();

fn store_session_cell() -> &'static std::sync::Mutex<Option<StoreSession>> {
    STORE_SESSION.get_or_init(|| std::sync::Mutex::new(None))
}

fn with_store_session<T>(f: impl FnOnce(&mut StoreSession) -> T) -> Result<T, String> {
    let mut guard = store_session_cell().lock().unwrap();
    if guard.is_none() {
        let client_id = neurolings_store::config::github_login_client_id();
        if client_id.is_empty() {
            return Err("GitHub login is not configured".to_string());
        }
        let credentials = neurolings_store::github::create_platform_credential_store();
        neurolings_store::github::migrate_ce_credential(credentials.as_ref());
        let auth = neurolings_store::github::GitHubAuth::new(client_id, credentials);
        *guard = Some(StoreSession {
            auth,
            device: None,
            poll_interval: 5,
            expires_at: None,
        });
    }
    Ok(f(guard.as_mut().expect("session initialized")))
}

fn store_github_status_command() -> Value {
    let configured = !neurolings_store::config::github_login_client_id().is_empty();
    let cell = store_session_cell();
    let mut guard = cell.lock().unwrap();
    if guard.is_none() && configured {
        let client_id = neurolings_store::config::github_login_client_id();
        let credentials = neurolings_store::github::create_platform_credential_store();
        neurolings_store::github::migrate_ce_credential(credentials.as_ref());
        *guard = Some(StoreSession {
            auth: neurolings_store::github::GitHubAuth::new(client_id, credentials),
            device: None,
            poll_interval: 5,
            expires_at: None,
        });
    }
    let signed_in = guard
        .as_ref()
        .is_some_and(|session| session.auth.is_signed_in());
    let login = guard
        .as_ref()
        .filter(|session| session.auth.is_signed_in())
        .map(|session| session.auth.user.login.clone())
        .unwrap_or_default();
    json!({ "configured": configured, "signed_in": signed_in, "login": login })
}

fn session_poll_interval() -> u64 {
    with_store_session(|session| session.poll_interval).unwrap_or(5)
}

fn store_github_start_command() -> Value {
    let info = match with_store_session(|session| session.auth.start_device_flow()).and_then(|r| r)
    {
        Ok(info) => info,
        Err(e) => return error_json(&e, "github_flow_failed", 502),
    };
    let _ = with_store_session(|session| {
        session.poll_interval = info.interval_seconds.max(1) as u64;
        session.expires_at = Some(
            std::time::Instant::now()
                + std::time::Duration::from_secs(info.expires_in_seconds.max(60) as u64),
        );
        session.device = Some(info.clone());
    });
    json!({
        "user_code": info.user_code,
        "verification_uri": info.verification_uri,
        "interval": session_poll_interval(),
        "expires_in": info.expires_in_seconds,
    })
}

fn store_github_step_command() -> Value {
    let outcome = match with_store_session(|session| {
        // 过期检查（对齐原版 device flow 超时语义）。
        if session
            .expires_at
            .is_some_and(|deadline| std::time::Instant::now() >= deadline)
        {
            return None;
        }
        let device = session.device.as_ref()?;
        Some(neurolings_store::github::poll_access_token(
            &session.auth.client_id,
            &device.device_code,
        ))
    }) {
        Ok(outcome) => outcome,
        Err(e) => return error_json(&e, "github_flow_failed", 502),
    };
    let Some(outcome) = outcome else {
        return error_json(
            "device flow not started or expired",
            "github_flow_failed",
            410,
        );
    };
    match outcome {
        neurolings_store::github::PollOutcome::Authorized { access_token, .. } => {
            match with_store_session(|session| session.auth.authorize_with_token(&access_token))
                .and_then(|r| r)
            {
                Ok(user) => json!({ "state": "authorized", "login": user.login }),
                Err(e) => error_json(&e, "github_user_failed", 502),
            }
        }
        neurolings_store::github::PollOutcome::Pending { slow_down } => {
            let interval = with_store_session(|session| {
                if slow_down {
                    session.poll_interval = (session.poll_interval + 5).min(600);
                }
                session.poll_interval
            })
            .unwrap_or(5);
            json!({ "state": "pending", "next_interval": interval })
        }
        neurolings_store::github::PollOutcome::Failed(e) => {
            error_json(&e, "github_flow_failed", 502)
        }
    }
}

fn store_github_signout_command() -> Value {
    let _ = with_store_session(|session| session.auth.sign_out());
    json!({ "signed_out": true })
}

/// 投稿：已登录令牌 + 两阶段鉴权 + multipart 上传（对齐原版提交对话框流程）。
fn store_submit_mascot_command(request: &Value) -> Value {
    let Some(path) = request.get("path").and_then(Value::as_str) else {
        return error_json("path is required", "bad_request", 400);
    };
    let id = request.get("id").and_then(Value::as_str).unwrap_or("");
    let name = request.get("name").and_then(Value::as_str).unwrap_or("");
    if id.trim().is_empty() || name.trim().is_empty() {
        return error_json("id and name are required", "bad_request", 400);
    }
    let token = with_store_session(|session| session.auth.access_token.clone()).unwrap_or_default();
    if token.is_empty() {
        return error_json("Sign in with GitHub first", "not_authenticated", 401);
    }
    let service_url = neurolings_store::config::submission_service_url();
    if service_url.is_empty() {
        return error_json(
            "Submission service is not configured",
            "store_not_configured",
            503,
        );
    }
    let metadata = json!({
        "id": id,
        "name": name,
        "version": request.get("version").and_then(Value::as_str).unwrap_or(""),
        "summary": request.get("summary").and_then(Value::as_str).unwrap_or(""),
        "description": request.get("description").and_then(Value::as_str).unwrap_or(""),
        "license": request.get("license").and_then(Value::as_str).unwrap_or(""),
        "maintainers": request.get("maintainers"),
    });
    let mut client = neurolings_store::SubmissionClient::new(service_url);
    client.set_access_token(&token);
    let idempotency_key = format!(
        "{}-{}",
        chrono::Utc::now().timestamp_millis(),
        with_store_session(|session| session.auth.user.user_id.clone()).unwrap_or_default()
    );
    let result = client.submit(
        std::path::Path::new(path),
        &metadata.to_string(),
        &idempotency_key,
        120_000,
    );
    json!({
        "ok": result.ok,
        "id": result.id,
        "status": result.status,
        "pr_url": result.pr_url,
        "pr_number": result.pr_number,
        "error_code": result.error_code,
        "error": result.error,
    })
}

/// 分析旧版 Shimeji 压缩包的可转换候选（供"创建"页第一步使用）。
fn analyze_archive_command(request: &Value) -> Value {
    let Some(path) = request.get("path").and_then(Value::as_str) else {
        return error_json("path is required", "bad_request", 400);
    };
    let analysis = neurolings_pack::legacy::analyze_legacy_archive(std::path::Path::new(path));
    let candidates: Vec<Value> = analysis
        .candidates
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "convertible": c.convertible,
                "generated_metadata": c.generated_metadata,
                "warnings": c.warnings,
                "errors": c.errors,
                "info_json": String::from_utf8_lossy(&c.info_json),
                "info_json_valid": c.info_json_valid,
                "info_json_error": c.info_json_error,
                "version": c.metadata.version,
                "author": c.metadata.author,
                "description": c.metadata.description,
            })
        })
        .collect();
    json!({
        "ok": analysis.ok,
        "error": analysis.error_message,
        "candidates": candidates,
    })
}

/// 把选中的候选转换为 .mascot 包写入指定输出目录（不导入存储）。
fn convert_archive_command(request: &Value) -> Value {
    let Some(path) = request.get("path").and_then(Value::as_str) else {
        return error_json("path is required", "bad_request", 400);
    };
    let Some(out_dir) = request.get("out_dir").and_then(Value::as_str) else {
        return error_json("out_dir is required", "bad_request", 400);
    };
    let mut selected_names: Vec<String> = Vec::new();
    let mut overrides: std::collections::BTreeMap<String, Vec<u8>> =
        std::collections::BTreeMap::new();
    if let Some(selections) = request.get("selections").and_then(Value::as_array) {
        for selection in selections {
            let Some(name) = selection.get("name").and_then(Value::as_str) else {
                continue;
            };
            selected_names.push(name.to_string());
            if let Some(info_json) = selection.get("info_json").and_then(Value::as_str) {
                overrides.insert(name.to_string(), info_json.as_bytes().to_vec());
            }
        }
    }
    if selected_names.is_empty() {
        return error_json("selections is required", "bad_request", 400);
    }
    let results = neurolings_pack::legacy::write_legacy_archive_selection_as_packages(
        std::path::Path::new(path),
        std::path::Path::new(out_dir),
        &selected_names,
        &overrides,
    );
    let entries: Vec<Value> = results
        .iter()
        .map(|r| {
            json!({
                "name": r.name,
                "ok": r.ok,
                "output": r.package_path,
                "error": r.error_message,
            })
        })
        .collect();
    let created = entries
        .iter()
        .filter(|e| e.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .count();
    json!({ "results": entries, "created": created })
}

/// 把预览图等比缩放到 128×128 内并居中合成固定尺寸画布（对齐原版预览）。
fn encode_preview_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let side = 128u32;
    let scale = side as f64 / img.width().max(img.height()) as f64;
    let w = ((img.width() as f64 * scale).round() as u32).clamp(1, side);
    let h = ((img.height() as f64 * scale).round() as u32).clamp(1, side);
    let scaled = img.resize_exact(w, h, image::imageops::FilterType::Lanczos3);
    let mut canvas = image::RgbaImage::from_pixel(side, side, image::Rgba([0, 0, 0, 0]));
    let x = (side - w) / 2;
    let y = (side - h) / 2;
    image::imageops::overlay(&mut canvas, &scaled, x as i64, y as i64);
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(canvas)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

fn preview_png(view: &RuntimeView, request: &Value) -> Value {
    let name = if let Some(name) = request.get("name").and_then(Value::as_str) {
        match view.templates.resolve(name) {
            Some(resolved) => resolved.to_string(),
            None => {
                return error_json("No such loaded mascot", "loaded_mascot_not_found", 404);
            }
        }
    } else if let Some(id) = request.get("id").and_then(Value::as_i64) {
        match template_name_for_data_id(view.templates, id) {
            Some(name) => name,
            None => {
                return error_json("No such loaded mascot", "loaded_mascot_not_found", 404);
            }
        }
    } else {
        return error_json("id or name is required", "bad_request", 400);
    };
    let name = name.as_str();
    // 虚拟模板 @ 的预览图直接取内嵌资源；其余模板读包目录。
    let embedded = || -> Option<Vec<u8>> {
        for candidate in ["a.png", "cover.png"] {
            if let Some(bytes) = crate::templates::DEFAULT_MASCOT
                .get_file(format!("img/{candidate}"))
                .map(|f| f.contents().to_vec())
            {
                return Some(bytes);
            }
        }
        crate::templates::DEFAULT_MASCOT
            .files()
            .find(|f| {
                f.path()
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("png"))
            })
            .map(|f| f.contents().to_vec())
    };
    let bytes = if crate::templates::is_default_template(name) {
        embedded()
    } else {
        let Some(img_dir) = view.templates.pack_dir(name).map(|p| p.join("img")) else {
            return error_json("No such loaded mascot", "loaded_mascot_not_found", 404);
        };
        // 选图优先级遵循既定契约（文件名大小写不敏感）：
        // a.png → cover.png → 名称排序后第一张 PNG。
        let mut pngs: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&img_dir) {
            pngs = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
                .collect();
            pngs.sort_by_key(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default()
            });
        }
        let named = |target: &str| {
            pngs.iter()
                .find(|p| {
                    p.file_name()
                        .is_some_and(|n| n.eq_ignore_ascii_case(target))
                })
                .cloned()
        };
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();
        candidates.extend(named("a.png"));
        candidates.extend(named("cover.png"));
        candidates.extend(pngs);
        candidates.iter().find_map(|p| std::fs::read(p).ok())
    };
    match bytes.as_deref().and_then(|b| encode_preview_png(b).ok()) {
        Some(png) => json!({ "preview_base64": crate::http::base64_encode(&png) }),
        None => error_json("No preview image", "preview_not_found", 404),
    }
}

fn target_session_mut<'a>(view: &'a mut RuntimeView, request: &Value) -> Option<&'a mut Session> {
    if let Some(id) = request.get("mascot_id").and_then(Value::as_u64) {
        return view.sessions.iter_mut().find(|s| s.id == id);
    }
    view.sessions.first_mut()
}

fn show_bubble(view: &mut RuntimeView, request: &Value) -> Value {
    let text = request
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if text.trim().is_empty() {
        return error_json("text is required", "bad_request", 400);
    }
    match target_session_mut(view, request) {
        Some(session) => {
            session.pending_bubble = Some(text);
            json!({ "handled": true })
        }
        None => error_json("No mascot to show a bubble on", "mascot_not_found", 404),
    }
}

fn show_codex_notification(view: &mut RuntimeView, request: &Value) -> Value {
    let Some(payload) = request.get("payload") else {
        return error_json("payload must be an object", "bad_request", 400);
    };
    let parsed = match neurolings_common::codex::parse_activity(payload) {
        Ok(parsed) => parsed,
        Err(err) => {
            return error_json(
                &format!("Invalid Codex notification: {err}"),
                "bad_request",
                400,
            );
        }
    };
    let activity = parsed.activity;
    if !parsed.recognized {
        return json!({
            "handled": false,
            "event_type": activity.event_type,
        });
    }

    // 选择陪伴模板：设置值 → 旧别名归一（Default/Default Mascot → @）
    // → 内嵌默认 @ → 历史落盘名 Default → 兜底 503。
    let mut selected = view
        .settings
        .get_string(crate::settings::KEY_CODEX_TEMPLATE, "@");
    if selected.trim().is_empty() {
        selected = "@".to_string();
    }
    if selected == "Default" || selected == "Default Mascot" {
        selected = "@".to_string();
    }
    let names = view.templates.names_sorted();
    let mut candidates = vec![selected, "@".to_string(), "Default".to_string()];
    candidates.dedup();
    let Some(selected) = candidates
        .into_iter()
        .find(|c| names.iter().any(|n| n == c))
    else {
        return error_json(
            "No mascot is available for the Codex notification",
            "codex_notification_unavailable",
            503,
        );
    };

    // 优先复用已运行的同名桌宠，否则召唤一只。
    let target = view
        .sessions
        .iter()
        .find(|s| s.name == selected)
        .map(|s| s.id);
    let target_id = match target {
        Some(id) => id,
        None => {
            let mut spawn_request = json!({
                "command": "spawn_mascot",
                "request": { "name": selected },
            });
            let response = spawn_mascot(view, &spawn_request);
            match response
                .get("mascot")
                .and_then(|m| m.get("id"))
                .and_then(Value::as_u64)
            {
                Some(id) => id,
                None => {
                    spawn_request["request"]["name"] = json!("Default");
                    let response = spawn_mascot(view, &spawn_request);
                    match response
                        .get("mascot")
                        .and_then(|m| m.get("id"))
                        .and_then(Value::as_u64)
                    {
                        Some(id) => id,
                        None => {
                            return error_json(
                                "No mascot is available for the Codex notification",
                                "codex_notification_unavailable",
                                503,
                            );
                        }
                    }
                }
            }
        }
    };

    // 通知去重（对齐原版 threadId+turnId 组合键，60 秒窗口、容量 64 逐出最旧）。
    {
        let now = std::time::Instant::now();
        view.codex_seen
            .retain(|(_thread, _turn, seen)| now.duration_since(*seen).as_secs() < 60);
        let key = (activity.thread_id.clone(), activity.turn_id.clone());
        if !key.0.is_empty()
            && !key.1.is_empty()
            && view
                .codex_seen
                .iter()
                .any(|(t, r, _)| (t, r) == (&key.0, &key.1))
        {
            return json!({ "handled": false, "deduplicated": true });
        }
        if view.codex_seen.len() >= 64 {
            view.codex_seen.pop_front();
        }
        view.codex_seen.push_back((key.0, key.1, now));
    }

    // 文案：新会话显示标题+描述，否则显示最终回复。
    let message = if activity.is_new_session {
        let mut text = activity.session_title.trim().to_string();
        let description = activity.session_description.trim();
        if !description.is_empty() {
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(description);
        }
        if text.is_empty() {
            "New session has no content to display.".to_string()
        } else {
            text
        }
    } else {
        let message = activity.last_assistant_message.trim().to_string();
        if message.is_empty() {
            "The task completed without a reply to display.".to_string()
        } else {
            message
        }
    };
    let excerpt = neurolings_common::codex::compact_bubble_source(
        &message,
        neurolings_common::codex::MAX_RETAINED_CHARS,
    );
    let retained = excerpt.chars().count();
    // 标题随语言设置本地化（译文对齐原版 shijima-qt_zh_CN.ts）。
    let zh = matches!(view.settings.locale(), crate::settings::Locale::ZhCn);
    let title = match (activity.is_new_session, zh) {
        (true, false) => "Codex · New session".to_string(),
        (true, true) => "Codex · 新会话".to_string(),
        (false, false) => "Codex · Completed".to_string(),
        (false, true) => "Codex · 已完成".to_string(),
    };
    let duration = crate::runtime::bubbles::codex_display_duration(retained);

    if let Some(session) = view.sessions.iter_mut().find(|s| s.id == target_id) {
        // 入队（上限 8，满时丢最旧，由气泡循环逐条展示）。
        session
            .codex_bubble_queue
            .push_back((title, excerpt, duration));
    }
    json!({
        "handled": true,
        "event_type": activity.event_type,
        "state": activity.state.name(),
    })
}

// ---- 组合 / 自启 / Codex / 窗口模式 / 设置 ----

/// 把组合序列化为命令响应中的对象形态。
fn combination_json(combo: &crate::combinations::SavedCombination) -> Value {
    json!({
        "id": combo.id,
        "name": combo.name,
        "saved_at": combo.saved_at,
        "mascots": combo.mascots.iter().map(|m| json!({
            "name": m.name,
            "count": m.count,
        })).collect::<Vec<_>>(),
        "total": combo.total(),
    })
}

fn save_combination(view: &mut RuntimeView, request: &Value) -> Value {
    let Some(name) = request.get("name").and_then(Value::as_str) else {
        return error_json("name is required", "bad_request", 400);
    };
    if name.trim().is_empty() {
        return error_json("name is required", "bad_request", 400);
    }
    let mascots = crate::combinations::aggregate(view.sessions.iter().map(|s| s.name.clone()));
    if mascots.is_empty() {
        return error_json("No running mascots to save", "bad_request", 400);
    }
    match view.combinations.save(name, mascots.clone()) {
        Ok(id) => json!({
            "saved": true,
            "id": id,
            "name": name,
            "count": mascots.iter().map(|m| m.count).sum::<u32>(),
        }),
        Err(e) => error_json(&e, "save_failed", 500),
    }
}

fn restore_combination(view: &mut RuntimeView, request: &Value) -> Value {
    let Some(id) = request.get("id").and_then(Value::as_str) else {
        return error_json("id is required", "bad_request", 400);
    };
    let Some(combo) = view.combinations.get(id) else {
        return error_json("No such combination", "combination_not_found", 404);
    };
    // 恢复 = 清场重建（对齐原版 50/200 安全限位与 missing/failed 去重）。
    view.sessions.clear();
    view.labels.clear();

    const K_MAX_PER_ENTRY: u32 = crate::combinations::MAX_MASCOTS_PER_ENTRY;
    const K_MAX_PER_COMBINATION: u32 = crate::combinations::MAX_MASCOTS_PER_COMBINATION;
    let mut attempted: u32 = 0;
    let mut spawned: u32 = 0;
    let mut missing: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    let mut last_error: Option<Value> = None;

    for member in &combo.mascots {
        if !view.templates.contains(&member.name) {
            missing.push(member.name.clone());
            continue;
        }
        let count = member.count.min(K_MAX_PER_ENTRY);
        for _ in 0..count {
            if attempted >= K_MAX_PER_COMBINATION {
                break;
            }
            attempted += 1;
            let req = json!({ "command": "spawn_mascot", "request": { "name": member.name } });
            let result = spawn_mascot(view, &req);
            if result.get("error").is_some() {
                failed.push(member.name.clone());
                last_error = Some(result);
            } else {
                spawned += 1;
            }
        }
        if attempted >= K_MAX_PER_COMBINATION {
            break;
        }
    }
    // 去重（与原版 removeDuplicates 对齐）
    missing.sort();
    missing.dedup();
    failed.sort();
    failed.dedup();

    let mut out = json!({
        "restored": true,
        "id": combo.id,
        "name": combo.name,
        "spawned": spawned,
        "attempted": attempted,
    });
    if !missing.is_empty() {
        out["missing"] = json!(missing);
    }
    if !failed.is_empty() {
        out["failed"] = json!(failed);
    }
    if let Some(err) = last_error {
        out["warning"] = err;
    }
    out
}

fn list_combinations(view: &RuntimeView) -> Value {
    let combinations: Vec<Value> = view
        .combinations
        .list()
        .iter()
        .map(combination_json)
        .collect();
    let last = view
        .combinations
        .get(crate::combinations::LAST_BEFORE_CLOSE_ID)
        .map(|combo| combination_json(&combo));
    json!({
        "combinations": combinations,
        "last_before_close": last,
    })
}

fn get_combination(view: &RuntimeView, request: &Value) -> Value {
    let Some(id) = request.get("id").and_then(Value::as_str) else {
        return error_json("id is required", "bad_request", 400);
    };
    if id.trim().is_empty() {
        return error_json("id is required", "bad_request", 400);
    }
    let Some(combo) = view.combinations.get(id) else {
        return error_json("No such combination", "combination_not_found", 404);
    };
    combination_json(&combo)
}

fn delete_combination(view: &mut RuntimeView, request: &Value) -> Value {
    let Some(id) = request.get("id").and_then(Value::as_str) else {
        return error_json("id is required", "bad_request", 400);
    };
    if id == crate::combinations::LAST_BEFORE_CLOSE_ID {
        return error_json(
            "The last combination before close cannot be deleted",
            "mascot_template_not_deletable",
            400,
        );
    }
    match view.combinations.delete(id) {
        Ok(true) => json!({ "deleted": true, "id": id }),
        Ok(false) => error_json("No such combination", "combination_not_found", 404),
        Err(e) => error_json(&e, "delete_failed", 500),
    }
}

fn set_autostart_command(request: &Value) -> Value {
    let enabled = request
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    match neurolings_platform::autostart::set_autostart(enabled, &exe) {
        Ok(()) => json!({ "enabled": enabled }),
        Err(e) => error_json(&e.to_string(), "autostart_failed", 500),
    }
}

fn get_autostart_command() -> Value {
    let enabled = neurolings_platform::autostart::is_autostart_enabled().unwrap_or(false);
    json!({ "enabled": enabled })
}

fn codex_setup_command(request: &Value) -> Value {
    let enabled = request
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match crate::codex::set_codex_notify_hook(enabled) {
        Ok(path) => json!({ "enabled": enabled, "config": path.to_string_lossy() }),
        Err(e) => error_json(&e, "codex_setup_failed", 500),
    }
}

fn codex_status_command() -> Value {
    json!({ "installed": crate::codex::is_codex_notify_hook_installed() })
}

/// 商店缓存目录（mascot-cache/store，与 C++ 版缓存布局一致）。
fn store_cache_dir() -> Option<std::path::PathBuf> {
    let storage = neurolings_pack::storage::default_storage_path()?;
    let dir = storage
        .parent()
        .map(|p| p.join("mascot-cache").join("store"))
        .unwrap_or_else(|| storage.join("mascot-cache").join("store"));
    Some(dir)
}

fn store_status_command() -> Value {
    json!({
        "configured": neurolings_store::config::is_configured(),
        "index_url": neurolings_store::config::index_url(),
        "login_configured": neurolings_store::config::is_login_configured(),
    })
}

/// 拉取（或读缓存）商店索引；refresh=true 时强制网络刷新。
/// 仅由后台 worker 调用，避免网络重试占住主 tick。
fn store_index_command(request: &Value) -> Value {
    use neurolings_store::{StoreCache, fetch_index};

    let refresh = request
        .get("refresh")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let url = neurolings_store::config::index_url();
    if url.is_empty() {
        return error_json(
            "Store index URL is not configured",
            "store_not_configured",
            503,
        );
    }
    let Some(dir) = store_cache_dir() else {
        return error_json("Storage unavailable", "storage_unavailable", 500);
    };
    let cache = StoreCache::new(dir);

    let cached = cache.load_index();
    // 缓存直返必须经过 StoreIndex::parse 校验，防止恶意缓存常驻
    if !refresh
        && let Some(c) = &cached
        && let Ok(index) = neurolings_store::StoreIndex::parse(&c.body)
    {
        return store_index_json(&index, true);
    }

    let (etag, lm) = cached
        .as_ref()
        .map(|c| (c.etag.clone(), c.last_modified.clone()))
        .unwrap_or_default();
    let mut response = fetch_index(&url, &etag, &lm, 15_000);
    // 索引拉取重试与原版一致：最多 2 次重试，间隔线性退避（1s、2s）。
    let mut attempt = 0u32;
    while !response.ok && attempt < 2 {
        attempt += 1;
        std::thread::sleep(std::time::Duration::from_millis(1000 * attempt as u64));
        response = fetch_index(&url, &etag, &lm, 15_000);
    }
    if !response.ok {
        // 网络失败时退回缓存（若有且校验通过）。
        if let Some(c) = &cached
            && let Ok(index) = neurolings_store::StoreIndex::parse(&c.body)
        {
            let mut out = store_index_json(&index, true);
            out["warning"] = json!({ "code": response.error_code, "error": response.error });
            // 标记 stale 与原版一致：若解析失败则视为 corrupt
            return out;
        }
        // 尝试 previous 缓存
        if let Some(prev) = cache.load_previous_index()
            && let Ok(index) = neurolings_store::StoreIndex::parse(&prev.body)
        {
            let mut out = store_index_json(&index, true);
            out["warning"] = json!({ "code": response.error_code, "error": response.error });
            out["stale"] = json!(true);
            return out;
        }
        return error_json(&response.error, &response.error_code, 502);
    }
    if response.not_modified {
        if let Some(c) = &cached
            && let Ok(index) = neurolings_store::StoreIndex::parse(&c.body)
        {
            return store_index_json(&index, true);
        }
        return error_json("Empty store cache", "store_empty", 502);
    }

    let index = match neurolings_store::StoreIndex::parse(&response.body) {
        Ok(v) => v,
        Err(e) => return error_json(&format!("Invalid store index: {e}"), "invalid_index", 502),
    };
    let _ = cache.save_index(&neurolings_store::CachedIndex {
        body: response.body.clone(),
        etag: response.etag.clone(),
        last_modified: response.last_modified.clone(),
    });
    crate::log::info(
        "store",
        &format!(
            "store_index: fetched {} entries from {}",
            index.entries.len(),
            url
        ),
    );
    store_index_json(&index, false)
}

fn store_index_json(index: &neurolings_store::StoreIndex, from_cache: bool) -> Value {
    json!({
        "ok": true,
        "from_cache": from_cache,
        "registry": index.registry,
        "generated_at": index.generated_at,
        "entries": index.entries,
    })
}

/// 商店安装后台工作：按 id 找条目、下载并导入，完成后返回待主线程提交的模板快照。
fn store_install_job(id: &str) -> Result<ImportedTemplates, Value> {
    use neurolings_store::{StoreCache, download};

    let Some(dir) = store_cache_dir() else {
        return Err(error_json(
            "Storage unavailable",
            "storage_unavailable",
            500,
        ));
    };
    let cache = StoreCache::new(&dir);
    let Some(cached) = cache.load_index().or_else(|| cache.load_previous_index()) else {
        return Err(error_json(
            "Store index not fetched yet; refresh first",
            "store_empty",
            409,
        ));
    };
    let index = match neurolings_store::StoreIndex::parse(&cached.body) {
        Ok(v) => v,
        Err(error) => {
            return Err(error_json(
                &format!("Invalid cached index: {error}"),
                "invalid_index",
                500,
            ));
        }
    };
    let Some(entry) = index.entries.iter().find(|entry| entry.id == id).cloned() else {
        return Err(error_json("No such store entry", "entry_not_found", 404));
    };
    if entry.download.url.is_empty() {
        return Err(error_json(
            "Entry has no download URL",
            "invalid_entry",
            400,
        ));
    }
    if !neurolings_store::index::is_trusted_download_url(&entry.download.url, &index.registry) {
        return Err(error_json(
            "Download URL is not from a trusted host",
            "untrusted_url",
            400,
        ));
    }

    // 原版命名：sanitized(id) + "-" + version + ".mascot"，避免 URL 尾段污染与重名
    let sanitized_id = neurolings_pack::package::sanitized_package_base_name(&entry.id);
    let sanitized_version = entry
        .version
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let file_name = format!("{sanitized_id}-{sanitized_version}.mascot");
    let downloads = dir.join("downloads");
    let _ = std::fs::create_dir_all(&downloads);
    let destination = downloads.join(&file_name);

    // 下载重试：对 network/timeout 错误最多 2 次重试（间隔 1500ms），对齐原版 kMaxDownloadRetries
    let mut last_err = String::new();
    let mut ok = false;
    for attempt in 0..3 {
        match download(
            &entry.download.url,
            &destination,
            &entry.download.sha256,
            60_000,
        ) {
            Ok(()) => {
                ok = true;
                break;
            }
            Err(e) => {
                last_err = e.clone();
                let is_retryable =
                    e.contains("network") || e.contains("timeout") || e.contains("failed");
                if !is_retryable || attempt == 2 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1500));
            }
        }
    }
    if !ok {
        return Err(error_json(&last_err, "download_failed", 502));
    }

    let (storage, template_cache) = template_storage_paths()?;
    let mut imported = import_mascot_template_job(&destination, &storage, &template_cache)?;
    imported.store_entry = Some(json!({
        "id": entry.id,
        "name": entry.name,
        "version": entry.version,
    }));
    Ok(imported)
}

fn set_window_mode(view: &mut RuntimeView, request: &Value) -> Value {
    let enabled = request
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    *view.windowed = enabled;
    json!({ "windowed": enabled })
}

fn get_settings_command(view: &RuntimeView, request: &Value) -> Value {
    // 指定 key 时只返回该项。
    if let Some(key) = request.get("key").and_then(Value::as_str) {
        if key == crate::settings::KEY_PROXY_PASS {
            return json!({
                "key": key,
                "configured": setting_has_non_empty_string(view.settings, key),
            });
        }
        if let Some(value) = view.settings.get(key) {
            return json!({ "key": key, "value": value });
        }
    }
    // 否则返回全部设置快照。
    let mut out = serde_json::Map::new();
    for key in [
        crate::settings::KEY_USER_SCALE,
        crate::settings::KEY_DETACH_THRESHOLD,
        crate::settings::KEY_WINDOW_PUSHING,
        crate::settings::KEY_BUBBLE_ENABLED,
        crate::settings::KEY_BUBBLE_CLICKS,
        crate::settings::KEY_MULTIPLICATION,
        crate::settings::KEY_CODEX_ENABLED,
        crate::settings::KEY_CODEX_TEMPLATE,
        crate::settings::KEY_CODEX_APP_SERVER_ENABLED,
        crate::settings::KEY_CODEX_APP_SERVER_EXECUTABLE,
        crate::settings::KEY_CODEX_APPROVAL_BUBBLE,
        crate::settings::KEY_CODEX_PLAN_BUBBLE,
        crate::settings::KEY_HTTP_ENABLED,
        crate::settings::KEY_STARTUP_SILENT,
        crate::settings::KEY_STARTUP_COMBO_MODE,
        crate::settings::KEY_STARTUP_COMBO_ID,
        crate::settings::KEY_WINDOWED_BG,
        crate::settings::KEY_UPDATE_CHECK,
        crate::settings::KEY_LANGUAGE,
        crate::settings::KEY_PROXY_MODE,
        crate::settings::KEY_PROXY_HOST,
        crate::settings::KEY_PROXY_PORT,
        crate::settings::KEY_PROXY_USER,
    ] {
        if let Some(value) = view.settings.get(key) {
            out.insert(key.to_string(), value.clone());
        }
    }
    out.insert(
        "update/proxyPasswordConfigured".to_string(),
        json!(setting_has_non_empty_string(
            view.settings,
            crate::settings::KEY_PROXY_PASS
        )),
    );
    out.insert("windowed".to_string(), json!(*view.windowed));
    Value::Object(out)
}

fn set_settings_command(view: &mut RuntimeView, request: &Value) -> Value {
    let Some(key) = request.get("key").and_then(Value::as_str) else {
        return error_json("key is required", "bad_request", 400);
    };
    let Some(value) = request.get("value") else {
        return error_json("value is required", "bad_request", 400);
    };
    match view.settings.set(key, value.clone()) {
        Ok(()) => {
            // 语言变化时同步托盘菜单文案（右键菜单下次构建即生效）。
            if key == crate::settings::KEY_LANGUAGE {
                #[cfg(any(windows, target_os = "macos"))]
                {
                    let locale = view.settings.locale();
                    crate::tray::set_locale(locale);
                    crate::tray::refresh(&view.templates.names_sorted());
                }
            }
            if key == crate::settings::KEY_PROXY_PASS {
                json!({
                    "key": key,
                    "configured": value.as_str().is_some_and(|text| !text.is_empty()),
                })
            } else {
                json!({ "key": key, "value": value })
            }
        }
        Err(e) => error_json(&e, "settings_failed", 500),
    }
}

fn setting_has_non_empty_string(settings: &crate::settings::Settings, key: &str) -> bool {
    settings
        .get(key)
        .and_then(Value::as_str)
        .is_some_and(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::templates::LoadedTemplate;

    fn register(store: &mut TemplateStore, name: &str) {
        store.register(&LoadedTemplate {
            name: name.to_string(),
            dir: std::path::PathBuf::new(),
            actions_xml: String::new(),
            behaviors_xml: String::new(),
            metadata: Default::default(),
            virtual_: true,
        });
    }

    /// Manager 显式传入的令牌必须是完整 256 位值；缺失时才允许安全生成。
    #[test]
    fn internal_control_token_rejects_invalid_supplied_value() {
        let generated = generate_internal_control_token().unwrap();
        assert_eq!(generated.len(), 43);
        assert!(valid_internal_control_token(&generated));
        assert_eq!(
            select_internal_control_token(Some(generated.clone())).unwrap(),
            generated
        );
        assert!(select_internal_control_token(Some("invalid".to_string())).is_err());
    }

    /// worker 睡眠期间主线程仍可推进；调用方断开后完成事件仍保留到主线程提交。
    #[test]
    fn background_job_does_not_block_ticks_or_drop_disconnected_completion() {
        let mut jobs = BackgroundJobs::new();
        let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
        let operation_id = jobs
            .submit(
                BackgroundJobKind::TestSleep(std::time::Duration::from_millis(80)),
                &reply_tx,
            )
            .unwrap();
        assert_eq!(
            jobs.operation_status(&json!({ "operation_id": operation_id }))["pending"],
            true
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(20);
        let mut ticks = 0;
        while std::time::Instant::now() < deadline {
            ticks += 1;
            assert!(jobs.try_recv().is_none());
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(ticks > 0);

        let completion_deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        let completion = loop {
            if let Some(completion) = jobs.try_recv() {
                break completion;
            }
            assert!(std::time::Instant::now() < completion_deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        assert!(matches!(
            completion.result,
            BackgroundResult::Reply(ref value) if value["ok"] == true
        ));

        // 模拟服务线程已在等待窗口后断开：主循环仍会先提交状态并保留最终结果。
        jobs.finish(completion.id, json!({ "committed": true }));
        let status = jobs.operation_status(&json!({ "operation_id": operation_id }));
        assert_eq!(status["operation_id"], operation_id);
        assert_eq!(status["pending"], false);
        assert_eq!(status["operation_state"], "completed");
        assert_eq!(status["committed"], true);
        drop(reply_rx);
        let reply = jobs.take_reply(completion.id).unwrap();
        assert!(reply.send(json!({ "committed": true })).is_err());
    }

    /// 仅任务已分配稳定 ID 时，等待超时才允许返回“已受理”。
    #[test]
    fn command_timeout_is_reported_as_pending() {
        let (_reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
        let (operation_tx, operation_rx) = std::sync::mpsc::sync_channel(1);
        operation_tx.send(41).unwrap();
        let response = wait_for_command_reply(
            &reply_rx,
            &operation_rx,
            std::time::Duration::from_millis(1),
        );
        assert_eq!(response["status"], 202);
        assert_eq!(response["accepted"], true);
        assert_eq!(response["pending"], true);
        assert_eq!(response["operation_id"], 41);
        assert_eq!(response["operation_state"], "pending");
        assert!(response.get("state").is_none());
        assert!(response.get("error").is_none());
    }

    /// 主循环未确认受理的命令超时必须报错，避免 Manager 轮询不存在的操作。
    #[test]
    fn command_timeout_without_operation_id_is_failure() {
        let (_reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
        let (_operation_tx, operation_rx) = std::sync::mpsc::sync_channel(1);
        let response = wait_for_command_reply(
            &reply_rx,
            &operation_rx,
            std::time::Duration::from_millis(1),
        );
        assert_eq!(response["status"], 504);
        assert_eq!(response["code"], "timeout");
        assert_ne!(response["pending"], true);
    }

    /// 已完成的后台失败必须保留原始错误状态，供 Manager 正确展示。
    #[test]
    fn completed_operation_preserves_failure_response() {
        let mut jobs = BackgroundJobs::new();
        jobs.finish(
            52,
            error_json("package signature is invalid", "invalid_archive", 422),
        );

        let response = jobs.operation_status(&json!({ "operation_id": 52 }));
        assert_eq!(response["status"], 422);
        assert_eq!(response["error"], "package signature is invalid");
        assert_eq!(response["pending"], false);
        assert_eq!(response["operation_state"], "failed");
    }

    /// 操作协议状态不得覆盖后台命令自身的状态字段。
    #[test]
    fn completed_operation_preserves_business_state() {
        let mut jobs = BackgroundJobs::new();
        for (id, state) in [(53, "pending"), (54, "authorized")] {
            jobs.finish(id, json!({ "state": state }));
            let response = jobs.operation_status(&json!({ "operation_id": id }));
            assert_eq!(response["state"], state);
            assert_eq!(response["operation_state"], "completed");
            assert_eq!(response["pending"], false);
        }
    }

    /// #3b/#3c：anchor 缺失或为 null 时跳过，不报错。
    #[test]
    fn anchor_null_or_missing_is_skipped() {
        assert_eq!(parse_anchor_patch(&json!({})).unwrap(), None);
        assert_eq!(parse_anchor_patch(&json!({"anchor": null})).unwrap(), None);
        // 其余字段存在与否不影响 anchor 判定。
        assert_eq!(
            parse_anchor_patch(&json!({"anchor": null, "behavior": "x"})).unwrap(),
            None
        );
    }

    /// #3b：anchor 存在但畸形时返回 bad_request。
    #[test]
    fn anchor_malformed_is_bad_request() {
        for body in [
            json!({"anchor": {"x": 1}}),
            json!({"anchor": {"x": 1, "y": "top"}}),
            json!({"anchor": "left"}),
        ] {
            let err = parse_anchor_patch(&body).unwrap_err();
            assert_eq!(err.get("status").and_then(Value::as_i64), Some(400));
            assert_eq!(err.get("code").and_then(Value::as_str), Some("bad_request"));
        }
    }

    #[test]
    fn anchor_valid_parses() {
        let anchor = parse_anchor_patch(&json!({"anchor": {"x": 1.5, "y": -2}}))
            .unwrap()
            .unwrap();
        assert_eq!((anchor.x, anchor.y), (1.5, -2.0));
    }

    /// #2：新增排序更靠前的模板不改变已有模板的 data_id。
    #[test]
    fn data_ids_stay_stable_when_new_template_added() {
        let mut store = TemplateStore::new();
        register(&mut store, "TestStableIdBeta");
        register(&mut store, "TestStableIdDelta");
        let beta = template_data_id(&store, "TestStableIdBeta");
        let delta = template_data_id(&store, "TestStableIdDelta");
        assert!(beta > 0 && delta > 0 && beta != delta);

        register(&mut store, "TestStableIdAlpha");
        assert_eq!(template_data_id(&store, "TestStableIdBeta"), beta);
        assert_eq!(template_data_id(&store, "TestStableIdDelta"), delta);
        let alpha = template_data_id(&store, "TestStableIdAlpha");
        assert!(alpha > beta && alpha > delta);
        assert_eq!(
            template_name_for_data_id(&store, beta).as_deref(),
            Some("TestStableIdBeta")
        );
    }

    /// #2：卸载后 id 不复用；同名模板重新注册拿到新 id。
    #[test]
    fn data_id_not_reused_after_removal() {
        let mut store = TemplateStore::new();
        register(&mut store, "TestReuseGone");
        register(&mut store, "TestReuseStay");
        let gone = template_data_id(&store, "TestReuseGone");
        let stay = template_data_id(&store, "TestReuseStay");

        store.deregister("TestReuseGone");
        remove_data_ids(["TestReuseGone"]);
        assert_eq!(template_data_id(&store, "TestReuseStay"), stay);
        assert_eq!(template_name_for_data_id(&store, gone), None);

        register(&mut store, "TestReuseGone");
        let reassigned = template_data_id(&store, "TestReuseGone");
        assert_ne!(reassigned, gone);
        assert_eq!(template_data_id(&store, "TestReuseStay"), stay);
    }

    /// #2：默认模板固定为 id 0，保持最先加载时 idCounter=0 的契约。
    #[test]
    fn default_template_id_is_zero() {
        let mut store = TemplateStore::new();
        register(&mut store, "TestDefaultZeroExtra");
        register(&mut store, crate::templates::DEFAULT_TEMPLATE_NAME);
        assert_eq!(
            template_data_id(&store, crate::templates::DEFAULT_TEMPLATE_NAME),
            0
        );
        assert!(template_data_id(&store, "TestDefaultZeroExtra") > 0);
    }

    #[test]
    fn macos_manager_heartbeat_uses_virtual_desktop_coordinates() {
        let heartbeat = Rect {
            left: 100,
            top: 40,
            right: 1100,
            bottom: 720,
        };
        // 参考屏左侧与上方各有一块显示器时，其在虚拟桌面中不再从零开始。
        let reference_screen = Rect {
            left: 1920,
            top: 1080,
            right: 3840,
            bottom: 2160,
        };
        assert_eq!(
            normalize_macos_manager_heartbeat_rect(heartbeat, Some(reference_screen)),
            Rect {
                left: 2020,
                top: 1120,
                right: 3020,
                bottom: 1800,
            }
        );
    }
}
