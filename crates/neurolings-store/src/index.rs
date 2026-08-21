//! 商店索引：模型、解析、版本比较与查询过滤。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct StoreMedia {
    #[serde(default)]
    pub url: String,
    #[serde(default = "default_neg_one")]
    pub size: i64,
    #[serde(default)]
    pub sha256: String,
}

fn default_neg_one() -> i64 {
    -1
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub minimum_neurolings_ce_version: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub maintainers: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub download: StoreMedia,
    #[serde(default)]
    pub icon: StoreMedia,
    #[serde(default)]
    pub previews: Vec<StoreMedia>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreIndex {
    #[serde(default)]
    pub schema_version: i32,
    #[serde(default)]
    pub registry: String,
    #[serde(default)]
    pub generated_at: String,
    #[serde(default, alias = "mascots")]
    pub entries: Vec<StoreEntry>,
}

pub fn is_valid_sha256(value: &str) -> bool {
    // 严格小写 hex，与原版 ^[0-9a-f]{64}$ 一致
    value.len() == 64
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// 受信任的下载源：与原版 MascotStoreIndex::isTrustedDownloadUrl 一致
/// - https:// 任意主机均信任
/// - http:// 仅 localhost / 127.0.0.1 / ::1 信任（本地调试）
pub fn is_trusted_download_url(url: &str, _registry: &str) -> bool {
    if let Some(rest) = url.strip_prefix("https://") {
        let host = rest.split('/').next().unwrap_or("");
        if !host.is_empty() {
            return true;
        }
    }
    if let Some(rest) = url.strip_prefix("http://") {
        let host = rest.split('/').next().unwrap_or("").to_ascii_lowercase();
        if host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]" {
            return true;
        }
    }
    false
}

/// 解析点分数字版本号（如 1.2.3）为分段；任一段非数字或为空返回 None。
pub fn parse_version(version: &str) -> Option<Vec<u32>> {
    let trimmed = version.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for seg in trimmed.split('.') {
        if seg.is_empty() {
            return None;
        }
        out.push(seg.parse::<u32>().ok()?);
    }
    Some(out)
}

pub fn is_valid_version(version: &str) -> bool {
    parse_version(version).is_some()
}

pub fn is_newer_version(candidate: &str, current: &str) -> bool {
    let (Some(cand), Some(cur)) = (parse_version(candidate), parse_version(current)) else {
        return false;
    };
    let len = cand.len().max(cur.len());
    for i in 0..len {
        let a = *cand.get(i).unwrap_or(&0);
        let b = *cur.get(i).unwrap_or(&0);
        if a != b {
            return a > b;
        }
    }
    false
}

impl StoreIndex {
    pub fn parse(bytes: &[u8]) -> Result<StoreIndex, String> {
        let mut parsed: StoreIndex = serde_json::from_slice(bytes)
            .map_err(|e| format!("Failed to parse store index: {e}"))?;
        if parsed.schema_version != 1 {
            return Err("Unsupported index schema version; expected 1".into());
        }
        // 与原版一致：generatedAt 必须有效（非空）
        if parsed.generated_at.trim().is_empty() {
            return Err("Index generatedAt is invalid".into());
        }
        for entry in &parsed.entries {
            let download_ok = !entry.download.url.is_empty()
                && is_trusted_download_url(&entry.download.url, &parsed.registry);
            if entry.id.is_empty()
                || entry.name.is_empty()
                || !is_valid_version(&entry.version)
                || entry.summary.is_empty()
                || !download_ok
                || !is_valid_sha256(&entry.download.sha256)
            {
                return Err("Index contains a mascot entry with invalid required fields".into());
            }
        }
        parsed.sort_entries();
        Ok(parsed)
    }

    pub fn sort_entries(&mut self) {
        self.entries.sort_by(|a, b| a.id.cmp(&b.id));
    }

    pub fn find_by_id(&self, id: &str) -> Option<&StoreEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn filter(&self, query: &str, tag_filter: &[String]) -> Vec<StoreEntry> {
        let query_terms: Vec<String> = query.split_whitespace().map(|s| s.to_lowercase()).collect();
        let mut result = Vec::new();
        for entry in &self.entries {
            if !tag_filter.is_empty() {
                let matched = tag_filter.iter().any(|tag| {
                    let tag = tag.to_lowercase();
                    entry.tags.iter().any(|t| t.to_lowercase() == tag)
                        || entry.categories.iter().any(|c| c.to_lowercase() == tag)
                });
                if !matched {
                    continue;
                }
            }
            if !query_terms.is_empty() {
                let haystack = format!(
                    "{} {} {} {}",
                    entry.name,
                    entry.summary,
                    entry.id,
                    entry.authors.join(" ")
                )
                .to_lowercase();
                let matched = query_terms.iter().all(|term| haystack.contains(term));
                if !matched {
                    continue;
                }
            }
            result.push(entry.clone());
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_valid_version("1.0"));
        assert!(is_valid_version("0.5.3"));
        assert!(!is_valid_version(""));
        assert!(!is_valid_version("abc"));
        assert!(is_newer_version("1.1", "1.0"));
        assert!(is_newer_version("1.0.1", "1.0"));
        assert!(!is_newer_version("1.0", "1.0"));
        assert!(!is_newer_version("0.9", "1.0"));
    }

    #[test]
    fn sha256_validation() {
        assert!(is_valid_sha256(&"a".repeat(64)));
        assert!(!is_valid_sha256("abc"));
        assert!(!is_valid_sha256(&"g".repeat(64)));
    }

    #[test]
    fn parses_and_filters_index() {
        let json = r#"{
            "schemaVersion": 1,
            "registry": "https://example.com/registry",
            "generatedAt": "2026-01-01T00:00:00Z",
            "entries": [
                {
                    "id": "b-mascot", "name": "Beta", "version": "1.0",
                    "summary": "Second mascot", "authors": ["alice"],
                    "tags": ["cute"], "categories": ["animal"],
                    "download": {"url": "https://github.com/x/y/releases/download/v1/b.mascot", "sha256": "%s"}
                },
                {
                    "id": "a-mascot", "name": "Alpha", "version": "2.1",
                    "summary": "First mascot", "authors": ["bob"],
                    "tags": ["cool"], "categories": ["robot"],
                    "download": {"url": "https://github.com/x/y/releases/download/v1/a.mascot", "sha256": "%s"}
                }
            ]
        }"#;
        let sha = "a".repeat(64);
        let json = json.replace("%s", &sha);
        let index = StoreIndex::parse(json.as_bytes()).unwrap();
        // 按 id 排序。
        assert_eq!(index.entries[0].id, "a-mascot");
        assert_eq!(index.entries[1].id, "b-mascot");
        // 按查询过滤。
        let hits = index.filter("alpha", &[]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "Alpha");
        // 按标签过滤。
        let hits = index.filter("", &["cute".to_string()]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "b-mascot");
    }

    #[test]
    fn rejects_invalid_entry() {
        let json = r#"{
            "schemaVersion": 1, "registry": "",
            "entries": [{"id": "x", "name": "X", "version": "1.0", "summary": "s",
                "download": {"url": "http://insecure.example/x.mascot", "sha256": "%s"}}]
        }"#;
        let json = json.replace("%s", &"a".repeat(64));
        assert!(StoreIndex::parse(json.as_bytes()).is_err());
    }
}
