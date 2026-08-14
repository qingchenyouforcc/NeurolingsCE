//! 共享类型：API/IPC JSON 契约、错误、Codex 通知解析、JSON 写出工具。

pub mod api;
pub mod codex;
pub mod error;
pub mod ipc;
pub mod json;
pub mod version;

pub use error::{Error, Result};
