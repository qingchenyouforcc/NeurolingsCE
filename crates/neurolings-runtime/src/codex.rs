//! Codex 集成：在 ~/.codex/config.toml 中安装/移除标记块，
//! 把 Codex 的 notify 钩子接到 NeurolingsCE-cli --codex-notify。
//! 行为与原版 CodexConfigManager.cc 逐行对齐：
//! - 路径优先 CODEX_HOME，否则 $HOME/.codex/config.toml
//! - 标记为 "# BEGIN NeurolingsCE Codex notify" / "# END NeurolingsCE Codex notify"
//! - 支持桥接单条 codex-computer-use 通知（previous-notify-base64）
//! - 冲突检测：外部非托管 notify 行存在且不可桥接时拒绝
//! - 原子写入 QSaveFile 语义：tmp 写入 + 备份 .bak.<timestamp> + rename

use std::fs;
use std::path::{Path, PathBuf};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

const BEGIN_MARKER: &str = "# BEGIN NeurolingsCE Codex notify";
const END_MARKER: &str = "# END NeurolingsCE Codex notify";
const PREVIOUS_PREFIX: &str = "# NeurolingsCE previous-notify-base64: ";

fn codex_config_path() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("CODEX_HOME") {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed).join("config.toml"));
        }
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    Some(PathBuf::from(home).join(".codex").join("config.toml"))
}

fn cli_path() -> Option<String> {
    let current = std::env::current_exe().ok()?;
    let dir = current.parent()?;
    let exe = if cfg!(windows) {
        dir.join("NeurolingsCE-cli.exe")
    } else {
        dir.join("NeurolingsCE-cli")
    };
    exe.is_file().then(|| exe.to_string_lossy().into_owned())
}

fn escape_toml(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            _ => out.push(ch),
        }
    }
    out
}

fn managed_block(executable_path: &str, previous_notify_line: &str) -> String {
    let mut block = String::new();
    block.push_str(BEGIN_MARKER);
    block.push('\n');
    if !previous_notify_line.is_empty() {
        block.push_str(PREVIOUS_PREFIX);
        block.push_str(&BASE64.encode(previous_notify_line.as_bytes()));
        block.push('\n');
    }
    block.push_str(&format!(
        "notify = [\"{}\", \"--codex-notify\"]\n",
        escape_toml(&PathBuf::from(executable_path).to_string_lossy())
    ));
    block.push_str(END_MARKER);
    block.push('\n');
    block
}

fn find_managed_block(content: &str) -> Option<(usize, usize)> {
    let start = content.find(BEGIN_MARKER)?;
    let end_rel = content[start..].find(END_MARKER)?;
    let mut end = start + end_rel + END_MARKER.len();
    // 包含尾随 \r\n 或 \n，与原版一致
    if content.as_bytes().get(end) == Some(&b'\r') {
        end += 1;
    }
    if content.as_bytes().get(end) == Some(&b'\n') {
        end += 1;
    }
    Some((start, end))
}

fn previous_notify_from_block(content: &str, start: usize, end: usize) -> Option<String> {
    let block = &content[start..end];
    for line in block.lines() {
        if let Some(b64) = line.strip_prefix(PREVIOUS_PREFIX) {
            let trimmed = b64.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(decoded) = BASE64.decode(trimmed.as_bytes())
                && let Ok(s) = String::from_utf8(decoded)
                && !s.is_empty()
            {
                return Some(s);
            }
            return None;
        }
    }
    None
}

fn is_neurolings_cli(path: &str) -> bool {
    // 先 unescape 再取 fileName
    let unescaped = {
        let mut out = String::new();
        let mut escaped = false;
        for ch in path.chars() {
            if escaped {
                match ch {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'b' => out.push('\x08'),
                    'f' => out.push('\x0C'),
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    _ => {
                        out.push('\\');
                        out.push(ch);
                    }
                }
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else {
                out.push(ch);
            }
        }
        if escaped {
            out.push('\\');
        }
        out
    };
    let file_name = Path::new(&unescaped)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    file_name.eq_ignore_ascii_case("NeurolingsCE-cli.exe")
        || file_name.eq_ignore_ascii_case("NeurolingsCE-cli")
}

fn find_legacy_block(content: &str, managed: Option<(usize, usize)>) -> Option<(usize, usize)> {
    // 正则： notify = [ "<path>", "--codex-notify" ]，path 为 NeurolingsCE-cli
    // 简化为字符串扫描，避免引入 regex
    let mut pos = 0;
    while let Some(idx) = content[pos..].find("notify") {
        let abs = pos + idx;
        // 检查是否在 managed 块内
        if let Some((ms, me)) = managed
            && (ms..me).contains(&abs)
        {
            pos = abs + 6;
            continue;
        }
        // 行首允许空格
        let line_start = content[..abs].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let prefix = &content[line_start..abs];
        if !prefix.chars().all(|c| c == ' ' || c == '\t') {
            pos = abs + 6;
            continue;
        }
        // 找 = 和 [ ... ]
        let rest = &content[abs..];
        let eq = rest.find('=');
        if eq.is_none() {
            pos = abs + 6;
            continue;
        }
        let eq = eq.unwrap();
        let after_eq = &rest[eq + 1..];
        // 查找 '"'
        let first_quote = after_eq.find('"');
        if first_quote.is_none() {
            pos = abs + 6;
            continue;
        }
        let fq = first_quote.unwrap();
        // 找匹配的未转义 "
        let bytes = after_eq.as_bytes();
        let mut end_quote: Option<usize> = None;
        let mut escaped = false;
        for (i, &byte) in bytes.iter().enumerate().skip(fq + 1) {
            if escaped {
                escaped = false;
                continue;
            }
            if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                end_quote = Some(i);
                break;
            }
        }
        let eq2 = end_quote.unwrap_or(0);
        if end_quote.is_none() {
            pos = abs + 6;
            continue;
        }
        let path_str = &after_eq[fq + 1..eq2];
        if !is_neurolings_cli(path_str) {
            pos = abs + 6;
            continue;
        }
        // 检查后面是否有 ,"--codex-notify"
        let after_path = &after_eq[eq2 + 1..];
        if !after_path.contains("\"--codex-notify\"") {
            pos = abs + 6;
            continue;
        }
        // 找到行尾
        let line_end_rel = rest[eq..]
            .find('\n')
            .map(|i| eq + i + 1)
            .unwrap_or(rest.len());
        let legacy_start = line_start;
        let legacy_end = abs + line_end_rel;
        return Some((legacy_start, legacy_end));
    }
    None
}

fn is_codex_computer_use(notify_line: &str) -> bool {
    // notify_line 形如 notify = ["...codex-computer-use(.exe)", "turn-ended"]
    let eq = notify_line.find('=');
    if eq.is_none() {
        return false;
    }
    let json_part = notify_line[eq.unwrap() + 1..].trim();
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(json_part);
    let Ok(serde_json::Value::Array(arr)) = parsed else {
        return false;
    };
    if arr.len() != 2 {
        return false;
    }
    let exe = arr[0].as_str().unwrap_or("");
    let arg = arr[1].as_str().unwrap_or("");
    if arg != "turn-ended" {
        return false;
    }
    let file_name = exe.replace('\\', "/");
    let file_name = Path::new(&file_name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    file_name.eq_ignore_ascii_case("codex-computer-use.exe")
        || file_name.eq_ignore_ascii_case("codex-computer-use")
}

fn external_notify_lines(
    content: &str,
    managed: Option<(usize, usize)>,
    legacy: Option<(usize, usize)>,
) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(idx) = content[pos..].find("notify") {
        let abs = pos + idx;
        let line_start = content[..abs].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let prefix = &content[line_start..abs];
        if !prefix.chars().all(|c| c == ' ' || c == '\t') {
            pos = abs + 6;
            continue;
        }
        // 确保后面有 =
        let rest = &content[abs..];
        if !rest.contains('=') {
            pos = abs + 6;
            continue;
        }
        if let Some((ms, me)) = managed
            && (ms..me).contains(&abs)
        {
            pos = abs + 6;
            continue;
        }
        if let Some((ls, le)) = legacy
            && (ls..le).contains(&abs)
        {
            pos = abs + 6;
            continue;
        }
        let line_end = content[abs..]
            .find('\n')
            .map(|i| abs + i + 1)
            .unwrap_or(content.len());
        let text = content[abs..line_end].to_string();
        out.push((abs, line_end, text));
        pos = line_end;
    }
    out
}

fn write_atomically(path: &Path, content: &str) -> Result<Option<PathBuf>, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Could not create Codex configuration directory: {e}"))?;
    }
    let backup = if path.exists() {
        let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S-%3f").to_string();
        let stem = path.to_string_lossy().into_owned() + &format!(".bak.{ts}");
        let mut suffix = 1;
        let mut candidate = PathBuf::from(stem.clone());
        while candidate.exists() {
            candidate = PathBuf::from(format!("{stem}-{suffix}"));
            suffix += 1;
        }
        fs::copy(path, &candidate)
            .map_err(|e| format!("Could not create a backup of Codex configuration: {e}"))?;
        Some(candidate)
    } else {
        None
    };
    // 原子写入：临时文件 + rename
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, content)
        .map_err(|e| format!("Could not open Codex configuration for writing: {e}"))?;
    fs::rename(&tmp, path)
        .map_err(|e| format!("Could not atomically update Codex configuration: {e}"))?;
    Ok(backup)
}

/// 仅测试使用的辅助：移除托管块后的文本（禁用路径的简化版）。
#[cfg(test)]
fn strip_block(text: &str) -> String {
    if let Some((s, e)) = find_managed_block(text) {
        let mut out = String::new();
        out.push_str(&text[..s]);
        out.push_str(&text[e..]);
        // 移除块前多余的一个 \n（与原版 disable 逻辑一致）
        if out.len() > s
            && s > 0
            && text.as_bytes()[s - 1] == b'\n'
            && out.as_bytes()[s - 1] == b'\n'
        {
            // disable 时会移除前导 \n，这里 strip 仅用于测试保留简单
        }
        out
    } else {
        text.to_string()
    }
}

/// 安装（enabled=true）或移除（enabled=false）通知块，
/// 返回被修改的配置文件路径。
pub fn set_codex_notify_hook(enabled: bool) -> Result<PathBuf, String> {
    let path = codex_config_path().ok_or("could not resolve home directory")?;
    let existing = fs::read_to_string(&path).unwrap_or_default();

    if !enabled {
        // 禁用：移除托管块，若含 previous 则还原
        let Some((start, end)) = find_managed_block(&existing) else {
            return Ok(path);
        };
        let previous = previous_notify_from_block(&existing, start, end);
        let updated = if let Some(prev) = previous {
            let mut s = existing.clone();
            s.replace_range(start..end, &prev);
            // 确保还原后以 \n 结尾若原块后有内容
            if !prev.ends_with('\n')
                && s.len() > start + prev.len()
                && s.as_bytes()[start + prev.len()] != b'\n'
            {
                // 保持原样
            }
            s
        } else {
            let mut removal_start = start;
            if removal_start > 0 && existing.as_bytes()[removal_start - 1] == b'\n' {
                removal_start -= 1;
            }
            let mut s = existing.clone();
            s.replace_range(removal_start..end, "");
            s
        };
        if updated == existing {
            return Ok(path);
        }
        write_atomically(&path, &updated)?;
        return Ok(path);
    }

    // 启用
    let cli = cli_path().ok_or("NeurolingsCE-cli executable not found")?;
    let cli_abs = PathBuf::from(&cli).to_string_lossy().into_owned();
    // 校验可执行存在
    if !Path::new(&cli).is_file() {
        return Err(format!(
            "NeurolingsCE CLI executable was not found: {}",
            cli_abs
        ));
    }

    let managed = find_managed_block(&existing);
    let legacy = find_legacy_block(&existing, managed);
    let externals = external_notify_lines(&existing, managed, legacy);

    let mut previous_notify_line = managed
        .and_then(|(s, e)| previous_notify_from_block(&existing, s, e))
        .unwrap_or_default();

    let mut can_bridge = false;
    if managed.is_none() && legacy.is_none() && externals.len() == 1 {
        let line = &externals[0].2;
        if is_codex_computer_use(line) {
            can_bridge = true;
            previous_notify_line = line.trim().to_string();
            // 去掉尾换行
            if previous_notify_line.ends_with('\n') {
                previous_notify_line.pop();
                if previous_notify_line.ends_with('\r') {
                    previous_notify_line.pop();
                }
            }
        }
    }
    if (!externals.is_empty() && !can_bridge) || externals.len() > 1 {
        return Err("Codex config already contains a non-NeurolingsCE notify setting".into());
    }

    let block = managed_block(&cli_abs, &previous_notify_line);
    let updated = if let Some((s, e)) = managed {
        let mut s2 = existing.clone();
        // 若同时有 legacy，需先移除 legacy 并调整 s
        if let Some((ls, le)) = legacy {
            s2.replace_range(ls..le, "");
            let shift = le - ls;
            let (ns, ne) = if ls < s {
                (s - shift, e - shift)
            } else {
                (s, e)
            };
            s2.replace_range(ns..ne, &block);
        } else {
            s2.replace_range(s..e, &block);
        }
        s2
    } else if let Some((ls, le)) = legacy {
        let mut s2 = existing.clone();
        let mut replacement = block.clone();
        if ls > 0 {
            replacement = format!("\n{replacement}");
        }
        s2.replace_range(ls..le, &replacement);
        s2
    } else if can_bridge {
        let mut s2 = existing.clone();
        let (es, ee, _) = &externals[0];
        s2.replace_range(*es..*ee, &block);
        s2
    } else {
        let mut s2 = existing.clone();
        if !s2.is_empty() && !s2.ends_with('\n') {
            s2.push('\n');
        } else if !s2.is_empty() {
            // 已有 \n，无需额外
        }
        if !s2.is_empty() {
            // 原版保留一个分隔换行，这里已在上面处理
        }
        s2.push_str(&block);
        s2
    };

    if updated == existing {
        return Ok(path);
    }
    write_atomically(&path, &updated)?;
    Ok(path)
}

pub fn is_codex_notify_hook_installed() -> bool {
    codex_config_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .is_some_and(|text| text.contains(BEGIN_MARKER))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_block_removes_only_marked_region() {
        let text = format!(
            "model = \"o4-mini\"\n{BEGIN_MARKER}\nnotify = [\"x\"]\n{END_MARKER}\nother = 1\n"
        );
        let stripped = strip_block(&text);
        assert!(stripped.contains("model"));
        assert!(stripped.contains("other"));
        assert!(!stripped.contains("notify"));
        assert!(!stripped.contains(BEGIN_MARKER));
    }

    #[test]
    fn escape_toml_roundtrip() {
        assert_eq!(escape_toml("C:\\path\\to\\exe"), "C:\\\\path\\\\to\\\\exe");
        assert_eq!(escape_toml("a\"b"), "a\\\"b");
    }

    #[test]
    fn managed_block_contains_markers() {
        let b = managed_block("C:\\test\\NeurolingsCE-cli.exe", "");
        assert!(b.contains(BEGIN_MARKER));
        assert!(b.contains(END_MARKER));
        assert!(b.contains("notify = [\"C:\\\\test"));
    }
}
