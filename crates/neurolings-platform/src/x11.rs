//! Linux X11 后端：透明置顶桌宠窗口。
//!
//! 使用 32 位 ARGB 视觉，由合成器（mutter/kwin/picom）实现逐像素透明；
//! 透明区域的点击穿透用 XFixes input shape 实现。
//! Wayland 会话经 XWayland 运行。

use std::collections::HashMap;
use std::sync::Mutex;

use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    self, AtomEnum, ColormapAlloc, ConnectionExt, CreateWindowAux, EventMask, ImageFormat,
    PropMode, VisualClass, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::{
    MascotBackend, MascotEvent, MascotEventKind, MascotWindow, PlatformError, PlatformResult,
    Point, Rect, ScreenInfo,
};

static EVENT_QUEUE: Mutex<Vec<MascotEvent>> = Mutex::new(Vec::new());

fn err(context: &str) -> PlatformError {
    PlatformError::Win32(format!("X11 {context}"))
}

struct X11Shared {
    conn: RustConnection,
    root: xproto::Window,
    visual_id: u32,
    colormap: xproto::Colormap,
    depth: u8,
    screen: ScreenInfo,
    net_wm_state: xproto::Atom,
    net_wm_state_above: xproto::Atom,
    wm_protocols: xproto::Atom,
    wm_delete_window: xproto::Atom,
    /// X 窗口 id → 桌宠 id。
    windows: HashMap<u32, u64>,
}

pub struct X11Backend {
    shared: std::rc::Rc<std::cell::RefCell<X11Shared>>,
}

fn intern(conn: &RustConnection, name: &str) -> PlatformResult<xproto::Atom> {
    let reply = conn
        .intern_atom(false, name.as_bytes())
        .map_err(|_| err("intern_atom"))?
        .reply()
        .map_err(|_| err("intern_atom reply"))?;
    Ok(reply.atom)
}

impl X11Backend {
    pub fn new() -> PlatformResult<Self> {
        let (conn, screen_index) =
            x11rb::rust_connection::RustConnection::connect(None).map_err(|_| err("connect"))?;
        let screen = &conn.setup().roots[screen_index];
        let root = screen.root;

        // 寻找 32 位 TrueColor 视觉以支持 ARGB 透明。
        let mut visual_id = 0u32;
        let mut depth = 0u8;
        for allowed in &screen.allowed_depths {
            if allowed.depth != 32 {
                continue;
            }
            for visual in &allowed.visuals {
                if visual.class == VisualClass::TRUE_COLOR {
                    visual_id = visual.visual_id;
                    depth = 32;
                    break;
                }
            }
            if visual_id != 0 {
                break;
            }
        }
        if visual_id == 0 {
            return Err(err("no 32-bit ARGB visual available"));
        }
        let colormap = conn.generate_id().map_err(|_| err("generate_id"))?;
        conn.create_colormap(ColormapAlloc::NONE, colormap, root, visual_id)
            .map_err(|_| err("create_colormap"))?;

        let net_wm_state = intern(&conn, "_NET_WM_STATE")?;
        let net_wm_state_above = intern(&conn, "_NET_WM_STATE_ABOVE")?;
        let wm_protocols = intern(&conn, "WM_PROTOCOLS")?;
        let wm_delete_window = intern(&conn, "WM_DELETE_WINDOW")?;

        // 有 _NET_WORKAREA 时用它作为工作区。
        let net_workarea = intern(&conn, "_NET_WORKAREA")?;
        let mut work = Rect {
            left: 0,
            top: 0,
            right: screen.width_in_pixels as i32,
            bottom: screen.height_in_pixels as i32,
        };
        if let Ok(cookie) = conn.get_property(false, root, net_workarea, AtomEnum::CARDINAL, 0, 4)
            && let Ok(reply) = cookie.reply()
        {
            let values: Vec<u32> = reply.value32().map(|v| v.collect()).unwrap_or_default();
            if values.len() >= 4 {
                work = Rect {
                    left: values[0] as i32,
                    top: values[1] as i32,
                    right: values[0] as i32 + values[2] as i32,
                    bottom: values[1] as i32 + values[3] as i32,
                };
            }
        }
        let info = ScreenInfo {
            monitor: Rect {
                left: 0,
                top: 0,
                right: screen.width_in_pixels as i32,
                bottom: screen.height_in_pixels as i32,
            },
            work_area: work,
        };

        conn.flush().map_err(|_| err("flush"))?;
        Ok(Self {
            shared: std::rc::Rc::new(std::cell::RefCell::new(X11Shared {
                conn,
                root,
                visual_id,
                colormap,
                depth,
                screen: info,
                net_wm_state,
                net_wm_state_above,
                wm_protocols,
                wm_delete_window,
                windows: HashMap::new(),
            })),
        })
    }

    fn pump(&self) {
        let mut shared = self.shared.borrow_mut();
        while let Ok(Some(event)) = shared.conn.poll_for_event() {
            match event {
                Event::ButtonPress(e) | Event::ButtonRelease(e) => {
                    if let Some(id) = shared.windows.get(&e.event).copied() {
                        let kind = match (e.detail, matches!(event, Event::ButtonPress(_))) {
                            (1, true) => MascotEventKind::LeftDown,
                            (1, false) => MascotEventKind::LeftUp,
                            (3, false) => MascotEventKind::RightUp,
                            _ => continue,
                        };
                        EVENT_QUEUE.lock().unwrap().push(MascotEvent {
                            mascot_id: id,
                            kind,
                            screen: Point::new(e.root_x as i32, e.root_y as i32),
                            local: Point::new(e.event_x as i32, e.event_y as i32),
                        });
                    }
                }
                Event::MotionNotify(e) => {
                    if let Some(id) = shared.windows.get(&e.event).copied() {
                        EVENT_QUEUE.lock().unwrap().push(MascotEvent {
                            mascot_id: id,
                            kind: MascotEventKind::Move,
                            screen: Point::new(e.root_x as i32, e.root_y as i32),
                            local: Point::new(e.event_x as i32, e.event_y as i32),
                        });
                    }
                }
                _ => {}
            }
        }
    }
}

impl MascotBackend for X11Backend {
    fn create_window(&mut self, mascot_id: u64) -> PlatformResult<Box<dyn MascotWindow>> {
        let mut shared = self.shared.borrow_mut();
        let wid = shared.conn.generate_id().map_err(|_| err("generate_id"))?;
        let values = CreateWindowAux::new()
            .override_redirect(1)
            .background_pixel(0)
            .border_pixel(0)
            .colormap(shared.colormap)
            .event_mask(
                EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE
                    | EventMask::POINTER_MOTION
                    | EventMask::STRUCTURE_NOTIFY,
            );
        shared
            .conn
            .create_window(
                shared.depth,
                wid,
                shared.root,
                -2000,
                -2000,
                1,
                1,
                0,
                WindowClass::INPUT_OUTPUT,
                shared.visual_id,
                &values,
            )
            .map_err(|_| err("create_window"))?;

        // 始终置顶。
        shared
            .conn
            .change_property32(
                PropMode::REPLACE,
                wid,
                shared.net_wm_state,
                AtomEnum::ATOM,
                &[shared.net_wm_state_above],
            )
            .map_err(|_| err("net_wm_state"))?;
        shared.conn.map_window(wid).map_err(|_| err("map_window"))?;
        shared.conn.flush().map_err(|_| err("flush"))?;
        shared.windows.insert(wid, mascot_id);
        Ok(Box::new(X11Window {
            shared: self.shared.clone(),
            wid,
        }))
    }

    fn screens(&self) -> Vec<ScreenInfo> {
        vec![self.shared.borrow().screen]
    }

    fn cursor_pos(&self) -> Point {
        let shared = self.shared.borrow();
        match shared.conn.query_pointer(shared.root) {
            Ok(cookie) => match cookie.reply() {
                Ok(reply) => Point::new(reply.root_x as i32, reply.root_y as i32),
                Err(_) => Point::new(0, 0),
            },
            Err(_) => Point::new(0, 0),
        }
    }

    fn pump_events(&mut self) -> Vec<MascotEvent> {
        self.pump();
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

struct X11Window {
    shared: std::rc::Rc<std::cell::RefCell<X11Shared>>,
    wid: xproto::Window,
}

impl MascotWindow for X11Window {
    fn update_frame(
        &mut self,
        bitmap_bgra_premul: &[u8],
        width: u32,
        height: u32,
        top_left: Point,
    ) -> PlatformResult<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        let mut shared = self.shared.borrow_mut();
        shared
            .conn
            .configure_window(
                self.wid,
                &xproto::ConfigureWindowAux::new()
                    .x(top_left.x)
                    .y(top_left.y)
                    .width(width)
                    .height(height),
            )
            .map_err(|_| err("configure_window"))?;
        // X11 小端序下期望 BGRA，与预乘缓冲一致。
        shared
            .conn
            .put_image(
                ImageFormat::Z_PIXMAP,
                self.wid,
                x11rb::protocol::xproto::Gcontext::from(0u32),
                width as u16,
                height as u16,
                0,
                0,
                0,
                shared.depth,
                bitmap_bgra_premul,
            )
            .map_err(|_| err("put_image"))?;
        // 输入形状：透明像素不再接收指针事件。
        apply_input_shape(&mut shared, self.wid, bitmap_bgra_premul, width, height);
        shared.conn.flush().map_err(|_| err("flush"))
    }
}

impl Drop for X11Window {
    fn drop(&mut self) {
        let mut shared = self.shared.borrow_mut();
        shared.windows.remove(&self.wid);
        let _ = shared.conn.destroy_window(self.wid);
        let _ = shared.conn.flush();
    }
}

/// 在不透明像素上构建行段矩形并设为 XFixes 输入形状，
/// 使点击穿透透明区域。
fn apply_input_shape(
    shared: &mut X11Shared,
    wid: xproto::Window,
    bitmap: &[u8],
    width: u32,
    height: u32,
) {
    use x11rb::protocol::shape;
    use x11rb::protocol::xfixes;
    let mut rects: Vec<xproto::Rectangle> = Vec::new();
    for y in 0..height {
        let row_start = (y * width * 4) as usize;
        let mut x = 0u32;
        while x < width {
            let alpha = bitmap[row_start + (x as usize) * 4 + 3];
            if alpha == 0 {
                x += 1;
                continue;
            }
            let start = x;
            while x < width && bitmap[row_start + (x as usize) * 4 + 3] > 0 {
                x += 1;
            }
            rects.push(xproto::Rectangle {
                x: start as i16,
                y: y as i16,
                width: (x - start) as u16,
                height: 1,
            });
        }
    }
    if rects.is_empty() {
        rects.push(xproto::Rectangle {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        });
    }
    let Ok(region) = shared.conn.generate_id() else {
        return;
    };
    let _ = xfixes::create_region(&shared.conn, region, &rects);
    let _ = xfixes::set_window_shape_region(&shared.conn, wid, shape::SK::INPUT, 0, 0, region);
    let _ = xfixes::destroy_region(&shared.conn, region);
}
