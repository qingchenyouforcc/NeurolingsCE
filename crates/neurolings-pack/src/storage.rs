//! 桌宠模板的存储位置（应用本地数据目录 + `/mascots`）。

use std::path::PathBuf;

/// 返回各平台默认的桌宠存储目录：
/// - Windows: `%LOCALAPPDATA%/NeurolingsCE/mascots`
/// - macOS: `~/Library/Application Support/NeurolingsCE/mascots`
/// - Linux/其他: `$XDG_DATA_HOME/NeurolingsCE/mascots`，
///   回退到 `~/.local/share/NeurolingsCE/mascots`
pub fn default_storage_path() -> Option<PathBuf> {
    let base = default_data_path()?;
    Some(base.join("mascots"))
}

fn default_data_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let local_app_data = std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty())?;
        Some(PathBuf::from(local_app_data).join("NeurolingsCE"))
    }

    #[cfg(target_os = "macos")]
    {
        let home = home_dir()?;
        Some(home.join("Library/Application Support/NeurolingsCE"))
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(data_home) = std::env::var_os("XDG_DATA_HOME")
            .filter(|value| !value.is_empty() && PathBuf::from(&value).is_absolute())
        {
            return Some(PathBuf::from(data_home).join("NeurolingsCE"));
        }
        let home = home_dir()?;
        Some(home.join(".local/share/NeurolingsCE"))
    }
}

#[cfg(not(target_os = "windows"))]
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_storage_path_ends_with_expected_components() {
        let Some(path) = default_storage_path() else {
            return; // environment without HOME/LOCALAPPDATA; nothing to assert
        };
        assert!(path.ends_with(PathBuf::from("NeurolingsCE/mascots")));
    }
}
