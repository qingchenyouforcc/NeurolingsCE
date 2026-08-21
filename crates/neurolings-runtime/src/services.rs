//! 命令服务层：本地 IPC 与 HTTP 共用的命令分发与执行。
//!
//! 请求在服务线程进入通道，由主循环执行（引擎状态为主线程独占），
//! 经一次性通道回传响应。命令形状与原版 IPC 契约逐一对齐：
//! spawn/alter 使用 request/patch 子对象，标签从 0 起分配，
//! selector 为 JS 表达式并带时间预算。

use std::sync::mpsc::{Receiver, Sender, SyncSender, sync_channel};

use serde_json::{Value, json};

use neurolings_common::version;
use neurolings_engine::math::Vec2;

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

pub struct PendingCommand {
    pub request: Value,
    pub reply: SyncSender<Value>,
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

/// 从服务线程提交请求并阻塞等待回复。
pub fn call(tx: &Sender<PendingCommand>, request: Value) -> Value {
    let (reply_tx, reply_rx) = sync_channel::<Value>(1);
    if tx
        .send(PendingCommand {
            request,
            reply: reply_tx,
        })
        .is_err()
    {
        return error_json("runtime is shutting down", "unavailable", 503);
    }
    match reply_rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(value) => value,
        Err(_) => error_json("command timed out", "timeout", 504),
    }
}

pub fn error_json(message: &str, code: &str, status: i32) -> Value {
    json!({ "error": message, "code": code, "status": status })
}

pub const MESSAGE_MAX_BYTES: usize = IPC_MESSAGE_MAX_BYTES;

/// 主循环状态的可变视图。
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
    pub windowed: &'a mut bool,
}

pub fn dispatch(request: &Value, view: &mut RuntimeView) -> Value {
    let Some(command) = request.get("command").and_then(Value::as_str) else {
        return error_json("Missing command", "bad_request", 400);
    };
    match command {
        "ping" => json!({
            "ok": true,
            "app": version::APP_NAME,
            "api_version": "v1",
        }),
        "list_mascots" => list_mascots(view, request),
        "list_loaded_mascots" => list_loaded_mascots(view),
        "import_mascot_template" => import_mascot_template(view, request),
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
        // 以下为运行时扩展命令（管理器与工具链使用）。
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
        "store_index" => store_index_command(view, request),
        "store_install" => store_install_command(view, request),
        _ => error_json("Unknown command", "bad_request", 400),
    }
}

/// 拉起管理器进程（与运行时同目录的可执行文件）。
pub fn launch_manager() {
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
        debug_log(&format!("launch_manager: 未找到 {}", path.display()));
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // 完全脱离父进程：不共享控制台、独立进程组，避免拖住父进程管道。
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        let result = std::process::Command::new(&path)
            .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        match result {
            Ok(_) => debug_log("launch_manager: 已拉起管理器"),
            Err(e) => debug_log(&format!("launch_manager: 拉起失败 {e}")),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

/// NEUROLINGS_DEBUG 开启时把诊断信息追加写入 exe 同目录日志。
pub fn debug_log(message: &str) {
    if std::env::var_os("NEUROLINGS_DEBUG").is_none() {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let log = exe.with_file_name("neurolings_runtime_debug.log");
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)
        {
            let _ = writeln!(f, "{message}");
        }
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

fn list_loaded_mascots(view: &RuntimeView) -> Value {
    let templates: Vec<Value> = view
        .templates
        .names_sorted()
        .into_iter()
        .enumerate()
        .map(|(id, name)| {
            let meta = view.templates.metadata(&name).cloned().unwrap_or_default();
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
    let anchor = req.get("anchor").and_then(|a| {
        let x = a.get("x")?.as_f64()?;
        let y = a.get("y")?.as_f64()?;
        Some(Vec2::new(x, y))
    });

    let template_names = view.templates.names_sorted();
    let spawn_name = match (&name, data_id) {
        (Some(name), _) => {
            if !template_names.iter().any(|n| n == name) {
                return error_json("Invalid mascot name or data ID", "invalid_mascot", 400);
            }
            name.clone()
        }
        (None, Some(id)) => match template_names.get(id as usize) {
            Some(n) => n.clone(),
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
    if let Some(anchor) = patch.get("anchor") {
        if let (Some(x), Some(y)) = (
            anchor.get("x").and_then(Value::as_f64),
            anchor.get("y").and_then(Value::as_f64),
        ) {
            session.manager.state.borrow_mut().anchor = Vec2::new(x, y);
        } else {
            return error_json("anchor must contain numeric x and y", "bad_request", 400);
        }
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
    json!({})
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

fn import_mascot_template(view: &mut RuntimeView, request: &Value) -> Value {
    let Some(path) = request.get("archive_path").and_then(Value::as_str) else {
        return error_json("archive_path must be a string", "bad_request", 400);
    };
    if path.is_empty() {
        return error_json("Archive path is required", "invalid_archive", 400);
    }
    let archive = std::path::Path::new(path);
    if !archive.is_file() {
        return error_json("Mascot archive does not exist", "invalid_arguments", 400);
    }

    let Some(storage) = neurolings_pack::storage::default_storage_path() else {
        return error_json(
            "Could not determine mascot storage directory",
            "storage_unavailable",
            500,
        );
    };
    let cache = storage
        .parent()
        .map(|p| p.join("mascot-cache"))
        .unwrap_or_else(|| storage.join("mascot-cache"));
    let _ = std::fs::create_dir_all(&cache);

    let changed = match neurolings_pack::import_archive(archive, &storage) {
        Ok(changed) if !changed.is_empty() => changed,
        _ => {
            return error_json(
                "Could not import any mascots from the specified archive",
                "import_failed",
                400,
            );
        }
    };

    // 重新加载变更的模板：注销旧会话与注册项，从存储重新读取。
    let mut loaded = Vec::new();
    let mut any_failed = false;
    for name in &changed {
        view.sessions.retain(|s| &s.name != name);
        view.labels
            .retain(|_, id| view.sessions.iter().any(|s| s.id == *id));
        view.templates.deregister(name);
        let _ = view.factory.deregister_template(name);
    }
    let reloaded = crate::templates::load_from_storage(&storage, &cache);
    for template in reloaded {
        if !changed.iter().any(|n| n == &template.name) {
            continue;
        }
        view.templates.register(&template);
        match view.factory.register_template(template.engine_template()) {
            Ok(()) => {}
            Err(_) => any_failed = true,
        }
    }

    for name in &changed {
        let Some(meta) = view.templates.metadata(name).cloned() else {
            continue;
        };
        let id = view
            .templates
            .names_sorted()
            .iter()
            .position(|n| n == name)
            .unwrap_or(0);
        loaded.push(json!({
            "id": id,
            "name": name,
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
    let _ = any_failed;
    json!({ "loaded_mascots": loaded })
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
    if name == "Default" || name == "Default Mascot" {
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
    // 存储目录外的路径拒绝删除。
    let storage = neurolings_pack::storage::default_storage_path().unwrap_or_default();
    if let (Ok(storage_c), Ok(target_c)) = (storage.canonicalize(), pack_dir.canonicalize())
        && !target_c.starts_with(&storage_c)
    {
        return error_json(
            "Refusing to delete a mascot outside the storage directory",
            "invalid_template_path",
            400,
        );
    }

    let removed = if pack_dir.is_dir() {
        std::fs::remove_dir_all(&pack_dir)
    } else {
        std::fs::remove_file(&pack_dir)
    };
    if removed.is_err() {
        return error_json("Could not remove mascot template", "remove_failed", 400);
    }
    view.sessions.retain(|s| s.name != name);
    view.labels
        .retain(|_, id| view.sessions.iter().any(|s| s.id == *id));
    view.templates.deregister(name);
    let _ = view.factory.deregister_template(name);
    json!({})
}

// ---- 预览与气泡 ----

fn preview_png(view: &RuntimeView, request: &Value) -> Value {
    let Some(id) = request.get("id").and_then(Value::as_i64) else {
        return error_json("id is required", "bad_request", 400);
    };
    let names = view.templates.names_sorted();
    let Some(name) = names.get(id as usize) else {
        return error_json("No such loaded mascot", "loaded_mascot_not_found", 404);
    };
    let Some(img_dir) = view.templates.pack_dir(name).map(|p| p.join("img")) else {
        return error_json("No such loaded mascot", "loaded_mascot_not_found", 404);
    };
    let candidate = img_dir.join("shime1.png");
    let bytes = if candidate.is_file() {
        std::fs::read(candidate).ok()
    } else {
        std::fs::read_dir(&img_dir).ok().and_then(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
                .and_then(|p| std::fs::read(p).ok())
        })
    };
    match bytes {
        Some(bytes) => json!({ "preview_base64": crate::http::base64_encode(&bytes) }),
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

    // 选择陪伴模板：设置 → 别名回退 → Default → 兜底。
    let mut selected = view
        .settings
        .get_string(crate::settings::KEY_CODEX_TEMPLATE, "@");
    if selected.trim().is_empty() {
        selected = "@".to_string();
    }
    let names = view.templates.names_sorted();
    if !names.iter().any(|n| n == &selected) {
        selected = "Default".to_string();
    }
    if !names.iter().any(|n| n == &selected) {
        return error_json(
            "No mascot is available for the Codex notification",
            "codex_notification_unavailable",
            503,
        );
    }

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
    let title = if activity.is_new_session {
        "Codex · New session".to_string()
    } else {
        "Codex · Completed".to_string()
    };
    let duration = crate::runtime::bubbles::codex_display_duration(retained);

    if let Some(session) = view.sessions.iter_mut().find(|s| s.id == target_id) {
        session.pending_codex_bubble = Some((title, excerpt, duration));
    }
    json!({
        "handled": true,
        "event_type": activity.event_type,
        "state": activity.state.name(),
    })
}

// ---- 组合 / 自启 / Codex / 窗口模式 / 设置 ----

fn save_combination(view: &mut RuntimeView, request: &Value) -> Value {
    let Some(name) = request.get("name").and_then(Value::as_str) else {
        return error_json("name is required", "bad_request", 400);
    };
    if name.trim().is_empty() {
        return error_json("name is required", "bad_request", 400);
    }
    let templates: Vec<String> = view.sessions.iter().map(|s| s.name.clone()).collect();
    if templates.is_empty() {
        return error_json("No running mascots to save", "bad_request", 400);
    }
    match view.combinations.save_combination(name, templates.clone()) {
        Ok(()) => json!({ "saved": true, "name": name, "count": templates.len() }),
        Err(e) => error_json(&e, "save_failed", 500),
    }
}

fn restore_combination(view: &mut RuntimeView, request: &Value) -> Value {
    let Some(name) = request.get("name").and_then(Value::as_str) else {
        return error_json("name is required", "bad_request", 400);
    };
    let Some(combo) = view.combinations.get(name) else {
        return error_json("No such combination", "combination_not_found", 404);
    };
    // 恢复 = 清场重建（对齐原版 50/200 安全限位与 missing/failed 去重）。
    view.sessions.clear();
    view.labels.clear();

    // 聚合计数，复刻原版按模板分组后逐项 clamp 50、上限 200。
    let mut counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for m in &combo.members {
        *counts.entry(m.template.clone()).or_insert(0) += 1;
    }
    const K_MAX_PER_ENTRY: u64 = 50;
    const K_MAX_PER_COMBINATION: u64 = 200;
    let mut attempted: u64 = 0;
    let mut spawned: u64 = 0;
    let mut missing: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    let mut last_error: Option<Value> = None;

    // 为保证可复现，按模板名排序后依次恢复
    let mut sorted: Vec<(String, u64)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (template, mut count) in sorted {
        if !view.templates.contains(&template) {
            missing.push(template.clone());
            continue;
        }
        if count > K_MAX_PER_ENTRY {
            count = K_MAX_PER_ENTRY;
        }
        for _ in 0..count {
            if attempted >= K_MAX_PER_COMBINATION {
                break;
            }
            attempted += 1;
            let req = json!({ "command": "spawn_mascot", "request": { "name": template } });
            let result = spawn_mascot(view, &req);
            if result.get("error").is_some() {
                failed.push(template.clone());
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

    let mut out = json!({ "restored": true, "name": name, "spawned": spawned, "attempted": attempted });
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
    let names = view.combinations.list();
    json!({ "combinations": names })
}

fn get_combination(view: &RuntimeView, request: &Value) -> Value {
    let Some(name) = request.get("name").and_then(Value::as_str) else {
        return error_json("name is required", "bad_request", 400);
    };
    if name.trim().is_empty() {
        return error_json("name is required", "bad_request", 400);
    }
    let Some(combo) = view.combinations.get(name) else {
        return error_json("No such combination", "combination_not_found", 404);
    };
    // 聚合为与原版兼容的 mascots 数组：[{name, count}]
    let mut counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for m in &combo.members {
        *counts.entry(m.template.clone()).or_insert(0) += 1;
    }
    let mut mascots: Vec<Value> = counts
        .iter()
        .map(|(k, v)| json!({ "name": k, "count": *v }))
        .collect();
    mascots.sort_by(|a, b| {
        a.get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .cmp(b.get("name").and_then(Value::as_str).unwrap_or(""))
    });
    let members: Vec<Value> = combo.members.iter().map(|m| json!({ "template": m.template })).collect();
    let total: u64 = counts.values().sum();
    json!({
        "name": name,
        "members": members,
        "mascots": mascots,
        "aggregated": mascots,
        "count": combo.members.len(),
        "total": total,
    })
}

fn delete_combination(view: &mut RuntimeView, request: &Value) -> Value {
    let Some(name) = request.get("name").and_then(Value::as_str) else {
        return error_json("name is required", "bad_request", 400);
    };
    match view.combinations.delete_combination(name) {
        Ok(()) => json!({ "deleted": true, "name": name }),
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
fn store_index_command(view: &mut RuntimeView, request: &Value) -> Value {
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
    let response = fetch_index(&url, &etag, &lm, 15_000);
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
    debug_log(&format!(
        "store_index: fetched {} entries from {}",
        index.entries.len(),
        url
    ));
    let _ = view; // 索引命令不改动运行时会话状态。
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

/// 商店安装：按 id 找条目 → 受信 URL 校验 → SHA-256 下载 → 复用
/// import_mascot_template 导入 → 返回导入结果。
fn store_install_command(view: &mut RuntimeView, request: &Value) -> Value {
    use neurolings_store::{StoreCache, download};

    let Some(id) = request.get("id").and_then(Value::as_str) else {
        return error_json("id is required", "bad_request", 400);
    };
    let Some(dir) = store_cache_dir() else {
        return error_json("Storage unavailable", "storage_unavailable", 500);
    };
    let cache = StoreCache::new(&dir);
    let Some(cached) = cache.load_index().or_else(|| cache.load_previous_index()) else {
        return error_json(
            "Store index not fetched yet; refresh first",
            "store_empty",
            409,
        );
    };
    let index = match neurolings_store::StoreIndex::parse(&cached.body) {
        Ok(v) => v,
        Err(e) => return error_json(&format!("Invalid cached index: {e}"), "invalid_index", 500),
    };
    let Some(entry) = index.entries.iter().find(|e| e.id == id) else {
        return error_json("No such store entry", "entry_not_found", 404);
    };
    if entry.download.url.is_empty() {
        return error_json("Entry has no download URL", "invalid_entry", 400);
    }
    if !neurolings_store::index::is_trusted_download_url(&entry.download.url, &index.registry) {
        return error_json(
            "Download URL is not from a trusted host",
            "untrusted_url",
            400,
        );
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
        match download(&entry.download.url, &destination, &entry.download.sha256, 60_000) {
            Ok(()) => {
                ok = true;
                break;
            }
            Err(e) => {
                last_err = e.clone();
                let is_retryable = e.contains("network") || e.contains("timeout") || e.contains("failed");
                if !is_retryable || attempt == 2 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1500));
            }
        }
    }
    if !ok {
        return error_json(&last_err, "download_failed", 502);
    }

    let mut import_request = json!({ "archive_path": destination.to_string_lossy() });
    if let Some(label) = request.get("label") {
        import_request["label"] = label.clone();
    }
    let mut result = import_mascot_template(view, &import_request);
    result["store_entry"] = json!({ "id": entry.id, "name": entry.name, "version": entry.version });
    result
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
    if let Some(key) = request.get("key").and_then(Value::as_str)
        && let Some(value) = view.settings.get(key)
    {
        return json!({ "key": key, "value": value });
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
    ] {
        if let Some(value) = view.settings.get(key) {
            out.insert(key.to_string(), value.clone());
        }
    }
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
        Ok(()) => json!({ "key": key, "value": value }),
        Err(e) => error_json(&e, "settings_failed", 500),
    }
}
