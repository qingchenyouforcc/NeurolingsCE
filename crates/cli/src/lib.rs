//! `NeurolingsCE-cli` 的参数解析与稳定输出。
//!
//! 解析器只负责把命令行语法转换为协议层请求，不启动运行时，也不直接连接 IPC。

use std::{ffi::OsString, fmt};

use api::{ApiRequest, Command, MascotPatch, SpawnMascotRequest};
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

/// CLI 默认连接超时（毫秒）。
pub const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 500;
/// CLI 默认读取超时（毫秒）。
pub const DEFAULT_READ_TIMEOUT_MS: u64 = 500;

/// 全局命令行选项。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CliGlobalOptions {
    /// 是否不输出人类可读文本。
    pub quiet: bool,
    /// 是否使用单行 JSON 输出。
    pub json: bool,
    /// 建立 IPC 连接的超时。
    pub connect_timeout_ms: u64,
    /// 等待读取 IPC 响应的超时。
    pub read_timeout_ms: u64,
}

impl Default for CliGlobalOptions {
    fn default() -> Self {
        Self {
            quiet: false,
            json: false,
            connect_timeout_ms: DEFAULT_CONNECT_TIMEOUT_MS,
            read_timeout_ms: DEFAULT_READ_TIMEOUT_MS,
        }
    }
}

/// 文档化 CLI 命令。
#[derive(Clone, Debug, PartialEq)]
pub enum CliCommand {
    /// 输出帮助。
    Help,
    /// 输出版本。
    Version,
    /// 列出屏幕 mascot。
    List,
    /// 召唤 mascot。
    Summon(SummonCommand),
    /// 关闭指定 label 或 selector。
    Close { selector: String },
    /// 关闭全部 mascot。
    CloseAll,
    /// 停止运行时。
    Stop,
    /// 管理 mascot 模板。
    Mascot(MascotCommand),
    /// 向运行时投递 Codex 通知。
    CodexNotify { payload: String },
    /// 兼容旧命令。
    Legacy(ApiRequest),
}

/// `--summon` 参数。
#[derive(Clone, Debug, PartialEq)]
pub enum SummonCommand {
    /// 按模板名称召唤。
    Name { name: String, label: Option<String> },
    /// 按模板数据 ID 召唤。
    DataId { data_id: i32, label: Option<String> },
    /// 随机召唤。
    Random { label: Option<String> },
}

/// `--mascot` 子命令。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MascotCommand {
    /// 列出本地模板。
    List,
    /// 导入模板压缩包。
    Add { archive: String },
    /// 删除模板。
    Remove { name: String },
}

/// 一次完整 CLI 调用。
#[derive(Clone, Debug, PartialEq)]
pub struct CliInvocation {
    /// 全局选项。
    pub global: CliGlobalOptions,
    /// 已解析的命令。
    pub command: CliCommand,
}

/// 参数解析错误。
#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize)]
pub struct CliError {
    /// 稳定机器可读错误代码。
    pub code: String,
    /// 面向用户的错误。
    #[serde(rename = "error")]
    pub message: String,
    /// 进程退出码。
    pub exit_code: i32,
}

impl CliError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            exit_code: 2,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

/// 解析包含程序名的 argv。
pub fn parse_args<I, S>(args: I) -> Result<CliInvocation, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut tokens = args
        .into_iter()
        .map(|arg| arg.into().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    if tokens
        .first()
        .is_some_and(|token| !token.starts_with('-') && !is_command_token(token))
    {
        tokens.remove(0);
    }
    parse_tokens(tokens)
}

/// 解析不包含程序名的参数 token。
pub fn parse_tokens<I, S>(args: I) -> Result<CliInvocation, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let tokens = args.into_iter().map(Into::into).collect::<Vec<_>>();
    let mut global = CliGlobalOptions::default();
    let mut index = 0;
    while index < tokens.len() {
        match tokens[index].as_str() {
            "--quiet" => global.quiet = true,
            "--json" => global.json = true,
            "--connect-timeout-ms" => {
                index += 1;
                global.connect_timeout_ms = parse_timeout(tokens.get(index), "connect-timeout-ms")?;
            }
            "--read-timeout-ms" => {
                index += 1;
                global.read_timeout_ms = parse_timeout(tokens.get(index), "read-timeout-ms")?;
            }
            _ => break,
        }
        index += 1;
    }

    let command_tokens = &tokens[index..];
    let command = parse_command(command_tokens)?;
    Ok(CliInvocation { global, command })
}

fn parse_command(tokens: &[String]) -> Result<CliCommand, CliError> {
    let Some(first) = tokens.first().map(String::as_str) else {
        return Ok(CliCommand::Help);
    };
    match first {
        "--help" | "-h" => Ok(CliCommand::Help),
        "--version" | "-v" => Ok(CliCommand::Version),
        "--list" | "-l" => Ok(CliCommand::List),
        "--close" => one_value(tokens, "--close").map(|selector| CliCommand::Close { selector }),
        "--close-all" => exact(tokens, "--close-all", CliCommand::CloseAll),
        "--stop" => exact(tokens, "--stop", CliCommand::Stop),
        "--summon" | "-s" => parse_summon(&tokens[1..]),
        "--mascot" | "-m" => parse_mascot(&tokens[1..]),
        "--codex-notify" => {
            one_value(tokens, "--codex-notify").map(|payload| CliCommand::CodexNotify { payload })
        }
        "list" => exact(
            tokens,
            "list",
            CliCommand::Legacy(ApiRequest::new(Command::ListMascots)),
        ),
        "list-loaded" => exact(
            tokens,
            "list-loaded",
            CliCommand::Legacy(ApiRequest::new(Command::ListLoadedMascots)),
        ),
        "spawn" => parse_legacy_spawn(&tokens[1..]),
        "alter" => parse_legacy_alter(&tokens[1..]),
        "dismiss" => one_value(tokens, "dismiss").map(|selector| {
            CliCommand::Legacy(
                ApiRequest::new(Command::DismissMascot)
                    .with_field("selector", Value::String(selector)),
            )
        }),
        "dismiss-all" => exact(
            tokens,
            "dismiss-all",
            CliCommand::Legacy(ApiRequest::new(Command::DismissAllMascots)),
        ),
        _ => Err(CliError::new(
            "unknown_command",
            format!("unknown command: {first}"),
        )),
    }
}

fn parse_summon(tokens: &[String]) -> Result<CliCommand, CliError> {
    let Some(mode) = tokens.first().map(String::as_str) else {
        return Err(CliError::new(
            "missing_summon_mode",
            "--summon requires mascot or random",
        ));
    };
    match mode {
        "random" => Ok(CliCommand::Summon(SummonCommand::Random {
            label: optional_single(&tokens[1..], "label")?,
        })),
        "mascot" => {
            let mut name = None;
            let mut data_id = None;
            let mut label = None;
            let mut index = 1;
            while index < tokens.len() {
                match tokens[index].as_str() {
                    "--name" => {
                        index += 1;
                        name = Some(required(tokens.get(index), "--name")?);
                    }
                    "--data-id" => {
                        index += 1;
                        data_id = Some(parse_id(tokens.get(index), "--data-id")?);
                    }
                    value if !value.starts_with('-') && label.is_none() => {
                        label = Some(value.to_owned())
                    }
                    value => {
                        return Err(CliError::new(
                            "invalid_summon_option",
                            format!("unexpected summon option: {value}"),
                        ));
                    }
                }
                index += 1;
            }
            match (name, data_id) {
                (Some(name), None) => Ok(CliCommand::Summon(SummonCommand::Name { name, label })),
                (None, Some(data_id)) => {
                    Ok(CliCommand::Summon(SummonCommand::DataId { data_id, label }))
                }
                (Some(_), Some(_)) => Err(CliError::new(
                    "ambiguous_summon",
                    "--name and --data-id cannot be used together",
                )),
                (None, None) => Err(CliError::new(
                    "missing_summon_target",
                    "mascot summon requires --name or --data-id",
                )),
            }
        }
        _ => Err(CliError::new(
            "invalid_summon_mode",
            "summon mode must be mascot or random",
        )),
    }
}

fn parse_mascot(tokens: &[String]) -> Result<CliCommand, CliError> {
    match tokens {
        [action] if action == "list" => Ok(CliCommand::Mascot(MascotCommand::List)),
        [action, archive] if action == "add" => Ok(CliCommand::Mascot(MascotCommand::Add {
            archive: archive.clone(),
        })),
        [action, name] if action == "remove" => Ok(CliCommand::Mascot(MascotCommand::Remove {
            name: name.clone(),
        })),
        _ => Err(CliError::new(
            "invalid_mascot_command",
            "--mascot expects list, add ARCHIVE, or remove NAME",
        )),
    }
}

fn parse_legacy_spawn(tokens: &[String]) -> Result<CliCommand, CliError> {
    match tokens {
        [name] => Ok(CliCommand::Legacy(
            ApiRequest::new(Command::SpawnMascot).with_field("name", Value::String(name.clone())),
        )),
        [flag, value] if flag == "--data-id" => Ok(CliCommand::Legacy(
            ApiRequest::new(Command::SpawnMascot).with_field(
                "data_id",
                Value::Number(
                    (*value)
                        .parse::<i32>()
                        .map_err(|_| {
                            CliError::new("invalid_data_id", "data id must be an integer")
                        })?
                        .into(),
                ),
            ),
        )),
        _ => Err(CliError::new(
            "invalid_spawn",
            "spawn expects NAME or --data-id ID",
        )),
    }
}

fn parse_legacy_alter(tokens: &[String]) -> Result<CliCommand, CliError> {
    let id = parse_id(tokens.first(), "alter")?;
    let mut request =
        ApiRequest::new(Command::AlterMascot).with_field("id", Value::Number(id.into()));
    if let Some(behavior) = tokens.get(1) {
        request = request.with_field("behavior", Value::String(behavior.clone()));
    }
    if tokens.len() > 2 {
        return Err(CliError::new(
            "invalid_alter",
            "alter accepts ID and optional behavior",
        ));
    }
    Ok(CliCommand::Legacy(request))
}

fn one_value(tokens: &[String], option: &str) -> Result<String, CliError> {
    if tokens.len() != 2 {
        return Err(CliError::new(
            "invalid_arguments",
            format!("{option} expects one value"),
        ));
    }
    Ok(tokens[1].clone())
}

fn exact<T>(tokens: &[String], option: &str, value: T) -> Result<T, CliError> {
    if tokens.len() != 1 {
        return Err(CliError::new(
            "invalid_arguments",
            format!("{option} takes no arguments"),
        ));
    }
    Ok(value)
}

fn required(value: Option<&String>, option: &str) -> Result<String, CliError> {
    value
        .cloned()
        .ok_or_else(|| CliError::new("missing_value", format!("{option} requires a value")))
}

fn parse_id(value: Option<&String>, option: &str) -> Result<i32, CliError> {
    required(value, option)?
        .parse::<i32>()
        .map_err(|_| CliError::new("invalid_id", format!("{option} must be an integer")))
}

fn parse_timeout(value: Option<&String>, option: &str) -> Result<u64, CliError> {
    let timeout = required(value, option)?.parse::<u64>().map_err(|_| {
        CliError::new(
            "invalid_timeout",
            format!("{option} must be a positive integer"),
        )
    })?;
    if timeout == 0 {
        return Err(CliError::new(
            "invalid_timeout",
            format!("{option} must be positive"),
        ));
    }
    Ok(timeout)
}

fn optional_single(tokens: &[String], name: &str) -> Result<Option<String>, CliError> {
    match tokens {
        [] => Ok(None),
        [value] if !value.starts_with('-') => Ok(Some(value.clone())),
        _ => Err(CliError::new(
            "invalid_arguments",
            format!("{name} accepts at most one value"),
        )),
    }
}

fn is_command_token(token: &str) -> bool {
    token.starts_with('-')
        || [
            "list",
            "list-loaded",
            "spawn",
            "alter",
            "dismiss",
            "dismiss-all",
        ]
        .contains(&token)
}

/// 将任意成功结果编码为单行 JSON。
pub fn format_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string(value)
}

/// 将 CLI 错误编码为单行 JSON。
pub fn format_error_json(error: &CliError) -> Result<String, serde_json::Error> {
    format_json(error)
}

/// 把文档化召唤命令转换为统一 API 请求。
pub fn summon_request(command: &SummonCommand) -> ApiRequest {
    match command {
        SummonCommand::Name { name, label } => spawn_request(
            SpawnMascotRequest {
                name: Some(name.clone()),
                data_id: None,
                patch: MascotPatch::default(),
            },
            label,
        ),
        SummonCommand::DataId { data_id, label } => spawn_request(
            SpawnMascotRequest {
                name: None,
                data_id: Some(*data_id),
                patch: MascotPatch::default(),
            },
            label,
        ),
        SummonCommand::Random { label } => {
            let mut request =
                ApiRequest::new(Command::SpawnMascot).with_field("random", Value::Bool(true));
            if let Some(label) = label {
                request = request.with_field("label", Value::String(label.clone()));
            }
            request
        }
    }
}

/// 将需要访问运行时的 CLI 命令转换为统一 API 请求。
pub fn request_for_command(command: &CliCommand) -> Option<ApiRequest> {
    match command {
        CliCommand::List => Some(ApiRequest::new(Command::ListMascots)),
        CliCommand::Summon(command) => Some(summon_request(command)),
        CliCommand::Close { selector } => Some(
            ApiRequest::new(Command::DismissAllMascots)
                .with_field("selector", Value::String(selector.clone())),
        ),
        CliCommand::CloseAll => Some(ApiRequest::new(Command::DismissAllMascots)),
        CliCommand::Stop => Some(ApiRequest::new(Command::StopRuntime)),
        CliCommand::CodexNotify { payload } => Some(
            ApiRequest::new(Command::ShowCodexNotification)
                .with_field("payload", Value::String(payload.clone())),
        ),
        CliCommand::Legacy(request) => Some(request.clone()),
        CliCommand::Help | CliCommand::Version | CliCommand::Mascot(_) => None,
    }
}

fn spawn_request(request: SpawnMascotRequest, label: &Option<String>) -> ApiRequest {
    let mut value = serde_json::to_value(request).expect("协议结构可序列化");
    let fields = value.as_object_mut().expect("spawn request 必须是 object");
    if let Some(label) = label {
        fields.insert("label".into(), Value::String(label.clone()));
    }
    ApiRequest {
        command: Command::SpawnMascot,
        fields: fields.clone().into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_document_commands_and_global_options() {
        let invocation = parse_args([
            "NeurolingsCE-cli",
            "--json",
            "--summon",
            "mascot",
            "--name",
            "Default",
            "desk",
        ])
        .unwrap();
        assert!(invocation.global.json);
        assert_eq!(
            invocation.command,
            CliCommand::Summon(SummonCommand::Name {
                name: "Default".into(),
                label: Some("desk".into())
            })
        );
    }

    #[test]
    fn parses_legacy_commands_into_api_requests() {
        let invocation = parse_tokens(["dismiss-all"]).unwrap();
        assert_eq!(
            invocation.command,
            CliCommand::Legacy(ApiRequest::new(Command::DismissAllMascots))
        );
    }

    #[test]
    fn rejects_ambiguous_summon_and_zero_timeout() {
        assert!(parse_tokens(["--summon", "mascot", "--name", "A", "--data-id", "1"]).is_err());
        assert_eq!(
            parse_tokens(["--connect-timeout-ms", "0", "--list"])
                .unwrap_err()
                .code,
            "invalid_timeout"
        );
    }

    #[test]
    fn formats_compact_machine_readable_error() {
        let error = CliError::new("unknown_command", "unknown command");
        let output = format_error_json(&error).unwrap();
        assert_eq!(
            output,
            r#"{"code":"unknown_command","error":"unknown command","exit_code":2}"#
        );
        assert!(!output.contains('\n'));
    }

    #[test]
    fn converts_runtime_commands_to_api_requests() {
        let command = CliCommand::Close {
            selector: "Default".into(),
        };
        let request = request_for_command(&command).unwrap();
        assert_eq!(request.command, Command::DismissAllMascots);
        assert_eq!(
            request.field("selector"),
            Some(&Value::String("Default".into()))
        );
        assert!(request_for_command(&CliCommand::Help).is_none());
    }
}
