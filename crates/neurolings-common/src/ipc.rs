//! 运行时与 CLI 共享的本地 IPC 契约。
//!
//! 端点名（必须保持字节级兼容）：
//! `io.github.qingchenyouforcc.NeurolingsCE.cli`
//! 协议：命名管道（Windows）/ Unix socket 上的 UTF-8 JSON 行。

pub const IPC_ENDPOINT: &str = "io.github.qingchenyouforcc.NeurolingsCE.cli";
