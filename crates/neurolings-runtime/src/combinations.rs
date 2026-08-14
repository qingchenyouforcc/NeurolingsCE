//! 桌宠组合：命名保存/恢复当前运行的桌宠集合。
//! 持久化为存储目录旁的 combinations.json；
//! 关闭前的最后状态保存在保留名 LAST_BEFORE_CLOSE 下。

/// 保留组合名：关闭前的桌宠组合，供静默启动恢复。
pub const LAST_BEFORE_CLOSE: &str = "__last_before_close__";

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CombinationMember {
    pub template: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Combination {
    pub members: Vec<CombinationMember>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    combinations: HashMap<String, Combination>,
}

pub struct CombinationStore {
    path: PathBuf,
}

impl CombinationStore {
    pub fn new(storage_root: &Path) -> Self {
        Self {
            path: storage_root.join("combinations.json"),
        }
    }

    fn load(&self) -> Store {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(&self, store: &Store) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_string_pretty(store).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }

    pub fn list(&self) -> Vec<String> {
        let mut names: Vec<String> = self.load().combinations.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn get(&self, name: &str) -> Option<Combination> {
        self.load().combinations.get(name).cloned()
    }

    pub fn save_combination(&self, name: &str, templates: Vec<String>) -> Result<(), String> {
        let mut store = self.load();
        store.combinations.insert(
            name.to_string(),
            Combination {
                members: templates
                    .into_iter()
                    .map(|template| CombinationMember { template })
                    .collect(),
            },
        );
        self.save(&store)
    }

    pub fn delete_combination(&self, name: &str) -> Result<(), String> {
        let mut store = self.load();
        store.combinations.remove(name);
        self.save(&store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_list_delete_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = CombinationStore::new(dir.path());
        store
            .save_combination("my-group", vec!["Default".into(), "Neuron".into()])
            .unwrap();
        assert_eq!(store.list(), vec!["my-group".to_string()]);
        let combo = store.get("my-group").unwrap();
        assert_eq!(combo.members.len(), 2);
        assert_eq!(combo.members[0].template, "Default");
        store.delete_combination("my-group").unwrap();
        assert!(store.list().is_empty());
    }
}
