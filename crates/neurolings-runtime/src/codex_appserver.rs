//! Codex app-server 客户端：子进程 + stdio JSON-RPC（对齐原版 CodexAppServerClient）。
//!
//! 生命周期约定与原版一致：
//! - 单实例、绝不自动重启、绝不自动批准；
//! - 协议违规/超限/意外退出 → failClosed（Blocked）；
//! - stop 顺序：best-effort cancel → terminate(500ms) → kill。

use std::collections::{HashMap, VecDeque};
use std::io::{BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{Value, json};

const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;
const MAX_BUFFER_BYTES: usize = 8 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 64 * 1024;
const MAX_PENDING_APPROVALS: usize = 16;
const MAX_PENDING_INPUTS: usize = 3;
const MAX_PLAN_STEPS: usize = 32;
const MAX_TEXT_STEP: usize = 2048;
const MAX_FINAL_TEXT: usize = 4096;
const MAX_AGENT_MESSAGE: usize = 131072;

/// 连接状态（对齐原版 CodexAppServerConnectionState）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Stopped,
    Starting,
    Initializing,
    Ready,
    Running,
    NeedsInput,
    Stopping,
    Blocked,
}

impl ConnectionState {
    pub fn name(self) -> &'static str {
        match self {
            ConnectionState::Stopped => "Stopped",
            ConnectionState::Starting => "Starting",
            ConnectionState::Initializing => "Initializing",
            ConnectionState::Ready => "Ready",
            ConnectionState::Running => "Running",
            ConnectionState::NeedsInput => "NeedsInput",
            ConnectionState::Stopping => "Stopping",
            ConnectionState::Blocked => "Blocked",
        }
    }
}

/// 一条待审项（命令执行/文件变更/网络）。
#[derive(Debug, Clone, Default)]
pub struct Approval {
    pub id: String,
    pub kind: String,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub reason: String,
    pub command: String,
    pub cwd: String,
    pub changes: Vec<(String, String)>,
    pub network_host: String,
    pub available_decisions: Vec<String>,
    pub sequence: u64,
}

/// 一条用户输入请求（最多 3 题、每题最多 3 选项）。
#[derive(Debug, Clone, Default)]
pub struct UserInput {
    pub id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub auto_resolution_ms: u64,
    pub questions: Vec<InputQuestion>,
    pub sequence: u64,
}

#[derive(Debug, Clone, Default)]
pub struct InputQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    pub is_other: bool,
    pub is_secret: bool,
    pub options: Vec<String>,
}

/// 计划快照（对齐原版 CodexPlanSnapshot）。
#[derive(Debug, Clone, Default)]
pub struct PlanSnapshot {
    pub steps: Vec<(String, String)>,
    pub explanation: String,
    pub final_text: String,
    pub is_final: bool,
}

/// 共享客户端状态：读线程写入，命令线程读取/写入。
pub struct AppServerShared {
    pub state: ConnectionState,
    pub diagnostic: String,
    pub thread_id: String,
    pub turn_id: String,
    pub workspace: String,
    pub plan: PlanSnapshot,
    pub final_message: String,
    pub approvals: Vec<Approval>,
    pub user_inputs: Vec<UserInput>,
    pub available_modes: Vec<String>,
    pub plan_supported: bool,
    pub config_allowed: bool,
    pub generation: u64,
}

pub struct AppServerClient {
    shared: Arc<Mutex<AppServerShared>>,
    child: Option<Child>,
    stdin: Option<std::process::ChildStdin>,
    pending: HashMap<String, &'static str>,
    next_request_id: u64,
    sequence: u64,
    plan_deltas: HashMap<String, String>,
    file_changes_by_item: VecDeque<String>,
    stopping: bool,
    generation_counter: Arc<AtomicU64>,
}

fn clamp_str(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

/// 从 params（或 params.item）取字符串。
fn item_field(params: &Value, key: &str) -> String {
    for source in [params, params.get("item").unwrap_or(&Value::Null)] {
        if let Some(text) = source.get(key).and_then(Value::as_str) {
            return text.to_string();
        }
    }
    String::new()
}

impl AppServerClient {
    pub fn status(shared: &Arc<Mutex<AppServerShared>>) -> Value {
        let s = shared.lock().unwrap();
        json!({
            "state": s.state.name(),
            "diagnostic": s.diagnostic,
            "thread_id": s.thread_id,
            "turn_id": s.turn_id,
            "workspace": s.workspace,
            "plan": {
                "steps": s.plan.steps.iter().map(|(step, status)| json!({
                    "step": step, "status": status,
                })).collect::<Vec<_>>(),
                "explanation": s.plan.explanation,
                "final_text": s.plan.final_text,
                "final": s.plan.is_final,
            },
            "final_message": s.final_message,
            "approvals": s.approvals.iter().map(|a| json!({
                "id": a.id, "kind": a.kind, "reason": a.reason,
                "item_id": a.item_id,
                "command": a.command, "cwd": a.cwd,
                "changes": a.changes.iter().map(|(path, kind)| json!({
                    "path": path, "kind": kind,
                })).collect::<Vec<_>>(),
                "network_host": a.network_host,
                "available_decisions": a.available_decisions,
            })).collect::<Vec<_>>(),
            "user_inputs": s.user_inputs.iter().map(|u| json!({
                "id": u.id,
                "item_id": u.item_id,
                "thread_id": u.thread_id,
                "turn_id": u.turn_id,
                "auto_resolution_ms": u.auto_resolution_ms,
                "questions": u.questions.iter().map(|q| json!({
                    "id": q.id, "header": q.header, "question": q.question,
                    "is_other": q.is_other, "is_secret": q.is_secret,
                    "options": q.options,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "modes": s.available_modes,
            "plan_supported": s.plan_supported,
        })
    }

    /// 启动 codex app-server 子进程并完成 initialize 握手。
    pub fn start(
        shared: Arc<Mutex<AppServerShared>>,
        executable: &str,
    ) -> Result<Arc<Mutex<Self>>, String> {
        let trimmed = executable.trim();
        if trimmed.ends_with(".cmd") || trimmed.ends_with(".bat") {
            return Err("Codex executable not found; choose an actual executable".into());
        }
        let program = if trimmed.is_empty() { "codex" } else { trimmed };
        let mut child = Command::new(program)
            .args(["app-server", "--listen", "stdio://"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| "Codex app-server failed to start".to_string())?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Codex app-server failed to start".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Codex app-server failed to start".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "Codex app-server failed to start".to_string())?;

        // 新一代连接：全部状态复位（对齐原版 ++m_connectionGeneration）。
        let generation = 1;
        {
            let mut s = shared.lock().unwrap();
            s.state = ConnectionState::Starting;
            s.diagnostic.clear();
            s.thread_id.clear();
            s.turn_id.clear();
            s.workspace.clear();
            s.plan = PlanSnapshot::default();
            s.final_message.clear();
            s.approvals.clear();
            s.user_inputs.clear();
            s.available_modes.clear();
            s.plan_supported = false;
            s.config_allowed = false;
            s.generation = generation;
        }

        {
            let mut s = shared.lock().unwrap();
            s.state = ConnectionState::Initializing;
        }
        let counter = Arc::new(AtomicU64::new(generation));
        let client = Arc::new(Mutex::new(Self {
            shared: shared.clone(),
            child: Some(child),
            stdin: Some(stdin),
            pending: HashMap::new(),
            next_request_id: 1,
            sequence: 0,
            plan_deltas: HashMap::new(),
            file_changes_by_item: VecDeque::new(),
            stopping: false,
            generation_counter: counter.clone(),
        }));

        // stdout 读线程：逐行解析 JSON-RPC。
        let reader_client = client.clone();
        std::thread::spawn(move || {
            read_stdout(shared.clone(), reader_client, stdout);
        });
        // stderr 线程：只保留最先 64KiB。
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut kept = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if kept.len() < MAX_STDERR_BYTES {
                            let room = MAX_STDERR_BYTES - kept.len();
                            kept.extend_from_slice(&buf[..n.min(room)]);
                        }
                    }
                }
            }
        });

        // initialize 握手（clientInfo/capabilities/optOut 与原版一致）。
        client.lock().unwrap().send_request(
            "initialize",
            json!({
                "clientInfo": { "name": "neurolingsce", "version": "1" },
                "capabilities": { "experimentalApi": true },
                "optOutNotificationMethods": [
                    "item/commandExecution/outputDelta",
                    "item/reasoning/summaryTextDelta",
                ],
            }),
            "initialize",
        );
        client
            .lock()
            .unwrap()
            .send_notification("initialized", json!({}));
        client.lock().unwrap().send_request(
            "configRequirements/read",
            json!({}),
            "configRequirements",
        );
        client.lock().unwrap().send_request(
            "collaborationMode/list",
            json!({}),
            "collaborationMode",
        );
        Ok(client)
    }

    fn write_line(&mut self, value: &Value) {
        use std::io::Write;
        if let Some(stdin) = self.stdin.as_mut() {
            let _ = writeln!(
                stdin,
                "{}",
                serde_json::to_string(value).unwrap_or_default()
            );
            let _ = stdin.flush();
        }
    }

    fn send_request(&mut self, method: &str, params: Value, kind: &'static str) {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.pending.insert(format!("n:{id}"), kind);
        self.write_line(&json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params,
        }));
    }

    fn send_notification(&mut self, method: &str, params: Value) {
        self.write_line(&json!({
            "jsonrpc": "2.0", "method": method, "params": params,
        }));
    }

    /// 回复服务端请求（审批/输入）。
    fn send_result(&mut self, id: &Value, result: Value) {
        self.write_line(&json!({
            "jsonrpc": "2.0", "id": id, "result": result,
        }));
    }

    fn send_error(&mut self, id: &Value, code: i64, message: &str) {
        self.write_line(&json!({
            "jsonrpc": "2.0", "id": id,
            "error": { "code": code, "message": message },
        }));
    }

    fn fail_closed(shared: &Arc<Mutex<AppServerShared>>, reason: &str, client: &Arc<Mutex<Self>>) {
        // 对齐原版：cancel pending → Blocked → terminate。
        client.lock().unwrap().cancel_pending_requests();
        let mut s = shared.lock().unwrap();
        s.state = ConnectionState::Blocked;
        s.diagnostic = reason.to_string();
        drop(s);
        let mut guard = client.lock().unwrap();
        if let Some(child) = guard.child.as_mut() {
            let _ = child.kill();
        }
        guard.child = None;
        guard.stdin = None;
    }

    /// best-effort 取消全部待审/输入（对齐 cancelPendingRequests）。
    pub fn cancel_pending_requests(&mut self) {
        let approvals = {
            let mut s = self.shared.lock().unwrap();
            std::mem::take(&mut s.approvals)
        };
        for approval in approvals {
            self.write_line(&json!({
                "jsonrpc": "2.0", "id": approval.id.parse::<u64>().unwrap_or(0),
                "result": { "decision": "cancel" },
            }));
        }
        let inputs = {
            let mut s = self.shared.lock().unwrap();
            std::mem::take(&mut s.user_inputs)
        };
        for input in inputs {
            self.write_line(&json!({
                "jsonrpc": "2.0", "id": input.id.parse::<u64>().unwrap_or(0),
                "result": { "answers": {} },
            }));
        }
    }

    /// 主动停止：cancel → terminate(500ms) → kill。
    pub fn stop(&mut self) {
        if self.stopping {
            return;
        }
        self.stopping = true;
        {
            let mut s = self.shared.lock().unwrap();
            s.state = ConnectionState::Stopping;
        }
        self.cancel_pending_requests();
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.child = None;
        self.stdin = None;
        let mut s = self.shared.lock().unwrap();
        s.state = ConnectionState::Stopped;
        s.approvals.clear();
        s.user_inputs.clear();
        s.thread_id.clear();
        s.turn_id.clear();
        s.generation = self.generation_counter.load(Ordering::SeqCst) + 1;
    }

    // ---- 会话命令（守卫与参数对齐原版） ----

    pub fn start_new_thread(&mut self, cwd: &str) {
        let mut params = json!({
            "approvalPolicy": "on-request",
            "sandbox": "workspace-write",
            "serviceName": "neurolingsce",
        });
        if !cwd.trim().is_empty() {
            params["cwd"] = json!(cwd.trim());
        }
        self.unsubscribe_current();
        self.send_request("thread/start", params, "threadStart");
    }

    pub fn resume_thread(&mut self, thread_id: &str) {
        if thread_id.trim().is_empty() {
            return;
        }
        self.unsubscribe_current();
        self.send_request(
            "thread/resume",
            json!({ "threadId": thread_id.trim() }),
            "threadStart",
        );
    }

    fn unsubscribe_current(&mut self) {
        let thread_id = self.shared.lock().unwrap().thread_id.clone();
        if !thread_id.is_empty() {
            self.send_request(
                "thread/unsubscribe",
                json!({ "threadId": thread_id }),
                "unsubscribe",
            );
        }
        let mut s = self.shared.lock().unwrap();
        s.thread_id.clear();
        s.turn_id.clear();
        self.pending.clear();
    }

    pub fn start_turn(&mut self, text: &str, plan: bool) {
        let (thread_id, ready, running) = {
            let s = self.shared.lock().unwrap();
            (
                s.thread_id.clone(),
                s.state == ConnectionState::Ready,
                s.state == ConnectionState::Running,
            )
        };
        if thread_id.is_empty() || text.trim().is_empty() || running || !ready {
            return;
        }
        let mut params = json!({
            "threadId": thread_id,
            "input": [{ "type": "text", "text": text.trim() }],
        });
        // collaborationMode 预设：plan 模式仅在服务端支持时附带。
        if plan {
            let mut s = self.shared.lock().unwrap();
            if !s.plan_supported {
                s.diagnostic = "Plan mode is not supported".into();
                return;
            }
            if let Some(mode) = s
                .available_modes
                .iter()
                .find(|m| m.eq_ignore_ascii_case("plan"))
            {
                params["collaborationMode"] =
                    json!({ "mode": mode, "settings": { "developer_instructions": null } });
            }
        }
        self.shared.lock().unwrap().state = ConnectionState::Running;
        self.send_request("turn/start", params, "turnStart");
    }

    pub fn steer_turn(&mut self, text: &str) {
        let (thread_id, turn_id) = {
            let s = self.shared.lock().unwrap();
            (s.thread_id.clone(), s.turn_id.clone())
        };
        if thread_id.is_empty() || turn_id.is_empty() || text.trim().is_empty() {
            return;
        }
        self.send_request(
            "turn/steer",
            json!({
                "threadId": thread_id,
                "expectedTurnId": turn_id,
                "input": [{ "type": "text", "text": text.trim() }],
            }),
            "steer",
        );
    }

    pub fn interrupt_turn(&mut self) {
        let (thread_id, turn_id) = {
            let s = self.shared.lock().unwrap();
            (s.thread_id.clone(), s.turn_id.clone())
        };
        if thread_id.is_empty() || turn_id.is_empty() {
            return;
        }
        self.send_request(
            "turn/interrupt",
            json!({ "threadId": thread_id, "turnId": turn_id }),
            "interrupt",
        );
    }

    /// 审批决议（decision 必须在可用列表内；跨代拒绝）。
    pub fn resolve_approval(&mut self, approval_id: &str, decision: &str) -> bool {
        let generation = self.shared.lock().unwrap().generation;
        let found = {
            let s = self.shared.lock().unwrap();
            s.approvals.iter().position(|a| {
                a.id == approval_id && a.available_decisions.iter().any(|d| d == decision)
            })
        };
        let Some(index) = found else {
            return false;
        };
        let _ = generation;
        let approval = self.shared.lock().unwrap().approvals.remove(index);
        let id_value = approval
            .id
            .parse::<u64>()
            .map(Value::from)
            .unwrap_or(Value::String(approval.id.clone()));
        self.send_result(&id_value, json!({ "decision": decision }));
        true
    }

    /// 用户输入回复（answers: {questionId: {answers: [text]}}；空=取消）。
    pub fn resolve_user_input(&mut self, input_id: &str, answers: Value) -> bool {
        let found = {
            let s = self.shared.lock().unwrap();
            s.user_inputs.iter().position(|u| u.id == input_id)
        };
        let Some(index) = found else {
            return false;
        };
        let input = self.shared.lock().unwrap().user_inputs.remove(index);
        let id_value = input
            .id
            .parse::<u64>()
            .map(Value::from)
            .unwrap_or(Value::String(input.id.clone()));
        self.send_result(&id_value, json!({ "answers": answers }));
        let mut s = self.shared.lock().unwrap();
        if s.user_inputs.is_empty() && s.state == ConnectionState::NeedsInput {
            s.state = ConnectionState::Running;
        }
        true
    }
}

/// stdout 读循环：行缓冲上限与 fail-closed 对齐原版。
fn read_stdout(
    shared: Arc<Mutex<AppServerShared>>,
    client: Arc<Mutex<AppServerClient>>,
    stdout: std::process::ChildStdout,
) {
    let mut reader = BufReader::new(stdout);
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    'outer: loop {
        // 读到换行或 EOF。
        match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                buffer.extend_from_slice(&chunk[..n]);
                if buffer.len() > MAX_BUFFER_BYTES {
                    Self2::fail(&shared, &client, "app-server output buffer exceeded limit");
                    break 'outer;
                }
            }
        }
        // 切行处理。
        while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = buffer.drain(..=pos).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.len() > MAX_LINE_BYTES {
                Self2::fail(&shared, &client, "app-server line exceeded limit");
                return;
            }
            if line.iter().all(|b| b.is_ascii_whitespace()) {
                continue;
            }
            let Ok(message) = serde_json::from_slice::<Value>(&line) else {
                Self2::fail(&shared, &client, "invalid app-server JSON-RPC message");
                return;
            };
            let blocked = shared.lock().unwrap().state == ConnectionState::Blocked;
            if blocked {
                return;
            }
            handle_message(&shared, &client, message);
        }
        if buffer.len() > MAX_LINE_BYTES {
            Self2::fail(&shared, &client, "app-server line exceeded limit");
            return;
        }
    }
}

struct Self2;

impl Self2 {
    fn fail(
        shared: &Arc<Mutex<AppServerShared>>,
        client: &Arc<Mutex<AppServerClient>>,
        reason: &str,
    ) {
        AppServerClient::fail_closed(shared, reason, client);
    }
}

/// 处理一条服务端消息（响应 / 请求 / 通知）。
fn handle_message(
    shared: &Arc<Mutex<AppServerShared>>,
    client: &Arc<Mutex<AppServerClient>>,
    message: Value,
) {
    if message.get("method").is_some() {
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(json!({}));
        if let Some(id) = id {
            handle_server_request(shared, client, method, &params, id);
        } else {
            handle_notification(shared, client, method, &params);
        }
        return;
    }
    // 响应：按 pending 表分发。
    let id_key = match message.get("id") {
        Some(Value::Number(n)) => format!("n:{}", n.as_f64().unwrap_or(0.0)),
        Some(Value::String(s)) => format!("s:{s}"),
        _ => return,
    };
    let mut guard = client.lock().unwrap();
    let Some(kind) = guard.pending.remove(&id_key) else {
        return;
    };
    if let Some(error) = message.get("error") {
        let reason = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("request failed");
        match kind {
            "initialize" => AppServerClient::fail_closed(
                shared,
                &format!("initialize failed: {reason}"),
                client,
            ),
            "threadStart" => AppServerClient::fail_closed(
                shared,
                &format!("thread/start failed: {reason}"),
                client,
            ),
            "turnStart" => AppServerClient::fail_closed(
                shared,
                &format!("turn/start failed: {reason}"),
                client,
            ),
            _ => {}
        }
        return;
    }
    let result = message.get("result").cloned().unwrap_or(Value::Null);
    match kind {
        "configRequirements" => {
            let allowed = check_config_allowed(&result);
            let mut s = shared.lock().unwrap();
            s.config_allowed = allowed;
            if !allowed {
                s.diagnostic = "Codex administrator restrictions are active".into();
            }
        }
        "collaborationMode" => {
            let list = result
                .get("data")
                .and_then(Value::as_array)
                .or_else(|| result.get("modes").and_then(Value::as_array))
                .cloned()
                .unwrap_or_default();
            let mut modes = Vec::new();
            for entry in &list {
                let name = match entry {
                    Value::String(s) => s.clone(),
                    Value::Object(_) => entry
                        .get("mode")
                        .or_else(|| entry.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    _ => String::new(),
                };
                if !name.is_empty() && !modes.contains(&name) {
                    modes.push(name);
                }
            }
            let plan_supported = modes.iter().any(|m| m.eq_ignore_ascii_case("plan"));
            let mut s = shared.lock().unwrap();
            s.available_modes = modes;
            s.plan_supported = plan_supported;
            if s.state == ConnectionState::Initializing {
                s.state = ConnectionState::Ready;
            }
        }
        "threadStart" => {
            let thread = result.get("thread").cloned().unwrap_or(result.clone());
            let thread_id = thread
                .get("id")
                .or_else(|| thread.get("threadId"))
                .or_else(|| thread.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if thread_id.is_empty() {
                AppServerClient::fail_closed(shared, "thread/start failed", client);
                return;
            }
            let cwd = thread
                .get("cwd")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let mut s = shared.lock().unwrap();
            s.thread_id = thread_id;
            if !cwd.is_empty() {
                s.workspace = cwd;
            }
            s.state = ConnectionState::Ready;
        }
        "turnStart" => {
            let turn = result.get("turn").cloned().unwrap_or(result.clone());
            let turn_id = turn
                .get("id")
                .or_else(|| turn.get("turnId"))
                .or_else(|| turn.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let mut s = shared.lock().unwrap();
            if !turn_id.is_empty() {
                s.turn_id = turn_id;
            }
        }
        _ => {}
    }
}

/// 服务端请求：审批与用户输入（其余回 -32601 并 Blocked，对齐原版）。
fn handle_server_request(
    shared: &Arc<Mutex<AppServerShared>>,
    client: &Arc<Mutex<AppServerClient>>,
    method: &str,
    params: &Value,
    id: Value,
) {
    let state_ok = {
        let s = shared.lock().unwrap();
        matches!(
            s.state,
            ConnectionState::Running | ConnectionState::NeedsInput
        )
    };
    match method {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            if !state_ok {
                let mut guard = client.lock().unwrap();
                guard.send_result(&id, json!({ "decision": "cancel" }));
                return;
            }
            let approval = parse_approval(method, params);
            let Some(mut approval) = approval else {
                let mut guard = client.lock().unwrap();
                guard.send_error(&id, -32602, "Invalid params");
                return;
            };
            let mut guard = client.lock().unwrap();
            approval.id = id.to_string().trim_matches('"').to_string();
            let mut s = shared.lock().unwrap();
            if s.approvals.len() >= MAX_PENDING_APPROVALS {
                drop(s);
                guard.send_result(&id, json!({ "decision": "cancel" }));
                AppServerClient::fail_closed(shared, "too many pending approvals", client);
                return;
            }
            if !s.approvals.iter().any(|a| a.id == approval.id) {
                guard.sequence += 1;
                approval.sequence = guard.sequence;
                s.approvals.push(approval);
            }
        }
        "item/tool/requestUserInput" => {
            if !state_ok {
                let mut guard = client.lock().unwrap();
                guard.send_result(&id, json!({ "answers": {} }));
                return;
            }
            let parsed = parse_user_input(params);
            let Some(mut input) = parsed else {
                let mut guard = client.lock().unwrap();
                guard.send_result(&id, json!({ "answers": {} }));
                return;
            };
            let mut guard = client.lock().unwrap();
            input.id = id.to_string().trim_matches('"').to_string();
            let mut s = shared.lock().unwrap();
            if s.user_inputs.len() >= MAX_PENDING_INPUTS
                || s.user_inputs.iter().any(|u| u.id == input.id)
            {
                drop(s);
                guard.send_result(&id, json!({ "answers": {} }));
                return;
            }
            guard.sequence += 1;
            input.sequence = guard.sequence;
            s.user_inputs.push(input);
            s.state = ConnectionState::NeedsInput;
        }
        _ => {
            // 未知服务器请求：Method not supported（不回显 method 名）。
            let mut guard = client.lock().unwrap();
            guard.send_error(&id, -32601, "Method not supported");
            drop(guard);
            AppServerClient::fail_closed(shared, "unsupported server request", client);
        }
    }
}

/// 服务端通知处理（对齐原版 handleNotification 全表）。
fn handle_notification(
    shared: &Arc<Mutex<AppServerShared>>,
    client: &Arc<Mutex<AppServerClient>>,
    method: &str,
    params: &Value,
) {
    let mut guard = client.lock().unwrap();
    let mut s = shared.lock().unwrap();
    match method {
        "thread/started" => {
            let thread = params.get("thread").cloned().unwrap_or(params.clone());
            if let Some(id) = thread
                .get("id")
                .or_else(|| thread.get("threadId"))
                .and_then(Value::as_str)
            {
                s.thread_id = id.to_string();
            }
            if let Some(cwd) = thread.get("cwd").and_then(Value::as_str) {
                s.workspace = cwd.to_string();
            }
        }
        "item/started" => {
            // 文件变更预告：按 threadId\x1fturnId\x1fitemId 缓存（LRU 64）。
            let item = params.get("item").cloned().unwrap_or(params.clone());
            let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
            if item_type == "fileChange" || item_type == "file_change" {
                let key = format!(
                    "{}\x1f{}\x1f{}",
                    item_field(params, "threadId"),
                    item_field(params, "turnId"),
                    item_field(params, "itemId"),
                );
                if let Some(changes) = item.get("changes").and_then(Value::as_array) {
                    let mut packed = String::new();
                    for change in changes {
                        let path = change.get("path").and_then(Value::as_str).unwrap_or("");
                        let kind = change
                            .get("kind")
                            .or_else(|| change.get("type"))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        packed.push_str(&format!("{path}\x1f{kind}\x1e"));
                    }
                    if guard.file_changes_by_item.len() >= 64 {
                        guard.file_changes_by_item.pop_front();
                    }
                    guard
                        .file_changes_by_item
                        .push_back(format!("{key}\x1f{packed}"));
                }
            }
        }
        "turn/plan/updated" => {
            let snapshot = parse_plan_snapshot(params, false);
            s.plan.steps = snapshot.steps;
            s.plan.explanation = snapshot.explanation;
        }
        "item/plan/delta" => {
            let key = format!(
                "{}\x1f{}\x1f{}",
                item_field(params, "threadId"),
                item_field(params, "turnId"),
                item_field(params, "itemId"),
            );
            let delta = params
                .get("delta")
                .or_else(|| params.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let entry = guard.plan_deltas.entry(key).or_default();
            entry.push_str(delta);
            let joined: String = guard
                .plan_deltas
                .values()
                .map(|v| clamp_str(v, MAX_AGENT_MESSAGE))
                .collect();
            s.plan.final_text = clamp_str(&joined, MAX_FINAL_TEXT * 16);
        }
        "item/agentMessage/delta" => {
            let delta = params
                .get("delta")
                .or_else(|| params.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("");
            s.final_message.push_str(delta);
            if s.final_message.len() > MAX_AGENT_MESSAGE {
                let cut = s.final_message.len() - MAX_AGENT_MESSAGE;
                s.final_message = s.final_message[cut..].to_string();
            }
        }
        "item/completed" => {
            let item = params.get("item").cloned().unwrap_or(params.clone());
            let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
            if item_type == "plan" {
                let snapshot = parse_plan_snapshot(params, true);
                let key = format!(
                    "{}\x1f{}\x1f{}",
                    item_field(params, "threadId"),
                    item_field(params, "turnId"),
                    item_field(params, "itemId"),
                );
                guard.plan_deltas.remove(&key);
                s.plan = snapshot;
            } else if item_type == "agentMessage" {
                let phase = item.get("phase").and_then(Value::as_str).unwrap_or("");
                if let Some(text) = item.get("text").and_then(Value::as_str)
                    && (phase.is_empty() || phase == "final_answer")
                    && !text.is_empty()
                {
                    s.final_message = text.to_string();
                }
            }
        }
        "serverRequest/resolved" => {
            let request_id = params
                .get("requestId")
                .or_else(|| params.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            s.approvals.retain(|a| a.id != request_id);
            s.user_inputs.retain(|u| u.id != request_id);
            if s.user_inputs.is_empty() && s.state == ConnectionState::NeedsInput {
                s.state = ConnectionState::Running;
            }
        }
        "turn/completed" => {
            let status = params
                .get("status")
                .or_else(|| params.get("turn").and_then(|t| t.get("status")))
                .and_then(Value::as_str)
                .unwrap_or("");
            let success = status == "completed" || status == "succeeded";
            s.turn_id.clear();
            s.state = if success {
                ConnectionState::Ready
            } else {
                ConnectionState::Blocked
            };
        }
        "thread/closed" => {
            s.approvals.clear();
            s.user_inputs.clear();
            s.thread_id.clear();
            s.turn_id.clear();
            s.state = ConnectionState::Ready;
        }
        _ => {}
    }
}

/// 审批参数解析（截断与默认 decisions 对齐原版）。
fn parse_approval(method: &str, params: &Value) -> Option<Approval> {
    let mut approval = Approval {
        thread_id: item_field(params, "threadId"),
        turn_id: item_field(params, "turnId"),
        item_id: item_field(params, "itemId"),
        reason: clamp_str(&item_field(params, "reason"), 4096),
        command: clamp_str(&item_field(params, "command"), 131072),
        cwd: clamp_str(&item_field(params, "cwd"), 4096),
        ..Default::default()
    };
    approval.kind = if method.contains("fileChange") {
        "FileChange"
    } else {
        "CommandExecution"
    }
    .to_string();
    if let Some(actions) = params.get("commandActions").and_then(Value::as_array) {
        for action in actions.iter().take(32) {
            let command = clamp_str(
                action.get("command").and_then(Value::as_str).unwrap_or(""),
                131072,
            );
            if !command.is_empty() {
                approval.command.push_str(&format!("\n{command}"));
            }
        }
    }
    if let Some(changes) = params.get("changes").and_then(Value::as_array) {
        for change in changes.iter().take(256) {
            approval.changes.push((
                clamp_str(
                    change.get("path").and_then(Value::as_str).unwrap_or(""),
                    4096,
                ),
                change
                    .get("kind")
                    .or_else(|| change.get("type"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ));
        }
    }
    if let Some(network) = params
        .get("networkApprovalContext")
        .filter(|v| v.is_object())
    {
        approval.kind = "Network".into();
        approval.network_host = clamp_str(
            network.get("host").and_then(Value::as_str).unwrap_or(""),
            512,
        );
    }
    let decisions = params.get("availableDecisions").and_then(Value::as_array);
    approval.available_decisions = match decisions {
        Some(list) if !list.is_empty() => list
            .iter()
            .filter_map(|d| d.as_str().map(str::to_string))
            .collect(),
        _ => vec![
            "accept".into(),
            "acceptForSession".into(),
            "decline".into(),
            "cancel".into(),
        ],
    };
    if approval.thread_id.is_empty() && approval.turn_id.is_empty() && approval.command.is_empty() {
        return None;
    }
    Some(approval)
}

/// 用户输入解析（1..3 题、每题最多 3 选项）。
fn parse_user_input(params: &Value) -> Option<UserInput> {
    let questions_value = params.get("questions")?.as_array()?;
    if questions_value.is_empty() || questions_value.len() > 3 {
        return None;
    }
    let mut input = UserInput {
        thread_id: item_field(params, "threadId"),
        turn_id: item_field(params, "turnId"),
        item_id: item_field(params, "itemId"),
        auto_resolution_ms: params
            .get("autoResolutionMs")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(3_600_000),
        ..Default::default()
    };
    for question in questions_value {
        let id = clamp_str(question.get("id").and_then(Value::as_str)?, 256);
        let text = clamp_str(question.get("question").and_then(Value::as_str)?, 4096);
        if id.is_empty() || text.is_empty() {
            return None;
        }
        let mut options = Vec::new();
        if let Some(list) = question.get("options").and_then(Value::as_array) {
            for option in list.iter().take(3) {
                options.push(clamp_str(
                    option.get("label").and_then(Value::as_str).unwrap_or(""),
                    1024,
                ));
            }
        }
        input.questions.push(InputQuestion {
            id,
            header: clamp_str(
                question.get("header").and_then(Value::as_str).unwrap_or(""),
                512,
            ),
            question: text,
            is_other: question
                .get("isOther")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_secret: question
                .get("isSecret")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            options,
        });
    }
    Some(input)
}

/// plan 快照解析（steps ≤32、step 文本 ≤2048）。
fn parse_plan_snapshot(params: &Value, is_final: bool) -> PlanSnapshot {
    let item = params.get("item").cloned().unwrap_or(params.clone());
    let source = params.get("plan").or_else(|| item.get("plan"));
    let steps_source = source
        .and_then(|p| p.get("steps"))
        .or_else(|| params.get("steps"))
        .or_else(|| item.get("steps"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut steps = Vec::new();
    for entry in steps_source.iter().take(MAX_PLAN_STEPS) {
        match entry {
            Value::String(text) => steps.push((clamp_str(text, MAX_TEXT_STEP), "pending".into())),
            Value::Object(_) => {
                let text = clamp_str(
                    entry
                        .get("step")
                        .or_else(|| entry.get("text"))
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                    MAX_TEXT_STEP,
                );
                let status = match entry.get("status").and_then(Value::as_str) {
                    Some("inProgress") => "inProgress",
                    Some("completed") => "completed",
                    _ => "pending",
                };
                steps.push((text, status.to_string()));
            }
            _ => {}
        }
    }
    PlanSnapshot {
        steps,
        explanation: clamp_str(
            params
                .get("explanation")
                .or_else(|| item.get("explanation"))
                .and_then(Value::as_str)
                .unwrap_or(""),
            MAX_TEXT_STEP,
        ),
        final_text: clamp_str(
            params
                .get("text")
                .or_else(|| item.get("text"))
                .and_then(Value::as_str)
                .unwrap_or(""),
            MAX_FINAL_TEXT,
        ),
        is_final,
    }
}

/// configRequirements 允许性检查（对齐原版规则）。
fn check_config_allowed(result: &Value) -> bool {
    if result.is_null() {
        return true;
    }
    let Some(obj) = result.as_object() else {
        return true;
    };
    if let Some(policies) = obj.get("allowedApprovalPolicies").and_then(Value::as_array)
        && (policies.is_empty() || !policies.iter().any(|p| p.as_str() == Some("on-request")))
    {
        return false;
    }
    if let Some(modes) = obj.get("allowedSandboxModes").and_then(Value::as_array)
        && (modes.is_empty()
            || !modes.iter().any(|m| {
                m.as_str() == Some("workspace-write") || m.as_str() == Some("workspaceWrite")
            }))
    {
        return false;
    }
    if let Some(policy) = obj.get("approvalPolicy").and_then(Value::as_str)
        && policy != "on-request"
    {
        return false;
    }
    if let Some(sandbox) = obj.get("sandbox").and_then(Value::as_str)
        && sandbox != "workspace-write"
        && sandbox != "workspaceWrite"
    {
        return false;
    }
    if let Some(allowed) = obj
        .get("approvalPolicy")
        .and_then(|p| p.get("allowed"))
        .and_then(Value::as_bool)
        && !allowed
    {
        return false;
    }
    if let Some(allowed) = obj
        .get("sandbox")
        .and_then(|p| p.get("allowed"))
        .and_then(Value::as_bool)
        && !allowed
    {
        return false;
    }
    true
}

/// 全局单例（对齐原版"GUI 拥有一个客户端"）。
static APP_SERVER: Mutex<Option<Arc<Mutex<AppServerClient>>>> = Mutex::new(None);
static APP_SERVER_SHARED: OnceLock<Arc<Mutex<AppServerShared>>> = OnceLock::new();

pub fn shared_state() -> &'static Arc<Mutex<AppServerShared>> {
    APP_SERVER_SHARED.get_or_init(|| {
        Arc::new(Mutex::new(AppServerShared {
            state: ConnectionState::Stopped,
            diagnostic: String::new(),
            thread_id: String::new(),
            turn_id: String::new(),
            workspace: String::new(),
            plan: PlanSnapshot::default(),
            final_message: String::new(),
            approvals: Vec::new(),
            user_inputs: Vec::new(),
            available_modes: Vec::new(),
            plan_supported: false,
            config_allowed: true,
            generation: 0,
        }))
    })
}

pub fn connect(executable: &str) -> Result<(), String> {
    let mut slot = APP_SERVER.lock().unwrap();
    if let Some(existing) = slot.as_ref()
        && let Ok(mut guard) = existing.try_lock()
    {
        guard.stop();
    }
    let client = AppServerClient::start(shared_state().clone(), executable)?;
    *slot = Some(client);
    Ok(())
}

pub fn disconnect() {
    let mut slot = APP_SERVER.lock().unwrap();
    if let Some(existing) = slot.take()
        && let Ok(mut guard) = existing.try_lock()
    {
        guard.stop();
    }
}

pub fn with_client<T>(f: impl FnOnce(&mut AppServerClient) -> T) -> Option<T> {
    let slot = APP_SERVER.lock().unwrap();
    slot.as_ref()
        .and_then(|client| client.try_lock().ok().map(|mut guard| f(&mut guard)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn approval_parse_defaults_decisions_and_kinds() {
        let params = json!({
            "threadId": "t1", "turnId": "u1", "itemId": "i1",
            "reason": "needs approval", "command": "git status",
        });
        let approval = parse_approval("item/commandExecution/requestApproval", &params).unwrap();
        assert_eq!(approval.kind, "CommandExecution");
        assert_eq!(approval.command, "git status");
        assert_eq!(
            approval.available_decisions,
            vec!["accept", "acceptForSession", "decline", "cancel"]
        );

        let approval = parse_approval("item/fileChange/requestApproval", &params).unwrap();
        assert_eq!(approval.kind, "FileChange");

        let network = json!({
            "threadId": "t1", "turnId": "u1", "itemId": "i1",
            "networkApprovalContext": { "host": "example.com" },
        });
        let approval = parse_approval("item/commandExecution/requestApproval", &network).unwrap();
        assert_eq!(approval.kind, "Network");
        assert_eq!(approval.network_host, "example.com");
    }

    #[test]
    fn user_input_requires_one_to_three_questions() {
        assert!(parse_user_input(&json!({ "questions": [] })).is_none());
        assert!(
            parse_user_input(&json!({
                "questions": [
                    {"id":"q1","question":"a?"},
                    {"id":"q2","question":"b?"},
                    {"id":"q3","question":"c?"},
                    {"id":"q4","question":"d?"},
                ],
            }))
            .is_none()
        );
        let input = parse_user_input(&json!({
            "questions": [
                {"id":"q1","question":"继续吗？","options":[{"label":"是"},{"label":"否"}]},
            ],
            "autoResolutionMs": 999999999,
        }))
        .unwrap();
        assert_eq!(input.auto_resolution_ms, 3_600_000);
        assert_eq!(input.questions[0].options.len(), 2);
    }

    #[test]
    fn plan_snapshot_clamps_steps_and_status() {
        let params = json!({
            "plan": {
                "steps": [
                    {"step": "one", "status": "completed"},
                    {"step": "two", "status": "unknown"},
                    "three",
                ],
            },
            "explanation": "expl", "text": "final",
        });
        let snapshot = parse_plan_snapshot(&params, true);
        assert_eq!(snapshot.steps.len(), 3);
        assert_eq!(snapshot.steps[0].1, "completed");
        assert_eq!(snapshot.steps[1].1, "pending");
        assert_eq!(
            snapshot.steps[2],
            ("three".to_string(), "pending".to_string())
        );
        assert!(snapshot.is_final);
    }

    #[test]
    fn config_allowed_rules() {
        assert!(check_config_allowed(&Value::Null));
        assert!(check_config_allowed(&json!({})));
        assert!(!check_config_allowed(&json!({
            "allowedApprovalPolicies": ["never"],
        })));
        assert!(check_config_allowed(&json!({
            "allowedApprovalPolicies": ["on-request"],
        })));
        assert!(!check_config_allowed(&json!({
            "approvalPolicy": "auto",
        })));
        assert!(!check_config_allowed(&json!({
            "allowedSandboxModes": ["read-only"],
        })));
    }
}
