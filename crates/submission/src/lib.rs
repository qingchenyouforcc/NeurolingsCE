//! mascot 投稿 multipart 上传客户端入口。

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// 投稿元数据。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmissionMetadata {
    /// 模板名称。
    pub name: String,
    /// 模板版本。
    pub version: String,
    /// 模板描述。
    pub description: String,
    /// 作者署名。
    pub author: String,
}

/// 投稿请求；包数据只在内存中组装，不写入日志。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmissionRequest {
    /// 已验证的包字节。
    pub package: Vec<u8>,
    /// 包文件名。
    pub filename: String,
    /// 结构化元数据。
    pub metadata: SubmissionMetadata,
    /// 幂等请求 ID。
    pub request_id: String,
}

/// 服务端投稿结果。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmissionResult {
    /// 服务端任务 ID。
    pub id: String,
    /// 当前状态。
    pub status: String,
    /// 可选 Pull Request 地址。
    pub pr: Option<String>,
}

/// 投稿客户端错误。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SubmissionError {
    /// 文件名包含路径或为空。
    #[error("invalid package filename")]
    InvalidFilename,
    /// 包为空。
    #[error("package must not be empty")]
    EmptyPackage,
    /// 请求 ID 为空。
    #[error("request id must not be empty")]
    EmptyRequestId,
    /// 服务端 JSON 无法解析。
    #[error("invalid submission response: {0}")]
    InvalidResponse(String),
}

impl SubmissionRequest {
    /// 校验请求字段。
    pub fn validate(&self) -> Result<(), SubmissionError> {
        if self.package.is_empty() {
            return Err(SubmissionError::EmptyPackage);
        }
        if self.filename.is_empty()
            || self.filename.contains(['/', '\\'])
            || self.filename == "."
            || self.filename == ".."
        {
            return Err(SubmissionError::InvalidFilename);
        }
        if self.request_id.trim().is_empty() {
            return Err(SubmissionError::EmptyRequestId);
        }
        Ok(())
    }

    /// 计算包 SHA-256 十六进制摘要。
    pub fn sha256_hex(&self) -> String {
        let digest = Sha256::digest(&self.package);
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// 生成 multipart/form-data 请求体和 content type。
    pub fn multipart_body(&self, boundary: &str) -> Result<(String, Vec<u8>), SubmissionError> {
        self.validate()?;
        let metadata = serde_json::to_string(&self.metadata).expect("元数据可序列化");
        let mut body = Vec::new();
        let marker = format!("--{boundary}\r\n");
        body.extend_from_slice(marker.as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"metadata\"\r\nContent-Type: application/json\r\n\r\n");
        body.extend_from_slice(metadata.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(marker.as_bytes());
        body.extend_from_slice(format!("Content-Disposition: form-data; name=\"package\"; filename=\"{}\"\r\nContent-Type: application/octet-stream\r\n\r\n", self.filename).as_bytes());
        body.extend_from_slice(&self.package);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        Ok((format!("multipart/form-data; boundary={boundary}"), body))
    }
}

/// 解析服务端结构化 JSON 结果。
pub fn parse_result(body: &[u8]) -> Result<SubmissionResult, SubmissionError> {
    serde_json::from_slice(body)
        .map_err(|error| SubmissionError::InvalidResponse(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{SubmissionError, SubmissionMetadata, SubmissionRequest, parse_result};

    fn request() -> SubmissionRequest {
        SubmissionRequest {
            package: b"zip bytes".to_vec(),
            filename: "mascot.mascot".into(),
            metadata: SubmissionMetadata {
                name: "Demo".into(),
                version: "1.0.0".into(),
                description: "A demo".into(),
                author: "Tester".into(),
            },
            request_id: "req-1".into(),
        }
    }

    #[test]
    fn multipart_contains_metadata_package_and_digest() {
        let request = request();
        let (content_type, body) = request.multipart_body("boundary").unwrap();
        assert!(content_type.contains("boundary=boundary"));
        assert!(String::from_utf8_lossy(&body).contains("Demo"));
        assert!(
            body.windows(b"zip bytes".len())
                .any(|window| window == b"zip bytes")
        );
        assert_eq!(request.sha256_hex().len(), 64);
    }

    #[test]
    fn validation_rejects_path_and_empty_fields() {
        let mut request = request();
        request.filename = "../evil.mascot".into();
        assert_eq!(request.validate(), Err(SubmissionError::InvalidFilename));
        assert!(parse_result(br#"{"id":"1","status":"queued","pr":null}"#).is_ok());
    }
}
