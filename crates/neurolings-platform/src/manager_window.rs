//! 管理器（Manager）主窗口的显示/隐藏/前置控制。
//!
//! Manager 是独立进程，这里按窗口标题定位其顶层窗口后操作，
//! 与原版托盘 Show/Hide、左键单击切换的语义保持一致。

/// Manager 主窗口标题（与 Manager 进程创建的窗口标题一致，对齐原版）。
pub const MANAGER_WINDOW_TITLE: &str = "NeurolingsCE — Mascot Manager";

#[cfg(windows)]
mod imp {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, IsWindowVisible, SW_HIDE, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
    };
    use windows::core::PCWSTR;

    fn encode_title() -> Vec<u16> {
        crate::manager_window::MANAGER_WINDOW_TITLE
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect()
    }

    fn find_manager() -> Option<HWND> {
        let title = encode_title();
        let hwnd = unsafe { FindWindowW(None, PCWSTR(title.as_ptr())) }.unwrap_or_default();
        (!hwnd.is_invalid()).then_some(hwnd)
    }

    /// Manager 进程的主窗口是否存在。
    pub fn is_running() -> bool {
        find_manager().is_some()
    }

    /// Manager 主窗口当前是否可见。
    pub fn is_visible() -> bool {
        find_manager().is_some_and(|hwnd| unsafe { IsWindowVisible(hwnd) }.as_bool())
    }

    /// 显示并前置 Manager 主窗口；窗口不存在时返回 false。
    pub fn show() -> bool {
        let Some(hwnd) = find_manager() else {
            return false;
        };
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
        true
    }

    /// 隐藏 Manager 主窗口；窗口不存在时返回 false。
    pub fn hide() -> bool {
        let Some(hwnd) = find_manager() else {
            return false;
        };
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        true
    }

    /// 切换可见性：可见则隐藏，否则显示前置。
    pub fn toggle() -> bool {
        if is_visible() { hide() } else { show() }
    }
}

#[cfg(windows)]
pub use imp::{hide, is_running, is_visible, show, toggle};

#[cfg(not(windows))]
mod imp_stub {
    /// 非 Windows 平台的占位实现（对齐原版：托盘仅 Windows 验证）。
    pub fn is_running() -> bool {
        false
    }
    pub fn is_visible() -> bool {
        false
    }
    pub fn show() -> bool {
        false
    }
    pub fn hide() -> bool {
        false
    }
    pub fn toggle() -> bool {
        false
    }
}

#[cfg(not(windows))]
pub use imp_stub::{hide, is_running, is_visible, show, toggle};
