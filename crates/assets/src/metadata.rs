use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const PORTABLE_PACKAGE_BASE_NAME_MAX_UTF8_BYTES: usize = 200;

/// mascot `info.json` 的元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MascotMetadata {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
}

impl Default for MascotMetadata {
    fn default() -> Self {
        Self {
            name: "Default".to_owned(),
            version: "1.0".to_owned(),
            description: "Default mascot for the application.".to_owned(),
            author: "pixelomer[https://github.com/pixelomer]".to_owned(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    #[error("invalid info.json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("info.json must contain a JSON object")]
    NotObject,
    #[error("info.json must contain a non-empty name")]
    EmptyName,
}

/// 解析 `info.json`，只接受 JSON object 和非空 name。
pub fn metadata_from_json(bytes: &[u8]) -> Result<MascotMetadata, MetadataError> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let object = value.as_object().ok_or(MetadataError::NotObject)?;
    let field = |name: &str| {
        object
            .get(name)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let metadata = MascotMetadata {
        name: field("name"),
        version: field("version"),
        description: field("description"),
        author: field("author"),
    };
    if metadata.name.trim().is_empty() {
        return Err(MetadataError::EmptyName);
    }
    Ok(MascotMetadata {
        name: metadata.name.trim().to_owned(),
        ..metadata
    })
}

/// 将元数据序列化为稳定的缩进 JSON。
pub fn metadata_to_json(metadata: &MascotMetadata) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec_pretty(metadata).map(|mut bytes| {
        bytes.push(b'\n');
        bytes
    })
}

/// 清洗为可跨平台使用的 `.mascot` 基名。
pub fn sanitized_package_base_name(name: &str) -> String {
    let invalid = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    let mut result: String = name
        .trim()
        .chars()
        .map(|character| {
            if invalid.contains(&character) || character.is_control() {
                '_'
            } else {
                character
            }
        })
        .collect();
    result = result.trim().to_owned();
    while result.ends_with('.') {
        result.pop();
    }
    if result.is_empty() {
        "Mascot".to_owned()
    } else {
        result
    }
}

/// 判断名称是否能安全用于跨平台 mascot 包名。
pub fn is_valid_package_name(name: &str) -> bool {
    if name.trim().is_empty() {
        return false;
    }
    let base_name = sanitized_package_base_name(name);
    if base_name.len() > PORTABLE_PACKAGE_BASE_NAME_MAX_UTF8_BYTES {
        return false;
    }
    let device_name = base_name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(device_name.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return false;
    }
    if device_name.len() == 4
        && (device_name.starts_with("COM") || device_name.starts_with("LPT"))
        && matches!(device_name.as_bytes()[3], b'1'..=b'9')
    {
        return false;
    }
    true
}

/// 返回 storage 目录中对应 mascot 名称的包路径。
pub fn package_path_for_name(storage_path: impl AsRef<Path>, name: &str) -> PathBuf {
    storage_path
        .as_ref()
        .join(format!("{}.mascot", sanitized_package_base_name(name)))
}

/// 返回 cache 根目录中对应 mascot 名称的解包目录。
pub fn cache_path_for_name(cache_root_path: impl AsRef<Path>, name: &str) -> PathBuf {
    cache_root_path
        .as_ref()
        .join(sanitized_package_base_name(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_serializes_metadata() {
        let metadata = metadata_from_json(
            br#"{"name":"  Fox  ","version":"1","description":"d","author":"a"}"#,
        )
        .unwrap();
        assert_eq!(metadata.name, "Fox");
        let json = String::from_utf8(metadata_to_json(&metadata).unwrap()).unwrap();
        assert!(json.contains("\"name\": \"Fox\""));
    }

    #[test]
    fn rejects_missing_or_empty_name() {
        assert!(metadata_from_json(br#"{"version":"1"}"#).is_err());
        assert!(metadata_from_json(br#"{"name":"  "}"#).is_err());
        assert!(metadata_from_json(br#"[]"#).is_err());
    }

    #[test]
    fn treats_non_string_optional_fields_as_empty() {
        let metadata = metadata_from_json(br#"{"name":"Fox","version":3}"#).unwrap();
        assert_eq!(metadata.version, "");
    }

    #[test]
    fn sanitizes_names_and_rejects_reserved_devices() {
        assert_eq!(sanitized_package_base_name("  A/B... "), "A_B");
        assert_eq!(sanitized_package_base_name("***"), "___");
        assert!(!is_valid_package_name("CON"));
        assert!(!is_valid_package_name("com1.foo"));
        assert!(!is_valid_package_name(&"x".repeat(201)));
        assert!(is_valid_package_name("Friendly Fox"));
    }

    #[test]
    fn builds_storage_and_cache_paths_from_clean_name() {
        assert_eq!(
            package_path_for_name("storage", "Fox/A"),
            PathBuf::from("storage").join("Fox_A.mascot")
        );
        assert_eq!(
            cache_path_for_name("cache", "Fox/A"),
            PathBuf::from("cache").join("Fox_A")
        );
    }
}
