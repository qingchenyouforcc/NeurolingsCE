//! 商店网络层：带 ETag/Last-Modified 的条件索引拉取与 SHA-256 校验下载。

use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use sha2::{Digest, Sha256};

pub struct IndexResponse {
    pub ok: bool,
    pub not_modified: bool,
    pub body: Vec<u8>,
    pub etag: String,
    pub last_modified: String,
    pub error_code: String,
    pub error: String,
}

impl IndexResponse {
    fn failure(code: &str, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            not_modified: false,
            body: Vec::new(),
            etag: String::new(),
            last_modified: String::new(),
            error_code: code.to_string(),
            error: message.into(),
        }
    }
}

pub fn fetch_index(url: &str, etag: &str, last_modified: &str, timeout_ms: u64) -> IndexResponse {
    let mut request = ureq::get(url)
        .timeout(Duration::from_millis(timeout_ms))
        .set("Accept", "application/json");
    if !etag.is_empty() {
        request = request.set("If-None-Match", etag);
    }
    if !last_modified.is_empty() {
        request = request.set("If-Modified-Since", last_modified);
    }

    match request.call() {
        Ok(response) => {
            if response.status() == 304 {
                return IndexResponse {
                    ok: true,
                    not_modified: true,
                    ..IndexResponse::failure("", "")
                };
            }
            let resp_etag = response.header("ETag").unwrap_or("").to_string();
            let resp_lm = response.header("Last-Modified").unwrap_or("").to_string();
            let mut body = Vec::new();
            if response.into_reader().read_to_end(&mut body).is_err() {
                return IndexResponse::failure("read_failed", "Could not read index body");
            }
            IndexResponse {
                ok: true,
                not_modified: false,
                body,
                etag: resp_etag,
                last_modified: resp_lm,
                error_code: String::new(),
                error: String::new(),
            }
        }
        Err(ureq::Error::Status(status, _)) => IndexResponse::failure(
            "http_error",
            format!("Index request failed with status {status}"),
        ),
        Err(err) => IndexResponse::failure("network_error", err.to_string()),
    }
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex::encode(hasher.finalize()))
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// 下载到临时文件、校验 SHA-256 后原子改名生效；失败时返回错误文本
/// 并清理残留文件。
pub fn download(
    url: &str,
    destination: &Path,
    expected_sha256: &str,
    timeout_ms: u64,
) -> Result<(), String> {
    let partial = destination.with_extension("part");
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let response = ureq::get(url)
        .timeout(Duration::from_millis(timeout_ms))
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    if response.status() != 200 {
        return Err(format!("download failed with status {}", response.status()));
    }

    let mut body = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut body)
        .map_err(|e| format!("download read failed: {e}"))?;

    let digest = sha256_bytes(&body);
    if !expected_sha256.is_empty() && !digest.eq_ignore_ascii_case(expected_sha256) {
        return Err("downloaded file failed SHA-256 verification".into());
    }

    fs::write(&partial, &body).map_err(|e| format!("write partial: {e}"))?;
    fs::rename(&partial, destination).map_err(|e| {
        let _ = fs::remove_file(&partial);
        format!("finalize download: {e}")
    })?;
    Ok(())
}
