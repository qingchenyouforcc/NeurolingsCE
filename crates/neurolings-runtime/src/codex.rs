//! Codex 集成：在 ~/.codex/config.toml 中安装/移除标记块，
//! 把 Codex 的 notify 钩子接到 NeurolingsCE-cli --codex-notify。

use std::fs;
use std::path::PathBuf;

const BEGIN_MARKER: &str = "# >>> NeurolingsCE notify >>>";
const END_MARKER: &str = "# <<< NeurolingsCE notify <<<";

fn codex_config_path() -> Option<PathBuf> {
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

fn block_text(cli: &str) -> String {
    format!(
        "{BEGIN_MARKER}\nnotify = [\"{}\", \"--codex-notify\"]\n{END_MARKER}\n",
        cli.replace('\\', "\\\\")
    )
}

fn strip_block(text: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in text.lines() {
        if line.trim() == BEGIN_MARKER {
            inside = true;
            continue;
        }
        if line.trim() == END_MARKER {
            inside = false;
            continue;
        }
        if !inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// 安装（enabled=true）或移除（enabled=false）通知块，
/// 返回被修改的配置文件路径。
pub fn set_codex_notify_hook(enabled: bool) -> Result<PathBuf, String> {
    let path = codex_config_path().ok_or("could not resolve home directory")?;
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let stripped = strip_block(&existing);
    let new_content = if enabled {
        let Some(cli) = cli_path() else {
            return Err("NeurolingsCE-cli executable not found".into());
        };
        format!("{}{}", stripped.trim_end(), "\n")
            .trim_start()
            .to_string()
            + &block_text(&cli)
    } else {
        stripped
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, new_content).map_err(|e| e.to_string())?;
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
}
