//! Windows 后端：每只桌宠一个 WS_EX_LAYERED 置顶工具窗口，用
//! `UpdateLayeredWindow` 绘制预乘 BGRA。逐像素 alpha 同时提供透明
//! 与透明区域命中穿透。
//!
//! 另含：前台窗口追踪（桌宠站立/攀附的目标）、窗口推移（ThrowIE）、
//! 结构化弹出菜单（子菜单/勾选项）、检查器文本对话框。

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BLENDFUNCTION, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, EnumDisplayMonitors, GetDC,
    GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITORINFO,
    MonitorFromPoint, MonitorFromRect, MonitorFromWindow, ReleaseDC, SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, GetDpiForWindow,
    MDT_EFFECTIVE_DPI, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CREATESTRUCTW, CS_DBLCLKS, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyMenu, DispatchMessageW, GWLP_USERDATA, GetClassNameW, GetCursorPos, GetDesktopWindow,
    GetForegroundWindow, GetMessagePos, GetShellWindow, GetSystemMetrics, GetWindowLongPtrW,
    GetWindowRect, GetWindowThreadProcessId, HMENU, IsIconic, IsWindow, IsWindowVisible,
    MB_ICONINFORMATION, MB_OK, MB_TOPMOST, MF_CHECKED, MF_POPUP, MF_SEPARATOR, MF_STRING,
    MONITORINFOF_PRIMARY, MSG, MessageBoxW, PM_REMOVE, PeekMessageW, RegisterClassExW, SM_CXSCREEN,
    SM_CYSCREEN, SPI_GETWORKAREA, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, SystemParametersInfoW,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, ULW_ALPHA,
    UpdateLayeredWindow, WINDOW_EX_STYLE, WINDOW_STYLE, WM_DESTROY, WM_LBUTTONDBLCLK,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WM_RBUTTONUP, WNDCLASSEXW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, w};

use crate::{
    ActiveWindowInfo, MascotBackend, MascotEvent, MascotEventKind, MascotWindow, MenuEntry,
    PlatformError, PlatformResult, Point, Rect, ScreenInfo,
};

fn err(context: &str) -> PlatformError {
    PlatformError::Win32(format!("{context}: {}", std::io::Error::last_os_error()))
}

static EVENT_QUEUE: OnceLock<Mutex<VecDeque<MascotEvent>>> = OnceLock::new();

fn event_queue() -> &'static Mutex<VecDeque<MascotEvent>> {
    EVENT_QUEUE.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// 本进程创建的桌宠窗口注册表：用于活动窗口过滤与菜单宿主选择。
static WINDOW_REGISTRY: OnceLock<Mutex<Vec<usize>>> = OnceLock::new();

fn window_registry() -> &'static Mutex<Vec<usize>> {
    WINDOW_REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

const CLASS_NAME: PCWSTR = w!("NeurolingsRSMascotWindow");

/// 主显示器 DPI，作为取不到所在显示器 DPI 时的兜底。
fn desktop_dpi() -> u32 {
    unsafe {
        let monitor = MonitorFromWindow(GetDesktopWindow(), MONITOR_DEFAULTTOPRIMARY);
        monitor_dpi(monitor)
    }
}

/// 显示器有效 DPI；失败时回退桌面窗口 DPI，再不行取 96。
fn monitor_dpi(monitor: HMONITOR) -> u32 {
    unsafe {
        let mut x = 0u32;
        let mut y = 0u32;
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y).is_ok() && x > 0 {
            return x;
        }
        let dpi = GetDpiForWindow(GetDesktopWindow());
        if dpi == 0 { 96 } else { dpi }
    }
}

/// 显示器缩放：物理像素 / 96-DPI 逻辑像素。
fn monitor_scale(monitor: HMONITOR) -> f64 {
    monitor_dpi(monitor) as f64 / 96.0
}

/// 物理像素 → 96-DPI 逻辑像素，scale 取该坐标**所在显示器**的倍率。
fn to_logical_with(v: i32, scale: f64) -> i32 {
    if !scale.is_finite() || scale <= 0.0 || scale == 1.0 {
        v
    } else {
        (v as f64 / scale).round() as i32
    }
}

/// 96-DPI 逻辑像素 → 物理像素，scale 取目标显示器的倍率。
fn to_physical_with(v: i32, scale: f64) -> i32 {
    if !scale.is_finite() || scale <= 0.0 || scale == 1.0 {
        v
    } else {
        (v as f64 * scale).round() as i32
    }
}

/// 物理矩形 → 逻辑矩形，scale 必须为该显示器自身的倍率（逐屏换算，
/// 逻辑坐标系全局统一，对齐 Qt：逻辑 = 物理 ÷ 所在屏 scale）。
fn rect_to_logical_with(r: RECT, scale: f64) -> Rect {
    Rect {
        left: to_logical_with(r.left, scale),
        top: to_logical_with(r.top, scale),
        right: to_logical_with(r.right, scale),
        bottom: to_logical_with(r.bottom, scale),
    }
}

/// 物理屏幕点所在显示器的缩放（光标、鼠标事件的全局坐标用）。
fn scale_at_physical_point(p: POINT) -> f64 {
    unsafe { monitor_scale(MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST)) }
}

/// 逻辑屏幕点所在显示器的缩放：逐屏把物理矩形按各自倍率换成逻辑矩形后
/// 做包含判断；点落在屏间逻辑缝隙（混合 DPI 下相邻屏逻辑矩形不一定拼合）
/// 时回退主屏。
fn scale_at_logical_point(p: Point) -> f64 {
    unsafe {
        let mut monitors: Vec<HMONITOR> = Vec::new();
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_enum_proc),
            LPARAM(&mut monitors as *mut Vec<HMONITOR> as isize),
        );
        for handle in &monitors {
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(*handle, &mut mi).as_bool() {
                continue;
            }
            let scale = monitor_scale(*handle);
            if rect_to_logical_with(mi.rcMonitor, scale).contains(p) {
                return scale;
            }
        }
        desktop_dpi() as f64 / 96.0
    }
}

/// 把预乘 BGRA 位图按最近邻放大到目标尺寸（像素风桌宠，避免平滑带来的糊边）。
fn scale_bgra_nearest(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    let mut out = vec![0u8; dw as usize * dh as usize * 4];
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 {
        return out;
    }
    for y in 0..dh {
        let sy = y * sh / dh;
        for x in 0..dw {
            let sx = x * sw / dw;
            let si = ((sy * sw + sx) * 4) as usize;
            let di = ((y * dw + x) * 4) as usize;
            out[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    out
}

/// 将 Windows 鼠标消息映射为运行时事件。
///
/// 双击消息替代第二次 `WM_LBUTTONDOWN` 到达；先补发按下事件，才能让随后的
/// `WM_LBUTTONUP` 正常结束点击手势，再单独通知繁殖逻辑。
fn mouse_event_kinds(msg: u32) -> Option<(MascotEventKind, Option<MascotEventKind>)> {
    match msg {
        WM_LBUTTONDOWN => Some((MascotEventKind::LeftDown, None)),
        WM_LBUTTONUP => Some((MascotEventKind::LeftUp, None)),
        WM_LBUTTONDBLCLK => Some((
            MascotEventKind::LeftDown,
            Some(MascotEventKind::LeftDoubleClick),
        )),
        WM_MOUSEMOVE => Some((MascotEventKind::Move, None)),
        WM_RBUTTONUP => Some((MascotEventKind::RightUp, None)),
        _ => None,
    }
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_NCCREATE => {
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_LBUTTONDOWN | WM_LBUTTONUP | WM_LBUTTONDBLCLK | WM_MOUSEMOVE | WM_RBUTTONUP => {
                let mascot_id = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as u64;
                if mascot_id != 0 {
                    // 诊断：NEUROLINGS_DEBUG=1 时记录鼠标按下/松开/双击（不含移动）。
                    if msg != WM_MOUSEMOVE && std::env::var_os("NEUROLINGS_DEBUG").is_some() {
                        let pos = GetMessagePos();
                        let gx = (pos & 0xFFFF) as i16 as i32;
                        let gy = ((pos >> 16) & 0xFFFF) as i16 as i32;
                        let name = match msg {
                            WM_LBUTTONDOWN => "down",
                            WM_LBUTTONUP => "up",
                            WM_LBUTTONDBLCLK => "dbl",
                            _ => "right",
                        };
                        if let Ok(exe) = std::env::current_exe() {
                            let log = exe.with_file_name("neurolings_mouse_debug.log");
                            use std::io::Write;
                            if let Ok(mut f) = std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(&log)
                            {
                                let _ = writeln!(
                                    f,
                                    "mascot={id} {name} screen=({gx},{gy})",
                                    id = mascot_id
                                );
                            }
                        }
                    }
                    // 按住期间捕获鼠标，拖出窗口也能持续收到移动事件。
                    if msg == WM_LBUTTONDOWN {
                        SetCapture(hwnd);
                    } else if msg == WM_LBUTTONUP {
                        let _ = ReleaseCapture();
                    }
                    // 客户区坐标是窗口当前 DPI 下的物理像素，按窗口所在屏
                    // 的倍率换算成逻辑像素交给引擎。
                    let win_dpi = GetDpiForWindow(hwnd);
                    let win_scale = if win_dpi == 0 {
                        1.0
                    } else {
                        win_dpi as f64 / 96.0
                    };
                    let local = Point::new(
                        to_logical_with((lparam.0 & 0xFFFF) as i16 as i32, win_scale),
                        to_logical_with(((lparam.0 >> 16) & 0xFFFF) as i16 as i32, win_scale),
                    );
                    // GetCursorPos 是完整 32 位屏幕坐标，避免 GetMessagePos 的 16 位截断。
                    let mut cursor = POINT::default();
                    let _ = GetCursorPos(&mut cursor);
                    let cursor_scale = scale_at_physical_point(cursor);
                    let screen = Point::new(
                        to_logical_with(cursor.x, cursor_scale),
                        to_logical_with(cursor.y, cursor_scale),
                    );
                    let Some((kind, follow_up)) = mouse_event_kinds(msg) else {
                        return DefWindowProcW(hwnd, msg, wparam, lparam);
                    };
                    let mascot_event = MascotEvent {
                        mascot_id,
                        kind,
                        screen,
                        local,
                    };
                    let mut queue = event_queue().lock().unwrap();
                    queue.push_back(mascot_event);
                    if let Some(kind) = follow_up {
                        queue.push_back(MascotEvent {
                            kind,
                            ..mascot_event
                        });
                    }
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                window_registry()
                    .lock()
                    .unwrap()
                    .retain(|h| *h != hwnd.0 as usize);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

pub struct WindowsBackend {
    class_registered: bool,
}

impl WindowsBackend {
    pub fn new() -> PlatformResult<Self> {
        // 声明 PerMonitorV2 感知：光标、显示器与窗口矩形统一为物理像素。
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        Ok(Self {
            class_registered: false,
        })
    }

    fn ensure_class(&mut self) -> PlatformResult<()> {
        if self.class_registered {
            return Ok(());
        }
        let instance = unsafe { GetModuleHandleW(None).map_err(|_| err("GetModuleHandleW"))? };
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            lpszClassName: CLASS_NAME,
            // 需要双击事件。
            style: CS_DBLCLKS,
            ..Default::default()
        };
        unsafe {
            RegisterClassExW(&wc);
        }
        self.class_registered = true;
        Ok(())
    }

    /// 任选一个仍存活的桌宠窗口作为菜单宿主。
    fn live_hwnd(&self) -> Option<HWND> {
        let registry = window_registry().lock().unwrap();
        registry
            .iter()
            .copied()
            .map(|raw| HWND(raw as *mut std::ffi::c_void))
            .find(|h| unsafe { IsWindow(Some(*h)) }.as_bool())
    }
}

/// 构建结构化菜单；返回首个可用句柄供宿主选择。
unsafe fn build_menu(entries: &[MenuEntry]) -> PlatformResult<HMENU> {
    unsafe {
        let menu: HMENU = CreatePopupMenu().map_err(|_| err("CreatePopupMenu"))?;
        for entry in entries {
            match entry {
                MenuEntry::Item { id, label, checked } => {
                    let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
                    let flags = if *checked {
                        MF_STRING | MF_CHECKED
                    } else {
                        MF_STRING
                    };
                    let _ = AppendMenuW(menu, flags, *id as usize, PCWSTR(wide.as_ptr()));
                }
                MenuEntry::Submenu { label, entries } => {
                    let sub = build_menu(entries)?;
                    let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
                    let _ = AppendMenuW(menu, MF_POPUP, sub.0 as usize, PCWSTR(wide.as_ptr()));
                }
                MenuEntry::Separator => {
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                }
            }
        }
        Ok(menu)
    }
}

impl MascotBackend for WindowsBackend {
    fn create_window(&mut self, mascot_id: u64) -> PlatformResult<Box<dyn MascotWindow>> {
        self.ensure_class()?;
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(
                    WS_EX_LAYERED.0 | WS_EX_TOPMOST.0 | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0,
                ),
                CLASS_NAME,
                PCWSTR::null(),
                WINDOW_STYLE(WS_POPUP.0),
                -2000,
                -2000,
                1,
                1,
                None,
                None,
                None,
                Some(mascot_id as usize as *const _),
            )
        }
        .map_err(|_| err("CreateWindowExW"))?;
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        window_registry().lock().unwrap().push(hwnd.0 as usize);
        Ok(Box::new(LayeredWindow { hwnd }))
    }

    fn screens(&self) -> Vec<ScreenInfo> {
        unsafe {
            let mut infos = Vec::new();
            let mut primary = Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            };
            let mut monitors: Vec<HMONITOR> = Vec::new();
            let _ = EnumDisplayMonitors(
                None,
                None,
                Some(monitor_enum_proc),
                LPARAM(&mut monitors as *mut Vec<HMONITOR> as isize),
            );
            for handle in &monitors {
                let mut mi = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                if !GetMonitorInfoW(*handle, &mut mi).as_bool() {
                    continue;
                }
                // 逐屏 DPI：每台的物理矩形除以各自的倍率得到统一逻辑矩形。
                let scale = monitor_scale(*handle);
                let monitor = rect_to_logical_with(mi.rcMonitor, scale);
                let work_area = rect_to_logical_with(mi.rcWork, scale);
                if mi.dwFlags & MONITORINFOF_PRIMARY != 0 {
                    primary = monitor;
                }
                infos.push(ScreenInfo {
                    monitor,
                    work_area,
                    scale,
                });
            }
            if infos.is_empty() {
                let scale = desktop_dpi() as f64 / 96.0;
                let width = GetSystemMetrics(SM_CXSCREEN);
                let height = GetSystemMetrics(SM_CYSCREEN);
                let mut work = RECT::default();
                let _ = SystemParametersInfoW(
                    SPI_GETWORKAREA,
                    0,
                    Some(std::ptr::addr_of_mut!(work).cast::<std::ffi::c_void>()),
                    Default::default(),
                );
                let work_area = rect_to_logical_with(work, scale);
                let monitor = Rect {
                    left: 0,
                    top: 0,
                    right: to_logical_with(width, scale),
                    bottom: to_logical_with(height, scale),
                };
                primary = monitor;
                infos.push(ScreenInfo {
                    monitor,
                    work_area,
                    scale,
                });
            }
            // 主显示器排最前，运行时的默认环境取第一项。
            infos.sort_by_key(|s| if s.monitor == primary { 0 } else { 1 });
            infos
        }
    }

    fn cursor_pos(&self) -> Point {
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
        }
        // 光标所在屏的倍率换算成逻辑像素。
        let scale = scale_at_physical_point(p);
        Point::new(to_logical_with(p.x, scale), to_logical_with(p.y, scale))
    }

    fn pump_events(&mut self) -> Vec<MascotEvent> {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let mut queue = event_queue().lock().unwrap();
        queue.drain(..).collect()
    }

    fn show_menu(&mut self, at: Point, entries: &[MenuEntry]) -> PlatformResult<Option<u32>> {
        if entries.is_empty() {
            return Ok(None);
        }
        unsafe {
            let menu = build_menu(entries)?;
            let owner = self.live_hwnd().unwrap_or_default();
            // 桌宠窗口不抢焦点（WS_EX_NOACTIVATE）；TrackPopupMenu 需要一个
            // 前台窗口才能正常接收选择。
            if !owner.is_invalid() {
                let _ = SetForegroundWindow(owner);
            }
            // TrackPopupMenu 要物理屏幕坐标；引擎传来的是逻辑像素，
            // 按目标点所在屏的倍率换算。
            let scale = scale_at_logical_point(at);
            let choice = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                to_physical_with(at.x, scale),
                to_physical_with(at.y, scale),
                None,
                owner,
                None,
            );
            let _ = DestroyMenu(menu);
            Ok((choice.0 > 0).then_some(choice.0 as u32))
        }
    }

    fn active_window(&mut self) -> Option<ActiveWindowInfo> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd == HWND::default() {
                return None;
            }
            let mut pid = 0u32;
            if GetWindowThreadProcessId(hwnd, Some(&mut pid)) == 0 {
                return None;
            }
            // 自身窗口不作为交互目标。管理器是独立进程，还要按 HWND 排除。
            if pid == std::process::id() {
                return None;
            }
            if crate::manager_window::is_hwnd(hwnd.0 as usize) {
                return None;
            }
            if hwnd == GetDesktopWindow() || hwnd == GetShellWindow() {
                return None;
            }
            // 任务栏与桌面壳窗口不作为交互目标。
            let mut class_buf = [0u16; 64];
            let len = GetClassNameW(hwnd, &mut class_buf);
            if len > 0 {
                let class = String::from_utf16_lossy(&class_buf[..len as usize]);
                if matches!(
                    class.as_str(),
                    "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "WorkerW" | "Progman"
                ) {
                    return None;
                }
            }
            if !IsWindow(Some(hwnd)).as_bool()
                || !IsWindowVisible(hwnd).as_bool()
                || IsIconic(hwnd).as_bool()
            {
                return None;
            }
            let mut rect = RECT::default();
            GetWindowRect(hwnd, &mut rect).ok()?;
            if rect.right <= rect.left || rect.bottom <= rect.top {
                return None;
            }
            // 注册表里的桌宠窗口即使拿到前台也不作为目标。
            if window_registry()
                .lock()
                .unwrap()
                .contains(&(hwnd.0 as usize))
            {
                return None;
            }
            // 窗口矩形按其面积最大所在屏的倍率换算成逻辑像素。
            let scale = monitor_scale(MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST));
            Some(ActiveWindowInfo {
                handle: hwnd.0 as u64,
                // 对齐 C++（pid-HWND 字符串）的窗口身份：HWND 已足够区分。
                uid: hwnd.0 as u64,
                area: rect_to_logical_with(rect, scale),
            })
        }
    }

    fn supports_window_pushing(&self) -> bool {
        true
    }

    fn push_window(&mut self, target: u64, dx: f64, dy: f64) -> bool {
        if !dx.is_finite() || !dy.is_finite() {
            return false;
        }
        // 桌宠包给出的推移量必须有界：超出范围的取值几乎必然是损坏数据。
        let dx = dx.clamp(-2000.0, 2000.0);
        let dy = dy.clamp(-2000.0, 2000.0);
        let hwnd = HWND(target as usize as *mut std::ffi::c_void);
        unsafe {
            if !IsWindow(Some(hwnd)).as_bool()
                || !IsWindowVisible(hwnd).as_bool()
                || IsIconic(hwnd).as_bool()
                || GetForegroundWindow() != hwnd
            {
                return false;
            }
            let mut pid = 0u32;
            if GetWindowThreadProcessId(hwnd, Some(&mut pid)) == 0 || pid == std::process::id() {
                return false;
            }
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_err() {
                return false;
            }
            // 目标窗口的物理像素偏移按其自身 DPI 换算。
            let dpi = GetDpiForWindow(hwnd);
            let scale = if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 };
            let offset_x = (dx * scale).round() as i32;
            let offset_y = (dy * scale).round() as i32;
            if offset_x == 0 && offset_y == 0 {
                return false;
            }
            SetWindowPos(
                hwnd,
                None,
                rect.left + offset_x,
                rect.top + offset_y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            )
            .is_ok()
        }
    }

    fn show_text_dialog(&mut self, title: &str, text: &str) {
        // 不在主循环线程弹模态框：MessageBox 会卡住全部桌宠的 tick。
        let title = title.to_string();
        let text = text.to_string();
        std::thread::spawn(move || {
            let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
            let text_w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            unsafe {
                MessageBoxW(
                    None,
                    PCWSTR(text_w.as_ptr()),
                    PCWSTR(title_w.as_ptr()),
                    MB_OK | MB_ICONINFORMATION | MB_TOPMOST,
                );
            }
        });
    }
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe extern "system" fn monitor_enum_proc(
    monitor: HMONITOR,
    _hdc: windows::Win32::Graphics::Gdi::HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> windows::core::BOOL {
    unsafe {
        let monitors = &mut *(data.0 as *mut Vec<HMONITOR>);
        monitors.push(monitor);
        windows::core::BOOL(1)
    }
}

struct LayeredWindow {
    hwnd: HWND,
}

impl MascotWindow for LayeredWindow {
    fn update_frame(
        &mut self,
        bitmap_bgra_premul: &[u8],
        width: u32,
        height: u32,
        top_left: Point,
    ) -> PlatformResult<()> {
        if bitmap_bgra_premul.len() != (width as usize) * (height as usize) * 4
            || width == 0
            || height == 0
        {
            return Err(PlatformError::Win32("invalid bitmap".into()));
        }
        // 引擎给的是逻辑像素位图；按窗口落点所在显示器的倍率放大后再提交，
        // 桌宠跨屏移动时尺寸跟随目标屏（对齐 Qt 的 per-screen devicePixelRatio）。
        let scale = scale_at_logical_point(top_left);
        let scaled;
        let (draw_w, draw_h, pixels): (u32, u32, &[u8]) = if scale == 1.0 {
            (width, height, bitmap_bgra_premul)
        } else {
            let dw = ((width as f64 * scale).round().max(1.0)) as u32;
            let dh = ((height as f64 * scale).round().max(1.0)) as u32;
            scaled = scale_bgra_nearest(bitmap_bgra_premul, width, height, dw, dh);
            (dw, dh, scaled.as_slice())
        };
        unsafe {
            let screen_dc = GetDC(None);
            if screen_dc.is_invalid() {
                return Err(err("GetDC"));
            }
            let dc = CreateCompatibleDC(Some(screen_dc));
            if dc.is_invalid() {
                let _ = ReleaseDC(None, screen_dc);
                return Err(err("CreateCompatibleDC"));
            }
            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader.biSize =
                std::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = draw_w as i32;
            bmi.bmiHeader.biHeight = -(draw_h as i32); // 负高度：自上而下的 DIB
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB.0;
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let hbmp = match CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(hbmp) => hbmp,
                Err(_) => {
                    let _ = DeleteDC(dc);
                    let _ = ReleaseDC(None, screen_dc);
                    return Err(err("CreateDIBSection"));
                }
            };
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len());
            let old = SelectObject(dc, windows::Win32::Graphics::Gdi::HGDIOBJ(hbmp.0));
            let dst = POINT {
                x: to_physical_with(top_left.x, scale),
                y: to_physical_with(top_left.y, scale),
            };
            let size = windows::Win32::Foundation::SIZE {
                cx: draw_w as i32,
                cy: draw_h as i32,
            };
            let src = POINT::default();
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let ok = UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&dst),
                Some(&size),
                Some(dc),
                Some(&src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
            .is_ok();
            SelectObject(dc, old);
            let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(hbmp.0));
            let _ = DeleteDC(dc);
            let _ = ReleaseDC(None, screen_dc);
            if !ok {
                return Err(err("UpdateLayeredWindow"));
            }
        }
        Ok(())
    }
}

impl Drop for LayeredWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.hwnd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        mouse_event_kinds, rect_to_logical_with, scale_bgra_nearest, to_logical_with,
        to_physical_with,
    };
    use crate::MascotEventKind;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::WM_LBUTTONDBLCLK;

    #[test]
    fn scale_bgra_nearest_doubles_a_pixel() {
        let src = [10u8, 20, 30, 40];
        let out = scale_bgra_nearest(&src, 1, 1, 2, 2);
        assert_eq!(out.len(), 16);
        assert_eq!(&out[0..4], &[10, 20, 30, 40]);
        assert_eq!(&out[12..16], &[10, 20, 30, 40]);
    }

    /// 96 DPI（scale=1.0）下物理与逻辑一致。
    #[test]
    fn conversion_is_identity_at_scale_one() {
        assert_eq!(to_logical_with(-300, 1.0), -300);
        assert_eq!(to_logical_with(1920, 1.0), 1920);
        assert_eq!(to_physical_with(-300, 1.0), -300);
        assert_eq!(to_physical_with(1920, 1.0), 1920);
    }

    /// 150% 缩放（144 DPI）：物理 150 ↔ 逻辑 100。
    #[test]
    fn conversion_uses_given_monitor_scale() {
        assert_eq!(to_logical_with(150, 1.5), 100);
        assert_eq!(to_physical_with(100, 1.5), 150);
        assert_eq!(to_logical_with(101, 1.5), 67); // 四舍五入
        // 非法 scale 直接透传，避免除零/NaN。
        assert_eq!(to_logical_with(42, 0.0), 42);
        assert_eq!(to_physical_with(42, f64::NAN), 42);
    }

    /// 主屏左侧的副屏（物理坐标为负）按自身倍率换算，符号保持。
    #[test]
    fn negative_coords_convert_with_own_scale() {
        // 副屏 144 DPI、位于主屏左侧：物理 [-1920, 0) → 逻辑 [-1280, 0)。
        assert_eq!(to_logical_with(-1920, 1.5), -1280);
        assert_eq!(to_physical_with(-1280, 1.5), -1920);
    }

    /// 矩形逐边换算：混合 DPI 下两台屏的逻辑矩形各自独立。
    #[test]
    fn rect_converts_per_monitor_scale() {
        let r = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        let logical = rect_to_logical_with(r, 1.5);
        assert_eq!(logical.left, -1280);
        assert_eq!(logical.right, 0);
        assert_eq!(logical.bottom, 720);
    }

    /// 同一点在不同 scale 下得到不同逻辑值——这正是逐屏换算的意义。
    #[test]
    fn same_physical_point_differs_by_monitor_scale() {
        assert_eq!(to_logical_with(2880, 1.5), 1920);
        assert_eq!(to_logical_with(2880, 1.0), 2880);
    }

    /// 第二击必须保留按下和双击两个阶段，后续松开才能结束手势。
    #[test]
    fn double_click_replays_second_press_before_breeding_event() {
        assert_eq!(
            mouse_event_kinds(WM_LBUTTONDBLCLK),
            Some((
                MascotEventKind::LeftDown,
                Some(MascotEventKind::LeftDoubleClick),
            )),
        );
    }
}
