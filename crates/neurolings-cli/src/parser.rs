//! 命令行参数解析：全局选项、文档式命令与遗留命令。

/// --codex-notify 负载的最大字节数。
pub const CODEX_NOTIFY_MAX_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CliCommandKind {
    #[default]
    Help,
    Version,
    DocumentList,
    DocumentSummon,
    DocumentClose,
    DocumentCloseAll,
    DocumentStop,
    DocumentMascot,
    CodexNotify,
    ListMascots,
    ListLoadedMascots,
    SpawnMascot,
    AlterMascot,
    DismissMascot,
    DismissAllMascots,
}

#[derive(Debug, Clone, Default)]
pub struct CliGlobalOptions {
    pub quiet: bool,
    pub json: bool,
    pub connect_timeout_ms: Option<u64>,
    pub read_timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct SpawnRequest {
    pub name: Option<String>,
    pub data_id: Option<i64>,
    pub anchor_x: Option<f64>,
    pub anchor_y: Option<f64>,
    pub behavior: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CliCommand {
    pub kind: CliCommandKind,
    pub global: CliGlobalOptions,
    pub command_name: String,
    pub document_style: bool,
    pub mascot_action: String,
    pub mascot_archive_path: String,
    pub mascot_template_name: String,
    pub cli_label: Option<i64>,
    pub summon_mode: String,
    pub spawn_request: SpawnRequest,
    pub patch_anchor_x: Option<f64>,
    pub patch_anchor_y: Option<f64>,
    pub codex_notify_payload: String,
    pub selector: String,
    pub selectors: Vec<String>,
    pub behaviors: Vec<String>,
    pub id_token: String,
    pub sort_by_id: bool,
}

/// 错误负载（契约 errorJson 形状）。
#[derive(Debug, Clone)]
pub struct CliError {
    pub code: String,
    pub error: String,
    pub details: String,
    pub usage: String,
    pub exit_code: i32,
    pub http_status: i32,
}

impl CliError {
    pub fn new(code: &str, error: &str, exit_code: i32) -> Self {
        CliError {
            code: code.to_string(),
            error: error.to_string(),
            details: String::new(),
            usage: String::new(),
            exit_code,
            http_status: 0,
        }
    }
}

/// 参数解析结果：成功命令，或携带全局选项的用法错误
/// （全局选项决定错误按 JSON 还是文本输出）。
pub enum ParseOutcome {
    Success(Box<CliCommand>),
    Failure {
        global: CliGlobalOptions,
        error: CliError,
    },
}

const LEGACY_COMMANDS: &[&str] = &[
    "list",
    "list-loaded",
    "spawn",
    "alter",
    "dismiss",
    "dismiss-all",
];
const DOCUMENT_COMMANDS: &[&str] = &[
    "--help",
    "-h",
    "--summon",
    "-s",
    "--close",
    "--close-all",
    "--stop",
    "--mascot",
    "-m",
    "--list",
    "-l",
    "--version",
    "-v",
    "--codex-notify",
];

fn is_legacy_command(token: &str) -> bool {
    LEGACY_COMMANDS.contains(&token)
}

fn is_document_command(token: &str) -> bool {
    DOCUMENT_COMMANDS.contains(&token)
}

fn is_boolean_global(token: &str) -> bool {
    token == "--quiet" || token == "--json"
}

fn is_valued_global(token: &str) -> bool {
    token == "--host"
        || token == "--port"
        || token == "--connect-timeout-ms"
        || token == "--read-timeout-ms"
}

fn help_synopsis(argv0: &str) -> String {
    format!(
        "Usage: {argv0} [--quiet] [--json] \
         [--connect-timeout-ms MS] [--read-timeout-ms MS] <command>"
    )
}

/// 无具体命令上下文时的完整用法文本。
pub fn document_help_text(argv0: &str) -> String {
    let executable = argv0;
    format!(
        "{}\n\
         \n\
         Document commands:\n\
         \u{20} {executable} --help|-h\n\
         \u{20} {executable} --version|-v\n\
         \u{20} {executable} --codex-notify JSON\n\
         \u{20} {executable} --list|-l\n\
         \u{20} {executable} --summon|-s mascot --name NAME [label]\n\
         \u{20} {executable} --summon|-s mascot --data-id ID [label]\n\
         \u{20} {executable} --summon|-s random [label]\n\
         \u{20} {executable} --close LABEL\n\
         \u{20} {executable} --close-all\n\
         \u{20} {executable} --stop\n\
         \u{20} {executable} --mascot|-m list\n\
         \u{20} {executable} --mascot|-m add ZIP\n\
         \u{20} {executable} --mascot|-m remove MASCOT\n\
         \u{20} {executable} --mascot|-m validate FILE [--json]\n\
         \n\
         Global options:\n\
         \u{20} --quiet  --json  --connect-timeout-ms MS  --read-timeout-ms MS\n\
         \n\
         Transport notes:\n\
         \u{20} Runtime commands auto-start a local runtime when needed.\n\
         \u{20} --codex-notify never auto-starts the runtime; closed-app callbacks are ignored.\n\
         \u{20} Commands use local IPC and do not use HTTP.\n\
         \u{20} --host and --port are no longer supported.\n\
         \n\
         Legacy commands remain supported:\n\
         \u{20} list, list-loaded, spawn, alter, dismiss, dismiss-all",
        help_synopsis(argv0),
    )
}

/// 各命令的用法文本。
pub fn command_usage(argv0: &str, command_name: &str) -> String {
    let usage = match command_name {
        "list" => "Usage: {exe} [globals...] list [--selector JS] [--json]".to_string(),
        "list-loaded" => {
            "Usage: {exe} [globals...] list-loaded [--sort-by-id] [--json]".to_string()
        }
        "spawn" => "Usage: {exe} [globals...] spawn (--name NAME | --data-id ID) \
             [--behavior NAME]... [--x X --y Y] [--json]"
            .to_string(),
        "alter" => "Usage: {exe} [globals...] alter --id ID_OR_AUTO [--selector JS]... \
             [--behavior NAME]... [--x X --y Y] [--json]"
            .to_string(),
        "dismiss" => {
            "Usage: {exe} [globals...] dismiss --id ID_OR_AUTO [--selector JS]".to_string()
        }
        "dismiss-all" => "Usage: {exe} [globals...] dismiss-all [--selector JS]".to_string(),
        "--summon" => {
            "Usage: {exe} [globals...] --summon|-s mascot (--name NAME | --data-id ID) [label]\n\
             \u{20}      {exe} [globals...] --summon|-s random [label]"
                .to_string()
        }
        "--close" => "Usage: {exe} [globals...] --close LABEL".to_string(),
        "--close-all" => "Usage: {exe} [globals...] --close-all".to_string(),
        "--stop" => "Usage: {exe} [globals...] --stop".to_string(),
        "--mascot" => "Usage: {exe} [globals...] --mascot|-m list\n\
             \u{20}      {exe} [globals...] --mascot|-m add ZIP\n\
             \u{20}      {exe} [globals...] --mascot|-m remove MASCOT\n\
             \u{20}      {exe} [globals...] --mascot|-m validate FILE [--json]"
            .to_string(),
        "--list" => "Usage: {exe} [globals...] --list|-l".to_string(),
        "--version" => "Usage: {exe} [globals...] --version|-v".to_string(),
        "--codex-notify" => "Usage: {exe} [globals...] --codex-notify JSON [--json]".to_string(),
        _ => return document_help_text(argv0),
    };
    usage.replace("{exe}", argv0)
}

fn parse_error(
    global: &CliGlobalOptions,
    message: &str,
    argv0: &str,
    command_name: &str,
    details: &str,
) -> CliError {
    let _ = global;
    CliError {
        code: "invalid_arguments".to_string(),
        error: message.to_string(),
        details: details.to_string(),
        usage: if command_name.is_empty() {
            document_help_text(argv0)
        } else {
            command_usage(argv0, command_name)
        },
        exit_code: 2,
        http_status: 0,
    }
}

struct ArgCursor<'a> {
    args: &'a [String],
    index: usize,
}

impl<'a> ArgCursor<'a> {
    fn new(args: &'a [String]) -> Self {
        ArgCursor { args, index: 0 }
    }

    fn has_next(&self) -> bool {
        self.index < self.args.len()
    }

    fn peek(&self) -> &str {
        &self.args[self.index]
    }

    fn take(&mut self) -> String {
        let value = self.args[self.index].clone();
        self.index += 1;
        value
    }
}

fn fail(
    global: &CliGlobalOptions,
    message: &str,
    argv0: &str,
    command_name: &str,
    details: &str,
) -> ParseOutcome {
    ParseOutcome::Failure {
        global: global.clone(),
        error: parse_error(global, message, argv0, command_name, details),
    }
}

fn parse_int_value(value: &str) -> Option<i64> {
    value.parse::<i64>().ok()
}

fn parse_optional_label(value: &str) -> Option<i64> {
    match parse_int_value(value) {
        Some(label) if label >= 0 => Some(label),
        _ => None,
    }
}

fn set_boolean_global(token: &str, global: &mut CliGlobalOptions) {
    if token == "--quiet" {
        global.quiet = true;
    } else if token == "--json" {
        global.json = true;
    }
}

/// 应用带值的全局选项；`Unsupported` 标记 `--host`/`--port`（消费后拒绝）。
enum GlobalApply {
    Ok,
    InvalidValue,
    Unsupported,
}

fn apply_global_option(token: &str, value: &str, global: &mut CliGlobalOptions) -> GlobalApply {
    if token == "--host" || token == "--port" {
        return GlobalApply::Unsupported;
    }
    let parsed = parse_int_value(value).and_then(|timeout| u64::try_from(timeout).ok());
    if token == "--connect-timeout-ms" {
        match parsed {
            Some(timeout) => {
                global.connect_timeout_ms = Some(timeout);
                GlobalApply::Ok
            }
            None => GlobalApply::InvalidValue,
        }
    } else if token == "--read-timeout-ms" {
        match parsed {
            Some(timeout) => {
                global.read_timeout_ms = Some(timeout);
                GlobalApply::Ok
            }
            None => GlobalApply::InvalidValue,
        }
    } else {
        GlobalApply::Ok
    }
}

fn document_command_kind(token: &str) -> CliCommandKind {
    match token {
        "--help" | "-h" => CliCommandKind::Help,
        "--version" | "-v" => CliCommandKind::Version,
        "--list" | "-l" => CliCommandKind::DocumentList,
        "--summon" | "-s" => CliCommandKind::DocumentSummon,
        "--close" => CliCommandKind::DocumentClose,
        "--close-all" => CliCommandKind::DocumentCloseAll,
        "--stop" => CliCommandKind::DocumentStop,
        "--mascot" | "-m" => CliCommandKind::DocumentMascot,
        _ => CliCommandKind::CodexNotify,
    }
}

fn legacy_command_kind(token: &str) -> CliCommandKind {
    match token {
        "list" => CliCommandKind::ListMascots,
        "list-loaded" => CliCommandKind::ListLoadedMascots,
        "spawn" => CliCommandKind::SpawnMascot,
        "alter" => CliCommandKind::AlterMascot,
        "dismiss" => CliCommandKind::DismissMascot,
        _ => CliCommandKind::DismissAllMascots,
    }
}

/// 解析 CLI 参数。argv[0] 为可执行文件名（用于用法文本）。
pub fn parse_cli_arguments(argv: &[String]) -> ParseOutcome {
    let argv0: &str = argv
        .first()
        .map(String::as_str)
        .unwrap_or("NeurolingsCE-cli");
    let mut global = CliGlobalOptions::default();
    let mut args = ArgCursor::new(&argv[argv.len().min(1)..]);

    while args.has_next() {
        let token = args.peek().to_string();
        if is_legacy_command(&token) || is_document_command(&token) {
            break;
        }
        let token = args.take();
        if is_boolean_global(&token) {
            set_boolean_global(&token, &mut global);
            continue;
        }
        if is_valued_global(&token) {
            if !args.has_next() {
                return fail(
                    &global,
                    &format!("Missing value for {token}"),
                    argv0,
                    "",
                    "",
                );
            }
            let value = args.take();
            match apply_global_option(&token, &value, &mut global) {
                GlobalApply::Ok => {}
                GlobalApply::InvalidValue => {
                    return fail(
                        &global,
                        &format!("Invalid value for {token}"),
                        argv0,
                        "",
                        "",
                    );
                }
                GlobalApply::Unsupported => {
                    return fail(
                        &global,
                        &format!("{token} is not supported by the local IPC CLI"),
                        argv0,
                        "",
                        "Use the local running instance instead of host/port routing.",
                    );
                }
            }
            continue;
        }
        return fail(
            &global,
            &format!("Unknown global option: {token}"),
            argv0,
            "",
            "",
        );
    }

    if !args.has_next() {
        return fail(&global, "Missing command", argv0, "", "");
    }

    let command_token = args.take();
    let command = CliCommand {
        global: global.clone(),
        command_name: command_token.clone(),
        ..Default::default()
    };

    if is_document_command(&command_token) {
        return parse_document_command(args, command, &command_token, argv0);
    }
    parse_legacy_command(args, command, &command_token, argv0)
}

fn parse_document_command(
    mut args: ArgCursor,
    mut command: CliCommand,
    command_token: &str,
    argv0: &str,
) -> ParseOutcome {
    command.document_style = true;
    command.kind = document_command_kind(command_token);

    if command.kind == CliCommandKind::CodexNotify {
        if !args.has_next() {
            return fail(
                &command.global,
                "Missing Codex notification JSON",
                argv0,
                command_token,
                "",
            );
        }
        command.codex_notify_payload = args.take();
        if command.codex_notify_payload.len() > CODEX_NOTIFY_MAX_BYTES {
            return fail(
                &command.global,
                &format!(
                    "Codex notification JSON exceeds the maximum size of {CODEX_NOTIFY_MAX_BYTES} bytes"
                ),
                argv0,
                command_token,
                "",
            );
        }
        let parsed: Result<serde_json::Value, _> =
            serde_json::from_str(&command.codex_notify_payload);
        match parsed {
            Ok(serde_json::Value::Object(_)) => {}
            Ok(_) => {
                return fail(
                    &command.global,
                    "Invalid Codex notification JSON: expected a JSON object",
                    argv0,
                    command_token,
                    "",
                );
            }
            Err(error) => {
                return fail(
                    &command.global,
                    &format!("Invalid Codex notification JSON: {error}"),
                    argv0,
                    command_token,
                    "",
                );
            }
        }
        while args.has_next() {
            let option = args.take();
            if option == "--json" {
                command.global.json = true;
                continue;
            }
            return fail(
                &command.global,
                &format!("Unexpected argument: {option}"),
                argv0,
                command_token,
                "",
            );
        }
        return ParseOutcome::Success(Box::new(command));
    }

    if matches!(
        command.kind,
        CliCommandKind::Help
            | CliCommandKind::Version
            | CliCommandKind::DocumentList
            | CliCommandKind::DocumentCloseAll
            | CliCommandKind::DocumentStop
    ) {
        if args.has_next() {
            let extra = args.take();
            return fail(
                &command.global,
                &format!("Unexpected argument: {extra}"),
                argv0,
                command_token,
                "",
            );
        }
        return ParseOutcome::Success(Box::new(command));
    }

    if command.kind == CliCommandKind::DocumentMascot {
        return parse_document_mascot_command(args, command, command_token, argv0);
    }

    if command.kind == CliCommandKind::DocumentClose {
        if !args.has_next() {
            return fail(
                &command.global,
                "Missing CLI label",
                argv0,
                command_token,
                "",
            );
        }
        let token = args.take();
        match parse_optional_label(&token) {
            Some(label) => command.cli_label = Some(label),
            None => {
                return fail(
                    &command.global,
                    "CLI label must be a non-negative integer",
                    argv0,
                    command_token,
                    "",
                );
            }
        }
        if args.has_next() {
            let extra = args.take();
            return fail(
                &command.global,
                &format!("Unexpected argument: {extra}"),
                argv0,
                command_token,
                "",
            );
        }
        return ParseOutcome::Success(Box::new(command));
    }

    parse_document_summon_command(args, command, command_token, argv0)
}

fn parse_document_mascot_command(
    mut args: ArgCursor,
    mut command: CliCommand,
    command_token: &str,
    argv0: &str,
) -> ParseOutcome {
    if !args.has_next() {
        return fail(
            &command.global,
            "Missing mascot command",
            argv0,
            command_token,
            "",
        );
    }
    command.mascot_action = args.take();

    match command.mascot_action.as_str() {
        "list" => {}
        "add" => {
            if !args.has_next() {
                return fail(
                    &command.global,
                    "Missing mascot archive path",
                    argv0,
                    command_token,
                    "",
                );
            }
            command.mascot_archive_path = args.take();
        }
        "remove" => {
            if !args.has_next() {
                return fail(
                    &command.global,
                    "Missing mascot template name",
                    argv0,
                    command_token,
                    "",
                );
            }
            command.mascot_template_name = args.take();
        }
        "validate" => {
            if !args.has_next() {
                return fail(
                    &command.global,
                    "Missing mascot package path",
                    argv0,
                    command_token,
                    "",
                );
            }
            command.mascot_archive_path = args.take();
        }
        _ => {
            return fail(
                &command.global,
                "Mascot command must be list, add, remove, or validate",
                argv0,
                command_token,
                "",
            );
        }
    }

    if args.has_next() {
        let extra = args.take();
        return fail(
            &command.global,
            &format!("Unexpected argument: {extra}"),
            argv0,
            command_token,
            "",
        );
    }
    ParseOutcome::Success(Box::new(command))
}

fn parse_document_summon_command(
    mut args: ArgCursor,
    mut command: CliCommand,
    command_token: &str,
    argv0: &str,
) -> ParseOutcome {
    if !args.has_next() {
        return fail(
            &command.global,
            "Missing summon mode",
            argv0,
            command_token,
            "",
        );
    }
    command.summon_mode = args.take();
    if command.summon_mode != "mascot" && command.summon_mode != "random" {
        return fail(
            &command.global,
            "Summon mode must be mascot or random",
            argv0,
            command_token,
            "",
        );
    }

    while args.has_next() {
        let token = args.peek().to_string();
        if token == "--name" {
            args.take();
            if !args.has_next() {
                return fail(
                    &command.global,
                    "Missing value for --name",
                    argv0,
                    command_token,
                    "",
                );
            }
            command.spawn_request.name = Some(args.take());
            continue;
        }
        if token == "--data-id" {
            args.take();
            if !args.has_next() {
                return fail(
                    &command.global,
                    "Missing value for --data-id",
                    argv0,
                    command_token,
                    "",
                );
            }
            let value = args.take();
            match parse_int_value(&value) {
                Some(data_id) => command.spawn_request.data_id = Some(data_id),
                None => {
                    return fail(
                        &command.global,
                        "Invalid value for --data-id",
                        argv0,
                        command_token,
                        "",
                    );
                }
            }
            continue;
        }

        let Some(label) = parse_optional_label(&token) else {
            return fail(
                &command.global,
                &format!("Unexpected argument: {token}"),
                argv0,
                command_token,
                "",
            );
        };
        command.cli_label = Some(label);
        args.take();
        if args.has_next() {
            let extra = args.take();
            return fail(
                &command.global,
                &format!("Unexpected argument: {extra}"),
                argv0,
                command_token,
                "",
            );
        }
    }

    if command.summon_mode == "mascot" {
        if command.spawn_request.name.is_some() == command.spawn_request.data_id.is_some() {
            return fail(
                &command.global,
                "You must specify one of --name or --data-id",
                argv0,
                command_token,
                "",
            );
        }
    } else if command.spawn_request.name.is_some() || command.spawn_request.data_id.is_some() {
        return fail(
            &command.global,
            "random summon does not accept --name or --data-id",
            argv0,
            command_token,
            "",
        );
    }

    ParseOutcome::Success(Box::new(command))
}

fn parse_legacy_command(
    mut args: ArgCursor,
    mut command: CliCommand,
    command_token: &str,
    argv0: &str,
) -> ParseOutcome {
    command.kind = legacy_command_kind(command_token);

    while args.has_next() {
        let token = args.take();
        if token == "--json" {
            command.global.json = true;
            continue;
        }

        macro_rules! take_value {
            () => {{
                if !args.has_next() {
                    return fail(
                        &command.global,
                        &format!("Missing value for {token}"),
                        argv0,
                        command_token,
                        "",
                    );
                }
                args.take()
            }};
        }

        match command.kind {
            CliCommandKind::ListMascots => {
                if token == "--selector" {
                    command.selector = take_value!();
                    continue;
                }
            }
            CliCommandKind::ListLoadedMascots => {
                if token == "--sort-by-id" {
                    command.sort_by_id = true;
                    continue;
                }
            }
            CliCommandKind::SpawnMascot => {
                if token == "--name" {
                    command.spawn_request.name = Some(take_value!());
                    continue;
                }
                if token == "--data-id" {
                    let value = take_value!();
                    match parse_int_value(&value) {
                        Some(data_id) => command.spawn_request.data_id = Some(data_id),
                        None => {
                            return fail(
                                &command.global,
                                "Invalid value for --data-id",
                                argv0,
                                command_token,
                                "",
                            );
                        }
                    }
                    continue;
                }
                if token == "--behavior" {
                    command.behaviors.push(take_value!());
                    continue;
                }
                if token == "--x" || token == "--y" {
                    let value = take_value!();
                    match value.parse::<f64>().ok() {
                        Some(number) => {
                            if token == "--x" {
                                command.spawn_request.anchor_x = Some(number);
                            } else {
                                command.spawn_request.anchor_y = Some(number);
                            }
                        }
                        None => {
                            return fail(
                                &command.global,
                                &format!("Invalid value for {token}"),
                                argv0,
                                command_token,
                                "",
                            );
                        }
                    }
                    continue;
                }
            }
            CliCommandKind::AlterMascot => {
                if token == "--id" {
                    command.id_token = take_value!();
                    continue;
                }
                if token == "--selector" {
                    command.selectors.push(take_value!());
                    continue;
                }
                if token == "--behavior" {
                    command.behaviors.push(take_value!());
                    continue;
                }
                if token == "--x" || token == "--y" {
                    let value = take_value!();
                    match value.parse::<f64>().ok() {
                        Some(number) => {
                            if token == "--x" {
                                command.patch_anchor_x = Some(number);
                            } else {
                                command.patch_anchor_y = Some(number);
                            }
                        }
                        None => {
                            return fail(
                                &command.global,
                                &format!("Invalid value for {token}"),
                                argv0,
                                command_token,
                                "",
                            );
                        }
                    }
                    continue;
                }
            }
            CliCommandKind::DismissMascot => {
                if token == "--id" {
                    command.id_token = take_value!();
                    continue;
                }
                if token == "--selector" {
                    command.selector = take_value!();
                    continue;
                }
            }
            CliCommandKind::DismissAllMascots if token == "--selector" => {
                command.selector = take_value!();
                continue;
            }
            _ => {}
        }

        return fail(
            &command.global,
            &format!("Unknown option: {token}"),
            argv0,
            command_token,
            "",
        );
    }

    if command.kind == CliCommandKind::ListLoadedMascots
        && command.global.json
        && command.sort_by_id
    {
        return fail(
            &command.global,
            "--json and --sort-by-id cannot be used together.",
            argv0,
            command_token,
            "",
        );
    }

    let validate_anchor = |x: Option<f64>, y: Option<f64>| -> bool {
        x.is_none() && y.is_none() || (x.is_some() && y.is_some())
    };

    match command.kind {
        CliCommandKind::SpawnMascot => {
            if command.spawn_request.name.is_some() == command.spawn_request.data_id.is_some() {
                return fail(
                    &command.global,
                    "You must specify one of name or data-id.",
                    argv0,
                    command_token,
                    "",
                );
            }
            if !validate_anchor(
                command.spawn_request.anchor_x,
                command.spawn_request.anchor_y,
            ) {
                return fail(
                    &command.global,
                    "X and Y must be specified together",
                    argv0,
                    command_token,
                    "",
                );
            }
        }
        CliCommandKind::AlterMascot => {
            if command.id_token.is_empty() {
                return fail(
                    &command.global,
                    "Missing required option --id",
                    argv0,
                    command_token,
                    "",
                );
            }
            if !validate_anchor(command.patch_anchor_x, command.patch_anchor_y) {
                return fail(
                    &command.global,
                    "X and Y must be specified together",
                    argv0,
                    command_token,
                    "",
                );
            }
        }
        CliCommandKind::DismissMascot if command.id_token.is_empty() => {
            return fail(
                &command.global,
                "Missing required option --id",
                argv0,
                command_token,
                "",
            );
        }
        _ => {}
    }

    ParseOutcome::Success(Box::new(command))
}
