//! 开机自启管理：Windows 注册表 Run 键、Linux XDG autostart、
//! macOS 启动代理（占位）。

#[cfg(windows)]
mod win {
    use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_SAM_FLAGS, REG_SZ, REG_VALUE_TYPE,
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    };
    use windows::core::PCWSTR;

    use super::APP_NAME;
    use crate::{PlatformError, PlatformResult};

    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn wide_bytes(s: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity((s.len() + 1) * 2);
        for c in s.encode_utf16().chain(std::iter::once(0)) {
            out.extend_from_slice(&c.to_le_bytes());
        }
        out
    }

    fn open_key(access: REG_SAM_FLAGS) -> PlatformResult<HKEY> {
        let mut key = HKEY::default();
        let name = wide(RUN_KEY);
        unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(name.as_ptr()),
                None,
                access,
                &mut key,
            )
        }
        .ok()
        .map_err(|e| PlatformError::Win32(format!("open run key: {e}")))?;
        Ok(key)
    }

    pub fn set_autostart(enabled: bool, exe_path: &str) -> PlatformResult<()> {
        let key = open_key(REG_SAM_FLAGS(KEY_READ.0 | KEY_WRITE.0))?;
        let name = wide(APP_NAME);
        let result = if enabled {
            let value = wide_bytes(&format!("\"{exe_path}\" --silent"));
            unsafe { RegSetValueExW(key, PCWSTR(name.as_ptr()), None, REG_SZ, Some(&value)) }
                .ok()
                .map_err(|e| PlatformError::Win32(format!("set run value: {e}")))
        } else {
            let err = unsafe { RegDeleteValueW(key, PCWSTR(name.as_ptr())) };
            if err == ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                err.ok()
                    .map_err(|e| PlatformError::Win32(format!("delete run value: {e}")))
            }
        };
        let _ = unsafe { RegCloseKey(key) };
        result
    }

    pub fn is_autostart_enabled() -> PlatformResult<bool> {
        let key = open_key(KEY_READ)?;
        let name = wide(APP_NAME);
        let mut value_type = REG_VALUE_TYPE::default();
        let mut size = 0u32;
        let err = unsafe {
            RegQueryValueExW(
                key,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut value_type),
                None,
                Some(&mut size),
            )
        };
        let _ = unsafe { RegCloseKey(key) };
        Ok(err == windows::Win32::Foundation::ERROR_SUCCESS)
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::APP_NAME;
    use crate::PlatformResult;

    fn autostart_path() -> std::path::PathBuf {
        let home = std::env::var("HOME").unwrap_or_default();
        std::path::PathBuf::from(home)
            .join(".config")
            .join("autostart")
            .join("neurolingsce.desktop")
    }

    pub fn set_autostart(enabled: bool, exe_path: &str) -> PlatformResult<()> {
        let path = autostart_path();
        if enabled {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let entry = format!(
                "[Desktop Entry]\nType=Application\nName={APP_NAME}\nExec=\"{exe_path}\" --silent\nX-GNOME-Autostart-enabled=true\n"
            );
            std::fs::write(&path, entry).map_err(|e| crate::PlatformError::Win32(e.to_string()))
        } else {
            match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(crate::PlatformError::Win32(e.to_string())),
            }
        }
    }

    pub fn is_autostart_enabled() -> PlatformResult<bool> {
        Ok(autostart_path().exists())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use crate::PlatformResult;

    pub fn set_autostart(_enabled: bool, _exe_path: &str) -> PlatformResult<()> {
        // macOS 启动代理管理暂未实现。
        Err(crate::PlatformError::Unsupported)
    }

    pub fn is_autostart_enabled() -> PlatformResult<bool> {
        Ok(false)
    }
}

const APP_NAME: &str = "NeurolingsCE";

#[cfg(target_os = "linux")]
pub use linux::{is_autostart_enabled, set_autostart};
#[cfg(target_os = "macos")]
pub use macos::{is_autostart_enabled, set_autostart};
#[cfg(windows)]
pub use win::{is_autostart_enabled, set_autostart};
