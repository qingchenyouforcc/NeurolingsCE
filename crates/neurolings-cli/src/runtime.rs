//! CLI 的运行时命令执行：经本地 IPC 与运行时对话。
//!
//! 命令流程与原版一致：
//! - 运行时命令在运行时不可用时自动拉起（--stop/--codex-notify 除外）；
//! - summon 两步走：spawn_mascot 后 register_cli_label 分配用户标签；
//! - alter/dismiss 支持数字 id 与 oldest/newest/random 自动 id（配合
//!   selector 过滤）；
//! - --codex-notify 先本地解析通知负载，未识别事件静默放过。

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::commands::{CliExecutionResult, LoadedMascotInfo, MascotInfo};
use crate::parser::{CliCommand, CliCommandKind, CliError, CliGlobalOptions};

// 默认超时与原版一致（0.5s/0.5s）：快速失败，避免脚本长时间挂起。
const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 500;
const DEFAULT_READ_TIMEOUT_MS: u64 = 500;

fn ipc_call_with_timeouts(
    request: &Value,
    connect_timeout: Duration,
    read_timeout: Duration,
) -> Result<Value, CliError> {
    let line = serde_json::to_string(request).map_err(|e| {
        CliError::new(
            "ipc_error",
            &format!("Failed to encode IPC request: {e}"),
            1,
        )
    })?;
    let response =
        crate::ipc_transport_call(&line, connect_timeout, read_timeout).map_err(|e| {
            let mut error = CliError::new(
                "transport_error",
                &format!("Could not reach the NeurolingsCE runtime: {}", e.0),
                1,
            );
            error.details = e.0;
            error
        })?;
    serde_json::from_str(&response).map_err(|e| {
        CliError::new(
            "ipc_error",
            &format!("Failed to parse IPC response: {e}"),
            1,
        )
    })
}

fn connect_timeout(global: &CliGlobalOptions) -> Duration {
    Duration::from_millis(
        global
            .connect_timeout_ms
            .unwrap_or(DEFAULT_CONNECT_TIMEOUT_MS),
    )
}

fn read_timeout(global: &CliGlobalOptions) -> Duration {
    Duration::from_millis(global.read_timeout_ms.unwrap_or(DEFAULT_READ_TIMEOUT_MS))
}

/// 自动拉起运行时后的首个请求使用较宽松的重试窗口。
fn request_timeouts(global: &CliGlobalOptions, runtime_was_started: bool) -> (Duration, Duration) {
    if runtime_was_started {
        (
            connect_timeout(global).max(Duration::from_millis(1000)),
            read_timeout(global).max(Duration::from_millis(5000)),
        )
    } else {
        (connect_timeout(global), read_timeout(global))
    }
}

fn ipc_call(
    request: &Value,
    global: &CliGlobalOptions,
    runtime_was_started: bool,
) -> Result<Value, CliError> {
    let (connect_timeout, read_timeout) = request_timeouts(global, runtime_was_started);
    ipc_call_with_timeouts(request, connect_timeout, read_timeout)
}

fn runtime_executable_names() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &["NeurolingsCE.exe", "shijima-qt.exe"]
    }
    #[cfg(not(windows))]
    {
        &["NeurolingsCE", "shijima-qt"]
    }
}

fn runtime_exe_candidates(dir: &std::path::Path) -> Vec<PathBuf> {
    runtime_executable_names()
        .iter()
        .map(|name| dir.join(name))
        .collect()
}

fn is_executable_file(path: &std::path::Path) -> bool {
    #[cfg(windows)]
    {
        path.is_file()
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        path.metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    }
}

fn select_runtime_executable(
    candidates: &[PathBuf],
    cli_path: &std::path::Path,
) -> Option<PathBuf> {
    let cli_path = cli_path
        .canonicalize()
        .unwrap_or_else(|_| cli_path.to_path_buf());
    candidates.iter().find_map(|candidate| {
        if !is_executable_file(candidate) {
            return None;
        }
        let canonical = candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.to_path_buf());
        (canonical != cli_path).then(|| candidate.clone())
    })
}

fn runtime_exe_path() -> Option<PathBuf> {
    let current = std::env::current_exe().ok()?;
    let dir = current.parent()?;
    select_runtime_executable(&runtime_exe_candidates(dir), &current)
}

fn ping_with_timeouts(connect_timeout: Duration, read_timeout: Duration) -> bool {
    ipc_call_with_timeouts(&json!({ "command": "ping" }), connect_timeout, read_timeout)
        .map(|v| v.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .unwrap_or(false)
}

fn ping(global: &CliGlobalOptions) -> bool {
    ping_with_timeouts(connect_timeout(global), read_timeout(global))
}

/// 运行时不可用时自动拉起并等待就绪。
/// 返回值表示本次命令是否实际拉起了运行时，以便首个请求使用启动重试窗口。
fn ensure_runtime(command: &CliCommand) -> Result<bool, CliError> {
    let global = &command.global;
    if ping(global) {
        return Ok(false);
    }
    let Some(exe) = runtime_exe_path() else {
        let mut error = CliError::new(
            "runtime_not_found",
            "Could not find the NeurolingsCE runtime executable next to the CLI",
            1,
        );
        error.details = exe_candidate_hint();
        return Err(error);
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // 与原版 startDetached 语义一致：脱离父进程，不共享控制台与管道，
        // 避免 runtime 常驻导致调用方（shell/脚本）等待其退出。
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        let started = std::process::Command::new(&exe)
            .arg("--neurolingsce-cli-runtime")
            .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if started.is_err() {
            let mut error = CliError::new(
                "runtime_start_failed",
                "Could not start the NeurolingsCE runtime executable",
                1,
            );
            error.details = exe.display().to_string();
            return Err(error);
        }
    }
    #[cfg(not(windows))]
    {
        let started = std::process::Command::new(&exe)
            .arg("--neurolingsce-cli-runtime")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if started.is_err() {
            let mut error = CliError::new(
                "runtime_start_failed",
                "Could not start the NeurolingsCE runtime executable",
                1,
            );
            error.details = exe.display().to_string();
            return Err(error);
        }
    }

    // 启动等待时间覆盖连接+读取超时，至少 10 秒。
    let startup_timeout =
        (connect_timeout(global) + read_timeout(global)).max(Duration::from_millis(10000));
    let deadline = Instant::now() + startup_timeout;
    let startup_connect_timeout = connect_timeout(global).min(Duration::from_millis(250));
    let startup_read_timeout = read_timeout(global).min(Duration::from_millis(500));
    loop {
        if ping_with_timeouts(startup_connect_timeout, startup_read_timeout) {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let mut error = CliError::new(
        "runtime_start_timeout",
        "NeurolingsCE runtime was started but did not become ready",
        1,
    );
    error.details = exe.display().to_string();
    Err(error)
}

fn exe_candidate_hint() -> String {
    let current = std::env::current_exe().ok();
    current
        .and_then(|p| p.parent().map(PathBuf::from))
        .map(|dir| {
            runtime_exe_candidates(&dir)
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_default()
}

// ---- Codex notify 外部转发 ----
//
// 参考实现 CommandExecutor.cc::executeCodexNotify 的转发分支：处理
// --codex-notify 时，若 ~/.codex/config.toml 的托管块记录了被替换的原
// notify 命令（previous-notify-base64 元数据，仅支持桥接 codex-computer-use），
// 把原始负载作为最后一个参数 startDetached 转发给它。
// 解析逻辑与 neurolings-runtime/src/codex.rs（安装侧）保持一致。

const CODEX_BEGIN_MARKER: &str = "# BEGIN NeurolingsCE Codex notify";
const CODEX_END_MARKER: &str = "# END NeurolingsCE Codex notify";
const CODEX_PREVIOUS_PREFIX: &str = "# NeurolingsCE previous-notify-base64: ";

/// Codex 配置文件路径：CODEX_HOME 优先，否则 ~/.codex/config.toml。
fn codex_config_path() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("CODEX_HOME") {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed).join("config.toml"));
        }
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    Some(PathBuf::from(home).join(".codex").join("config.toml"))
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let vals: Vec<u32> = text
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            b'=' => 0,
            _ => u32::MAX,
        })
        .collect();
    if vals.contains(&u32::MAX) || vals.is_empty() || !vals.len().is_multiple_of(4) {
        return None;
    }
    for chunk in vals.chunks(4) {
        let n = (chunk[0] << 18) | (chunk[1] << 12) | (chunk[2] << 6) | chunk[3];
        out.push((n >> 16 & 0xFF) as u8);
        out.push((n >> 8 & 0xFF) as u8);
        out.push((n & 0xFF) as u8);
    }
    let padding = text.chars().rev().take(2).filter(|c| *c == '=').count();
    out.truncate(out.len().saturating_sub(padding));
    Some(out)
}

/// 从托管块读出被替换的原 notify 命令参数列表。
/// Ok(None) 表示未配置转发；Err 表示配置存在但无法解析（告警用）。
fn codex_forward_notify_command() -> Result<Option<Vec<String>>, String> {
    let Some(path) = codex_config_path() else {
        return Ok(None);
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        // 配置文件不存在视为未配置（未安装 Codex 是常见情况）。
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Could not read Codex configuration: {e}")),
    };
    let Some(start) = content.find(CODEX_BEGIN_MARKER) else {
        return Ok(None);
    };
    let Some(end) = content[start..].find(CODEX_END_MARKER) else {
        return Ok(None);
    };
    let block = &content[start..start + end];
    let Some(previous) = block.lines().find_map(|line| {
        line.strip_prefix(CODEX_PREVIOUS_PREFIX)
            .map(str::trim)
            .filter(|b64| !b64.is_empty())
    }) else {
        return Ok(None);
    };
    let decoded = base64_decode(previous)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            "NeurolingsCE's managed Codex block contains invalid forwarding metadata".to_string()
        })?;
    // 解析 `notify = ["程序", "参数", ...]`；TOML 数组按 JSON 规则解析。
    let json_part = decoded
        .split_once('=')
        .map(|(_, rest)| rest.trim())
        .ok_or_else(|| {
            "NeurolingsCE's managed Codex block contains an unsupported forwarding command"
                .to_string()
        })?;
    let parsed = serde_json::from_str::<Value>(json_part).map_err(|_| {
        "NeurolingsCE's managed Codex block contains an unsupported forwarding command".to_string()
    })?;
    let Some(args) = parsed.as_array().and_then(|items| {
        items
            .iter()
            .map(|item| item.as_str().map(str::to_string))
            .collect::<Option<Vec<String>>>()
    }) else {
        return Err(
            "NeurolingsCE's managed Codex block contains an unsupported forwarding command"
                .to_string(),
        );
    };
    // 仅桥接 codex-computer-use 的 turn-ended 通知（行为基准：isCodexComputerUseNotify）。
    let supported = args.len() == 2 && args[1] == "turn-ended" && {
        let file_name = args[0].replace('\\', "/");
        let file_name = file_name.rsplit('/').next().unwrap_or("");
        file_name.eq_ignore_ascii_case("codex-computer-use.exe")
            || file_name.eq_ignore_ascii_case("codex-computer-use")
    };
    if !supported {
        return Err(
            "NeurolingsCE's managed Codex block contains an unsupported forwarding command"
                .to_string(),
        );
    }
    Ok(Some(args))
}

/// 把原始 notify 负载转发给用户原来的 notify 命令；失败仅告警，不阻断主流程。
fn forward_codex_notify(payload: &str) {
    let args = match codex_forward_notify_command() {
        Ok(Some(args)) => args,
        Ok(None) => return,
        Err(error) => {
            eprintln!("warning: Could not load Codex notify forwarding command: {error}");
            return;
        }
    };
    let (program, forward_args) = args.split_first().unwrap();
    let spawned = {
        let mut cmd = std::process::Command::new(program);
        cmd.args(forward_args)
            .arg(payload)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        // 采用 QProcess::startDetached 的语义：脱离父进程独立运行。
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            cmd.creation_flags(DETACHED_PROCESS);
        }
        cmd.spawn()
    };
    if spawned.is_err() {
        let file_name = PathBuf::from(program);
        eprintln!(
            "warning: Could not forward Codex notification to {}",
            file_name
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| program.clone())
        );
    }
}

fn to_error(response: &Value) -> CliError {
    let message = response
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("IPC request failed")
        .to_string();
    let code = response
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or("ipc_error")
        .to_string();
    let mut error = CliError::new(&code, &message, 1);
    error.http_status = response.get("status").and_then(Value::as_i64).unwrap_or(0) as i32;
    error
}

fn invalid_response(message: &str, details: &str) -> CliError {
    let mut error = CliError::new("invalid_response", message, 1);
    error.details = details.to_string();
    error
}

fn parse_mascot_anchor(value: Option<&Value>) -> Result<(f64, f64), &'static str> {
    let Some(anchor) = value.and_then(Value::as_object) else {
        return Err("anchor must be an object");
    };
    let (Some(x), Some(y)) = (
        anchor.get("x").and_then(Value::as_f64),
        anchor.get("y").and_then(Value::as_f64),
    ) else {
        return Err("anchor must contain numeric x and y");
    };
    Ok((x, y))
}

fn parse_mascot_info(value: &Value) -> Result<MascotInfo, &'static str> {
    let Some(object) = value.as_object() else {
        return Err("mascot entry must be an object");
    };
    let (Some(id), Some(data_id), Some(name)) = (
        object.get("id").and_then(Value::as_i64),
        object.get("data_id").and_then(Value::as_i64),
        object.get("name").and_then(Value::as_str),
    ) else {
        return Err("mascot entry is missing required fields");
    };
    let (anchor_x, anchor_y) = parse_mascot_anchor(object.get("anchor"))?;
    Ok(MascotInfo {
        id,
        data_id,
        name: name.to_string(),
        cli_label: object.get("label").and_then(Value::as_i64),
        anchor_x,
        anchor_y,
        active_behavior: object
            .get("active_behavior")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn parse_mascot_array(response: &Value, key: &str) -> Result<Vec<MascotInfo>, CliError> {
    let Some(items) = response.get(key).and_then(Value::as_array) else {
        return Err(invalid_response("Malformed response from local IPC", ""));
    };
    items
        .iter()
        .map(|item| {
            parse_mascot_info(item)
                .map_err(|details| invalid_response("Malformed mascot payload", details))
        })
        .collect()
}

fn parse_mascot_object(response: &Value, key: &str) -> Result<MascotInfo, CliError> {
    let value = response.get(key).unwrap_or(&Value::Null);
    parse_mascot_info(value)
        .map_err(|details| invalid_response("Malformed mascot payload", details))
}

fn parse_loaded_mascot_info(value: &Value) -> Result<LoadedMascotInfo, &'static str> {
    let Some(object) = value.as_object() else {
        return Err("loaded mascot entry must be an object");
    };
    let (Some(id), Some(name)) = (
        object.get("id").and_then(Value::as_i64),
        object.get("name").and_then(Value::as_str),
    ) else {
        return Err("loaded mascot entry is missing required fields");
    };
    Ok(LoadedMascotInfo {
        id,
        name: name.to_string(),
        version: object
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        description: object
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        author: object
            .get("author")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

fn parse_loaded_mascot_array(
    response: &Value,
    key: &str,
) -> Result<Vec<LoadedMascotInfo>, CliError> {
    let Some(items) = response.get(key).and_then(Value::as_array) else {
        return Err(invalid_response("Malformed response from local IPC", ""));
    };
    items
        .iter()
        .map(|item| {
            parse_loaded_mascot_info(item)
                .map_err(|details| invalid_response("Malformed loaded mascot payload", details))
        })
        .collect()
}

fn parse_cli_label_object(response: &Value) -> Result<(i64, i64), CliError> {
    let (Some(label), Some(mascot_id)) = (
        response.get("label").and_then(Value::as_i64),
        response.get("mascot_id").and_then(Value::as_i64),
    ) else {
        return Err(invalid_response(
            "Malformed CLI label payload",
            "CLI label entry is missing required fields",
        ));
    };
    Ok((label, mascot_id))
}

/// 低强度随机源：当前时间的亚秒纳秒部分。
fn now_subsec_nanos() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0)
}

fn list_mascots_result(response: Value) -> Result<CliExecutionResult, CliError> {
    Ok(CliExecutionResult {
        mascots: parse_mascot_array(&response, "mascots")?,
        ..Default::default()
    })
}

/// id 解析：数字 id 或 oldest/newest/random（配合 selector）。
fn resolve_mascot_id(
    command: &CliCommand,
    runtime_was_started: &mut bool,
) -> Result<i64, CliError> {
    let global = &command.global;
    if let Ok(id) = command.id_token.parse::<i32>() {
        if !command.selectors.is_empty() || !command.selector.is_empty() {
            return Err(CliError::new(
                "invalid_arguments",
                "You can't specify a numeric ID and a selector at the same time",
                2,
            ));
        }
        if id < 0 {
            return Err(CliError::new(
                "invalid_arguments",
                "ID must be greater than or equal to 0",
                2,
            ));
        }
        return Ok(i64::from(id));
    }
    if !matches!(command.id_token.as_str(), "oldest" | "newest" | "random") {
        let mut error = CliError::new("invalid_arguments", "Invalid auto ID.", 2);
        error.details = "Expected one of: oldest, newest, random".to_string();
        return Err(error);
    }

    let selectors: Vec<String> = if !command.selectors.is_empty() {
        command.selectors.clone()
    } else if !command.selector.is_empty() {
        vec![command.selector.clone()]
    } else {
        vec![String::new()]
    };

    for selector in selectors {
        let mut request = json!({ "command": "list_mascots" });
        if !selector.is_empty() {
            request["selector"] = json!(selector);
        }
        let response = ipc_call(&request, global, std::mem::take(runtime_was_started))?;
        if response.get("error").is_some() {
            return Err(to_error(&response));
        }
        let mascots = parse_mascot_array(&response, "mascots")?;
        if mascots.is_empty() {
            continue;
        }
        return match command.id_token.as_str() {
            "oldest" => Ok(mascots.first().unwrap().id),
            "newest" => Ok(mascots.last().unwrap().id),
            _ => {
                // random：用系统时间做简单扰动（无需密码学强度）。
                let idx = (now_subsec_nanos() as usize) % mascots.len();
                Ok(mascots[idx].id)
            }
        };
    }
    Err(CliError::new(
        "not_found",
        "Failed to determine ID (are any mascots spawned?)",
        1,
    ))
}

/// 从多个 --behavior 中随机选一个。
fn choose_behavior(behaviors: &[String]) -> Option<String> {
    if behaviors.is_empty() {
        return None;
    }
    let idx = (now_subsec_nanos() as usize) % behaviors.len();
    behaviors.get(idx).cloned()
}

fn codex_response_status(response: &Value, fallback_state: &str) -> (bool, String) {
    let handled = response
        .get("handled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let state = response
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or(fallback_state)
        .to_string();
    (handled, state)
}

pub fn execute_runtime(command: &CliCommand) -> CliExecutionResult {
    let mut result = CliExecutionResult::default();
    let global = &command.global;

    match command.kind {
        CliCommandKind::DocumentList | CliCommandKind::ListMascots => {
            let mut runtime_was_started = match ensure_runtime(command) {
                Ok(started) => started,
                Err(error) => {
                    result.error = Some(error);
                    return result;
                }
            };
            let mut request = json!({ "command": "list_mascots" });
            if !command.selector.is_empty() {
                request["selector"] = json!(command.selector);
            }
            match ipc_call(&request, global, std::mem::take(&mut runtime_was_started)) {
                Ok(response) if response.get("error").is_none() => {
                    match list_mascots_result(response) {
                        Ok(parsed) => return parsed,
                        Err(error) => result.error = Some(error),
                    }
                }
                Ok(response) => result.error = Some(to_error(&response)),
                Err(error) => result.error = Some(error),
            }
        }
        CliCommandKind::ListLoadedMascots => {
            let mut runtime_was_started = match ensure_runtime(command) {
                Ok(started) => started,
                Err(error) => {
                    result.error = Some(error);
                    return result;
                }
            };
            match ipc_call(
                &json!({ "command": "list_loaded_mascots" }),
                global,
                std::mem::take(&mut runtime_was_started),
            ) {
                Ok(response) if response.get("error").is_none() => {
                    match parse_loaded_mascot_array(&response, "loaded_mascots") {
                        Ok(loaded) => result.loaded_mascots = loaded,
                        Err(error) => result.error = Some(error),
                    }
                }
                Ok(response) => result.error = Some(to_error(&response)),
                Err(error) => result.error = Some(error),
            }
        }
        CliCommandKind::DocumentSummon | CliCommandKind::SpawnMascot => {
            let mut runtime_was_started = match ensure_runtime(command) {
                Ok(started) => started,
                Err(error) => {
                    result.error = Some(error);
                    return result;
                }
            };
            let mut spawn_request = command.spawn_request.clone();

            // summon random：先列出模板再随机选一个。
            if command.kind == CliCommandKind::DocumentSummon && command.summon_mode == "random" {
                match ipc_call(
                    &json!({ "command": "list_loaded_mascots" }),
                    global,
                    std::mem::take(&mut runtime_was_started),
                ) {
                    Ok(response) if response.get("error").is_none() => {
                        let loaded = match parse_loaded_mascot_array(&response, "loaded_mascots") {
                            Ok(loaded) => loaded,
                            Err(error) => {
                                result.error = Some(error);
                                return result;
                            }
                        };
                        if loaded.is_empty() {
                            result.error = Some(CliError::new(
                                "not_found",
                                "No loaded mascots are available",
                                1,
                            ));
                            return result;
                        }
                        let idx = (now_subsec_nanos() as usize) % loaded.len();
                        spawn_request.data_id = Some(loaded[idx].id);
                        spawn_request.name = None;
                    }
                    Ok(response) => {
                        result.error = Some(to_error(&response));
                        return result;
                    }
                    Err(error) => {
                        result.error = Some(error);
                        return result;
                    }
                }
            }

            if let Some(behavior) = choose_behavior(&command.behaviors) {
                spawn_request.behavior = Some(behavior);
            }

            let mut request = json!({
                "command": "spawn_mascot",
                "request": {
                    "name": spawn_request.name,
                    "data_id": spawn_request.data_id,
                },
            });
            if let (Some(x), Some(y)) = (spawn_request.anchor_x, spawn_request.anchor_y) {
                request["request"]["anchor"] = json!({ "x": x, "y": y });
            }
            if let Some(behavior) = &spawn_request.behavior {
                request["request"]["behavior"] = json!(behavior);
            }
            match ipc_call(&request, global, std::mem::take(&mut runtime_was_started)) {
                Ok(response) if response.get("error").is_none() => {
                    match parse_mascot_object(&response, "mascot") {
                        Ok(mascot) => result.mascot = Some(mascot),
                        Err(error) => {
                            result.error = Some(error);
                            return result;
                        }
                    }
                }
                Ok(response) => {
                    result.error = Some(to_error(&response));
                    return result;
                }
                Err(error) => {
                    result.error = Some(error);
                    return result;
                }
            }

            // 文档式 summon：为桌宠分配用户标签。
            if command.kind == CliCommandKind::DocumentSummon {
                let Some(mascot) = result.mascot.as_ref() else {
                    return result;
                };
                let mascot_id = mascot.id;
                let mut label_request =
                    json!({ "command": "register_cli_label", "mascot_id": mascot_id });
                if let Some(label) = command.cli_label {
                    label_request["label"] = json!(label);
                }
                match ipc_call(
                    &label_request,
                    global,
                    std::mem::take(&mut runtime_was_started),
                ) {
                    Ok(response) if response.get("error").is_none() => {
                        let (label, _) = match parse_cli_label_object(&response) {
                            Ok(label_info) => label_info,
                            Err(error) => {
                                result.error = Some(error);
                                return result;
                            }
                        };
                        if let Some(m) = result.mascot.as_mut() {
                            m.cli_label = Some(label);
                        }
                        result.assigned_label = Some(label);
                    }
                    Ok(response) => result.error = Some(to_error(&response)),
                    Err(error) => result.error = Some(error),
                }
            }
        }
        CliCommandKind::AlterMascot => {
            let mut runtime_was_started = match ensure_runtime(command) {
                Ok(started) => started,
                Err(error) => {
                    result.error = Some(error);
                    return result;
                }
            };
            let id = match resolve_mascot_id(command, &mut runtime_was_started) {
                Ok(id) => id,
                Err(error) => {
                    result.error = Some(error);
                    return result;
                }
            };
            let mut patch = json!({});
            if let (Some(x), Some(y)) = (command.patch_anchor_x, command.patch_anchor_y) {
                patch["anchor"] = json!({ "x": x, "y": y });
            }
            if let Some(behavior) = choose_behavior(&command.behaviors) {
                patch["behavior"] = json!(behavior);
            }
            let request = json!({
                "command": "alter_mascot",
                "mascot_id": id,
                "patch": patch,
            });
            match ipc_call(&request, global, std::mem::take(&mut runtime_was_started)) {
                Ok(response) if response.get("error").is_none() => {
                    match parse_mascot_object(&response, "mascot") {
                        Ok(mascot) => result.mascot = Some(mascot),
                        Err(error) => result.error = Some(error),
                    }
                }
                Ok(response) => result.error = Some(to_error(&response)),
                Err(error) => result.error = Some(error),
            }
        }
        CliCommandKind::DocumentClose => {
            let mut runtime_was_started = match ensure_runtime(command) {
                Ok(started) => started,
                Err(error) => {
                    result.error = Some(error);
                    return result;
                }
            };
            let Some(label) = command.cli_label else {
                result.error = Some(CliError::new("invalid_label", "Label is required", 2));
                return result;
            };
            let resolved = ipc_call(
                &json!({ "command": "get_cli_label", "label": label }),
                global,
                std::mem::take(&mut runtime_was_started),
            );
            match resolved {
                Ok(response) if response.get("error").is_none() => {
                    match parse_cli_label_object(&response) {
                        Ok((_, mascot_id)) => match ipc_call(
                            &json!({ "command": "dismiss_mascot", "mascot_id": mascot_id }),
                            global,
                            std::mem::take(&mut runtime_was_started),
                        ) {
                            Ok(response) if response.get("error").is_none() => {
                                result.closed_label = Some(label);
                            }
                            Ok(response) => result.error = Some(to_error(&response)),
                            Err(error) => result.error = Some(error),
                        },
                        Err(error) => result.error = Some(error),
                    }
                }
                Ok(response) => result.error = Some(to_error(&response)),
                Err(error) => result.error = Some(error),
            }
        }
        CliCommandKind::DismissMascot => {
            let mut runtime_was_started = match ensure_runtime(command) {
                Ok(started) => started,
                Err(error) => {
                    result.error = Some(error);
                    return result;
                }
            };
            let id = match resolve_mascot_id(command, &mut runtime_was_started) {
                Ok(id) => id,
                Err(error) => {
                    result.error = Some(error);
                    return result;
                }
            };
            match ipc_call(
                &json!({ "command": "dismiss_mascot", "mascot_id": id }),
                global,
                std::mem::take(&mut runtime_was_started),
            ) {
                Ok(response) if response.get("error").is_none() => {}
                Ok(response) => result.error = Some(to_error(&response)),
                Err(error) => result.error = Some(error),
            }
        }
        CliCommandKind::DocumentCloseAll | CliCommandKind::DismissAllMascots => {
            let mut runtime_was_started = match ensure_runtime(command) {
                Ok(started) => started,
                Err(error) => {
                    result.error = Some(error);
                    return result;
                }
            };
            let mut request = json!({ "command": "dismiss_all_mascots" });
            if !command.selector.is_empty() {
                request["selector"] = json!(command.selector);
            }
            match ipc_call(&request, global, std::mem::take(&mut runtime_was_started)) {
                Ok(response) if response.get("error").is_none() => {}
                Ok(response) => result.error = Some(to_error(&response)),
                Err(error) => result.error = Some(error),
            }
        }
        CliCommandKind::DocumentStop => {
            // --stop 幂等且不自动拉起运行时。
            match ipc_call(&json!({ "command": "stop_runtime" }), global, false) {
                Ok(response) if response.get("error").is_none() => {
                    result.stopped = true;
                }
                Ok(response) => result.error = Some(to_error(&response)),
                Err(_error) if !ping(global) => {
                    result.stopped = true;
                }
                Err(error) => result.error = Some(error),
            }
        }
        CliCommandKind::CodexNotify => {
            // 先解析负载：非法 JSON 直接报错；未识别事件静默放过。
            let payload: Value = match serde_json::from_str::<Value>(&command.codex_notify_payload)
            {
                Ok(v) if v.is_object() => v,
                Ok(_) => {
                    result.error = Some(CliError::new(
                        "invalid_arguments",
                        "Invalid Codex notification JSON: expected a JSON object",
                        2,
                    ));
                    return result;
                }
                Err(e) => {
                    result.error = Some(CliError::new(
                        "invalid_arguments",
                        &format!("Invalid Codex notification JSON: {e}"),
                        2,
                    ));
                    return result;
                }
            };
            // 负载是合法 JSON 对象后即尝试外部转发（失败仅告警）。
            forward_codex_notify(&command.codex_notify_payload);
            let parsed = match neurolings_common::codex::parse_activity(&payload) {
                Ok(parsed) => parsed,
                Err(e) => {
                    result.error = Some(CliError::new(
                        "invalid_arguments",
                        &format!("Invalid Codex notification: {e}"),
                        2,
                    ));
                    return result;
                }
            };
            result.codex_event_type = Some(parsed.activity.event_type.clone());
            if !parsed.recognized {
                return result;
            }
            // 通知是尽力而为：只尝试一次请求，运行时不在时静默忽略且不拉起。
            let request = json!({
                "command": "show_codex_notification",
                "payload": payload,
            });
            match ipc_call(&request, global, false) {
                Ok(response) if response.get("error").is_none() => {
                    let (handled, state) =
                        codex_response_status(&response, parsed.activity.state.name());
                    result.codex_handled = handled;
                    result.codex_state = Some(state);
                }
                Ok(response) => result.error = Some(to_error(&response)),
                Err(_) => {}
            }
        }
        _ => {
            result.error = Some(CliError::new(
                "not_implemented",
                "This command requires the runtime and is not supported here",
                2,
            ));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn mascot_payload_requires_all_contract_fields() {
        let error = parse_mascot_info(&json!({"id": 1})).unwrap_err();
        assert_eq!(error, "mascot entry is missing required fields");

        let error = parse_mascot_info(&json!({
            "id": 1,
            "data_id": 2,
            "name": "Default",
        }))
        .unwrap_err();
        assert_eq!(error, "anchor must be an object");

        let mascot = parse_mascot_info(&json!({
            "id": 1,
            "data_id": 2,
            "name": "Default",
            "anchor": {"x": 10.0, "y": 20.0},
            "label": 3,
            "active_behavior": "Walk",
        }))
        .unwrap();
        assert_eq!(mascot.id, 1);
        assert_eq!(mascot.data_id, 2);
        assert_eq!(mascot.anchor_x, 10.0);
        assert_eq!(mascot.cli_label, Some(3));
    }

    #[test]
    fn malformed_response_arrays_return_invalid_response() {
        let error = parse_mascot_array(&json!({}), "mascots").unwrap_err();
        assert_eq!(error.code, "invalid_response");
        assert_eq!(error.error, "Malformed response from local IPC");

        let error = parse_mascot_array(&json!({"mascots": [{}]}), "mascots").unwrap_err();
        assert_eq!(error.error, "Malformed mascot payload");
        assert_eq!(error.details, "mascot entry is missing required fields");

        let error =
            parse_loaded_mascot_array(&json!({"loaded_mascots": [{"id": 1}]}), "loaded_mascots")
                .unwrap_err();
        assert_eq!(error.error, "Malformed loaded mascot payload");
        assert_eq!(
            error.details,
            "loaded mascot entry is missing required fields"
        );
    }

    #[test]
    fn cli_label_payload_requires_label_and_mascot_id() {
        let error = parse_cli_label_object(&json!({"label": 1})).unwrap_err();
        assert_eq!(error.code, "invalid_response");
        assert_eq!(error.error, "Malformed CLI label payload");
        assert_eq!(error.details, "CLI label entry is missing required fields");

        assert_eq!(
            parse_cli_label_object(&json!({"label": 1, "mascot_id": 2})).unwrap(),
            (1, 2)
        );
    }

    #[test]
    fn mascot_id_rejects_values_outside_i32_range() {
        let command = CliCommand {
            id_token: "2147483648".to_string(),
            ..Default::default()
        };
        let mut runtime_was_started = false;
        let error = resolve_mascot_id(&command, &mut runtime_was_started).unwrap_err();
        assert_eq!(error.code, "invalid_arguments");
        assert_eq!(error.error, "Invalid auto ID.");
    }

    #[test]
    fn auto_started_runtime_uses_the_startup_retry_window() {
        let global = CliGlobalOptions {
            connect_timeout_ms: Some(1),
            read_timeout_ms: Some(2),
            ..Default::default()
        };
        assert_eq!(
            request_timeouts(&global, false),
            (Duration::from_millis(1), Duration::from_millis(2))
        );
        assert_eq!(
            request_timeouts(&global, true),
            (Duration::from_millis(1000), Duration::from_millis(5000))
        );
    }

    #[test]
    fn codex_response_defaults_match_the_runtime_contract() {
        assert_eq!(
            codex_response_status(&json!({}), "ready"),
            (true, "ready".to_string())
        );
        assert_eq!(
            codex_response_status(&json!({"handled": false, "state": "blocked"}), "ready"),
            (false, "blocked".to_string())
        );
    }

    #[test]
    fn runtime_candidates_include_legacy_install_name() {
        assert!(
            runtime_executable_names()
                .iter()
                .any(|name| name.starts_with("shijima-qt"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn runtime_candidate_selection_skips_the_cli_binary() {
        let temp = tempfile::tempdir().unwrap();
        let cli = temp.path().join("NeurolingsCE.exe");
        let legacy_runtime = temp.path().join("shijima-qt.exe");
        std::fs::File::create(&cli).unwrap();
        std::fs::File::create(&legacy_runtime).unwrap();

        let selected = select_runtime_executable(&[cli.clone(), legacy_runtime.clone()], &cli);
        assert_eq!(selected, Some(legacy_runtime));
    }
}
