//! Codex notify 配置与 app-server JSONL 协议边界。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// app-server JSON-RPC 请求、通知或响应。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RpcEnvelope {
    /// 请求 ID；通知没有 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    /// 方法名。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// 方法参数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    /// 成功返回值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// 失败返回值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl RpcEnvelope {
    /// 创建请求。
    pub fn request(id: Value, method: impl Into<String>, params: Value) -> Self {
        Self {
            id: Some(id),
            method: Some(method.into()),
            params: Some(params),
            result: None,
            error: None,
        }
    }

    /// 为未知 server request 创建 JSON-RPC 方法不存在响应。
    pub fn method_not_found(&self) -> Self {
        Self {
            id: self.id.clone(),
            method: None,
            params: None,
            result: None,
            error: Some(RpcError {
                code: -32601,
                message: "Method not found".into(),
                data: None,
            }),
        }
    }
}

/// JSON-RPC 错误。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    /// 标准错误代码。
    pub code: i32,
    /// 错误描述。
    pub message: String,
    /// 可选附加信息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// JSONL 解码器错误。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    /// 一帧超过限制。
    #[error("JSONL frame has {actual} bytes, limit is {limit}")]
    FrameTooLarge { actual: usize, limit: usize },
    /// 输入包含多帧。
    #[error("input contains multiple JSONL frames")]
    MultipleFrames,
    /// 输入不是有效 UTF-8。
    #[error("JSONL frame is not UTF-8")]
    InvalidUtf8,
    /// JSON 解析失败。
    #[error("invalid JSON-RPC envelope: {0}")]
    InvalidJson(String),
    /// 配置中存在冲突托管块。
    #[error("managed notify configuration markers conflict")]
    ConflictingConfig,
}

/// 有界 JSONL 编解码器。
#[derive(Clone, Copy, Debug)]
pub struct JsonlCodec {
    limit: usize,
}

impl JsonlCodec {
    /// 创建带最大帧大小的编解码器。
    pub const fn new(limit: usize) -> Self {
        Self { limit }
    }

    /// 解码一行 JSONL，拒绝空输入和多行输入。
    pub fn decode(&self, bytes: &[u8]) -> Result<RpcEnvelope, ProtocolError> {
        let line = bytes.strip_suffix(b"\n").unwrap_or(bytes);
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.len() > self.limit {
            return Err(ProtocolError::FrameTooLarge {
                actual: line.len(),
                limit: self.limit,
            });
        }
        if line.contains(&b'\n') || line.contains(&b'\r') {
            return Err(ProtocolError::MultipleFrames);
        }
        let text = std::str::from_utf8(line).map_err(|_| ProtocolError::InvalidUtf8)?;
        serde_json::from_str(text).map_err(|error| ProtocolError::InvalidJson(error.to_string()))
    }

    /// 编码一帧 JSONL。
    pub fn encode(&self, envelope: &RpcEnvelope) -> Result<Vec<u8>, ProtocolError> {
        let mut bytes = serde_json::to_vec(envelope)
            .map_err(|error| ProtocolError::InvalidJson(error.to_string()))?;
        if bytes.len() > self.limit {
            return Err(ProtocolError::FrameTooLarge {
                actual: bytes.len(),
                limit: self.limit,
            });
        }
        bytes.push(b'\n');
        Ok(bytes)
    }
}

/// Codex 配置中的托管 notify 片段。
pub struct NotifyConfig;

impl NotifyConfig {
    /// 托管块起始标记。
    pub const BEGIN: &'static str = "# BEGIN NEUROLINGSCE NOTIFY";
    /// 托管块结束标记。
    pub const END: &'static str = "# END NEUROLINGSCE NOTIFY";

    /// 添加或替换唯一托管块。
    pub fn enable(config: &str, command: &str) -> Result<String, ProtocolError> {
        let body = format!("{}\nnotify = {:?}\n{}", Self::BEGIN, command, Self::END);
        Self::replace(config, Some(&body))
    }

    /// 删除唯一托管块。
    pub fn disable(config: &str) -> Result<String, ProtocolError> {
        Self::replace(config, None)
    }

    fn replace(config: &str, body: Option<&str>) -> Result<String, ProtocolError> {
        let begins = config.match_indices(Self::BEGIN).count();
        let ends = config.match_indices(Self::END).count();
        if begins > 1 || ends > 1 || begins != ends {
            return Err(ProtocolError::ConflictingConfig);
        }
        if begins == 0 {
            return Ok(match body {
                Some(body) => format!("{}\n{}\n", config.trim_end(), body),
                None => config.to_owned(),
            });
        }
        let start = config.find(Self::BEGIN).expect("marker count checked");
        let end = config[start..]
            .find(Self::END)
            .expect("marker count checked")
            + start
            + Self::END.len();
        let mut result = String::with_capacity(config.len());
        result.push_str(&config[..start]);
        let has_body = body.is_some();
        if let Some(body) = body {
            result.push_str(body);
        }
        let suffix = &config[end..];
        if !has_body && result.ends_with('\n') && suffix.starts_with('\n') {
            result.push_str(&suffix[1..]);
        } else {
            result.push_str(suffix);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonlCodec, NotifyConfig, ProtocolError, RpcEnvelope};
    use serde_json::json;

    #[test]
    fn jsonl_codec_accepts_one_bounded_message() {
        let codec = JsonlCodec::new(1024);
        let envelope = codec
            .decode(
                br#"{"method":"ping","id":1}
"#,
            )
            .unwrap();
        assert_eq!(envelope.method.as_deref(), Some("ping"));
        assert_eq!(envelope.id, Some(json!(1)));
    }

    #[test]
    fn jsonl_codec_rejects_multiple_or_oversized_frames() {
        let codec = JsonlCodec::new(32);
        assert!(matches!(
            codec.decode(
                br#"{}
{}
"#
            ),
            Err(ProtocolError::MultipleFrames)
        ));
        assert!(matches!(
            codec.decode(&[b'{'; 33]),
            Err(ProtocolError::FrameTooLarge { .. })
        ));
    }

    #[test]
    fn unknown_server_request_gets_method_not_found_response() {
        let request = RpcEnvelope::request(json!(7), "unexpected", json!({}));
        let response = request.method_not_found();
        assert_eq!(response.id, Some(json!(7)));
        assert_eq!(response.error.as_ref().unwrap().code, -32601);
    }

    #[test]
    fn notify_config_replaces_only_owned_block() {
        let config = "before\n# user setting\n";
        let updated = NotifyConfig::enable(config, "codex notify").unwrap();
        assert!(updated.contains(NotifyConfig::BEGIN));
        assert!(updated.contains("codex notify"));
        let disabled = NotifyConfig::disable(&updated).unwrap();
        assert_eq!(disabled, config);
    }

    #[test]
    fn notify_config_detects_conflicting_markers() {
        let config = format!(
            "{}\nold\n{}\n{}\n",
            NotifyConfig::BEGIN,
            NotifyConfig::END,
            NotifyConfig::END
        );
        assert!(matches!(
            NotifyConfig::disable(&config),
            Err(ProtocolError::ConflictingConfig)
        ));
    }
}
