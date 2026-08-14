//! CLI 输出格式化：JSON 为单行紧凑格式带末尾换行，
//! 文本错误输出到 stderr（`ERROR: <msg>`）。
//! 契约见 `docs/contracts/cli-contract.md`。

use neurolings_common::version::{APP_NAME, VERSION};
use neurolings_pack::MascotPackageReport;
use serde_json::{Value, json};

use crate::commands::{CliExecutionResult, LoadedMascotInfo, MascotInfo};
use crate::parser::{CliCommand, CliCommandKind, CliError, CliGlobalOptions};

/// 格式化后的 CLI 输出（供 run 与测试捕获）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CliOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// `--help` 完整文本。
pub fn help_text() -> String {
    format!(
        "{APP_NAME} CLI\n\
         \n\
         Document commands:\n\
         \u{20} --help, -h\n\
         \u{20}     Show help information.\n\
         \u{20} --summon, -s mascot --name NAME [label]\n\
         \u{20}     Summon a specific mascot by name.\n\
         \u{20} --summon, -s mascot --data-id ID [label]\n\
         \u{20}     Summon a specific mascot by data ID.\n\
         \u{20} --summon, -s random [label]\n\
         \u{20}     Summon a random loaded mascot.\n\
         \u{20} --close LABEL\n\
         \u{20}     Close the mascot mapped to the user label.\n\
         \u{20} --close-all\n\
         \u{20}     Close all running mascots.\n\
         \u{20} --stop\n\
         \u{20}     Close all mascots and stop the {APP_NAME} runtime.\n\
         \u{20} --mascot, -m list\n\
         \u{20}     List loaded mascot templates.\n\
         \u{20} --mascot, -m add ZIP\n\
         \u{20}     Import mascot templates from a zip archive.\n\
         \u{20} --mascot, -m remove MASCOT\n\
         \u{20}     Remove a loaded mascot template by name.\n\
         \u{20} --mascot, -m validate FILE\n\
         \u{20}     Validate a .mascot package; --json for stable machine output.\n\
         \u{20} --list, -l\n\
         \u{20}     List running mascots.\n\
         \u{20} --version, -v\n\
         \u{20}     Show version information.\n\
         \u{20} --codex-notify JSON\n\
         \u{20}     Receive one Codex agent-turn-complete notification.\n\
         \n\
         Global options:\n\
         \u{20} --quiet\n\
         \u{20} --json\n\
         \u{20} --connect-timeout-ms MS\n\
         \u{20} --read-timeout-ms MS\n\
         \n\
         Notes:\n\
         \u{20} - Labels are user-facing IDs and are separate from runtime mascot IDs.\n\
         \u{20} - Labels are kept in memory only for the current {APP_NAME} process.\n\
         \u{20} - --mascot template management works standalone without a running {APP_NAME} instance.\n\
         \u{20} - Runtime commands auto-start {APP_NAME} when no local runtime is ready.\n\
         \u{20} - --codex-notify never auto-starts {APP_NAME}; callbacks are ignored when it is closed.\n\
         \u{20} - --host and --port are no longer supported.\n\
         \n\
         Legacy commands remain supported:\n\
         \u{20} list, list-loaded, spawn, alter, dismiss, dismiss-all\n"
    )
}

/// `--help --json` 负载。
pub fn help_json() -> Value {
    let command = |name: &str, aliases: &[&str], usage: &str, description: &str| {
        json!({
            "name": name,
            "aliases": aliases,
            "usage": usage,
            "description": description,
        })
    };
    json!({
        "app": APP_NAME,
        "version": VERSION,
        "global_options": ["--quiet", "--json", "--connect-timeout-ms", "--read-timeout-ms"],
        "commands": [
            command("--help", &["-h"], "--help|-h", "Show help information."),
            command(
                "--summon",
                &["-s"],
                "--summon mascot --name NAME [label] | --summon mascot --data-id ID [label] | --summon random [label]",
                "Summon a mascot and optionally assign a user label."
            ),
            command("--close", &[], "--close LABEL", "Close the mascot mapped to the user label."),
            command("--close-all", &[], "--close-all", "Close all running mascots."),
            command("--stop", &[], "--stop", "Close all mascots and stop the NeurolingsCE runtime."),
            command(
                "--mascot",
                &["-m"],
                "--mascot list | --mascot add ZIP | --mascot remove MASCOT | --mascot validate FILE",
                "List, import, remove, or validate mascot templates."
            ),
            command("--list", &["-l"], "--list|-l", "List running mascots and their labels."),
            command("--version", &["-v"], "--version|-v", "Show version information."),
            command("--codex-notify", &[], "--codex-notify JSON", "Receive one Codex notification."),
        ],
        "legacy_commands": ["list", "list-loaded", "spawn", "alter", "dismiss", "dismiss-all"],
        "label_scope": "current_app_run",
    })
}

fn loaded_mascot_info_to_json(mascot: &LoadedMascotInfo) -> Value {
    json!({
        "id": mascot.id,
        "name": mascot.name,
        "version": mascot.version,
        "description": mascot.description,
        "author": mascot.author,
    })
}

fn mascot_info_to_json(mascot: &MascotInfo) -> Value {
    json!({
        "id": mascot.id,
        "data_id": mascot.data_id,
        "name": mascot.name,
        "label": mascot.cli_label,
        "anchor": { "x": mascot.anchor_x, "y": mascot.anchor_y },
        "active_behavior": mascot.active_behavior,
    })
}

fn build_list_json(result: &CliExecutionResult) -> Value {
    let mascots: Vec<Value> = result.mascots.iter().map(mascot_info_to_json).collect();
    json!({ "mascots": mascots })
}

fn build_loaded_list_json(result: &CliExecutionResult) -> Value {
    let mascots: Vec<Value> = result
        .loaded_mascots
        .iter()
        .map(loaded_mascot_info_to_json)
        .collect();
    json!({ "loaded_mascots": mascots })
}

fn error_json(error: &CliError) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("error".to_string(), json!(error.error));
    object.insert("code".to_string(), json!(error.code));
    if !error.details.is_empty() {
        object.insert("details".to_string(), json!(error.details));
    }
    if !error.usage.is_empty() {
        object.insert("usage".to_string(), json!(error.usage));
    }
    if error.http_status > 0 {
        object.insert("status".to_string(), json!(error.http_status));
    }
    Value::Object(object)
}

/// 校验报告 JSON（契约形状）。
pub fn mascot_validation_json(report: &MascotPackageReport) -> Value {
    json!({
        "ok": report.ok,
        "mascot": {
            "name": report.metadata.name,
            "version": report.metadata.version,
            "description": report.metadata.description,
            "author": report.metadata.author,
        },
        "package_version": report.metadata.version,
        "entry_count": report.entry_count,
        "file_count": report.file_count,
        "extracted_bytes": report.extracted_bytes,
        "errors": report.errors,
    })
}

fn build_document_mascot_json(command: &CliCommand, result: &CliExecutionResult) -> Value {
    if command.mascot_action == "validate"
        && let Some(report) = &result.mascot_validation
    {
        return mascot_validation_json(report);
    }
    if command.mascot_action == "remove" {
        let removed = if result.removed_template_name.is_empty() {
            &command.mascot_template_name
        } else {
            &result.removed_template_name
        };
        return json!({ "removed": removed });
    }
    let templates: Vec<Value> = result
        .loaded_mascots
        .iter()
        .map(loaded_mascot_info_to_json)
        .collect();
    json!({ "templates": templates })
}

fn success_json(command: &CliCommand, result: &CliExecutionResult) -> Value {
    match command.kind {
        CliCommandKind::Help => help_json(),
        CliCommandKind::Version => json!({ "app": APP_NAME, "version": VERSION }),
        CliCommandKind::DocumentMascot => build_document_mascot_json(command, result),
        CliCommandKind::DocumentList | CliCommandKind::ListMascots => build_list_json(result),
        CliCommandKind::ListLoadedMascots => build_loaded_list_json(result),
        CliCommandKind::CodexNotify => {
            let mut object = serde_json::Map::new();
            object.insert("handled".to_string(), json!(result.codex_handled));
            if let Some(event_type) = &result.codex_event_type {
                object.insert("event_type".to_string(), json!(event_type));
            }
            if let Some(state) = &result.codex_state {
                object.insert("state".to_string(), json!(state));
            }
            Value::Object(object)
        }
        CliCommandKind::DocumentStop => json!({ "stopped": true }),
        _ => match &result.mascot {
            Some(mascot) => {
                let mut object = serde_json::Map::new();
                object.insert("mascot".to_string(), mascot_info_to_json(mascot));
                if command.kind == CliCommandKind::DocumentSummon
                    && let Some(label) = result.assigned_label
                {
                    object.insert("label".to_string(), json!(label));
                }
                Value::Object(object)
            }
            None => json!({}),
        },
    }
}

fn to_json_line(value: &Value) -> String {
    format!("{}\n", neurolings_common::json::to_compact_string(value))
}

fn write_document_mascot_text(command: &CliCommand, result: &CliExecutionResult) -> String {
    let mut out = String::new();
    if command.mascot_action == "validate"
        && let Some(report) = &result.mascot_validation
    {
        if report.ok {
            out.push_str(&format!("Valid mascot package: {}", report.metadata.name));
            if !report.metadata.version.is_empty() {
                out.push_str(&format!(" v{}", report.metadata.version));
            }
            out.push_str(&format!(
                " ({} files, {} bytes)\n",
                report.file_count, report.extracted_bytes
            ));
        } else {
            out.push_str("Invalid mascot package:\n");
            for error in &report.errors {
                out.push_str(&format!("  - {error}\n"));
            }
        }
        return out;
    }

    if command.mascot_action == "remove" {
        let removed = if result.removed_template_name.is_empty() {
            &command.mascot_template_name
        } else {
            &result.removed_template_name
        };
        out.push_str(&format!("Removed mascot template {removed}\n"));
        return out;
    }

    if command.mascot_action == "add" {
        out.push_str("Imported mascot template(s):\n");
    }
    out.push_str(&format_templates(&result.loaded_mascots));
    out
}

fn format_templates(mascots: &[LoadedMascotInfo]) -> String {
    let mut out = String::new();
    for mascot in mascots {
        out.push_str(&format!("[{}] {}\n", mascot.id, mascot.name));
        if !mascot.version.is_empty() {
            out.push_str(&format!("  Version: {}\n", mascot.version));
        }
        if !mascot.author.is_empty() {
            out.push_str(&format!("  Author: {}\n", mascot.author));
        }
    }
    out
}

fn document_mascot_line(mascot: &MascotInfo) -> String {
    let label_text = match mascot.cli_label {
        Some(label) => label.to_string(),
        None => "-".to_string(),
    };
    format!(
        "[label:{label_text}] [runtime:{}] {}\n",
        mascot.id, mascot.name
    )
}

fn legacy_mascot_block(mascot: &MascotInfo) -> String {
    let mut out = format!("[{}] {}\n", mascot.id, mascot.name);
    out.push_str(&format!("  Data ID: {}\n", mascot.data_id));
    out.push_str(&format!(
        "  Active behavior: {}\n",
        mascot.active_behavior.clone().unwrap_or_default()
    ));
    // 文本模式的浮点按 6 位有效数字输出。
    out.push_str(&format!(
        "  Anchor: {{{}, {}}}\n",
        neurolings_common::json::format_g6(mascot.anchor_x),
        neurolings_common::json::format_g6(mascot.anchor_y)
    ));
    out
}

fn write_standard_text_output(command: &CliCommand, result: &CliExecutionResult) -> String {
    match command.kind {
        CliCommandKind::Help => help_text(),
        CliCommandKind::Version => format!("{APP_NAME} {VERSION}\n"),
        CliCommandKind::DocumentMascot => write_document_mascot_text(command, result),
        CliCommandKind::DocumentList => result.mascots.iter().map(document_mascot_line).collect(),
        CliCommandKind::DocumentSummon => result
            .mascot
            .as_ref()
            .map(document_mascot_line)
            .unwrap_or_default(),
        CliCommandKind::DocumentClose => {
            format!("Closed label {}\n", command.cli_label.unwrap_or(-1))
        }
        CliCommandKind::DocumentCloseAll => "Closed all mascots\n".to_string(),
        CliCommandKind::DocumentStop => "Stopped NeurolingsCE runtime\n".to_string(),
        CliCommandKind::ListMascots => result.mascots.iter().map(legacy_mascot_block).collect(),
        CliCommandKind::ListLoadedMascots => {
            let mut mascots = result.loaded_mascots.clone();
            if command.sort_by_id {
                mascots.sort_by_key(|m| m.id);
            }
            let mut out = String::new();
            for mascot in &mascots {
                out.push_str(&format!("[{}] {}\n", mascot.id, mascot.name));
                if !mascot.version.is_empty() {
                    out.push_str(&format!("  Version: {}\n", mascot.version));
                }
                if !mascot.author.is_empty() {
                    out.push_str(&format!("  Author: {}\n", mascot.author));
                }
            }
            out
        }
        CliCommandKind::CodexNotify => String::new(),
        _ => result
            .mascot
            .as_ref()
            .map(legacy_mascot_block)
            .unwrap_or_default(),
    }
}

/// 格式化解析/执行错误。
pub fn write_cli_error(global: &CliGlobalOptions, error: &CliError) -> CliOutput {
    if global.json {
        CliOutput {
            stdout: to_json_line(&error_json(error)),
            stderr: String::new(),
            exit_code: error.exit_code,
        }
    } else {
        let mut stderr = format!("ERROR: {}\n", error.error);
        if !error.details.is_empty() {
            stderr.push_str(&error.details);
            stderr.push('\n');
        }
        if !error.usage.is_empty() {
            stderr.push_str(&error.usage);
            stderr.push('\n');
        }
        CliOutput {
            stdout: String::new(),
            stderr,
            exit_code: error.exit_code,
        }
    }
}

/// 格式化完整 CLI 结果。
pub fn write_cli_output(command: &CliCommand, result: &CliExecutionResult) -> CliOutput {
    if let Some(error) = &result.error {
        return write_cli_error(&command.global, error);
    }

    if command.kind == CliCommandKind::DocumentMascot
        && command.mascot_action == "validate"
        && result.mascot_validation.is_some()
    {
        let ok = result
            .mascot_validation
            .as_ref()
            .is_some_and(|report| report.ok);
        if command.global.quiet {
            return CliOutput {
                exit_code: if ok { 0 } else { 1 },
                ..Default::default()
            };
        }
        if command.global.json {
            return CliOutput {
                stdout: to_json_line(&success_json(command, result)),
                exit_code: if ok { 0 } else { 1 },
                ..Default::default()
            };
        }
        return CliOutput {
            stdout: write_standard_text_output(command, result),
            exit_code: if ok { 0 } else { 1 },
            ..Default::default()
        };
    }

    if command.global.quiet {
        return CliOutput::default();
    }

    if command.global.json {
        return CliOutput {
            stdout: to_json_line(&success_json(command, result)),
            exit_code: 0,
            ..Default::default()
        };
    }

    CliOutput {
        stdout: write_standard_text_output(command, result),
        exit_code: 0,
        ..Default::default()
    }
}
