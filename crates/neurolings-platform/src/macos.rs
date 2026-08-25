//! macOS 后端：无边框浮动 NSWindow，逐像素透明。
//! 每只桌宠一个窗口：内容视图绘制帧并做基于 alpha 的命中测试，
//! 使点击穿透透明像素。

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::ffi::c_void;
use std::sync::{LazyLock, Mutex};

use objc2::rc::{Retained, autoreleasepool};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSBitmapImageRep, NSColor,
    NSCompositingOperation, NSControlStateValueOff, NSControlStateValueOn, NSDeviceRGBColorSpace,
    NSEvent, NSEventMask, NSEventType, NSImage, NSMenu, NSMenuItem, NSScreen, NSView, NSWindow,
    NSWindowCollectionBehavior, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{
    NSDate, NSDefaultRunLoopMode, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use crate::{
    ActiveWindowInfo, MascotBackend, MascotEvent, MascotWindow, PlatformError, PlatformResult,
    Point, Rect, ScreenInfo,
};

static EVENT_QUEUE: Mutex<Vec<MascotEvent>> = Mutex::new(Vec::new());
/// 窗口指针（usize）→ 桌宠 id。
static WINDOW_IDS: LazyLock<Mutex<HashMap<usize, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// 右键事件与随后同步弹出的菜单宿主一一对应，避免多事件同帧时错用窗口。
static CONTEXT_MENU_SELECTION: Mutex<Option<u32>> = Mutex::new(None);

/// AppKit 全局坐标系的虚拟桌面边界。
#[derive(Clone, Copy)]
struct DesktopBounds {
    left: f64,
    bottom: f64,
    right: f64,
    top: f64,
}

impl DesktopBounds {
    fn fallback() -> Self {
        Self {
            left: 0.0,
            bottom: 0.0,
            right: 1920.0,
            top: 1080.0,
        }
    }
}

fn desktop_bounds(mtm: MainThreadMarker) -> DesktopBounds {
    let screens = NSScreen::screens(mtm);
    let mut screens = screens.iter();
    let Some(first) = screens.next() else {
        return DesktopBounds::fallback();
    };
    let first = first.frame();
    let mut bounds = DesktopBounds {
        left: first.origin.x,
        bottom: first.origin.y,
        right: first.origin.x + first.size.width,
        top: first.origin.y + first.size.height,
    };
    for screen in screens {
        let frame = screen.frame();
        bounds.left = bounds.left.min(frame.origin.x);
        bounds.bottom = bounds.bottom.min(frame.origin.y);
        bounds.right = bounds.right.max(frame.origin.x + frame.size.width);
        bounds.top = bounds.top.max(frame.origin.y + frame.size.height);
    }
    bounds
}

fn logical_coordinate(value: f64) -> i32 {
    value.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

/// 将 AppKit 左下全局坐标转为运行时使用的虚拟桌面左上逻辑坐标。
fn appkit_point_to_global(bounds: DesktopBounds, point: NSPoint) -> Point {
    Point::new(
        logical_coordinate(point.x - bounds.left),
        logical_coordinate(bounds.top - point.y),
    )
}

/// 将运行时的虚拟桌面左上逻辑坐标转回 AppKit 左下全局坐标。
fn global_point_to_appkit(bounds: DesktopBounds, point: Point) -> NSPoint {
    NSPoint::new(bounds.left + point.x as f64, bounds.top - point.y as f64)
}

fn appkit_rect_to_global(bounds: DesktopBounds, rect: NSRect) -> Rect {
    Rect {
        left: logical_coordinate(rect.origin.x - bounds.left),
        top: logical_coordinate(bounds.top - (rect.origin.y + rect.size.height)),
        right: logical_coordinate(rect.origin.x + rect.size.width - bounds.left),
        bottom: logical_coordinate(bounds.top - rect.origin.y),
    }
}

type AxError = i32;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXUIElementCreateApplication(pid: i32) -> *const c_void;
    fn AXUIElementCopyAttributeValue(
        element: *const c_void,
        attribute: *const c_void,
        value: *mut *const c_void,
    ) -> AxError;
    fn AXUIElementGetPid(element: *const c_void, pid: *mut i32) -> AxError;
    fn AXValueGetValue(value: *const c_void, value_type: i32, out_value: *mut c_void) -> u8;
    fn _AXUIElementGetWindow(element: *const c_void, window_id: *mut u32) -> AxError;

    static kAXFocusedWindowAttribute: *const c_void;
    static kAXPositionAttribute: *const c_void;
    static kAXSizeAttribute: *const c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const c_void);
}

/// 持有由 AX "Create" / "Copy" 规则返回的 Core Foundation 对象。
struct AxObject(*const c_void);

impl AxObject {
    fn from_owned(value: *const c_void) -> Option<Self> {
        (!value.is_null()).then_some(Self(value))
    }
}

impl Drop for AxObject {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.0);
        }
    }
}

fn ax_copy_attribute(element: &AxObject, attribute: *const c_void) -> Option<AxObject> {
    let mut value = std::ptr::null();
    let result = unsafe { AXUIElementCopyAttributeValue(element.0, attribute, &mut value) };
    (result == 0).then(|| AxObject::from_owned(value)).flatten()
}

const AX_VALUE_CG_POINT: i32 = 1;
const AX_VALUE_CG_SIZE: i32 = 2;

/// 将辅助功能 API 相对主显示器左上角的坐标转换为运行时虚拟桌面坐标。
fn ax_rect_to_global(
    bounds: DesktopBounds,
    main_frame: NSRect,
    origin: NSPoint,
    size: NSSize,
) -> Rect {
    let appkit_rect = NSRect::new(
        NSPoint::new(
            main_frame.origin.x + origin.x,
            main_frame.origin.y + main_frame.size.height - origin.y - size.height,
        ),
        size,
    );
    appkit_rect_to_global(bounds, appkit_rect)
}

#[allow(dead_code)]
fn err(context: &str) -> PlatformError {
    PlatformError::Win32(format!("AppKit {context}"))
}

struct MascotViewIvars {
    image: RefCell<Option<Retained<NSImage>>>,
    /// 位图引用此缓冲，需保持存活。
    backing: RefCell<Vec<u8>>,
    alpha_mask: RefCell<Vec<u8>>,
    frame_size: RefCell<(u32, u32)>,
}

define_class!(
    // SAFETY：NSView 无特殊子类化要求；MascotView 未实现 Drop。
    #[unsafe(super = NSView)]
    #[name = "NeurolingsMascotView"]
    #[ivars = MascotViewIvars]
    struct MascotView;

    // SAFETY：NSObjectProtocol 无安全要求。
    unsafe impl NSObjectProtocol for MascotView {}

    impl MascotView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _rect: NSRect) {
            if let Some(image) = &*self.ivars().image.borrow() {
                let bounds = NSView::bounds(self);
                image.drawInRect_fromRect_operation_fraction(
                    bounds,
                    NSRect::new(NSPoint::new(0.0, 0.0), bounds.size),
                    NSCompositingOperation::Copy,
                    1.0,
                );
            }
        }

        #[unsafe(method(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> *mut NSView {
            let (w, h) = *self.ivars().frame_size.borrow();
            if w == 0 || h == 0 {
                return std::ptr::null_mut();
            }
            let mask = self.ivars().alpha_mask.borrow();
            let x = point.x as i64;
            let y = point.y as i64;
            if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
                return std::ptr::null_mut();
            }
            let idx = ((y * w as i64 + x) * 4 + 3) as usize;
            if mask.get(idx).copied().unwrap_or(0) == 0 {
                return std::ptr::null_mut();
            }
            self as *const MascotView as *mut NSView
        }
    }
);

impl MascotView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(MascotViewIvars {
            image: RefCell::new(None),
            backing: RefCell::new(Vec::new()),
            alpha_mask: RefCell::new(Vec::new()),
            frame_size: RefCell::new((0, 0)),
        });
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    fn set_frame_data(&self, rgba: &[u8], width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        *self.ivars().alpha_mask.borrow_mut() = rgba.to_vec();
        *self.ivars().frame_size.borrow_mut() = (width, height);

        *self.ivars().backing.borrow_mut() = rgba.to_vec();
        let backing = self.ivars().backing.borrow();
        let mut planes = [backing.as_ptr() as *mut u8];
        let rep = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(),
                planes.as_mut_ptr(),
                    width as isize,
                    height as isize,
                    8,
                    4,
                    true,
                    false,
                    NSDeviceRGBColorSpace,
                    (width * 4) as isize,
                    32,
                )
        };
        drop(backing);
        if let Some(rep) = rep {
            let image =
                NSImage::initWithSize(NSImage::alloc(), NSSize::new(width as f64, height as f64));
            image.addRepresentation(&rep);
            *self.ivars().image.borrow_mut() = Some(image);
        }
    }
}

struct ContextMenuTargetIvars {
    item_id: u32,
}

define_class!(
    #[unsafe(super = NSObject)]
    #[name = "NeurolingsContextMenuTarget"]
    #[ivars = ContextMenuTargetIvars]
    struct ContextMenuTarget;

    impl ContextMenuTarget {
        #[unsafe(method(onMenuAction:))]
        fn on_menu_action(&self, _sender: Option<&NSObject>) {
            if let Ok(mut selected) = CONTEXT_MENU_SELECTION.lock() {
                *selected = Some(self.ivars().item_id);
            }
        }
    }
);

impl ContextMenuTarget {
    fn new(item_id: u32) -> Retained<Self> {
        let this = Self::alloc().set_ivars(ContextMenuTargetIvars { item_id });
        unsafe { msg_send![super(this), init] }
    }
}

fn append_context_entries(
    menu: &NSMenu,
    entries: &[crate::MenuEntry],
    targets: &mut Vec<Retained<ContextMenuTarget>>,
    mtm: MainThreadMarker,
) {
    for entry in entries {
        match entry {
            crate::MenuEntry::Item { id, label, checked } => {
                let item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(label),
                        None,
                        &NSString::from_str(""),
                    )
                };
                let target = ContextMenuTarget::new(*id);
                // NSMenuItem 对 target 为弱引用，弹出期间由 targets 持有。
                unsafe {
                    let _: () = msg_send![&*item, setTarget: Some(&*target)];
                    let _: () = msg_send![&*item, setAction: objc2::sel!(onMenuAction:)];
                }
                item.setState(if *checked {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
                targets.push(target);
                menu.addItem(&item);
            }
            crate::MenuEntry::Submenu { label, entries } => {
                let submenu = NSMenu::new(mtm);
                append_context_entries(&submenu, entries, targets, mtm);
                let item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(label),
                        None,
                        &NSString::from_str(""),
                    )
                };
                item.setSubmenu(Some(&submenu));
                menu.addItem(&item);
            }
            crate::MenuEntry::Separator => {
                menu.addItem(&NSMenuItem::separatorItem(mtm));
            }
        }
    }
}

pub struct MacOSBackend {
    menu_hosts: VecDeque<Retained<NSWindow>>,
    last_active_pid: Option<i32>,
}

impl MacOSBackend {
    pub fn new() -> PlatformResult<Self> {
        let Some(mtm) = MainThreadMarker::new() else {
            return Err(err("must run on the main thread"));
        };
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        Ok(Self {
            menu_hosts: VecDeque::new(),
            last_active_pid: None,
        })
    }

    fn queue_mascot_event(
        &mut self,
        event: &NSEvent,
        mtm: MainThreadMarker,
        bounds: DesktopBounds,
    ) {
        let kind = match event.r#type() {
            NSEventType::LeftMouseDown => crate::MascotEventKind::LeftDown,
            NSEventType::LeftMouseUp => crate::MascotEventKind::LeftUp,
            NSEventType::LeftMouseDragged => crate::MascotEventKind::Move,
            NSEventType::RightMouseUp => crate::MascotEventKind::RightUp,
            _ => return,
        };
        let Some(window) = event.window(mtm) else {
            return;
        };
        let key = Retained::as_ptr(&window) as usize;
        let Some(mascot_id) = WINDOW_IDS
            .lock()
            .ok()
            .and_then(|ids| ids.get(&key).copied())
        else {
            return;
        };
        let frame = window.frame();
        let location = event.locationInWindow();
        let screen = appkit_point_to_global(
            bounds,
            NSPoint::new(frame.origin.x + location.x, frame.origin.y + location.y),
        );
        let local = Point::new(
            logical_coordinate(location.x),
            logical_coordinate(frame.size.height - location.y),
        );
        let mascot_event = MascotEvent {
            mascot_id,
            kind,
            screen,
            local,
        };
        if let Ok(mut queue) = EVENT_QUEUE.lock() {
            queue.push(mascot_event);
            // AppKit 的第二次左键按下携带 clickCount=2；既定契约要求双击也先执行按下逻辑。
            if kind == crate::MascotEventKind::LeftDown && event.clickCount() >= 2 {
                queue.push(MascotEvent {
                    kind: crate::MascotEventKind::LeftDoubleClick,
                    ..mascot_event
                });
            }
        }
        if kind == crate::MascotEventKind::RightUp {
            self.menu_hosts.push_back(window);
        }
    }
}

impl MascotBackend for MacOSBackend {
    fn create_window(&mut self, mascot_id: u64) -> PlatformResult<Box<dyn MascotWindow>> {
        let Some(mtm) = MainThreadMarker::new() else {
            return Err(err("must run on the main thread"));
        };
        let frame = NSRect::new(NSPoint::new(-2000.0, -2000.0), NSSize::new(1.0, 1.0));
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                frame,
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        window.setLevel(objc2_app_kit::NSFloatingWindowLevel);
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        window.setHasShadow(false);
        window.setIgnoresMouseEvents(false);
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );

        let view = MascotView::new(mtm, frame);
        window.setContentView(Some(&view));
        window.orderFront(None);

        let key = Retained::as_ptr(&window) as usize;
        WINDOW_IDS.lock().unwrap().insert(key, mascot_id);

        Ok(Box::new(MacOSWindow { window, view, key }))
    }

    fn screens(&self) -> Vec<ScreenInfo> {
        let fallback = vec![ScreenInfo {
            monitor: Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            work_area: Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1040,
            },
            scale: 1.0,
        }];
        let Some(mtm) = MainThreadMarker::new() else {
            return fallback;
        };
        let bounds = desktop_bounds(mtm);
        let mut infos = Vec::new();
        for screen in NSScreen::screens(mtm).iter() {
            let frame = screen.frame();
            let visible = screen.visibleFrame();
            infos.push(ScreenInfo {
                monitor: appkit_rect_to_global(bounds, frame),
                work_area: appkit_rect_to_global(bounds, visible),
                // AppKit 的 frame 已是点（逻辑）坐标，scale 仅作信息暴露。
                scale: screen.backingScaleFactor(),
            });
        }
        if infos.is_empty() { fallback } else { infos }
    }

    fn cursor_pos(&self) -> Point {
        let location = NSEvent::mouseLocation();
        let bounds = MainThreadMarker::new()
            .map(desktop_bounds)
            .unwrap_or_else(DesktopBounds::fallback);
        appkit_point_to_global(bounds, location)
    }

    fn pump_events(&mut self) -> Vec<MascotEvent> {
        if let Some(mtm) = MainThreadMarker::new() {
            autoreleasepool(|_| {
                let app = NSApplication::sharedApplication(mtm);
                let deadline = NSDate::distantPast();
                let bounds = desktop_bounds(mtm);
                // Foundation 导出的运行循环模式是外部静态对象，只在主线程读取。
                let mode = unsafe { NSDefaultRunLoopMode };
                while let Some(event) = app.nextEventMatchingMask_untilDate_inMode_dequeue(
                    NSEventMask::Any,
                    Some(&deadline),
                    mode,
                    true,
                ) {
                    self.queue_mascot_event(&event, mtm, bounds);
                    // 继续交给 AppKit，托盘菜单 target 和系统窗口行为才能被消费。
                    app.sendEvent(&event);
                }
            });
        }
        EVENT_QUEUE.lock().unwrap().drain(..).collect()
    }

    fn show_menu(
        &mut self,
        at: Point,
        entries: &[crate::MenuEntry],
    ) -> PlatformResult<Option<u32>> {
        let Some(mtm) = MainThreadMarker::new() else {
            return Err(err("must show menus on the main thread"));
        };
        let Some(window) = self.menu_hosts.pop_front() else {
            return Ok(None);
        };
        if entries.is_empty() {
            return Ok(None);
        }
        let Some(view) = window.contentView() else {
            return Ok(None);
        };
        let menu = NSMenu::new(mtm);
        let mut targets = Vec::new();
        append_context_entries(&menu, entries, &mut targets, mtm);
        let bounds = desktop_bounds(mtm);
        let appkit_point = global_point_to_appkit(bounds, at);
        let frame = window.frame();
        // MascotView 是翻转视图，菜单锚点也须转换为视图左上坐标。
        let point = NSPoint::new(
            appkit_point.x - frame.origin.x,
            frame.size.height - (appkit_point.y - frame.origin.y),
        );
        if let Ok(mut selected) = CONTEXT_MENU_SELECTION.lock() {
            *selected = None;
        }
        let _ = menu.popUpMenuPositioningItem_atLocation_inView(None, point, Some(&view));
        CONTEXT_MENU_SELECTION
            .lock()
            .ok()
            .and_then(|mut selected| selected.take())
            .map_or_else(|| Ok(None), |selected| Ok(Some(selected)))
    }

    fn active_window(&mut self) -> Option<ActiveWindowInfo> {
        // AX 查询只有在用户已授予“辅助功能”权限后才可访问其他进程的聚焦窗口。
        if unsafe { AXIsProcessTrusted() } == 0 {
            return None;
        }
        let frontmost = NSWorkspace::sharedWorkspace().frontmostApplication()?;
        let frontmost_pid = frontmost.processIdentifier();
        let own_pid = std::process::id() as i32;
        // 桌宠窗口不应成为攀附对象；保留上一个外部进程。
        let pid = if frontmost_pid == own_pid {
            self.last_active_pid?
        } else {
            frontmost_pid
        };
        let app = AxObject::from_owned(unsafe { AXUIElementCreateApplication(pid) })?;
        let focused = ax_copy_attribute(&app, unsafe { kAXFocusedWindowAttribute })?;

        let mut focused_pid = 0i32;
        if unsafe { AXUIElementGetPid(focused.0, &mut focused_pid) } != 0 || focused_pid == own_pid
        {
            return None;
        }
        let position = ax_copy_attribute(&focused, unsafe { kAXPositionAttribute })?;
        let size = ax_copy_attribute(&focused, unsafe { kAXSizeAttribute })?;
        let mut origin = NSPoint::new(0.0, 0.0);
        let mut dimensions = NSSize::new(0.0, 0.0);
        if unsafe {
            AXValueGetValue(
                position.0,
                AX_VALUE_CG_POINT,
                (&mut origin as *mut NSPoint).cast(),
            )
        } == 0
            || unsafe {
                AXValueGetValue(
                    size.0,
                    AX_VALUE_CG_SIZE,
                    (&mut dimensions as *mut NSSize).cast(),
                )
            } == 0
            || dimensions.width <= 0.0
            || dimensions.height <= 0.0
        {
            return None;
        }
        let mut window_id = 0u32;
        if unsafe { _AXUIElementGetWindow(focused.0, &mut window_id) } != 0 {
            return None;
        }
        self.last_active_pid = Some(focused_pid);
        let mtm = MainThreadMarker::new()?;
        let bounds = desktop_bounds(mtm);
        let main_frame = NSScreen::mainScreen(mtm)?.frame();
        Some(ActiveWindowInfo {
            handle: window_id.into(),
            uid: ((focused_pid as u64) << 32) | u64::from(window_id),
            area: ax_rect_to_global(bounds, main_frame, origin, dimensions),
        })
    }
}

struct MacOSWindow {
    window: Retained<NSWindow>,
    view: Retained<MascotView>,
    key: usize,
}

impl MascotWindow for MacOSWindow {
    fn update_frame(
        &mut self,
        bitmap_bgra_premul: &[u8],
        width: u32,
        height: u32,
        top_left: Point,
    ) -> PlatformResult<()> {
        // 预乘 BGRA 转直通 RGBA，供 NSBitmapImageRep 使用。
        let mut rgba = vec![0u8; bitmap_bgra_premul.len()];
        for (i, chunk) in bitmap_bgra_premul.as_chunks::<4>().0.iter().enumerate() {
            let b = chunk[0] as u32;
            let g = chunk[1] as u32;
            let r = chunk[2] as u32;
            let a = chunk[3] as u32;
            let o = i * 4;
            rgba[o] = (r * 255).checked_div(a).unwrap_or(0).min(255) as u8;
            rgba[o + 1] = (g * 255).checked_div(a).unwrap_or(0).min(255) as u8;
            rgba[o + 2] = (b * 255).checked_div(a).unwrap_or(0).min(255) as u8;
            rgba[o + 3] = a as u8;
        }
        self.view.set_frame_data(&rgba, width, height);

        // 运行时统一使用虚拟桌面左上坐标，不能只按主屏高度翻转。
        if let Some(mtm) = MainThreadMarker::new() {
            let bounds = desktop_bounds(mtm);
            let appkit_top_left = global_point_to_appkit(bounds, top_left);
            let frame = NSRect::new(
                NSPoint::new(appkit_top_left.x, appkit_top_left.y - height as f64),
                NSSize::new(width as f64, height as f64),
            );
            self.window.setFrame_display(frame, true);
        }
        self.view.setNeedsDisplay(true);
        Ok(())
    }
}

impl Drop for MacOSWindow {
    fn drop(&mut self) {
        WINDOW_IDS.lock().unwrap().remove(&self.key);
        self.window.orderOut(None);
    }
}

// ---------------------------------------------------------------------------
// 系统托盘（NSStatusItem）：菜单项点击进入命令队列，主循环轮询取出。
// ---------------------------------------------------------------------------

pub mod tray {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use objc2::rc::Retained;
    use objc2::{AnyThread, DefinedClass, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{NSBitmapImageRep, NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem};
    use objc2_foundation::{NSObject, NSString};

    /// 托盘命令队列（菜单点击写入，poll 取出）。
    static COMMAND_QUEUE: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

    /// 菜单项 target 的实例变量（持有命令名）。
    struct TargetIvars {
        command: std::cell::RefCell<String>,
    }

    define_class!(
        #[unsafe(super = NSObject)]
        #[name = "NeurolingsTrayTarget"]
        #[ivars = TargetIvars]
        struct TrayTarget;

        impl TrayTarget {
            #[unsafe(method(onMenuAction:))]
            fn on_menu_action(&self, _sender: Option<&NSObject>) {
                let command = self.ivars().command.borrow().clone();
                if let Ok(mut queue) = COMMAND_QUEUE.lock() {
                    queue.push_back(command);
                }
            }
        }
    );

    impl TrayTarget {
        fn new(command: &str) -> Retained<Self> {
            let this = Self::alloc().set_ivars(TargetIvars {
                command: std::cell::RefCell::new(command.to_string()),
            });
            unsafe { msg_send![super(this), init] }
        }
    }

    /// 状态栏对象及其非拥有式位图缓冲、菜单 target 的保活容器。
    struct StatusItemState {
        item: Retained<NSStatusItem>,
        _image: Option<Retained<NSImage>>,
        _icon_backing: Vec<u8>,
        menu_targets: Vec<Retained<TrayTarget>>,
    }

    // AppKit 对象始终仅在创建它的主线程访问；Mutex 只用于保存唯一实例。
    unsafe impl Send for StatusItemState {}

    /// 托盘图标、位图缓冲和当前菜单 target 的保活引用。
    static STATUS_ITEM: Mutex<Option<StatusItemState>> = Mutex::new(None);

    /// 创建/初始化托盘图标与初始菜单。
    pub fn tray_init(tooltip: &str, rgba: &[u8], width: u32, height: u32) -> bool {
        let Some(mtm) = objc2::MainThreadMarker::new() else {
            return false;
        };
        let bar = NSStatusBar::systemStatusBar();
        let item = bar.statusItemWithLength(-1.0); // NSVariableStatusItemLength
        let mut icon_backing = Vec::new();
        let mut image = None;
        if let Some(button) = item.button(mtm) {
            button.setToolTip(Some(&NSString::from_str(tooltip)));
            let expected_len = (width as usize)
                .checked_mul(height as usize)
                .and_then(|pixels| pixels.checked_mul(4));
            if expected_len == Some(rgba.len()) {
                icon_backing.extend_from_slice(rgba);
                let bytes_per_row = (width * 4) as isize;
                let mut planes = [icon_backing.as_mut_ptr()];
                let rep = unsafe {
                    NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                        NSBitmapImageRep::alloc(),
                        planes.as_mut_ptr(),
                        width as isize,
                        height as isize,
                        8,
                        4,
                        true,
                        false,
                        objc2_app_kit::NSDeviceRGBColorSpace,
                        bytes_per_row,
                        32,
                    )
                };
                if let Some(rep) = rep {
                    let icon = NSImage::initWithSize(
                        NSImage::alloc(),
                        objc2_foundation::NSSize::new(width as f64, height as f64),
                    );
                    icon.addRepresentation(&rep);
                    button.setImage(Some(&icon));
                    image = Some(icon);
                }
            }
        }
        if let Ok(mut slot) = STATUS_ITEM.lock() {
            if let Some(previous) = slot.take() {
                bar.removeStatusItem(&previous.item);
            }
            *slot = Some(StatusItemState {
                item,
                _image: image,
                _icon_backing: icon_backing,
                menu_targets: Vec::new(),
            });
            true
        } else {
            false
        }
    }

    /// 重建托盘菜单（条目：toggle/spawn 子菜单/kill_all/quit + 本地化文本）。
    #[allow(clippy::too_many_arguments)]
    pub fn tray_set_menu(
        toggle_text: &str,
        spawn_text: &str,
        none_text: &str,
        kill_all_text: &str,
        quit_text: &str,
        spawn_names: &[String],
    ) {
        let Some(mtm) = objc2::MainThreadMarker::new() else {
            return;
        };
        let menu = NSMenu::new(mtm);
        let mut targets = Vec::new();
        append_item(&menu, "toggle_manager", toggle_text, true, &mut targets);
        let spawn_menu = NSMenu::new(mtm);
        if spawn_names.is_empty() {
            append_item_to(&spawn_menu, "none", none_text, false, &mut targets);
        } else {
            let mut sorted = spawn_names.to_vec();
            sorted.sort_by_key(|n| n.to_lowercase());
            for name in sorted {
                append_item_to(
                    &spawn_menu,
                    &format!("spawn:{name}"),
                    &name,
                    true,
                    &mut targets,
                );
            }
        }
        let submenu = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(spawn_text),
                None,
                &NSString::from_str(""),
            )
        };
        submenu.setSubmenu(Some(&spawn_menu));
        menu.addItem(&submenu);
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        append_item(&menu, "close_all", kill_all_text, true, &mut targets);
        append_item(&menu, "quit", quit_text, true, &mut targets);
        if let Ok(mut slot) = STATUS_ITEM.lock()
            && let Some(item) = slot.as_mut()
        {
            item.item.setMenu(Some(&menu));
            item.menu_targets = targets;
        }
    }

    fn append_item(
        menu: &NSMenu,
        command: &str,
        title: &str,
        enabled: bool,
        targets: &mut Vec<Retained<TrayTarget>>,
    ) {
        append_item_to(menu, command, title, enabled, targets);
    }

    fn append_item_to(
        menu: &NSMenu,
        command: &str,
        title: &str,
        enabled: bool,
        targets: &mut Vec<Retained<TrayTarget>>,
    ) {
        let Some(mtm) = objc2::MainThreadMarker::new() else {
            return;
        };
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                None,
                &NSString::from_str(""),
            )
        };
        let target = TrayTarget::new(command);
        unsafe {
            let _: () = msg_send![&*item, setTarget: Some(&*target)];
            let _: () = msg_send![&*item, setAction: objc2::sel!(onMenuAction:)];
        }
        // NSMenuItem 不拥有 target，刷新菜单前由状态栏对象显式保活。
        targets.push(target);
        item.setEnabled(enabled);
        menu.addItem(&item);
    }

    /// 取出一个待处理托盘命令（无则返回 None）。
    pub fn tray_poll() -> Option<String> {
        COMMAND_QUEUE.lock().ok().and_then(|mut q| q.pop_front())
    }

    /// 移除状态栏图标并释放位图和菜单 target。
    pub fn tray_remove() {
        let Some(_) = objc2::MainThreadMarker::new() else {
            return;
        };
        let bar = NSStatusBar::systemStatusBar();
        if let Ok(mut slot) = STATUS_ITEM.lock()
            && let Some(item) = slot.take()
        {
            bar.removeStatusItem(&item.item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appkit_coordinates_round_trip_in_virtual_desktop() {
        let bounds = DesktopBounds {
            left: -1600.0,
            bottom: -900.0,
            right: 2560.0,
            top: 1800.0,
        };
        let original = NSPoint::new(-320.0, 720.0);
        let global = appkit_point_to_global(bounds, original);

        assert_eq!(global, Point::new(1280, 1080));
        assert_eq!(global_point_to_appkit(bounds, global), original);
    }

    #[test]
    fn ax_coordinates_map_to_virtual_desktop_top_left() {
        let bounds = DesktopBounds {
            left: -1600.0,
            bottom: -900.0,
            right: 2560.0,
            top: 1800.0,
        };
        let main_frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(2560.0, 1440.0));

        let area = ax_rect_to_global(
            bounds,
            main_frame,
            NSPoint::new(100.0, 60.0),
            NSSize::new(300.0, 200.0),
        );

        assert_eq!(
            area,
            Rect {
                left: 1700,
                top: 420,
                right: 2000,
                bottom: 620,
            }
        );
    }
}
