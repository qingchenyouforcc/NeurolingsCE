//! 桌宠组合：命名保存/恢复当前运行的桌宠集合。
//!
//! 持久化为应用数据目录旁的 combinations.json：
//! - 已保存组合以 epoch 毫秒字符串为 id，追加式存储（同名不去重，与原版一致）；
//! - 关闭前的最后状态保存在专用字段 last_before_close（空组合也写入）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::Local;
use serde::{Deserialize, Serialize};

/// 启动恢复模式取值（与原版 QSettings 键值一致）。
pub const RESTORE_MODE_NONE: &str = "none";
pub const RESTORE_MODE_LAST: &str = "last";
pub const RESTORE_MODE_SAVED: &str = "saved";
/// "关闭前组合"在命令层的固定 id（与原版列表首项一致）。
pub const LAST_BEFORE_CLOSE_ID: &str = "lastBeforeClose";

/// 单条目与组合总量的安全限位（对齐原版 kMaxMascotsPerEntry/kMaxMascotsPerCombination）。
pub const MAX_MASCOTS_PER_ENTRY: u32 = 50;
pub const MAX_MASCOTS_PER_COMBINATION: u32 = 200;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CombinationMember {
    pub name: String,
    pub count: u32,
}

/// 一条已保存的组合：id 主键 + 用户命名 + 内容。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SavedCombination {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub saved_at: String,
    #[serde(default)]
    pub mascots: Vec<CombinationMember>,
}

impl SavedCombination {
    pub fn total(&self) -> u32 {
        self.mascots.iter().map(|m| m.count).sum()
    }
}

/// 旧版存储格式：{"组合名": {"members": [{"template": "..."}]}}。
/// 仅用于读取兼容，加载后立即转换为新格式。
#[derive(Debug, Deserialize)]
struct LegacyCombination {
    members: Vec<LegacyMember>,
}

#[derive(Debug, Deserialize)]
struct LegacyMember {
    template: String,
}

/// combinations 字段的新旧两种形态（数组=新格式，对象=旧格式）。
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CombinationsRaw {
    Current(Vec<SavedCombination>),
    Legacy(HashMap<String, LegacyCombination>),
}

impl Default for CombinationsRaw {
    fn default() -> Self {
        CombinationsRaw::Current(Vec::new())
    }
}

#[derive(Debug, Default, Deserialize)]
struct StoreFile {
    #[serde(default)]
    combinations: CombinationsRaw,
    #[serde(default)]
    last_before_close: Option<SavedCombination>,
}

/// 落盘形态：旧格式在加载时已归一化。
#[derive(Debug, Serialize)]
struct StoreOut<'a> {
    combinations: &'a [SavedCombination],
    last_before_close: Option<&'a SavedCombination>,
}

/// 把模板名列表按首次出现顺序聚合为 {name, count} 成员表。
pub fn aggregate(templates: impl IntoIterator<Item = String>) -> Vec<CombinationMember> {
    let mut order: Vec<String> = Vec::new();
    let mut counts: HashMap<String, u32> = HashMap::new();
    for name in templates {
        if !counts.contains_key(&name) {
            order.push(name.clone());
        }
        *counts.entry(name).or_insert(0) += 1;
    }
    order
        .into_iter()
        .map(|name| {
            let count = counts[&name];
            CombinationMember { name, count }
        })
        .collect()
}

/// 本地时间 ISO 8601 时间戳（对齐原版 savedAt 语义）。
fn now_stamp() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

/// 生成不与现有末条冲突的 epoch 毫秒 id。
fn next_id(existing: &[SavedCombination]) -> String {
    let mut ms = Local::now().timestamp_millis();
    if let Some(last) = existing.last()
        && let Ok(last_ms) = last.id.parse::<i64>()
        && ms <= last_ms
    {
        ms = last_ms + 1;
    }
    ms.to_string()
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

    fn load(&self) -> Vec<SavedCombination> {
        let file: Option<StoreFile> = std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok());
        let Some(file) = file else {
            return Vec::new();
        };
        // 旧格式（按名索引的 map）转换为追加式数组：保持原键排序以稳定生成 id。
        match file.combinations {
            CombinationsRaw::Current(list) => list,
            CombinationsRaw::Legacy(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                let mut out: Vec<SavedCombination> = Vec::new();
                for key in keys {
                    let Some(entry) = map.get(key) else {
                        continue;
                    };
                    let mascots = aggregate(
                        entry
                            .members
                            .iter()
                            .map(|m| m.template.clone())
                            .collect::<Vec<String>>(),
                    );
                    let id = next_id(&out);
                    out.push(SavedCombination {
                        id,
                        name: key.clone(),
                        saved_at: String::new(),
                        mascots,
                    });
                }
                let _ = self.save_all(&out, None);
                out
            }
        }
    }

    fn load_last(&self) -> Option<SavedCombination> {
        let file: StoreFile = std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())?;
        file.last_before_close
    }

    fn save_all(
        &self,
        combinations: &[SavedCombination],
        last: Option<&SavedCombination>,
    ) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let out = StoreOut {
            combinations,
            last_before_close: last,
        };
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }

    /// 全量保存（同时保留关闭前状态）。
    fn save_with_last(
        &self,
        combinations: &[SavedCombination],
        last: Option<&SavedCombination>,
    ) -> Result<(), String> {
        self.save_all(combinations, last)
    }

    /// 已保存组合列表（按保存顺序，不含关闭前状态）。
    pub fn list(&self) -> Vec<SavedCombination> {
        self.load()
    }

    /// 按 id 查询；id 为固定值 LAST_BEFORE_CLOSE_ID 时返回关闭前状态。
    pub fn get(&self, id: &str) -> Option<SavedCombination> {
        if id == LAST_BEFORE_CLOSE_ID {
            return self.load_last();
        }
        self.load().into_iter().find(|c| c.id == id)
    }

    /// 追加保存一条组合，返回生成的 id。
    pub fn save(&self, name: &str, mascots: Vec<CombinationMember>) -> Result<String, String> {
        let mut combinations = self.load();
        let id = next_id(&combinations);
        combinations.push(SavedCombination {
            id: id.clone(),
            name: name.to_string(),
            saved_at: now_stamp(),
            mascots,
        });
        let last = self.load_last();
        self.save_with_last(&combinations, last.as_ref())?;
        Ok(id)
    }

    /// 按 id 删除；固定项 LAST_BEFORE_CLOSE_ID 不可删除。
    pub fn delete(&self, id: &str) -> Result<bool, String> {
        if id == LAST_BEFORE_CLOSE_ID {
            return Ok(false);
        }
        let mut combinations = self.load();
        let before = combinations.len();
        combinations.retain(|c| c.id != id);
        if combinations.len() == before {
            return Ok(false);
        }
        let last = self.load_last();
        self.save_with_last(&combinations, last.as_ref())?;
        Ok(true)
    }

    /// 写入"关闭前组合"（空组合也写入：恢复 last 时恢复 0 只，与原版一致）。
    pub fn save_last_before_close(&self, mascots: Vec<CombinationMember>) -> Result<(), String> {
        let combinations = self.load();
        let last = SavedCombination {
            id: LAST_BEFORE_CLOSE_ID.to_string(),
            name: String::new(),
            saved_at: now_stamp(),
            mascots,
        };
        self.save_with_last(&combinations, Some(&last))
    }

    /// 迁移合并：把原版 QSettings 中的组合数据并入当前存储。
    /// saved 为原版 combinations/saved 的 JSON 数组，last 为 lastBeforeClose 的 JSON 对象；
    /// 按 id 去重追加，已有数据不会被覆盖。
    pub fn merge_from_ce(&self, saved: &serde_json::Value, last: &serde_json::Value) {
        let mut combinations = self.load();
        if let Some(entries) = saved.as_array() {
            for entry in entries {
                let id = entry
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if id.is_empty() || combinations.iter().any(|c| c.id == id) {
                    continue;
                }
                let body = entry.get("combination").cloned().unwrap_or_default();
                let mascots = parse_ce_mascots(&body);
                if mascots.is_empty() {
                    continue;
                }
                let name = entry
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let saved_at = body
                    .get("savedAt")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                combinations.push(SavedCombination {
                    id,
                    name,
                    saved_at,
                    mascots,
                });
            }
        }
        let mut last_entry: Option<SavedCombination> = self.load_last();
        if last_entry.is_none()
            && let Some(mascots) = parse_ce_mascots_opt(last)
            && last.as_object().is_some_and(|o| !o.is_empty())
        {
            last_entry = Some(SavedCombination {
                id: LAST_BEFORE_CLOSE_ID.to_string(),
                name: String::new(),
                saved_at: last
                    .get("savedAt")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                mascots,
            });
        }
        let _ = self.save_with_last(&combinations, last_entry.as_ref());
    }
}

/// 解析原版 combination 对象中的 mascots 数组；缺失或为空返回空表。
fn parse_ce_mascots(body: &serde_json::Value) -> Vec<CombinationMember> {
    parse_ce_mascots_opt(body).unwrap_or_default()
}

fn parse_ce_mascots_opt(body: &serde_json::Value) -> Option<Vec<CombinationMember>> {
    let arr = body.get("mascots")?.as_array()?;
    let out = arr
        .iter()
        .filter_map(|m| {
            let name = m.get("name")?.as_str()?.to_string();
            let count = m
                .get("count")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            (count > 0).then(|| CombinationMember {
                name,
                count: count.min(MAX_MASCOTS_PER_ENTRY as i64) as u32,
            })
        })
        .collect();
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn save_list_get_delete_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = CombinationStore::new(dir.path());
        let id = store
            .save(
                "my-group",
                vec![CombinationMember {
                    name: "Neuron".into(),
                    count: 2,
                }],
            )
            .unwrap();
        let list = store.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);
        assert_eq!(list[0].name, "my-group");
        assert_eq!(list[0].mascots[0].count, 2);
        let combo = store.get(&id).unwrap();
        assert_eq!(combo.total(), 2);
        assert!(store.delete(&id).unwrap());
        assert!(store.list().is_empty());
        assert!(!store.delete(&id).unwrap());
    }

    #[test]
    fn same_name_combinations_do_not_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let store = CombinationStore::new(dir.path());
        let first = store
            .save(
                "dup",
                vec![CombinationMember {
                    name: "A".into(),
                    count: 1,
                }],
            )
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = store
            .save(
                "dup",
                vec![CombinationMember {
                    name: "B".into(),
                    count: 1,
                }],
            )
            .unwrap();
        assert_ne!(first, second);
        assert_eq!(store.list().len(), 2);
    }

    #[test]
    fn last_before_close_writes_even_when_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = CombinationStore::new(dir.path());
        store
            .save_last_before_close(vec![CombinationMember {
                name: "A".into(),
                count: 3,
            }])
            .unwrap();
        assert_eq!(store.get(LAST_BEFORE_CLOSE_ID).map(|c| c.total()), Some(3));
        // 空组合也写入：恢复语义与原版一致（下次恢复 0 只）。
        store.save_last_before_close(vec![]).unwrap();
        assert_eq!(store.get(LAST_BEFORE_CLOSE_ID).map(|c| c.total()), Some(0));
        // 固定项不可删除。
        assert!(!store.delete(LAST_BEFORE_CLOSE_ID).unwrap());
    }

    #[test]
    fn legacy_map_format_is_upgraded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("combinations.json");
        std::fs::write(
            &path,
            r#"{"combinations":{"old-group":{"members":[{"template":"Neuron"},{"template":"Neuron"}]}}}"#,
        )
        .unwrap();
        let store = CombinationStore::new(dir.path());
        let list = store.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "old-group");
        assert_eq!(list[0].mascots[0].name, "Neuron");
        assert_eq!(list[0].mascots[0].count, 2);
    }

    #[test]
    fn merge_from_ce_dedupes_and_keeps_existing() {
        let dir = tempfile::tempdir().unwrap();
        let store = CombinationStore::new(dir.path());
        let ce_saved = json!([
            {
                "id": "1000",
                "name": "From CE",
                "combination": {
                    "version": 1,
                    "savedAt": "2026-08-01T10:00:00+08:00",
                    "mascots": [{"name": "Neuron", "count": 2}]
                }
            }
        ]);
        let ce_last = json!({
            "version": 1,
            "savedAt": "2026-08-02T10:00:00+08:00",
            "mascots": [{"name": "Vedaling", "count": 1}]
        });
        store.merge_from_ce(&ce_saved, &ce_last);
        store.merge_from_ce(&ce_saved, &ce_last); // 幂等
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.list()[0].id, "1000");
        assert_eq!(
            store
                .get(LAST_BEFORE_CLOSE_ID)
                .map(|c| c.mascots[0].name.clone()),
            Some("Vedaling".to_string())
        );
    }
}
