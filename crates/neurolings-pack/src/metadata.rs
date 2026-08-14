//! 桌宠元数据（info.json）的读写。

use serde::{Deserialize, Serialize};

use crate::error::{PackError, Result};

/// 单个桌宠模板的元数据。
///
/// info.json 中缺失的字段一律按空字符串处理。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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

impl MascotMetadata {
    /// 读取 JSON 对象的字符串字段；缺失或非字符串时返回空串。
    fn string_field(object: &serde_json::Map<String, serde_json::Value>, key: &str) -> String {
        match object.get(key) {
            Some(serde_json::Value::String(value)) => value.clone(),
            _ => String::new(),
        }
    }
}

/// 把 info.json 字节解析为 [`MascotMetadata`]。
///
/// 错误语义：
/// - JSON 损坏或不是对象 -> `"Invalid info.json"`
/// - name 缺失或为空 -> `"info.json must contain a non-empty name"`
pub fn metadata_from_json(bytes: &[u8]) -> Result<MascotMetadata> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| PackError::msg("Invalid info.json"))?;
    let serde_json::Value::Object(object) = value else {
        return Err(PackError::msg("Invalid info.json"));
    };
    let metadata = MascotMetadata {
        name: MascotMetadata::string_field(&object, "name")
            .trim()
            .to_string(),
        version: MascotMetadata::string_field(&object, "version"),
        description: MascotMetadata::string_field(&object, "description"),
        author: MascotMetadata::string_field(&object, "author"),
    };
    if metadata.name.is_empty() {
        return Err(PackError::msg("info.json must contain a non-empty name"));
    }
    Ok(metadata)
}

/// 序列化为带缩进的 JSON，字段顺序固定为
/// name / version / description / author。
pub fn metadata_to_json(metadata: &MascotMetadata) -> String {
    let object = serde_json::json!({
        "name": metadata.name,
        "version": metadata.version,
        "description": metadata.description,
        "author": metadata.author,
    });
    serde_json::to_string_pretty(&object).unwrap_or_else(|_| String::from("{}")) + "\n"
}

/// 内置默认桌宠的元数据。
pub fn default_metadata() -> MascotMetadata {
    MascotMetadata {
        name: "Default".to_string(),
        version: "1.0".to_string(),
        description: "Default mascot for the application.".to_string(),
        author: "pixelomer[https://github.com/pixelomer]".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_become_empty_strings() {
        let metadata = metadata_from_json(br#"{"name": "Foo"}"#).unwrap();
        assert_eq!(metadata.name, "Foo");
        assert_eq!(metadata.version, "");
        assert_eq!(metadata.description, "");
        assert_eq!(metadata.author, "");
    }

    #[test]
    fn name_is_trimmed() {
        let metadata = metadata_from_json(br#"{"name": "  Foo  "}"#).unwrap();
        assert_eq!(metadata.name, "Foo");
    }

    #[test]
    fn empty_name_is_rejected() {
        let error = metadata_from_json(br#"{"name": "   "}"#).unwrap_err();
        assert_eq!(error.to_string(), "info.json must contain a non-empty name");
    }

    #[test]
    fn malformed_json_is_rejected() {
        let error = metadata_from_json(b"not json").unwrap_err();
        assert_eq!(error.to_string(), "Invalid info.json");
        let error = metadata_from_json(b"[1, 2]").unwrap_err();
        assert_eq!(error.to_string(), "Invalid info.json");
    }

    #[test]
    fn json_round_trips() {
        let metadata = MascotMetadata {
            name: "Cerber".to_string(),
            version: "1.1".to_string(),
            description: "Cerber desk pet".to_string(),
            author: "someone".to_string(),
        };
        let encoded = metadata_to_json(&metadata);
        let decoded = metadata_from_json(encoded.as_bytes()).unwrap();
        assert_eq!(metadata, decoded);
    }

    #[test]
    fn non_string_fields_become_empty() {
        let metadata =
            metadata_from_json(br#"{"name": "Foo", "version": 2, "author": null}"#).unwrap();
        assert_eq!(metadata.version, "");
        assert_eq!(metadata.author, "");
    }
}
