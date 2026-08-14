//! 商店索引的磁盘原子缓存。

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CachedIndex {
    pub body: Vec<u8>,
    pub etag: String,
    pub last_modified: String,
}

pub struct StoreCache {
    root: PathBuf,
}

impl StoreCache {
    pub fn new(cache_root: impl Into<PathBuf>) -> Self {
        Self {
            root: cache_root.into(),
        }
    }

    pub fn index_file_path(&self) -> PathBuf {
        self.root.join("index.json")
    }

    pub fn previous_index_file_path(&self) -> PathBuf {
        self.root.join("index.previous.json")
    }

    pub fn metadata_file_path(&self) -> PathBuf {
        self.root.join("index.meta.json")
    }

    pub fn load_index(&self) -> Option<CachedIndex> {
        self.read_index_file(&self.index_file_path())
    }

    pub fn load_previous_index(&self) -> Option<CachedIndex> {
        self.read_index_file(&self.previous_index_file_path())
    }

    fn read_index_file(&self, path: &Path) -> Option<CachedIndex> {
        let body = fs::read(path).ok()?;
        if body.is_empty() {
            return None;
        }
        let meta = fs::read_to_string(self.metadata_file_path()).unwrap_or_default();
        let meta: serde_json::Value = serde_json::from_str(&meta).unwrap_or_default();
        Some(CachedIndex {
            body,
            etag: meta
                .get("etag")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            last_modified: meta
                .get("lastModified")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
    }

    /// 原子写入：保留旧索引，新索引写入临时文件后改名生效。
    pub fn save_index(&self, index: &CachedIndex) -> Result<(), String> {
        fs::create_dir_all(&self.root).map_err(|e| format!("cache dir: {e}"))?;

        // 替换前把当前索引轮转为 previous。
        if self.index_file_path().exists() {
            let _ = fs::copy(self.index_file_path(), self.previous_index_file_path());
        }

        let tmp = self.root.join("index.json.tmp");
        fs::write(&tmp, &index.body).map_err(|e| format!("write tmp: {e}"))?;
        fs::rename(&tmp, self.index_file_path()).map_err(|e| format!("rename: {e}"))?;

        let meta = serde_json::json!({
            "etag": index.etag,
            "lastModified": index.last_modified,
        });
        let meta_tmp = self.root.join("index.meta.json.tmp");
        fs::write(&meta_tmp, meta.to_string()).map_err(|e| format!("meta: {e}"))?;
        fs::rename(&meta_tmp, self.metadata_file_path())
            .map_err(|e| format!("meta rename: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_roundtrip_with_rotation() {
        let dir = tempfile::tempdir().unwrap();
        let cache = StoreCache::new(dir.path());

        let first = CachedIndex {
            body: b"first".to_vec(),
            etag: "e1".into(),
            last_modified: "lm1".into(),
        };
        cache.save_index(&first).unwrap();
        assert_eq!(cache.load_index().unwrap(), first);
        assert!(cache.load_previous_index().is_none());

        let second = CachedIndex {
            body: b"second".to_vec(),
            etag: "e2".into(),
            last_modified: "lm2".into(),
        };
        cache.save_index(&second).unwrap();
        assert_eq!(cache.load_index().unwrap(), second);
        // 旧索引此时已可用作回退。
        assert_eq!(cache.load_previous_index().unwrap().body, b"first");
    }
}
