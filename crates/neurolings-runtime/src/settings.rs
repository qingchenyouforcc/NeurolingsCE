//! 运行时设置：JSON 持久化，供主循环与管理器（经 IPC）读写。
//!
//! 存储在应用数据目录的 settings.json；键名与历史设置保持兼容。
//! 所有读取都带默认值与范围钳制，损坏文件不会阻断启动。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// 全部设置键及其默认值。
pub const KEY_USER_SCALE: &str = "userScale";
pub const KEY_DETACH_THRESHOLD: &str = "detachThreshold";
pub const KEY_WINDOW_PUSHING: &str = "windowPushingEnabled";
pub const KEY_BUBBLE_ENABLED: &str = "speechBubbleEnabled";
pub const KEY_BUBBLE_CLICKS: &str = "speechBubbleClickCount";
pub const KEY_CODEX_ENABLED: &str = "codex/enabled";
pub const KEY_CODEX_TEMPLATE: &str = "codex/companionTemplate";
pub const KEY_HTTP_ENABLED: &str = "http/enabled";
pub const KEY_STARTUP_SILENT: &str = "startup/silent";
pub const KEY_STARTUP_COMBO_MODE: &str = "startup/restoreCombinationMode";
pub const KEY_STARTUP_COMBO_ID: &str = "startup/restoreCombinationId";
pub const KEY_LANGUAGE: &str = "language";

/// 语言的运行时文案（右键菜单/托盘）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
    ZhCn,
}

impl Locale {
    pub fn from_setting(value: &str) -> Self {
        let lower = value.trim().to_lowercase();
        if lower == "zh_cn" || lower.starts_with("zh") {
            Locale::ZhCn
        } else {
            Locale::En
        }
    }
}

pub struct Settings {
    path: PathBuf,
    values: HashMap<String, Value>,
}

impl Settings {
    pub fn load(app_data_dir: &Path) -> Self {
        let path = app_data_dir.join("settings.json");
        let values = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|v| v.as_object().cloned())
            .map(|m| m.into_iter().collect())
            .unwrap_or_default();
        Self { path, values }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn get_f64(&self, key: &str, default: f64) -> f64 {
        self.get(key)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    }

    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        self.get(key).and_then(Value::as_bool).unwrap_or(default)
    }

    pub fn get_i64(&self, key: &str, default: i64) -> i64 {
        self.get(key).and_then(Value::as_i64).unwrap_or(default)
    }

    pub fn get_string(&self, key: &str, default: &str) -> String {
        self.get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| default.to_string())
    }

    pub fn set(&mut self, key: &str, value: Value) -> Result<(), String> {
        self.values.insert(key.to_string(), value);
        self.save()
    }

    /// 应用带钳制的便捷读取。
    pub fn user_scale(&self) -> f64 {
        self.get_f64(KEY_USER_SCALE, 1.0).clamp(0.1, 10.0)
    }

    pub fn detach_threshold(&self) -> f64 {
        self.get_f64(KEY_DETACH_THRESHOLD, 30.0).max(0.0)
    }

    pub fn locale(&self) -> Locale {
        // 未设置时参考 LANG 环境变量（由管理器在首次启动时写入用户语言）。
        let value = self.get_string(KEY_LANGUAGE, "");
        if value.is_empty() {
            if let Ok(lang) = std::env::var("LANG") {
                return Locale::from_setting(&lang);
            }
            return Locale::En;
        }
        Locale::from_setting(&value)
    }

    fn save(&self) -> Result<(), String> {
        let value = Value::Object(self.values.clone().into_iter().collect());
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }
}
