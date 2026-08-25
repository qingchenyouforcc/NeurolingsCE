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
    self, AtomEnum, ColormapAlloc, ConnectionExt, CreateGCAux, CreateWindowAux, EventMask,
    ImageFormat, MapState, PropMode, VisualClass, WindowClass,
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
    roots: Vec<xproto::Window>,
    root_visual: xproto::Visualid,
    root_depth: u8,
    black_pixel: u32,
    white_pixel: u32,
    visual_id: u32,
    colormap: xproto::Colormap,
    depth: u8,
    screens: Vec<ScreenInfo>,
    net_wm_state: xproto::Atom,
    net_wm_state_above: xproto::Atom,
    net_active_window: xproto::Atom,
    net_wm_pid: xproto::Atom,
    net_wm_window_type: xproto::Atom,
    net_wm_window_type_desktop: xproto::Atom,
    net_wm_window_type_dock: xproto::Atom,
    net_wm_window_type_notification: xproto::Atom,
    /// X 窗口 id → 桌宠 id。
    windows: HashMap<u32, u64>,
    last_left_press: Option<LastLeftPress>,
}

#[derive(Clone, Copy)]
struct LastLeftPress {
    window: xproto::Window,
    time: u32,
    root_x: i16,
    root_y: i16,
}

fn is_double_left_press(
    previous: LastLeftPress,
    window: xproto::Window,
    time: u32,
    root_x: i16,
    root_y: i16,
) -> bool {
    let elapsed = time.wrapping_sub(previous.time);
    previous.window == window
        && elapsed <= 500
        && i32::from(root_x).abs_diff(i32::from(previous.root_x)) <= 4
        && i32::from(root_y).abs_diff(i32::from(previous.root_y)) <= 4
}

/// 使用 X11 ARGB 窗口承载桌宠的后端。
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

fn property_values(
    conn: &RustConnection,
    window: xproto::Window,
    property: xproto::Atom,
    property_type: xproto::Atom,
    long_length: u32,
) -> Vec<u32> {
    let Ok(cookie) = conn.get_property(false, window, property, property_type, 0, long_length)
    else {
        return Vec::new();
    };
    let Ok(reply) = cookie.reply() else {
        return Vec::new();
    };
    reply
        .value32()
        .map(|values| values.collect())
        .unwrap_or_default()
}

fn rect_from_cardinals(values: &[u32], offset: usize) -> Option<Rect> {
    let [x, y, width, height] = values.get(offset..offset + 4)? else {
        return None;
    };
    if *width == 0 || *height == 0 {
        return None;
    }
    let left = *x as i32;
    let top = *y as i32;
    // EWMH 将坐标放在 CARDINAL 中传输；负坐标以 32 位二补码表示。
    let right = i64::from(left) + i64::from(*width);
    let bottom = i64::from(top) + i64::from(*height);
    Some(Rect {
        left,
        top,
        right: right.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        bottom: bottom.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
    })
}

fn screen_at(screens: &[ScreenInfo], point: Point) -> Option<ScreenInfo> {
    screens
        .iter()
        .copied()
        .find(|screen| screen.monitor.contains(point))
        .or_else(|| {
            screens.iter().copied().min_by_key(|screen| {
                let dx = if point.x < screen.monitor.left {
                    i64::from(screen.monitor.left) - i64::from(point.x)
                } else if point.x >= screen.monitor.right {
                    i64::from(point.x) - i64::from(screen.monitor.right) + 1
                } else {
                    0
                };
                let dy = if point.y < screen.monitor.top {
                    i64::from(screen.monitor.top) - i64::from(point.y)
                } else if point.y >= screen.monitor.bottom {
                    i64::from(point.y) - i64::from(screen.monitor.bottom) + 1
                } else {
                    0
                };
                dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy))
            })
        })
}

fn work_area(
    conn: &RustConnection,
    root: xproto::Window,
    net_workarea: xproto::Atom,
    net_current_desktop: xproto::Atom,
    monitor: Rect,
) -> Rect {
    let values = property_values(conn, root, net_workarea, AtomEnum::CARDINAL.into(), 4096);
    let desktop = property_values(
        conn,
        root,
        net_current_desktop,
        AtomEnum::CARDINAL.into(),
        1,
    )
    .first()
    .copied()
    .unwrap_or(0) as usize;
    rect_from_cardinals(&values, desktop.saturating_mul(4)).unwrap_or(monitor)
}

impl X11Backend {
    /// 连接当前 X11 显示并初始化桌宠窗口后端。
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
        let net_active_window = intern(&conn, "_NET_ACTIVE_WINDOW")?;
        let net_wm_pid = intern(&conn, "_NET_WM_PID")?;
        let net_wm_window_type = intern(&conn, "_NET_WM_WINDOW_TYPE")?;
        let net_wm_window_type_desktop = intern(&conn, "_NET_WM_WINDOW_TYPE_DESKTOP")?;
        let net_wm_window_type_dock = intern(&conn, "_NET_WM_WINDOW_TYPE_DOCK")?;
        let net_wm_window_type_notification = intern(&conn, "_NET_WM_WINDOW_TYPE_NOTIFICATION")?;
        let net_workarea = intern(&conn, "_NET_WORKAREA")?;
        let net_current_desktop = intern(&conn, "_NET_CURRENT_DESKTOP")?;

        // x11rb 当前依赖未启用 RandR/Xinerama 特性，因此先枚举 X11
        // protocol 暴露的全部根屏；每根屏的工作区读取 EWMH 当前桌面值。
        // 在常见的 Xinerama 单根屏布局下，根屏矩形就是可靠的全局 fallback。
        let root_geometries: Vec<_> = conn
            .setup()
            .roots
            .iter()
            .map(|root| {
                (
                    root.root,
                    root.root_visual,
                    root.root_depth,
                    root.black_pixel,
                    root.white_pixel,
                    root.width_in_pixels,
                    root.height_in_pixels,
                )
            })
            .collect();
        let mut screens: Vec<(usize, ScreenInfo)> = root_geometries
            .iter()
            .enumerate()
            .map(|(index, (root, _, _, _, _, width, height))| {
                let monitor = Rect {
                    left: 0,
                    top: 0,
                    right: i32::from(*width),
                    bottom: i32::from(*height),
                };
                (
                    index,
                    ScreenInfo {
                        monitor,
                        work_area: work_area(
                            &conn,
                            *root,
                            net_workarea,
                            net_current_desktop,
                            monitor,
                        ),
                        // X11 后端不做 DPI 缩放，物理与逻辑一致。
                        scale: 1.0,
                    },
                )
            })
            .collect();
        screens.sort_by_key(|(index, _)| usize::from(*index != screen_index));
        let screens = screens.into_iter().map(|(_, info)| info).collect();
        let (_, root_visual, root_depth, black_pixel, white_pixel, _, _) = root_geometries
            .get(screen_index)
            .copied()
            .ok_or_else(|| err("selected root screen"))?;

        conn.flush().map_err(|_| err("flush"))?;
        Ok(Self {
            shared: std::rc::Rc::new(std::cell::RefCell::new(X11Shared {
                conn,
                root,
                roots: root_geometries.iter().map(|item| item.0).collect(),
                root_visual,
                root_depth,
                black_pixel,
                white_pixel,
                visual_id,
                colormap,
                depth,
                screens,
                net_wm_state,
                net_wm_state_above,
                net_active_window,
                net_wm_pid,
                net_wm_window_type,
                net_wm_window_type_desktop,
                net_wm_window_type_dock,
                net_wm_window_type_notification,
                windows: HashMap::new(),
                last_left_press: None,
            })),
        })
    }

    fn pump(&self) {
        let mut shared = self.shared.borrow_mut();
        while let Ok(Some(event)) = shared.conn.poll_for_event() {
            match event {
                Event::ButtonPress(e) => {
                    let Some(id) = shared.windows.get(&e.event).copied() else {
                        continue;
                    };
                    let button = e.detail;
                    // 其他按键不能打断左键的双击判定，否则右键菜单会吞掉首击。
                    let is_double = button == 1
                        && shared.last_left_press.take().is_some_and(|last| {
                            is_double_left_press(last, e.event, e.time, e.root_x, e.root_y)
                        });
                    if !is_double && button == 1 {
                        shared.last_left_press = Some(LastLeftPress {
                            window: e.event,
                            time: e.time,
                            root_x: e.root_x,
                            root_y: e.root_y,
                        });
                    }
                    let kind = match button {
                        1 if is_double => MascotEventKind::LeftDoubleClick,
                        1 => MascotEventKind::LeftDown,
                        _ => continue,
                    };
                    EVENT_QUEUE.lock().unwrap().push(MascotEvent {
                        mascot_id: id,
                        kind,
                        screen: Point::new(e.root_x as i32, e.root_y as i32),
                        local: Point::new(e.event_x as i32, e.event_y as i32),
                    });
                }
                Event::ButtonRelease(e) => {
                    let Some(id) = shared.windows.get(&e.event).copied() else {
                        continue;
                    };
                    let kind = match e.detail {
                        1 => MascotEventKind::LeftUp,
                        3 => MascotEventKind::RightUp,
                        _ => continue,
                    };
                    EVENT_QUEUE.lock().unwrap().push(MascotEvent {
                        mascot_id: id,
                        kind,
                        screen: Point::new(e.root_x as i32, e.root_y as i32),
                        local: Point::new(e.event_x as i32, e.event_y as i32),
                    });
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
        let gc = shared
            .conn
            .generate_id()
            .map_err(|_| err("generate_gc_id"))?;
        shared
            .conn
            .create_gc(gc, wid, &CreateGCAux::default())
            .map_err(|_| err("create_gc"))?
            .check()
            .map_err(|_| err("create_gc reply"))?;

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
            gc,
        }))
    }

    fn screens(&self) -> Vec<ScreenInfo> {
        self.shared.borrow().screens.clone()
    }

    fn cursor_pos(&self) -> Point {
        let shared = self.shared.borrow();
        for root in &shared.roots {
            let Ok(cookie) = shared.conn.query_pointer(*root) else {
                continue;
            };
            let Ok(reply) = cookie.reply() else {
                continue;
            };
            if reply.same_screen {
                return Point::new(reply.root_x as i32, reply.root_y as i32);
            }
        }
        Point::new(0, 0)
    }

    fn pump_events(&mut self) -> Vec<MascotEvent> {
        self.pump();
        EVENT_QUEUE.lock().unwrap().drain(..).collect()
    }

    fn show_menu(
        &mut self,
        _at: Point,
        entries: &[crate::MenuEntry],
    ) -> PlatformResult<Option<u32>> {
        show_popup_menu(&self.shared, _at, entries)
    }

    fn active_window(&mut self) -> Option<crate::ActiveWindowInfo> {
        let shared = self.shared.borrow();
        let (root, window) = shared.roots.iter().copied().find_map(|root| {
            property_values(
                &shared.conn,
                root,
                shared.net_active_window,
                AtomEnum::WINDOW.into(),
                1,
            )
            .first()
            .copied()
            .filter(|window| *window != 0 && *window != root)
            .map(|window| (root, window))
        })?;

        // 自身的透明窗口和 Manager 不应成为桌宠的交互区。
        if shared.windows.contains_key(&window) || crate::manager_window::is_hwnd(window as usize) {
            return None;
        }
        let attributes = shared
            .conn
            .get_window_attributes(window)
            .ok()?
            .reply()
            .ok()?;
        if attributes.map_state != MapState::VIEWABLE {
            return None;
        }

        let pid = property_values(
            &shared.conn,
            window,
            shared.net_wm_pid,
            AtomEnum::CARDINAL.into(),
            1,
        )
        .first()
        .copied();
        if pid == Some(std::process::id()) {
            return None;
        }

        let window_types = property_values(
            &shared.conn,
            window,
            shared.net_wm_window_type,
            AtomEnum::ATOM.into(),
            16,
        );
        if window_types.iter().any(|kind| {
            *kind == shared.net_wm_window_type_desktop
                || *kind == shared.net_wm_window_type_dock
                || *kind == shared.net_wm_window_type_notification
        }) {
            return None;
        }

        let geometry = shared.conn.get_geometry(window).ok()?.reply().ok()?;
        if geometry.width == 0 || geometry.height == 0 {
            return None;
        }
        let position = shared
            .conn
            .translate_coordinates(window, root, 0, 0)
            .ok()?
            .reply()
            .ok()?;
        let area = Rect {
            left: i32::from(position.dst_x),
            top: i32::from(position.dst_y),
            right: i32::from(position.dst_x) + i32::from(geometry.width),
            bottom: i32::from(position.dst_y) + i32::from(geometry.height),
        };
        (area.right > area.left && area.bottom > area.top).then_some(crate::ActiveWindowInfo {
            handle: u64::from(window),
            // X11 Window 在连接生命周期内稳定，可作为跨帧窗口身份。
            uid: u64::from(window),
            area,
        })
    }
}

#[derive(Clone)]
struct PopupRow {
    id: Option<u32>,
    label: String,
    checked: bool,
    separator: bool,
    top: u16,
    height: u16,
}

fn flatten_menu_entries(entries: &[crate::MenuEntry], indent: usize, rows: &mut Vec<PopupRow>) {
    for entry in entries {
        match entry {
            crate::MenuEntry::Item { id, label, checked } => rows.push(PopupRow {
                id: Some(*id),
                label: format!("{}{}", " ".repeat(indent), label),
                checked: *checked,
                separator: false,
                top: 0,
                height: 24,
            }),
            crate::MenuEntry::Submenu { label, entries } => {
                rows.push(PopupRow {
                    id: None,
                    label: format!("{}{}", " ".repeat(indent), label),
                    checked: false,
                    separator: false,
                    top: 0,
                    height: 24,
                });
                flatten_menu_entries(entries, indent + 2, rows);
            }
            crate::MenuEntry::Separator => rows.push(PopupRow {
                id: None,
                label: String::new(),
                checked: false,
                separator: true,
                top: 0,
                height: 8,
            }),
        }
    }
}

fn x11_label(label: &str) -> Vec<u8> {
    label
        .chars()
        .map(|ch| if ch.is_ascii() { ch as u8 } else { b'?' })
        .take(240)
        .collect()
}

fn draw_popup_menu(
    conn: &RustConnection,
    window: xproto::Window,
    gc: xproto::Gcontext,
    rows: &[PopupRow],
    width: u16,
    black_pixel: u32,
) {
    for row in rows {
        if row.separator {
            let _ = conn.poly_fill_rectangle(
                window,
                gc,
                &[xproto::Rectangle {
                    x: 8,
                    y: row.top as i16 + 3,
                    width: width.saturating_sub(16),
                    height: 1,
                }],
            );
            continue;
        }
        let mut label = String::new();
        if row.checked {
            label.push_str("[x] ");
        } else {
            label.push_str("    ");
        }
        label.push_str(&row.label);
        let bytes = x11_label(&label);
        let _ = conn.image_text8(window, gc, 8, row.top as i16 + 17, &bytes);
    }
    let _ = conn.change_gc(gc, &xproto::ChangeGCAux::new().foreground(black_pixel));
}

fn show_popup_menu(
    shared: &std::rc::Rc<std::cell::RefCell<X11Shared>>,
    at: Point,
    entries: &[crate::MenuEntry],
) -> PlatformResult<Option<u32>> {
    if entries.is_empty() {
        return Ok(None);
    }
    let shared = shared.borrow_mut();
    let mut rows = Vec::new();
    flatten_menu_entries(entries, 0, &mut rows);
    if rows.is_empty() {
        return Ok(None);
    }
    let mut top = 0u16;
    let mut max_chars = 0usize;
    for row in &mut rows {
        row.top = top;
        top = top.saturating_add(row.height);
        let extra = if row.checked { 4 } else { 0 };
        max_chars = max_chars.max(row.label.chars().count() + extra);
    }
    let width = (max_chars.saturating_mul(8).saturating_add(28)).clamp(180, 480) as u16;
    let height = top.max(8);
    let screen = screen_at(&shared.screens, at).unwrap_or(ScreenInfo {
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
            bottom: 1080,
        },
        scale: 1.0,
    });
    let x = at.x.clamp(
        screen.monitor.left,
        (screen.monitor.right - i32::from(width)).max(screen.monitor.left),
    );
    let y = at.y.clamp(
        screen.monitor.top,
        (screen.monitor.bottom - i32::from(height)).max(screen.monitor.top),
    );
    let x = x.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
    let y = y.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;

    let window = shared
        .conn
        .generate_id()
        .map_err(|_| err("menu window id"))?;
    let create_values = CreateWindowAux::new()
        .background_pixel(shared.white_pixel)
        .border_pixel(shared.black_pixel)
        .override_redirect(1u32)
        .event_mask(
            EventMask::EXPOSURE
                | EventMask::BUTTON_PRESS
                | EventMask::BUTTON_RELEASE
                | EventMask::KEY_PRESS,
        );
    let create_window = shared
        .conn
        .create_window(
            shared.root_depth,
            window,
            shared.root,
            x,
            y,
            width,
            height,
            1,
            WindowClass::INPUT_OUTPUT,
            shared.root_visual,
            &create_values,
        )
        .map_err(|error| err(&format!("create menu window: {error}")))?;
    if let Err(error) = create_window.check() {
        return Err(err(&format!("create menu window: {error}")));
    }
    let gc = shared.conn.generate_id().map_err(|_| err("menu gc id"))?;
    let create_gc = shared
        .conn
        .create_gc(
            gc,
            window,
            &CreateGCAux::new()
                .foreground(shared.black_pixel)
                .background(shared.white_pixel),
        )
        .map_err(|error| err(&format!("create menu gc: {error}")))?;
    if let Err(error) = create_gc.check() {
        let _ = shared.conn.destroy_window(window);
        let _ = shared.conn.flush();
        return Err(err(&format!("create menu gc: {error}")));
    }
    shared
        .conn
        .map_window(window)
        .map_err(|_| err("map menu window"))?
        .check()
        .map_err(|_| err("map menu window reply"))?;
    draw_popup_menu(&shared.conn, window, gc, &rows, width, shared.black_pixel);
    let grab = shared
        .conn
        .grab_pointer(
            false,
            window,
            EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
            xproto::GrabMode::ASYNC,
            xproto::GrabMode::ASYNC,
            0u32,
            0u32,
            xproto::Time::CURRENT_TIME,
        )
        .map_err(|_| err("grab menu pointer"))?
        .reply()
        .map_err(|_| err("grab menu pointer reply"))?;
    if u8::from(grab.status) != 0 {
        let _ = shared.conn.destroy_window(window);
        let _ = shared.conn.free_gc(gc);
        let _ = shared.conn.flush();
        return Err(err("grab menu pointer rejected"));
    }
    let _ = shared.conn.set_input_focus(
        xproto::InputFocus::PARENT,
        window,
        xproto::Time::CURRENT_TIME,
    );
    shared.conn.flush().map_err(|_| err("show menu flush"))?;

    let choice = loop {
        let event = match shared.conn.wait_for_event() {
            Ok(event) => event,
            Err(_) => break None,
        };
        match event {
            Event::Expose(event) if event.window == window => {
                draw_popup_menu(&shared.conn, window, gc, &rows, width, shared.black_pixel);
            }
            Event::ButtonRelease(event) => {
                let local_y = i32::from(event.root_y) - i32::from(y);
                if event.root_x < x
                    || event.root_x >= x.saturating_add(width as i16)
                    || local_y < 0
                    || local_y >= i32::from(height)
                {
                    break None;
                }
                let row = rows.iter().find(|row| {
                    local_y >= i32::from(row.top)
                        && local_y < i32::from(row.top.saturating_add(row.height))
                });
                break row.and_then(|row| row.id);
            }
            Event::ButtonPress(event) => {
                if event.root_x < x
                    || event.root_x >= x.saturating_add(width as i16)
                    || event.root_y < y
                    || event.root_y >= y.saturating_add(height as i16)
                {
                    break None;
                }
            }
            Event::KeyPress(event) if event.detail == 9 => break None,
            Event::ClientMessage(_) | Event::DestroyNotify(_) => break None,
            _ => {}
        }
    };
    let _ = shared.conn.ungrab_pointer(xproto::Time::CURRENT_TIME);
    let _ = shared.conn.destroy_window(window);
    let _ = shared.conn.free_gc(gc);
    let _ = shared.conn.flush();
    Ok(choice)
}

struct X11Window {
    shared: std::rc::Rc<std::cell::RefCell<X11Shared>>,
    wid: xproto::Window,
    gc: xproto::Gcontext,
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
        let Some(expected_len) = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
        else {
            return Err(err("invalid bitmap dimensions"));
        };
        if width > u32::from(u16::MAX)
            || height > u32::from(u16::MAX)
            || bitmap_bgra_premul.len() != expected_len
        {
            return Err(err("invalid bitmap"));
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
                self.gc,
                width as u16,
                height as u16,
                0,
                0,
                0,
                shared.depth,
                bitmap_bgra_premul,
            )
            .map_err(|_| err("put_image"))?
            .check()
            .map_err(|_| err("put_image reply"))?;
        // 输入形状：透明像素不再接收指针事件。
        apply_input_shape(&mut shared, self.wid, bitmap_bgra_premul, width, height);
        shared.conn.flush().map_err(|_| err("flush"))
    }
}

impl Drop for X11Window {
    fn drop(&mut self) {
        let mut shared = self.shared.borrow_mut();
        shared.windows.remove(&self.wid);
        if let Ok(cookie) = shared.conn.free_gc(self.gc) {
            let _ = cookie.check();
        }
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
        let row_start = y as usize * width as usize * 4;
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

#[cfg(test)]
mod tests {
    use super::{LastLeftPress, is_double_left_press, rect_from_cardinals, screen_at};
    use crate::{Point, Rect, ScreenInfo};

    #[test]
    fn work_area_preserves_negative_coordinates() {
        let rect = rect_from_cardinals(&[(-1920_i32) as u32, (-40_i32) as u32, 1920, 1040], 0)
            .expect("有效工作区");
        assert_eq!(
            rect,
            Rect {
                left: -1920,
                top: -40,
                right: 0,
                bottom: 1000,
            }
        );
    }

    #[test]
    fn menu_uses_monitor_containing_negative_point() {
        let screens = [
            ScreenInfo {
                monitor: Rect {
                    left: -1920,
                    top: 0,
                    right: 0,
                    bottom: 1080,
                },
                work_area: Rect {
                    left: -1920,
                    top: 0,
                    right: 0,
                    bottom: 1040,
                },
                scale: 1.0,
            },
            ScreenInfo {
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
            },
        ];
        assert_eq!(
            screen_at(&screens, Point::new(-24, 20))
                .expect("存在命中显示器")
                .monitor
                .left,
            -1920
        );
    }

    #[test]
    fn double_click_requires_same_window_and_nearby_press() {
        let previous = LastLeftPress {
            window: 42,
            time: 1_000,
            root_x: -12,
            root_y: 24,
        };
        assert!(is_double_left_press(previous, 42, 1_500, -8, 28));
        assert!(!is_double_left_press(previous, 43, 1_200, -12, 24));
        assert!(!is_double_left_press(previous, 42, 1_501, -12, 24));
        assert!(!is_double_left_press(previous, 42, 1_100, -7, 24));
    }
}
