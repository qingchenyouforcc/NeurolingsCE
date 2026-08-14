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
    GetMonitorInfoW, HMONITOR, MONITORINFO, ReleaseDC, SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CREATESTRUCTW, CS_DBLCLKS, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyMenu, DispatchMessageW, GWLP_USERDATA, GetClassNameW, GetCursorPos, GetDesktopWindow,
    GetForegroundWindow, GetMessagePos, GetShellWindow, GetSystemMetrics, GetWindowLongPtrW,
    GetWindowRect, GetWindowThreadProcessId, HMENU, IsIconic, IsWindow, IsWindowVisible,
    MB_ICONINFORMATION, MB_OK, MB_SYSTEMMODAL, MB_TOPMOST, MF_CHECKED, MF_POPUP, MF_SEPARATOR,
    MF_STRING, MONITORINFOF_PRIMARY, MSG, MessageBoxW, PM_REMOVE, PeekMessageW, RegisterClassExW,
    SM_CXSCREEN, SM_CYSCREEN, SPI_GETWORKAREA, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE,
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
                    if msg != WM_MOUSEMOVE
                        && std::env::var_os("NEUROLINGS_DEBUG").is_some()
                    {
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
                    let local = Point::new(
                        (lparam.0 & 0xFFFF) as i16 as i32,
                        ((lparam.0 >> 16) & 0xFFFF) as i16 as i32,
                    );
                    let pos = GetMessagePos();
                    let screen = Point::new(
                        (pos & 0xFFFF) as i16 as i32,
                        ((pos >> 16) & 0xFFFF) as i16 as i32,
                    );
                    let kind = match msg {
                        WM_LBUTTONDOWN => MascotEventKind::LeftDown,
                        WM_LBUTTONUP => MascotEventKind::LeftUp,
                        WM_LBUTTONDBLCLK => MascotEventKind::LeftDoubleClick,
                        WM_MOUSEMOVE => MascotEventKind::Move,
                        _ => MascotEventKind::RightUp,
                    };
                    event_queue().lock().unwrap().push_back(MascotEvent {
                        mascot_id,
                        kind,
                        screen,
                        local,
                    });
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
                let monitor = Rect {
                    left: mi.rcMonitor.left,
                    top: mi.rcMonitor.top,
                    right: mi.rcMonitor.right,
                    bottom: mi.rcMonitor.bottom,
                };
                let work_area = Rect {
                    left: mi.rcWork.left,
                    top: mi.rcWork.top,
                    right: mi.rcWork.right,
                    bottom: mi.rcWork.bottom,
                };
                if mi.dwFlags & MONITORINFOF_PRIMARY != 0 {
                    primary = monitor;
                }
                infos.push(ScreenInfo { monitor, work_area });
            }
            if infos.is_empty() {
                let width = GetSystemMetrics(SM_CXSCREEN);
                let height = GetSystemMetrics(SM_CYSCREEN);
                let mut work = RECT::default();
                let _ = SystemParametersInfoW(
                    SPI_GETWORKAREA,
                    0,
                    Some(std::ptr::addr_of_mut!(work).cast::<std::ffi::c_void>()),
                    Default::default(),
                );
                let work_area = Rect {
                    left: work.left,
                    top: work.top,
                    right: work.right,
                    bottom: work.bottom,
                };
                let monitor = Rect {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: height,
                };
                primary = monitor;
                infos.push(ScreenInfo { monitor, work_area });
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
        Point::new(p.x, p.y)
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
            let choice = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                at.x,
                at.y,
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
            // 自身窗口（含管理器进程内窗口）不作为交互目标。
            if pid == std::process::id() {
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
            Some(ActiveWindowInfo {
                handle: hwnd.0 as u64,
                area: Rect {
                    left: rect.left,
                    top: rect.top,
                    right: rect.right,
                    bottom: rect.bottom,
                },
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
        let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let text_w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            MessageBoxW(
                None,
                PCWSTR(text_w.as_ptr()),
                PCWSTR(title_w.as_ptr()),
                MB_OK | MB_ICONINFORMATION | MB_TOPMOST | MB_SYSTEMMODAL,
            );
        }
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
            bmi.bmiHeader.biWidth = width as i32;
            bmi.bmiHeader.biHeight = -(height as i32); // 负高度：自上而下的 DIB
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
            std::ptr::copy_nonoverlapping(
                bitmap_bgra_premul.as_ptr(),
                bits as *mut u8,
                bitmap_bgra_premul.len(),
            );
            let old = SelectObject(dc, windows::Win32::Graphics::Gdi::HGDIOBJ(hbmp.0));
            let dst = POINT {
                x: top_left.x,
                y: top_left.y,
            };
            let size = windows::Win32::Foundation::SIZE {
                cx: width as i32,
                cy: height as i32,
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
