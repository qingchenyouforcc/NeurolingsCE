//! 桌宠投稿客户端：两阶段鉴权（GitHub 令牌换短时会话令牌）后
//! multipart 上传。

use std::fs;
use std::path::Path;
use std::time::Duration;

#[derive(Default)]
pub struct SubmissionResult {
    pub ok: bool,
    pub id: String,
    pub status: String,
    pub pr_url: String,
    pub pr_number: i64,
    pub error_code: String,
    pub error: String,
}

pub struct SubmissionClient {
    service_base_url: String,
    access_token: String,
    session_token: String,
}

fn join_url(base: &str, path: &str) -> String {
    if base.ends_with('/') {
        format!("{}{}", base, path)
    } else {
        format!("{}/{}", base, path)
    }
}

/// 最小化的 multipart/form-data 构造器。
struct Multipart {
    boundary: String,
    body: Vec<u8>,
}

impl Multipart {
    fn new() -> Self {
        Self {
            boundary: "----NeurolingsRSBoundary7d2f9a".to_string(),
            body: Vec::new(),
        }
    }

    fn add_text(&mut self, name: &str, value: &str) {
        self.body.extend_from_slice(
            format!(
                "--{}\r\nContent-Disposition: form-data; name=\"{}\"\r\n\r\n",
                self.boundary, name
            )
            .as_bytes(),
        );
        self.body.extend_from_slice(value.as_bytes());
        self.body.extend_from_slice(b"\r\n");
    }

    fn add_file(&mut self, name: &str, filename: &str, bytes: &[u8]) {
        self.body.extend_from_slice(
            format!(
                "--{}\r\nContent-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: application/octet-stream\r\n\r\n",
                self.boundary, name, filename
            )
            .as_bytes(),
        );
        self.body.extend_from_slice(bytes);
        self.body.extend_from_slice(b"\r\n");
    }

    fn finish(mut self) -> (String, Vec<u8>) {
        self.body
            .extend_from_slice(format!("--{}--\r\n", self.boundary).as_bytes());
        (
            format!("multipart/form-data; boundary={}", self.boundary),
            self.body,
        )
    }
}

impl SubmissionClient {
    pub fn new(service_base_url: String) -> Self {
        Self {
            service_base_url,
            access_token: String::new(),
            session_token: String::new(),
        }
    }

    pub fn set_access_token(&mut self, token: &str) {
        self.access_token = token.to_string();
        self.session_token.clear();
    }

    pub fn clear_session(&mut self) {
        self.session_token.clear();
    }

    pub fn session_token(&self) -> &str {
        &self.session_token
    }

    fn obtain_session_token(&mut self, timeout: Duration) -> Result<(), String> {
        let url = join_url(&self.service_base_url, "v1/auth/github");
        let response = ureq::post(&url)
            .timeout(timeout)
            .set("Authorization", &format!("Bearer {}", self.access_token))
            .set("Accept", "application/json")
            .send_string("")
            .map_err(|e| format!("auth failed: {e}"))?;
        let parsed: serde_json::Value = response
            .into_json()
            .map_err(|e| format!("auth parse failed: {e}"))?;
        let token = parsed
            .get("sessionToken")
            .or_else(|| parsed.get("session_token"))
            .or_else(|| parsed.get("token"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if token.is_empty() {
            return Err("The submission service returned no session token".into());
        }
        self.session_token = token.to_string();
        Ok(())
    }

    pub fn submit(
        &mut self,
        package_path: &Path,
        metadata_json: &str,
        idempotency_key: &str,
        timeout_ms: u64,
    ) -> SubmissionResult {
        let timeout = Duration::from_millis(timeout_ms);

        if self.access_token.is_empty() {
            return SubmissionResult {
                error_code: "not_authenticated".into(),
                error: "No GitHub access token configured".into(),
                ..Default::default()
            };
        }

        if self.session_token.is_empty()
            && let Err(e) = self.obtain_session_token(timeout)
        {
            return SubmissionResult {
                error_code: "auth_failed".into(),
                error: e,
                ..Default::default()
            };
        }

        let file_bytes = match fs::read(package_path) {
            Ok(b) => b,
            Err(e) => {
                return SubmissionResult {
                    error_code: "read_failed".into(),
                    error: format!("Could not read package: {e}"),
                    ..Default::default()
                };
            }
        };
        let filename = package_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "mascot.mascot".into());

        let mut multipart = Multipart::new();
        multipart.add_text("metadata", metadata_json);
        multipart.add_file("file", &filename, &file_bytes);
        let (content_type, body) = multipart.finish();

        let url = join_url(&self.service_base_url, "v1/submissions");
        let mut request = ureq::post(&url)
            .timeout(timeout)
            .set("Authorization", &format!("Bearer {}", self.session_token))
            .set("Content-Type", &content_type);
        if !idempotency_key.is_empty() {
            request = request.set("X-Idempotency-Key", idempotency_key);
        }

        match request.send_bytes(&body) {
            Ok(response) => {
                let parsed: serde_json::Value = match response.into_json() {
                    Ok(v) => v,
                    Err(e) => {
                        return SubmissionResult {
                            error_code: "parse_failed".into(),
                            error: format!("Could not parse submission response: {e}"),
                            ..Default::default()
                        };
                    }
                };
                SubmissionResult {
                    ok: true,
                    id: parsed
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    status: parsed
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    pr_url: parsed
                        .get("prUrl")
                        .or_else(|| parsed.get("pr_url"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    pr_number: parsed
                        .get("prNumber")
                        .or_else(|| parsed.get("pr_number"))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0),
                    ..Default::default()
                }
            }
            Err(ureq::Error::Status(status, response)) => {
                let message = response.into_string().unwrap_or_default();
                SubmissionResult {
                    error_code: "http_error".into(),
                    error: format!("Submission failed with status {status}: {message}"),
                    ..Default::default()
                }
            }
            Err(e) => SubmissionResult {
                error_code: "network_error".into(),
                error: e.to_string(),
                ..Default::default()
            },
        }
    }
}
