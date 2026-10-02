//! mascot 商店索引、缓存、下载和安装协调入口。

use std::path::{Path, PathBuf};

use assets::{
    ArchiveReader, MascotMetadata, extract_archive, metadata_from_json, open_archive,
    package_path_for_name,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 商店索引中的模板条目。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreEntry {
    /// 稳定模板 ID。
    pub id: i32,
    /// 模板名称。
    pub name: String,
    /// 当前版本。
    pub version: String,
    /// 简介。
    #[serde(default)]
    pub description: String,
    /// 标签。
    #[serde(default)]
    pub tags: Vec<String>,
    /// 下载地址。
    pub download_url: String,
}

/// 可搜索的商店索引。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreIndex {
    entries: Vec<StoreEntry>,
}

/// 商店错误。
#[derive(Debug, Error)]
pub enum StoreError {
    /// JSON 解析失败。
    #[error("invalid store index: {0}")]
    Json(#[from] serde_json::Error),
    /// schemaVersion 不是 1。
    #[error("unsupported store index schema")]
    UnsupportedSchema,
    /// 归档处理失败。
    #[error("mascot package failed: {0}")]
    Package(String),
    /// 目录操作失败。
    #[error("store I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

impl StoreIndex {
    /// 解析 schemaVersion=1 的索引并按 ID 排序。
    pub fn from_json(bytes: &[u8]) -> Result<Self, StoreError> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "schemaVersion")]
            schema_version: u32,
            entries: Vec<StoreEntry>,
        }
        let wire: Wire = serde_json::from_slice(bytes)?;
        if wire.schema_version != 1 {
            return Err(StoreError::UnsupportedSchema);
        }
        let mut entries = wire.entries;
        entries.sort_by_key(|entry| entry.id);
        Ok(Self { entries })
    }

    /// 返回全部条目。
    pub fn entries(&self) -> &[StoreEntry] {
        &self.entries
    }

    /// 按名称、描述或标签执行大小写不敏感搜索。
    pub fn search(&self, query: &str) -> Vec<StoreEntry> {
        let query = query.to_ascii_lowercase();
        self.entries
            .iter()
            .filter(|entry| {
                entry.name.to_ascii_lowercase().contains(&query)
                    || entry.description.to_ascii_lowercase().contains(&query)
                    || entry
                        .tags
                        .iter()
                        .any(|tag| tag.to_ascii_lowercase().contains(&query))
            })
            .cloned()
            .collect()
    }
}

/// 本地模板安装协调器。
#[derive(Clone, Debug)]
pub struct StoreInstaller {
    storage: PathBuf,
    cache: PathBuf,
}

impl StoreInstaller {
    /// 创建安装器并确保目标目录存在。
    pub fn new(storage: impl Into<PathBuf>, cache: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let installer = Self {
            storage: storage.into(),
            cache: cache.into(),
        };
        std::fs::create_dir_all(&installer.storage)?;
        std::fs::create_dir_all(&installer.cache)?;
        Ok(installer)
    }

    /// 安全验证、解包并保存 `.mascot` 原包，返回元数据与路径。
    pub fn install_bytes(&self, bytes: &[u8]) -> Result<(MascotMetadata, PathBuf), StoreError> {
        let mut reader =
            open_archive(bytes).map_err(|error| StoreError::Package(error.to_string()))?;
        let metadata = metadata_from_json(
            &reader
                .read_entry("info.json")
                .map_err(|error| StoreError::Package(error.to_string()))?,
        )
        .map_err(|error| StoreError::Package(error.to_string()))?;
        let package_path = package_path_for_name(&self.storage, &metadata.name);
        let cache_path = self
            .cache
            .join(assets::sanitized_package_base_name(&metadata.name));
        let temp = self.cache.join(format!(
            ".{}.part",
            assets::sanitized_package_base_name(&metadata.name)
        ));
        if temp.exists() {
            std::fs::remove_dir_all(&temp)?;
        }
        extract_archive(&mut reader, &temp)
            .map_err(|error| StoreError::Package(error.to_string()))?;
        if cache_path.exists() {
            std::fs::remove_dir_all(&cache_path)?;
        }
        std::fs::rename(&temp, &cache_path)?;
        std::fs::write(&package_path, bytes)?;
        Ok((metadata, package_path))
    }

    /// 返回模板存储目录。
    pub fn storage_path(&self) -> &Path {
        &self.storage
    }
}

#[cfg(test)]
mod tests {
    use super::{StoreIndex, StoreInstaller};
    use std::io::Write;

    #[test]
    fn parses_sorts_and_searches_store_index() {
        let index = StoreIndex::from_json(br#"{"schemaVersion":1,"entries":[{"id":2,"name":"Zed","version":"1","description":"cat","tags":["cute"],"download_url":"https://z"},{"id":1,"name":"Alpha","version":"1","description":"fox","tags":[],"download_url":"https://a"}]}"#).unwrap();
        assert_eq!(index.entries()[0].id, 1);
        assert_eq!(index.search("CUTE").len(), 1);
    }

    #[test]
    fn installs_validated_zip_to_storage_and_cache() {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("info.json", options).unwrap();
        writer
            .write_all(br#"{"name":"Fox","version":"1"}"#)
            .unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let root = std::env::temp_dir().join(format!("neurolingsce-store-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        let installer = StoreInstaller::new(root.join("storage"), root.join("cache")).unwrap();
        let (metadata, path) = installer.install_bytes(&bytes).unwrap();
        assert_eq!(metadata.name, "Fox");
        assert!(path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
