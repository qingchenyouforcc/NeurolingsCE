//! macOS 后端：无边框浮动 NSWindow，逐像素透明。
//! 每只桌宠一个窗口：内容视图绘制帧并做基于 alpha 的命中测试，
//! 使点击穿透透明像素。

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use objc2::rc::Retained;
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSBitmapImageRep, NSColor,
    NSCompositingOperation, NSDeviceRGBColorSpace, NSEvent, NSImage, NSScreen, NSView, NSWindow,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSObjectProtocol, NSPoint, NSRect, NSSize};

use crate::{
    MascotBackend, MascotEvent, MascotWindow, PlatformError, PlatformResult, Point, Rect,
    ScreenInfo,
};

static EVENT_QUEUE: Mutex<Vec<MascotEvent>> = Mutex::new(Vec::new());
/// 窗口指针（usize）→ 桌宠 id。
static WINDOW_IDS: LazyLock<Mutex<HashMap<usize, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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

pub struct MacOSBackend;

impl MacOSBackend {
    pub fn new() -> PlatformResult<Self> {
        let Some(mtm) = MainThreadMarker::new() else {
            return Err(err("must run on the main thread"));
        };
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        Ok(Self)
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
        }];
        let Some(mtm) = MainThreadMarker::new() else {
            return fallback;
        };
        let mut infos = Vec::new();
        for screen in NSScreen::screens(mtm).iter() {
            let frame = screen.frame();
            let visible = screen.visibleFrame();
            infos.push(ScreenInfo {
                monitor: Rect {
                    left: frame.origin.x as i32,
                    top: frame.origin.y as i32,
                    right: (frame.origin.x + frame.size.width) as i32,
                    bottom: (frame.origin.y + frame.size.height) as i32,
                },
                work_area: Rect {
                    left: visible.origin.x as i32,
                    top: visible.origin.y as i32,
                    right: (visible.origin.x + visible.size.width) as i32,
                    bottom: (visible.origin.y + visible.size.height) as i32,
                },
            });
        }
        if infos.is_empty() { fallback } else { infos }
    }

    fn cursor_pos(&self) -> Point {
        let location = NSEvent::mouseLocation();
        Point::new(location.x as i32, location.y as i32)
    }

    fn pump_events(&mut self) -> Vec<MascotEvent> {
        EVENT_QUEUE.lock().unwrap().drain(..).collect()
    }

    fn show_menu(
        &mut self,
        _at: Point,
        _entries: &[crate::MenuEntry],
    ) -> PlatformResult<Option<u32>> {
        // 弹出菜单暂未在该平台实现；运行时将 None 视为"未选择"。
        Ok(None)
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
        for (i, chunk) in bitmap_bgra_premul.chunks_exact(4).enumerate() {
            let b = chunk[0] as u32;
            let g = chunk[1] as u32;
            let r = chunk[2] as u32;
            let a = chunk[3] as u32;
            let o = i * 4;
            if a == 0 {
                rgba[o] = 0;
                rgba[o + 1] = 0;
                rgba[o + 2] = 0;
            } else {
                rgba[o] = (r * 255 / a).min(255) as u8;
                rgba[o + 1] = (g * 255 / a).min(255) as u8;
                rgba[o + 2] = (b * 255 / a).min(255) as u8;
            }
            rgba[o + 3] = a as u8;
        }
        self.view.set_frame_data(&rgba, width, height);

        // AppKit 原点在左下角，需从左上坐标转换。
        if let Some(mtm) = MainThreadMarker::new()
            && let Some(screen) = NSScreen::mainScreen(mtm)
        {
            let screen_height = screen.frame().size.height;
            let frame = NSRect::new(
                NSPoint::new(
                    top_left.x as f64,
                    screen_height - top_left.y as f64 - height as f64,
                ),
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
