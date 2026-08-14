//! 操作系统抽象层：透明置顶桌宠窗口、鼠标事件、弹出菜单、显示器、
//! 光标、活动窗口追踪、窗口推移与本地 IPC 传输。
//!
//! Windows 使用分层窗口后端（逐像素 alpha + 命中穿透），Linux 使用
//! X11 ARGB 后端（Wayland 会话经 XWayland 运行），macOS 使用 NSPanel 后端。
//!
//! 坐标约定：全部使用物理像素。Windows 后端在启动时声明 PerMonitorV2
//! DPI 感知，保证光标、显示器与窗口矩形处于同一坐标空间。

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub mod pipe;

#[cfg(target_os = "linux")]
pub mod x11;

#[cfg(target_os = "macos")]
pub mod macos;

pub mod autostart;
pub mod bubble;
pub mod ipc;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("platform backend unavailable")]
    Unsupported,
    #[error("win32 error: {0}")]
    Win32(String),
}

pub type PlatformResult<T> = Result<T, PlatformError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.left && p.x < self.right && p.y >= self.top && p.y < self.bottom
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ScreenInfo {
    pub monitor: Rect,
    pub work_area: Rect,
}

/// 前台活动窗口的快照；句柄供窗口推移功能回指。
#[derive(Debug, Clone, Copy)]
pub struct ActiveWindowInfo {
    pub handle: u64,
    pub area: Rect,
}

/// 结构化弹出菜单项。
#[derive(Debug, Clone)]
pub enum MenuEntry {
    /// 普通菜单项；id 为回调编号，checked 显示勾选标记。
    Item {
        id: u32,
        label: String,
        checked: bool,
    },
    /// 子菜单。
    Submenu {
        label: String,
        entries: Vec<MenuEntry>,
    },
    Separator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MascotEventKind {
    LeftDown,
    LeftUp,
    LeftDoubleClick,
    Move,
    RightUp,
}

/// 鼠标事件；local 为相对桌宠窗口左上角的坐标，screen 为全局屏幕坐标。
#[derive(Debug, Clone, Copy)]
pub struct MascotEvent {
    pub mascot_id: u64,
    pub kind: MascotEventKind,
    pub screen: Point,
    pub local: Point,
}

/// 透明无边框、置顶、逐像素 alpha 的桌宠窗口。
pub trait MascotWindow {
    /// 重绘并移动窗口。位图为预乘 BGRA。
    fn update_frame(
        &mut self,
        bitmap_bgra_premul: &[u8],
        width: u32,
        height: u32,
        top_left: Point,
    ) -> PlatformResult<()>;
}

/// 平台后端：窗口工厂与全局服务。
///
/// 新增方法均带默认实现，尚不支持的平台自动降级为空操作。
pub trait MascotBackend {
    fn create_window(&mut self, mascot_id: u64) -> PlatformResult<Box<dyn MascotWindow>>;
    fn screens(&self) -> Vec<ScreenInfo>;
    fn cursor_pos(&self) -> Point;
    fn pump_events(&mut self) -> Vec<MascotEvent>;

    /// 阻塞式弹出菜单；返回被选中项的 id，取消返回 None。
    fn show_menu(&mut self, at: Point, entries: &[MenuEntry]) -> PlatformResult<Option<u32>> {
        let _ = (at, entries);
        Ok(None)
    }

    /// 当前前台窗口（过滤掉自身窗口、任务栏与桌面壳窗口）。
    fn active_window(&mut self) -> Option<ActiveWindowInfo> {
        None
    }

    /// 平台是否支持推移外部窗口（ThrowIE 动作）。
    fn supports_window_pushing(&self) -> bool {
        false
    }

    /// 把指定窗口平移 (dx, dy) 物理像素；返回是否成功。
    fn push_window(&mut self, target: u64, dx: f64, dy: f64) -> bool {
        let _ = (target, dx, dy);
        false
    }

    /// 显示检查器文本对话框（模态、置顶）。
    fn show_text_dialog(&mut self, title: &str, text: &str) {
        let _ = (title, text);
    }
}

pub fn create_backend() -> PlatformResult<Box<dyn MascotBackend>> {
    #[cfg(windows)]
    {
        Ok(Box::new(windows::WindowsBackend::new()?))
    }
    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(x11::X11Backend::new()?))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(macos::MacOSBackend::new()?))
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        Err(PlatformError::Unsupported)
    }
}
