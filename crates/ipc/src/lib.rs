//! 本地 JSONL IPC 服务端和客户端入口。
//!
//! 当前模块只负责一帧 JSONL 数据的边界校验与编解码，socket 生命周期和运行时
//! 调度由上层传输服务负责。将 framing 限制集中在这里，避免不同入口对请求大小
//! 和换行规则产生不一致的判断。

use std::{str, str::Utf8Error};

use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

/// 单个 JSONL 帧允许的最大字节数，包含可选的行结束符。
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// `MAX_FRAME_BYTES` 的语义别名，供按大小命名的调用方使用。
pub const MAX_FRAME_SIZE: usize = MAX_FRAME_BYTES;

/// JSONL framing 校验和编解码错误。
#[derive(Debug, Error)]
pub enum FrameError {
    /// 收到空帧或只包含空白的行。
    #[error("JSONL frame is empty")]
    EmptyFrame,
    /// 收到超过一行的内容，或存在不完整的行结束符。
    #[error("JSONL frame must contain exactly one line")]
    MultipleLines,
    /// 帧（包含换行终止符）超过大小上限。
    #[error("JSONL frame is too large: {size} bytes (maximum {max} bytes)")]
    FrameTooLarge { size: usize, max: usize },
    /// 帧不是合法 UTF-8。
    #[error("JSONL frame is not valid UTF-8: {0}")]
    InvalidUtf8(#[from] Utf8Error),
    /// 帧内容不是合法 JSON。
    #[error("JSONL frame contains invalid JSON: {0}")]
    InvalidJson(#[source] serde_json::Error),
    /// JSON 根值必须是 object。
    #[error("JSONL frame root must be an object")]
    NotObject,
    /// 合法 object 无法反序列化为调用方要求的类型。
    #[error("JSONL object cannot be decoded: {0}")]
    Deserialize(#[source] serde_json::Error),
    /// 调用方对象无法序列化为 JSON。
    #[error("value cannot be encoded as JSONL: {0}")]
    Serialize(#[source] serde_json::Error),
}

/// 将一个 JSON object 编码为带单个换行终止符的 JSONL 帧。
///
/// 编码结果的字节数（包含终止符）不能超过 [`MAX_FRAME_BYTES`]；数组、字符串、
/// 数字和 null 等非 object JSON 值会被拒绝，以保持 IPC 请求的统一 wire shape。
///
/// # Errors
///
/// 当值无法序列化、序列化结果不是 object 或超过大小限制时返回 [`FrameError`]。
pub fn encode<T>(value: &T) -> Result<Vec<u8>, FrameError>
where
    T: Serialize,
{
    let json = serde_json::to_value(value).map_err(FrameError::Serialize)?;
    if !json.is_object() {
        return Err(FrameError::NotObject);
    }

    let mut frame = serde_json::to_vec(&json).map_err(FrameError::Serialize)?;
    frame.push(b'\n');
    ensure_size(frame.len())?;
    Ok(frame)
}

/// 解码一个 JSONL object 帧。
///
/// 输入可以带一个 `\n` 或 `\r\n` 终止符，也可以是不带终止符的单行内容；任何额外
/// 行、空行、裸 `\r` 或超限输入都会在 JSON 解析前被拒绝。
///
/// # Errors
///
/// 返回边界校验错误、JSON 解析错误或目标类型反序列化错误。
pub fn decode<T>(frame: &[u8]) -> Result<T, FrameError>
where
    T: DeserializeOwned,
{
    let payload = strip_line_terminator(frame)?;
    let text = str::from_utf8(payload)?;
    let json: serde_json::Value = serde_json::from_str(text).map_err(FrameError::InvalidJson)?;
    if !json.is_object() {
        return Err(FrameError::NotObject);
    }
    serde_json::from_value(json).map_err(FrameError::Deserialize)
}

/// 校验单帧大小，保持 encode 与 decode 对上限的判断一致。
fn ensure_size(size: usize) -> Result<(), FrameError> {
    if size > MAX_FRAME_BYTES {
        return Err(FrameError::FrameTooLarge {
            size,
            max: MAX_FRAME_BYTES,
        });
    }
    Ok(())
}

/// 去除可选的单个行终止符，并拒绝其他换行内容。
fn strip_line_terminator(frame: &[u8]) -> Result<&[u8], FrameError> {
    ensure_size(frame.len())?;
    if frame.is_empty() {
        return Err(FrameError::EmptyFrame);
    }

    let payload = if let Some(payload) = frame.strip_suffix(b"\n") {
        payload.strip_suffix(b"\r").unwrap_or(payload)
    } else {
        frame
    };

    if payload.iter().any(|byte| *byte == b'\n' || *byte == b'\r') {
        return Err(FrameError::MultipleLines);
    }
    if payload.iter().all(|byte| byte.is_ascii_whitespace()) {
        return Err(FrameError::EmptyFrame);
    }
    Ok(payload)
}
