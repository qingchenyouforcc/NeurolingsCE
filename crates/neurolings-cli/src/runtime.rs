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

use crate::commands::{CliExecutionResult, MascotInfo};
use crate::parser::{CliCommand, CliCommandKind, CliError, CliGlobalOptions};

// 默认超时与原版一致（0.5s/0.5s）：快速失败，避免脚本长时间挂起。
const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 500;
const DEFAULT_READ_TIMEOUT_MS: u64 = 500;

fn ipc_call(request: &Value, timeout: Duration) -> Result<Value, CliError> {
    let line = serde_json::to_string(request).map_err(|e| {
        CliError::new(
            "ipc_error",
            &format!("Failed to encode IPC request: {e}"),
            1,
        )
    })?;
    let response = crate::ipc_transport_call(&line, timeout).map_err(|e| {
        let mut error = CliError::new(
            "ipc_unavailable",
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

fn runtime_exe_path() -> Option<PathBuf> {
    let current = std::env::current_exe().ok()?;
    let dir = current.parent()?;
    let candidate = if cfg!(windows) {
        dir.join("NeurolingsCE.exe")
    } else {
        dir.join("NeurolingsCE")
    };
    candidate.is_file().then_some(candidate)
}

fn ping(_global: &CliGlobalOptions) -> bool {
    ipc_call(&json!({ "command": "ping" }), Duration::from_millis(500))
        .map(|v| v.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .unwrap_or(false)
}

/// 运行时不可用时自动拉起并等待就绪。
fn ensure_runtime(command: &CliCommand) -> Result<(), CliError> {
    let global = &command.global;
    if ping(global) {
        return Ok(());
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
    while Instant::now() < deadline {
        if ping(global) {
            return Ok(());
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
            let names = if cfg!(windows) {
                ["NeurolingsCE.exe"]
            } else {
                ["NeurolingsCE"]
            };
            names
                .iter()
                .map(|n| dir.join(n).display().to_string())
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_default()
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

fn parse_mascot_info(value: &Value) -> MascotInfo {
    let anchor = value.get("anchor");
    MascotInfo {
        id: value.get("id").and_then(Value::as_i64).unwrap_or(0),
        data_id: value.get("data_id").and_then(Value::as_i64).unwrap_or(0),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        cli_label: value.get("label").and_then(Value::as_i64),
        anchor_x: anchor
            .and_then(|a| a.get("x"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        anchor_y: anchor
            .and_then(|a| a.get("y"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        active_behavior: value
            .get("active_behavior")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

/// 低强度随机源：当前时间的亚秒纳秒部分。
fn now_subsec_nanos() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0)
}

fn list_mascots_result(response: Value) -> CliExecutionResult {
    CliExecutionResult {
        mascots: response
            .get("mascots")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().map(parse_mascot_info).collect())
            .unwrap_or_default(),
        ..Default::default()
    }
}

/// id 解析：数字 id 或 oldest/newest/random（配合 selector）。
fn resolve_mascot_id(command: &CliCommand) -> Result<i64, CliError> {
    let global = &command.global;
    if let Ok(id) = command.id_token.parse::<i64>() {
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
        return Ok(id);
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
        let response = ipc_call(&request, read_timeout(global))?;
        if response.get("error").is_some() {
            return Err(to_error(&response));
        }
        let mascots: Vec<MascotInfo> = response
            .get("mascots")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().map(parse_mascot_info).collect())
            .unwrap_or_default();
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

pub fn execute_runtime(command: &CliCommand) -> CliExecutionResult {
    let mut result = CliExecutionResult::default();
    let global = &command.global;
    let read_to = read_timeout(global);

    match command.kind {
        CliCommandKind::DocumentList | CliCommandKind::ListMascots => {
            if let Err(error) = ensure_runtime(command) {
                result.error = Some(error);
                return result;
            }
            let mut request = json!({ "command": "list_mascots" });
            if !command.selector.is_empty() {
                request["selector"] = json!(command.selector);
            }
            match ipc_call(&request, read_to) {
                Ok(response) if response.get("error").is_none() => {
                    return list_mascots_result(response);
                }
                Ok(response) => result.error = Some(to_error(&response)),
                Err(error) => result.error = Some(error),
            }
        }
        CliCommandKind::ListLoadedMascots => {
            if let Err(error) = ensure_runtime(command) {
                result.error = Some(error);
                return result;
            }
            match ipc_call(&json!({ "command": "list_loaded_mascots" }), read_to) {
                Ok(response) if response.get("error").is_none() => {
                    result.loaded_mascots = response
                        .get("loaded_mascots")
                        .and_then(Value::as_array)
                        .map(|arr| {
                            arr.iter()
                                .map(|m| crate::commands::LoadedMascotInfo {
                                    id: m.get("id").and_then(Value::as_i64).unwrap_or(0),
                                    name: m
                                        .get("name")
                                        .and_then(Value::as_str)
                                        .unwrap_or_default()
                                        .to_string(),
                                    version: m
                                        .get("version")
                                        .and_then(Value::as_str)
                                        .unwrap_or_default()
                                        .to_string(),
                                    description: m
                                        .get("description")
                                        .and_then(Value::as_str)
                                        .unwrap_or_default()
                                        .to_string(),
                                    author: m
                                        .get("author")
                                        .and_then(Value::as_str)
                                        .unwrap_or_default()
                                        .to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                }
                Ok(response) => result.error = Some(to_error(&response)),
                Err(error) => result.error = Some(error),
            }
        }
        CliCommandKind::DocumentSummon | CliCommandKind::SpawnMascot => {
            if let Err(error) = ensure_runtime(command) {
                result.error = Some(error);
                return result;
            }
            let mut spawn_request = command.spawn_request.clone();

            // summon random：先列出模板再随机选一个。
            if command.kind == CliCommandKind::DocumentSummon && command.summon_mode == "random" {
                match ipc_call(&json!({ "command": "list_loaded_mascots" }), read_to) {
                    Ok(response) if response.get("error").is_none() => {
                        let loaded: Vec<Value> = response
                            .get("loaded_mascots")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default();
                        if loaded.is_empty() {
                            result.error = Some(CliError::new(
                                "not_found",
                                "No loaded mascots are available",
                                1,
                            ));
                            return result;
                        }
                        let idx = (now_subsec_nanos() as usize) % loaded.len();
                        spawn_request.data_id = loaded[idx].get("id").and_then(Value::as_i64);
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
            match ipc_call(&request, read_to) {
                Ok(response) if response.get("error").is_none() => {
                    let mascot = response.get("mascot").map(parse_mascot_info);
                    result.mascot = mascot;
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
                match ipc_call(&label_request, read_to) {
                    Ok(response) if response.get("error").is_none() => {
                        let label = response.get("label").and_then(Value::as_i64);
                        if let Some(m) = result.mascot.as_mut() {
                            m.cli_label = label;
                        }
                        result.assigned_label = label;
                    }
                    Ok(response) => result.error = Some(to_error(&response)),
                    Err(error) => result.error = Some(error),
                }
            }
        }
        CliCommandKind::AlterMascot => {
            if let Err(error) = ensure_runtime(command) {
                result.error = Some(error);
                return result;
            }
            let id = match resolve_mascot_id(command) {
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
            match ipc_call(&request, read_to) {
                Ok(response) if response.get("error").is_none() => {
                    result.mascot = response.get("mascot").map(parse_mascot_info);
                }
                Ok(response) => result.error = Some(to_error(&response)),
                Err(error) => result.error = Some(error),
            }
        }
        CliCommandKind::DocumentClose => {
            if let Err(error) = ensure_runtime(command) {
                result.error = Some(error);
                return result;
            }
            let Some(label) = command.cli_label else {
                result.error = Some(CliError::new("invalid_label", "Label is required", 2));
                return result;
            };
            let resolved = ipc_call(
                &json!({ "command": "get_cli_label", "label": label }),
                read_to,
            );
            match resolved {
                Ok(response) if response.get("error").is_none() => {
                    let mascot_id = response.get("mascot_id").and_then(Value::as_i64);
                    if let Some(mascot_id) = mascot_id {
                        match ipc_call(
                            &json!({ "command": "dismiss_mascot", "mascot_id": mascot_id }),
                            read_to,
                        ) {
                            Ok(response) if response.get("error").is_none() => {
                                result.closed_label = Some(label);
                            }
                            Ok(response) => result.error = Some(to_error(&response)),
                            Err(error) => result.error = Some(error),
                        }
                    }
                }
                Ok(response) => result.error = Some(to_error(&response)),
                Err(error) => result.error = Some(error),
            }
        }
        CliCommandKind::DismissMascot => {
            if let Err(error) = ensure_runtime(command) {
                result.error = Some(error);
                return result;
            }
            let id = match resolve_mascot_id(command) {
                Ok(id) => id,
                Err(error) => {
                    result.error = Some(error);
                    return result;
                }
            };
            match ipc_call(
                &json!({ "command": "dismiss_mascot", "mascot_id": id }),
                read_to,
            ) {
                Ok(response) if response.get("error").is_none() => {}
                Ok(response) => result.error = Some(to_error(&response)),
                Err(error) => result.error = Some(error),
            }
        }
        CliCommandKind::DocumentCloseAll | CliCommandKind::DismissAllMascots => {
            if let Err(error) = ensure_runtime(command) {
                result.error = Some(error);
                return result;
            }
            let mut request = json!({ "command": "dismiss_all_mascots" });
            if !command.selector.is_empty() {
                request["selector"] = json!(command.selector);
            }
            match ipc_call(&request, read_to) {
                Ok(response) if response.get("error").is_none() => {}
                Ok(response) => result.error = Some(to_error(&response)),
                Err(error) => result.error = Some(error),
            }
        }
        CliCommandKind::DocumentStop => {
            // --stop 幂等且不自动拉起运行时。
            if !ping(global) {
                result.stopped = true;
                return result;
            }
            match ipc_call(&json!({ "command": "stop_runtime" }), read_to) {
                Ok(response) if response.get("error").is_none() => {
                    result.stopped = true;
                }
                Ok(response) => result.error = Some(to_error(&response)),
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
            // 通知是尽力而为：运行时不在时静默忽略，不拉起。
            if ping(global) {
                let request = json!({
                    "command": "show_codex_notification",
                    "payload": payload,
                });
                if let Ok(response) = ipc_call(&request, read_to) {
                    result.codex_handled = response
                        .get("handled")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    result.codex_state = response
                        .get("state")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
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
