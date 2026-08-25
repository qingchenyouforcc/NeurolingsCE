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

/// 剥离 Qt NativeFormat 字符串的 @ 编码：
/// - "@@x" 是 Qt 对真实 "@x" 字符串的转义，还原为 "@x"；
/// - "@类型(...)"（如 "@Variant(...)"、"@QDateTime(...)"）返回括号内内容；
/// - 其余（含无法识别的 @ 形式）原样返回。
#[cfg_attr(not(windows), allow(dead_code))]
fn strip_qt_at_encoding(text: &str) -> &str {
    let Some(rest) = text.strip_prefix('@') else {
        return text;
    };
    // "@@..."：Qt 转义，真实内容以单个 @ 开头。
    if rest.starts_with('@') {
        return rest;
    }
    // "@类型(内容)"：类型名为 ASCII 标识符且整体以右括号结尾。
    if let Some(open) = rest.find('(') {
        let type_name = &rest[..open];
        if rest.ends_with(')')
            && !type_name.is_empty()
            && type_name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return &rest[open + 1..rest.len() - 1];
        }
    }
    text
}

/// QSettings 字符串值 → JSON：先剥离 Qt 的 @ 编码，再把布尔/数字还原为对应类型，
/// 其余保持字符串。"@Invalid()" 表示 Qt 无效值，返回 None（跳过该键）。
#[cfg_attr(not(windows), allow(dead_code))]
fn qsettings_string_to_json(text: &str) -> Option<serde_json::Value> {
    if text == "@Invalid()" {
        return None;
    }
    let stripped = strip_qt_at_encoding(text);
    Some(match stripped {
        "true" => serde_json::json!(true),
        "false" => serde_json::json!(false),
        _ => {
            if let Ok(int) = stripped.parse::<i64>() {
                serde_json::json!(int)
            } else if let Ok(float) = stripped.parse::<f64>() {
                serde_json::json!(float)
            } else {
                serde_json::json!(stripped)
            }
        }
    })
}

/// 按 Qt NativeFormat 存储类型读取注册表值并转 JSON：
/// - REG_SZ（QString 及 "@..." 序列化类型）→ 字符串解析；
/// - REG_DWORD（Qt 的 bool/int/uint）→ JSON number；
/// - REG_QWORD（Qt 的 qlonglong/qulonglong）→ JSON number。
///
/// 值不存在或类型不支持时返回 None。
#[cfg(windows)]
fn read_reg_value(reg: &winreg::RegKey, name: &str) -> Option<serde_json::Value> {
    if let Ok(text) = reg.get_value::<String, _>(name) {
        return qsettings_string_to_json(&text);
    }
    if let Ok(dword) = reg.get_value::<u32, _>(name) {
        return Some(serde_json::json!(dword));
    }
    if let Ok(qword) = reg.get_value::<u64, _>(name) {
        return Some(serde_json::json!(qword));
    }
    None
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

/// 导入单个注册表值：不覆盖 rs 侧已有设置；值存在但类型不支持时告警跳过，
/// 不让单个坏键中断整体迁移。
#[cfg(windows)]
fn import_registry_value(
    reg: &winreg::RegKey,
    name: &str,
    key: String,
    settings: &mut Settings,
    migrated: &mut usize,
) {
    if settings.get(&key).is_some() {
        return;
    }
    match read_reg_value(reg, name) {
        Some(value) => {
            let _ = settings.set(&key, value);
            *migrated += 1;
        }
        None => {
            // 读取失败但值确实存在 → 类型不支持，跳过并告警。
            if reg.get_raw_value(name).is_ok() {
                crate::log::warn(
                    "migrate",
                    &format!("skip registry value with unsupported type: {key}"),
                );
            }
        }
    }
}

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
    for name in TOP_LEVEL_KEYS {
        import_registry_value(&root, name, (*name).to_string(), settings, &mut migrated);
    }
    for (group, names) in GROUP_KEYS {
        let Ok(sub) = root.open_subkey_with_flags(group, KEY_READ) else {
            continue;
        };
        for name in *names {
            import_registry_value(
                &sub,
                name,
                format!("{group}/{name}"),
                settings,
                &mut migrated,
            );
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

    #[test]
    fn qsettings_string_conversion() {
        // 布尔/数字/字符串的基础还原。
        assert_eq!(
            qsettings_string_to_json("true"),
            Some(serde_json::json!(true))
        );
        assert_eq!(
            qsettings_string_to_json("false"),
            Some(serde_json::json!(false))
        );
        assert_eq!(qsettings_string_to_json("30"), Some(serde_json::json!(30)));
        assert_eq!(qsettings_string_to_json("-2"), Some(serde_json::json!(-2)));
        assert_eq!(
            qsettings_string_to_json("1.5"),
            Some(serde_json::json!(1.5))
        );
        assert_eq!(
            qsettings_string_to_json("zh_CN"),
            Some(serde_json::json!("zh_CN"))
        );
        assert_eq!(qsettings_string_to_json(""), Some(serde_json::json!("")));
    }

    #[test]
    fn strips_qt_at_encoding() {
        // "@类型(...)" 包装：取括号内内容后再按常规规则解析。
        assert_eq!(
            qsettings_string_to_json("@Variant(1.5)"),
            Some(serde_json::json!(1.5))
        );
        assert_eq!(
            qsettings_string_to_json("@Variant(8080)"),
            Some(serde_json::json!(8080))
        );
        assert_eq!(
            qsettings_string_to_json("@QDateTime(2025-01-02T03:04:05)"),
            Some(serde_json::json!("2025-01-02T03:04:05"))
        );
        // "@Invalid()" 是 Qt 的无效值，跳过该键。
        assert_eq!(qsettings_string_to_json("@Invalid()"), None);
        // "@@..." 是 Qt 对真实 "@..." 字符串的转义，还原后不再当包装处理。
        assert_eq!(
            qsettings_string_to_json("@@home"),
            Some(serde_json::json!("@home"))
        );
        // 无法识别的 @ 形式原样保留。
        assert_eq!(
            qsettings_string_to_json("@dangling"),
            Some(serde_json::json!("@dangling"))
        );
        assert_eq!(
            qsettings_string_to_json("@Variant(unclosed"),
            Some(serde_json::json!("@Variant(unclosed"))
        );
        // 二进制序列化内容无法解析为数字时按字符串保留，不丢键。
        assert_eq!(
            qsettings_string_to_json("@Variant(\u{0}\u{1})"),
            Some(serde_json::json!("\u{0}\u{1}"))
        );
    }

    #[test]
    fn strip_qt_at_encoding_cases() {
        assert_eq!(strip_qt_at_encoding("plain"), "plain");
        assert_eq!(strip_qt_at_encoding("@@a"), "@a");
        assert_eq!(strip_qt_at_encoding("@Type(x)"), "x");
        assert_eq!(strip_qt_at_encoding("@Type()"), "");
        assert_eq!(strip_qt_at_encoding("@"), "@");
        // 类型名不是标识符（含空格）时不视为包装。
        assert_eq!(strip_qt_at_encoding("@not a type(x)"), "@not a type(x)");
    }
}
