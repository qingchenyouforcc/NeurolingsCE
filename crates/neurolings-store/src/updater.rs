//! 安全更新检查器：解析静态发布清单、判定更新是否适用，
//! 并在安装前校验下载资产的 SHA-256。

use serde::Deserialize;

use crate::index::{is_newer_version, is_valid_version};
use crate::network;

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct UpdateAsset {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub content_type: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct UpdateManifest {
    #[serde(default)]
    pub schema: i32,
    #[serde(default)]
    pub app: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub published_at: String,
    #[serde(default)]
    pub mandatory: bool,
    #[serde(default)]
    pub min_supported_version: String,
    #[serde(default)]
    pub release_page: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub assets: std::collections::HashMap<String, UpdateAsset>,
}

/// 当前平台/架构的资产键（与清单 assets 的键对应）。
pub fn current_asset_key() -> &'static str {
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    if cfg!(target_os = "windows") {
        if arch == "aarch64" {
            "windows-aarch64"
        } else {
            "windows-x86_64"
        }
    } else if cfg!(target_os = "macos") {
        if arch == "aarch64" {
            "macos-aarch64"
        } else {
            "macos-x86_64"
        }
    } else {
        if arch == "aarch64" {
            "linux-aarch64"
        } else {
            "linux-x86_64"
        }
    }
}

pub fn parse_manifest(bytes: &[u8]) -> Result<UpdateManifest, String> {
    let manifest: UpdateManifest = serde_json::from_slice(bytes)
        .map_err(|e| format!("Failed to parse update manifest: {e}"))?;
    if manifest.schema < 1 {
        return Err("Update manifest schema is missing or invalid".into());
    }
    if !is_valid_version(&manifest.version) {
        return Err("Update manifest version is invalid".into());
    }
    Ok(manifest)
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateDecision {
    /// 无可用或不适用的更新。
    UpToDate,
    /// 有可用更新。
    Available(UpdateAsset),
    /// 当前版本低于 min_supported_version：无论常规比较结果如何，
    /// 更新都是强制性的。
    Mandatory(UpdateAsset),
}

/// 依据清单判定 current_version 是否应更新。
pub fn decide(current_version: &str, manifest: &UpdateManifest) -> UpdateDecision {
    if !is_valid_version(current_version) {
        return UpdateDecision::UpToDate;
    }
    let key = current_asset_key();
    let Some(asset) = manifest.assets.get(key) else {
        return UpdateDecision::UpToDate;
    };
    if asset.url.is_empty() {
        return UpdateDecision::UpToDate;
    }

    // 低于最低支持版本 → 强制更新。
    if !manifest.min_supported_version.is_empty()
        && is_valid_version(&manifest.min_supported_version)
        && !is_newer_version(current_version, &manifest.min_supported_version)
        && current_version != manifest.min_supported_version
    {
        return UpdateDecision::Mandatory(asset.clone());
    }

    if manifest.mandatory && is_newer_version(&manifest.version, current_version) {
        return UpdateDecision::Mandatory(asset.clone());
    }

    if is_newer_version(&manifest.version, current_version) {
        UpdateDecision::Available(asset.clone())
    } else {
        UpdateDecision::UpToDate
    }
}

/// 从 url 拉取清单（感知 ETag）并返回。
pub fn fetch_manifest(url: &str, timeout_ms: u64) -> Result<UpdateManifest, String> {
    let response = network::fetch_index(url, "", "", timeout_ms);
    if !response.ok {
        return Err(format!("{}: {}", response.error_code, response.error));
    }
    parse_manifest(&response.body)
}

/// 下载资产到 destination 并校验 SHA-256；清单声明的校验和
/// 不匹配时拒绝安装。
pub fn download_update(
    asset: &UpdateAsset,
    destination: &std::path::Path,
    timeout_ms: u64,
) -> Result<(), String> {
    network::download(&asset.url, destination, &asset.sha256, timeout_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_with(version: &str, min: &str, mandatory: bool) -> UpdateManifest {
        let mut assets = std::collections::HashMap::new();
        assets.insert(
            current_asset_key().to_string(),
            UpdateAsset {
                name: "pkg".into(),
                url: "https://example.com/pkg".into(),
                sha256: "a".repeat(64),
                size: 1,
                content_type: "application/octet-stream".into(),
            },
        );
        UpdateManifest {
            schema: 1,
            app: "NeurolingsCE".into(),
            channel: "stable".into(),
            version: version.into(),
            tag: format!("v{version}"),
            min_supported_version: min.into(),
            mandatory,
            assets,
            ..Default::default()
        }
    }

    #[test]
    fn parses_example_schema() {
        let json = r#"{"schema":1,"app":"NeurolingsCE","version":"0.5.1",
            "min_supported_version":"0.2.0",
            "assets":{"windows-x86_64":{"name":"x","url":"https://e/x","sha256":""}}}"#;
        let m = parse_manifest(json.as_bytes()).unwrap();
        assert_eq!(m.version, "0.5.1");
    }

    #[test]
    fn decision_newer_available() {
        let m = manifest_with("2.0.0", "", false);
        assert!(matches!(decide("1.0.0", &m), UpdateDecision::Available(_)));
        assert!(matches!(decide("2.0.0", &m), UpdateDecision::UpToDate));
        assert!(matches!(decide("3.0.0", &m), UpdateDecision::UpToDate));
    }

    #[test]
    fn decision_below_min_is_mandatory() {
        let m = manifest_with("2.0.0", "1.5.0", false);
        assert!(matches!(decide("1.0.0", &m), UpdateDecision::Mandatory(_)));
        assert!(matches!(decide("1.5.0", &m), UpdateDecision::Available(_)));
    }

    #[test]
    fn decision_mandatory_flag() {
        let m = manifest_with("2.0.0", "", true);
        assert!(matches!(decide("1.0.0", &m), UpdateDecision::Mandatory(_)));
    }

    #[test]
    fn rejects_invalid_manifest() {
        assert!(parse_manifest(b"{}").is_err());
        assert!(parse_manifest(br#"{"schema":1,"version":"bad"}"#).is_err());
    }
}
