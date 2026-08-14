//! NeurolingsCE-cli：命令行控制与独立模板管理。
//!
//! 解析 → 执行 → 格式化全流程以函数形式暴露，便于测试直接驱动；
//! `main.rs` 只是 [`run`] 的薄封装。

pub mod commands;
pub mod output;
pub mod parser;
pub mod runtime;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

/// 经平台传输发送一行 IPC 请求并读取响应。
pub fn ipc_transport_call(line: &str, timeout: Duration) -> Result<String, CliTransportError> {
    neurolings_platform::ipc::ipc_client_call(
        neurolings_common::ipc::IPC_ENDPOINT,
        line,
        timeout,
        1024 * 1024,
    )
    .map_err(|e| CliTransportError(e.to_string()))
}

/// IPC 传输错误（包装平台错误文本）。
#[derive(Debug)]
pub struct CliTransportError(pub String);

/// 执行 CLI 全流程，返回捕获的输出与退出码。
/// `storage_override` 用于测试重定向模板存储。
pub fn run_to_output(argv: &[String], storage_override: Option<PathBuf>) -> output::CliOutput {
    match parser::parse_cli_arguments(argv) {
        parser::ParseOutcome::Failure { global, error } => output::write_cli_error(&global, &error),
        parser::ParseOutcome::Success(command) => {
            let result = commands::execute(&command, storage_override.as_deref());
            output::write_cli_output(&command, &result)
        }
    }
}

/// 执行 CLI 并打印 stdout/stderr，返回进程退出码。
pub fn run(argv: Vec<String>, storage_override: Option<PathBuf>) -> ExitCode {
    let output = run_to_output(&argv, storage_override);
    if !output.stdout.is_empty() {
        print!("{}", output.stdout);
    }
    if !output.stderr.is_empty() {
        eprint!("{}", output.stderr);
    }
    ExitCode::from(output.exit_code.clamp(0, 255) as u8)
}
