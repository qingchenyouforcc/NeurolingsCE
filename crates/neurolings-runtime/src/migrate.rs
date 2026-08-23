//! 一次性迁移：把 CE（C++/Qt 版）保存在注册表中的用户数据并入当前存储。
//!
//! - 设置：HKCU\Software\pixelomer\Shijima-Qt（QSettings NativeFormat）→ settings.json；
//! - 组合：combinations/saved、combinations/lastBeforeClose → combinations.json；
//! - 另外负责清洗早期版本写入的非标准 startup/restoreCombinationMode 值。
//!
//! 迁移只在 settings.json 缺少迁移标记时执行一次；已有设置不会被覆盖，
//! 组合按 id 去重合并。未安装过 CE 的机器只做键值清洗后直接打标记。

use std::path::Path;

use crate::combinations::CombinationStore;
use crate::settings::{self, Settings};

/// 迁移标记键：存在于 settings.json 即表示迁移已完成。
const MIGRATION_DONE_KEY: &str = "migration/ceDone";

/// 入口：在运行时加载设置前调用一次。
pub fn run_once(app_data_dir: &Path) {
    let mut settings = Settings::load(app_data_dir);
    // 无论是否安装过 CE，都先清洗早期版本的启动恢复模式值。
    normalize_restore_mode(&mut settings);
    if settings.get_bool(MIGRATION_DONE_KEY, false) {
        return;
    }
    #[cfg(windows)]
    migrate_windows(app_data_dir, &mut settings);
    #[cfg(not(windows))]
    {
        let _ = app_data_dir;
    }
    let _ = settings.set(MIGRATION_DONE_KEY, serde_json::json!(true));
}

/// 把早期版本的 "last:" / "id:<组合名>" 归一化为原版值域 none/last/saved+Id。
fn normalize_restore_mode(settings: &mut Settings) {
    let mode = settings.get_string(settings::KEY_STARTUP_COMBO_MODE, "");
    if mode == "last:" {
        let _ = settings.set(
            settings::KEY_STARTUP_COMBO_MODE,
            serde_json::json!(crate::combinations::RESTORE_MODE_LAST),
        );
    } else if let Some(id) = mode.strip_prefix("id:") {
        // 旧格式把组合名编码在模式值里，拆成 saved + 独立 Id 键。
        if !settings
            .get(settings::KEY_STARTUP_COMBO_ID)
            .is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty()))
        {
            let _ = settings.set(settings::KEY_STARTUP_COMBO_ID, serde_json::json!(id));
        }
        let _ = settings.set(
            settings::KEY_STARTUP_COMBO_MODE,
            serde_json::json!(crate::combinations::RESTORE_MODE_SAVED),
        );
    }
}

/// QSettings 字符串值 → JSON：布尔/数字还原为对应类型，其余保持字符串。
#[cfg(windows)]
fn qsettings_string_to_json(text: &str) -> serde_json::Value {
    match text {
        "true" => serde_json::json!(true),
        "false" => serde_json::json!(false),
        _ => {
            if let Ok(int) = text.parse::<i64>() {
                serde_json::json!(int)
            } else if let Ok(float) = text.parse::<f64>() {
                serde_json::json!(float)
            } else {
                serde_json::json!(text)
            }
        }
    }
}

/// 顶层设置键（QSettings 注册表值名与 settings.json 键名相同）。
#[cfg(windows)]
const TOP_LEVEL_KEYS: &[&str] = &[
    "userScale",
    "detachThreshold",
    "windowPushingEnabled",
    "speechBubbleEnabled",
    "speechBubbleClickCount",
    "multiplicationEnabled",
    "language",
    "windowedModeBackground",
];

/// 分组设置键：QSettings 的 "codex/enabled" 在注册表中是子键 codex 下的值 enabled。
#[cfg(windows)]
const GROUP_KEYS: &[(&str, &[&str])] = &[
    (
        "codex",
        &[
            "enabled",
            "companionTemplate",
            "appServerEnabled",
            "appServerExecutable",
            "approvalBubbleEnabled",
            "planBubbleEnabled",
            "lastThreadId",
            "lastWorkspace",
        ],
    ),
    ("http", &["enabled"]),
    (
        "startup",
        &["silent", "restoreCombinationMode", "restoreCombinationId"],
    ),
    (
        "update",
        &[
            "checkOnStartup",
            "proxyMode",
            "proxyHost",
            "proxyPort",
            "proxyUsername",
            "proxyPassword",
            "ignoredVersion",
            "lastCheckedAt",
            "downloadedVersion",
            "downloadedInstallerPath",
            "downloadedInstallerSha256",
            "remindVersion",
            "remindAt",
        ],
    ),
];

#[cfg(windows)]
fn migrate_windows(app_data_dir: &Path, settings: &mut Settings) {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(root) = hkcu.open_subkey_with_flags("Software\\pixelomer\\Shijima-Qt", KEY_READ) else {
        // 未安装过 CE：无需迁移。
        return;
    };

    let mut migrated = 0usize;
    let mut import = |key: String, raw: Option<String>, settings: &mut Settings| {
        let Some(raw) = raw else { return };
        // 不覆盖已有设置：rs 侧已保存的值优先。
        if settings.get(&key).is_some() {
            return;
        }
        let _ = settings.set(&key, qsettings_string_to_json(&raw));
        migrated += 1;
    };

    for name in TOP_LEVEL_KEYS {
        let raw: Option<String> = root.get_value::<String, _>(name).ok();
        import((*name).to_string(), raw, settings);
    }
    for (group, names) in GROUP_KEYS {
        let Ok(sub) = root.open_subkey_with_flags(group, KEY_READ) else {
            continue;
        };
        for name in *names {
            let raw: Option<String> = sub.get_value::<String, _>(name).ok();
            import(format!("{group}/{name}"), raw, settings);
        }
    }

    // 组合数据迁移：按 id 去重合并，不覆盖 rs 侧已有内容。
    let saved: Option<String> = root
        .open_subkey_with_flags("combinations", KEY_READ)
        .and_then(|k| k.get_value::<String, _>("saved"))
        .ok();
    let last: Option<String> = root
        .open_subkey_with_flags("combinations", KEY_READ)
        .and_then(|k| k.get_value::<String, _>("lastBeforeClose"))
        .ok();
    if saved.is_some() || last.is_some() {
        let saved_json = saved
            .as_deref()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
            .unwrap_or(serde_json::json!([]));
        let last_json = last
            .as_deref()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
            .unwrap_or(serde_json::json!({}));
        CombinationStore::new(app_data_dir).merge_from_ce(&saved_json, &last_json);
    }

    crate::log::info(
        "migrate",
        &format!(
            "CE registry migration done: settings={} saved={} last={}",
            migrated,
            saved.is_some(),
            last.is_some()
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_legacy_mode_values() {
        let dir = tempfile::tempdir().unwrap();

        // "last:" → "last"
        let mut settings = Settings::load(dir.path());
        let _ = settings.set(settings::KEY_STARTUP_COMBO_MODE, serde_json::json!("last:"));
        normalize_restore_mode(&mut settings);
        assert_eq!(
            settings.get_string(settings::KEY_STARTUP_COMBO_MODE, ""),
            "last"
        );

        // "id:<名>" → saved + 独立 Id 键
        let mut settings = Settings::load(dir.path());
        let _ = settings.set(
            settings::KEY_STARTUP_COMBO_MODE,
            serde_json::json!("id:My Combo"),
        );
        normalize_restore_mode(&mut settings);
        assert_eq!(
            settings.get_string(settings::KEY_STARTUP_COMBO_MODE, ""),
            "saved"
        );
        assert_eq!(
            settings.get_string(settings::KEY_STARTUP_COMBO_ID, ""),
            "My Combo"
        );
    }

    #[test]
    fn run_once_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        run_once(dir.path());
        run_once(dir.path());
        let settings = Settings::load(dir.path());
        assert!(settings.get_bool(MIGRATION_DONE_KEY, false));
    }

    #[cfg(windows)]
    #[test]
    fn qsettings_string_conversion() {
        assert_eq!(qsettings_string_to_json("true"), serde_json::json!(true));
        assert_eq!(qsettings_string_to_json("false"), serde_json::json!(false));
        assert_eq!(qsettings_string_to_json("30"), serde_json::json!(30));
        assert_eq!(qsettings_string_to_json("1.5"), serde_json::json!(1.5));
        assert_eq!(
            qsettings_string_to_json("zh_CN"),
            serde_json::json!("zh_CN")
        );
    }
}
