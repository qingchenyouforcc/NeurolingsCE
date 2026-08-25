//! 运行时设置：JSON 持久化，供主循环与管理器（经 IPC）读写。
//!
//! 存储在应用数据目录的 settings.json；键名与历史设置保持兼容。
//! 所有读取都带默认值与范围钳制，损坏文件不会阻断启动。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// 全部设置键及其默认值（与原版 QSettings 键名逐一对齐）。
pub const KEY_USER_SCALE: &str = "userScale";
pub const KEY_DETACH_THRESHOLD: &str = "detachThreshold";
pub const KEY_WINDOW_PUSHING: &str = "windowPushingEnabled";
pub const KEY_BUBBLE_ENABLED: &str = "speechBubbleEnabled";
pub const KEY_BUBBLE_CLICKS: &str = "speechBubbleClickCount";
pub const KEY_MULTIPLICATION: &str = "multiplicationEnabled";
pub const KEY_CODEX_ENABLED: &str = "codex/enabled";
pub const KEY_CODEX_TEMPLATE: &str = "codex/companionTemplate";
pub const KEY_CODEX_APP_SERVER_ENABLED: &str = "codex/appServerEnabled";
pub const KEY_CODEX_APP_SERVER_EXECUTABLE: &str = "codex/appServerExecutable";
pub const KEY_CODEX_APPROVAL_BUBBLE: &str = "codex/approvalBubbleEnabled";
pub const KEY_CODEX_PLAN_BUBBLE: &str = "codex/planBubbleEnabled";
pub const KEY_HTTP_ENABLED: &str = "http/enabled";
pub const KEY_STARTUP_SILENT: &str = "startup/silent";
pub const KEY_STARTUP_COMBO_MODE: &str = "startup/restoreCombinationMode";
pub const KEY_STARTUP_COMBO_ID: &str = "startup/restoreCombinationId";
pub const KEY_WINDOWED_BG: &str = "windowedModeBackground";
pub const KEY_UPDATE_CHECK: &str = "update/checkOnStartup";
pub const KEY_UPDATE_DOWNLOADED_VERSION: &str = "update/downloadedVersion";
pub const KEY_UPDATE_DOWNLOADED_PATH: &str = "update/downloadedInstallerPath";
pub const KEY_UPDATE_DOWNLOADED_SHA256: &str = "update/downloadedInstallerSha256";
pub const KEY_LANGUAGE: &str = "language";
pub const KEY_PROXY_MODE: &str = "update/proxyMode";
pub const KEY_PROXY_HOST: &str = "update/proxyHost";
pub const KEY_PROXY_PORT: &str = "update/proxyPort";
pub const KEY_PROXY_USER: &str = "update/proxyUsername";
pub const KEY_PROXY_PASS: &str = "update/proxyPassword";

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
        match self.get(key) {
            Some(Value::Bool(b)) => *b,
            // 兼容 CE 迁移值：Qt 把 bool 存为 REG_DWORD，迁移后是数字 0/1。
            Some(Value::Number(n)) => n
                .as_i64()
                .or_else(|| n.as_f64().map(|v| v as i64))
                .map(|v| v != 0)
                .unwrap_or(default),
            _ => default,
        }
    }

    pub fn get_i64(&self, key: &str, default: i64) -> i64 {
        match self.get(key) {
            Some(Value::Number(n)) => n
                .as_i64()
                .or_else(|| n.as_f64().map(|v| v as i64))
                .unwrap_or(default),
            _ => default,
        }
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
        // 未设置时：LANG → Windows UI 语言 → 英语。
        let value = self.get_string(KEY_LANGUAGE, "");
        if !value.is_empty() {
            return Locale::from_setting(&value);
        }
        if let Ok(lang) = std::env::var("LANG")
            && !lang.is_empty()
        {
            return Locale::from_setting(&lang);
        }
        if system_ui_is_chinese() {
            return Locale::ZhCn;
        }
        Locale::En
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

fn system_ui_is_chinese() -> bool {
    #[cfg(windows)]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey("Control Panel\\International")
            && let Ok(name) = key.get_value::<String, _>("LocaleName")
        {
            return name.to_lowercase().starts_with("zh");
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn get_i64_accepts_float_json_numbers() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = Settings::load(dir.path());
        settings.set(KEY_BUBBLE_CLICKS, json!(3.0)).unwrap();
        assert_eq!(settings.get_i64(KEY_BUBBLE_CLICKS, 1), 3);
    }

    #[test]
    fn get_bool_accepts_migrated_dword_numbers() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = Settings::load(dir.path());
        // CE 迁移来的 bool 是 REG_DWORD 数字：0/1 应读作 false/true。
        settings.set(KEY_BUBBLE_ENABLED, json!(1)).unwrap();
        assert!(settings.get_bool(KEY_BUBBLE_ENABLED, false));
        settings.set(KEY_BUBBLE_ENABLED, json!(0)).unwrap();
        assert!(!settings.get_bool(KEY_BUBBLE_ENABLED, true));
        // 原生 bool 与非数字值的行为不变。
        settings.set(KEY_BUBBLE_ENABLED, json!(true)).unwrap();
        assert!(settings.get_bool(KEY_BUBBLE_ENABLED, false));
        settings.set(KEY_BUBBLE_ENABLED, json!("yes")).unwrap();
        assert!(settings.get_bool(KEY_BUBBLE_ENABLED, true));
    }
}
