//! Windows、Linux 和 macOS 平台能力适配入口。

use std::collections::BTreeMap;

/// 透明窗口需要的活动窗口几何信息。
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveWindow {
    /// 是否有可用窗口。
    pub available: bool,
    /// 平台侧稳定窗口标识。
    pub uid: String,
    /// 所属进程 ID。
    pub pid: u64,
    /// 左上角横坐标。
    pub x: f64,
    /// 左上角纵坐标。
    pub y: f64,
    /// 窗口宽度。
    pub width: f64,
    /// 窗口高度。
    pub height: f64,
    /// 平台原生句柄；跨平台实现可保持为零。
    pub native_handle: u64,
}

impl ActiveWindow {
    /// 创建可用活动窗口并校验几何值。
    pub fn new(
        uid: impl Into<String>,
        pid: u64,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> Option<Self> {
        if ![x, y, width, height].iter().all(|value| value.is_finite())
            || width < 0.0
            || height < 0.0
        {
            return None;
        }
        Some(Self {
            available: true,
            uid: uid.into(),
            pid,
            x,
            y,
            width,
            height,
            native_handle: 0,
        })
    }

    /// 创建不可用窗口占位值。
    pub fn unavailable() -> Self {
        Self {
            available: false,
            uid: String::new(),
            pid: 0,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            native_handle: 0,
        }
    }
}

/// 当前编译目标可提供的平台能力。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    /// 是否支持把桌宠窗口置于所有桌面。
    pub all_desktops: bool,
    /// 是否支持窗口 mask。
    pub window_masks: bool,
    /// 是否支持推动活动窗口。
    pub window_pushing: bool,
    /// 是否提供安全凭据存储。
    pub credential_store: bool,
}

/// 返回当前平台的保守能力声明。
pub const fn capabilities() -> Capabilities {
    #[cfg(target_os = "windows")]
    {
        return Capabilities {
            all_desktops: true,
            window_masks: true,
            window_pushing: true,
            credential_store: true,
        };
    }
    #[cfg(target_os = "linux")]
    {
        return Capabilities {
            all_desktops: true,
            window_masks: true,
            window_pushing: false,
            credential_store: true,
        };
    }
    #[cfg(target_os = "macos")]
    {
        return Capabilities {
            all_desktops: true,
            window_masks: true,
            window_pushing: false,
            credential_store: true,
        };
    }
    #[allow(unreachable_code)]
    Capabilities {
        all_desktops: false,
        window_masks: false,
        window_pushing: false,
        credential_store: false,
    }
}

/// 根据平台能力判断是否可以执行推动窗口操作。
pub fn push_window(window: &ActiveWindow, dx: f64, dy: f64) -> bool {
    capabilities().window_pushing && window.available && dx.is_finite() && dy.is_finite()
}

/// 最小的凭据存储抽象；平台适配层可将实现替换为系统钥匙串。
pub trait CredentialStore {
    /// 保存凭据。
    fn save(&mut self, service: &str, account: &str, secret: &str) -> Result<(), CredentialError>;
    /// 读取凭据。
    fn load(&self, service: &str, account: &str) -> Result<Option<String>, CredentialError>;
    /// 删除单个凭据。
    fn remove(&mut self, service: &str, account: &str) -> Result<bool, CredentialError>;
}

/// 凭据存储错误。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredentialError {
    EmptyField,
    Unavailable,
}

/// 供无系统钥匙串环境使用的内存实现。
#[derive(Default)]
pub struct MemoryCredentialStore {
    values: BTreeMap<(String, String), String>,
}

impl CredentialStore for MemoryCredentialStore {
    fn save(&mut self, service: &str, account: &str, secret: &str) -> Result<(), CredentialError> {
        if service.is_empty() || account.is_empty() {
            return Err(CredentialError::EmptyField);
        }
        self.values
            .insert((service.to_owned(), account.to_owned()), secret.to_owned());
        Ok(())
    }

    fn load(&self, service: &str, account: &str) -> Result<Option<String>, CredentialError> {
        if service.is_empty() || account.is_empty() {
            return Err(CredentialError::EmptyField);
        }
        Ok(self
            .values
            .get(&(service.to_owned(), account.to_owned()))
            .cloned())
    }

    fn remove(&mut self, service: &str, account: &str) -> Result<bool, CredentialError> {
        if service.is_empty() || account.is_empty() {
            return Err(CredentialError::EmptyField);
        }
        Ok(self
            .values
            .remove(&(service.to_owned(), account.to_owned()))
            .is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActiveWindow, CredentialError, CredentialStore, MemoryCredentialStore, push_window,
    };

    #[test]
    fn active_window_rejects_invalid_geometry() {
        assert!(ActiveWindow::new("window", 1, 0.0, 0.0, 100.0, 50.0).is_some());
        assert!(ActiveWindow::new("window", 1, 0.0, 0.0, -1.0, 50.0).is_none());
    }

    #[test]
    fn push_requires_available_window_and_finite_delta() {
        let window = ActiveWindow::new("window", 1, 0.0, 0.0, 100.0, 50.0).unwrap();
        assert!(!push_window(&window, f64::NAN, 1.0));
        assert!(!push_window(&ActiveWindow::unavailable(), 1.0, 1.0));
    }

    #[test]
    fn memory_credentials_validate_and_round_trip() {
        let mut store = MemoryCredentialStore::default();
        assert_eq!(
            store.save("", "account", "secret"),
            Err(CredentialError::EmptyField)
        );
        store.save("service", "account", "secret").unwrap();
        assert_eq!(
            store.load("service", "account").unwrap().as_deref(),
            Some("secret")
        );
        assert!(store.remove("service", "account").unwrap());
        assert_eq!(store.load("service", "account").unwrap(), None);
    }
}
