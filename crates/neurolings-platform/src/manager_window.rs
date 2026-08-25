//! 管理器（Manager）主窗口的显示/隐藏/前置控制。
//!
//! Manager 是独立进程，这里按窗口标题定位其顶层窗口后操作，
//! 与托盘 Show/Hide、左键单击切换的语义保持一致。

#[cfg(any(target_os = "macos", test))]
use std::time::{Duration, Instant};

/// Manager 主窗口英文标题（FindWindow 与托盘定位用）。
pub const MANAGER_WINDOW_TITLE: &str = "NeurolingsCE — Mascot Manager";
/// Manager 主窗口中文标题（语言切换后窗口标题会变成这一条）。
pub const MANAGER_WINDOW_TITLE_ZH: &str = "NeurolingsCE — 桌宠管理器";

/// 管理器心跳的可见性在此期限内可信。
#[cfg(any(target_os = "macos", test))]
const MANAGER_VISIBILITY_HEARTBEAT_TTL: Duration = Duration::from_secs(3);

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy)]
struct ManagerVisibilityHeartbeat {
    is_visible: bool,
    received_at: Instant,
}

#[cfg(any(target_os = "macos", test))]
fn recent_manager_visibility(
    heartbeat: Option<ManagerVisibilityHeartbeat>,
    now: Instant,
) -> Option<bool> {
    heartbeat.and_then(|heartbeat| {
        now.checked_duration_since(heartbeat.received_at)
            .filter(|elapsed| *elapsed < MANAGER_VISIBILITY_HEARTBEAT_TTL)
            .map(|_| heartbeat.is_visible)
    })
}

#[cfg(any(target_os = "macos", test))]
fn is_manager_executable_name(name: &str) -> bool {
    name.trim()
        .strip_suffix(".app")
        .unwrap_or(name.trim())
        .eq_ignore_ascii_case("neurolings_manager")
}

#[cfg(windows)]
mod imp {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, IsWindowVisible, SW_HIDE, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
    };
    use windows::core::PCWSTR;

    fn encode_title(title: &str) -> Vec<u16> {
        title.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn find_by_title(title: &str) -> Option<HWND> {
        let title = encode_title(title);
        let hwnd = unsafe { FindWindowW(None, PCWSTR(title.as_ptr())) }.unwrap_or_default();
        (!hwnd.is_invalid()).then_some(hwnd)
    }

    fn find_manager() -> Option<HWND> {
        // 中英文标题都查：语言切换后窗口标题会变，托盘/活动窗口过滤仍需找得到。
        find_by_title(crate::manager_window::MANAGER_WINDOW_TITLE)
            .or_else(|| find_by_title(crate::manager_window::MANAGER_WINDOW_TITLE_ZH))
    }

    /// 该 HWND 是否为管理器主窗口（桌宠不可把它当活动窗口攀附）。
    pub fn is_hwnd(handle: usize) -> bool {
        find_manager().is_some_and(|hwnd| hwnd.0 as usize == handle)
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
pub use imp::{hide, is_hwnd, is_running, is_visible, show, toggle};

#[cfg(target_os = "linux")]
mod imp_linux {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        self, AtomEnum, ClientMessageData, ClientMessageEvent, ConnectionExt, EventMask, MapState,
    };
    use x11rb::rust_connection::RustConnection;

    struct Atoms {
        client_list: xproto::Atom,
        net_wm_name: xproto::Atom,
        utf8_string: xproto::Atom,
        net_active_window: xproto::Atom,
        net_wm_state: xproto::Atom,
        net_wm_state_hidden: xproto::Atom,
    }

    fn intern(conn: &RustConnection, name: &str) -> Option<xproto::Atom> {
        conn.intern_atom(false, name.as_bytes())
            .ok()?
            .reply()
            .ok()
            .map(|reply| reply.atom)
    }

    fn connect() -> Option<(RustConnection, xproto::Window, Atoms)> {
        let (conn, screen_index) = RustConnection::connect(None).ok()?;
        let root = conn.setup().roots.get(screen_index)?.root;
        let atoms = Atoms {
            client_list: intern(&conn, "_NET_CLIENT_LIST")?,
            net_wm_name: intern(&conn, "_NET_WM_NAME")?,
            utf8_string: intern(&conn, "UTF8_STRING")?,
            net_active_window: intern(&conn, "_NET_ACTIVE_WINDOW")?,
            net_wm_state: intern(&conn, "_NET_WM_STATE")?,
            net_wm_state_hidden: intern(&conn, "_NET_WM_STATE_HIDDEN")?,
        };
        Some((conn, root, atoms))
    }

    fn property_values(
        conn: &RustConnection,
        window: xproto::Window,
        property: xproto::Atom,
        property_type: xproto::Atom,
    ) -> Vec<u32> {
        let Ok(cookie) = conn.get_property(false, window, property, property_type, 0, 4096) else {
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

    fn manager_windows(
        conn: &RustConnection,
        root: xproto::Window,
        atoms: &Atoms,
    ) -> Vec<xproto::Window> {
        let mut windows = property_values(conn, root, atoms.client_list, AtomEnum::WINDOW.into());
        if windows.is_empty()
            && let Ok(cookie) = conn.query_tree(root)
            && let Ok(reply) = cookie.reply()
        {
            windows.extend(reply.children);
        }
        windows
    }

    fn title(conn: &RustConnection, window: xproto::Window, atoms: &Atoms) -> String {
        let value = conn
            .get_property(false, window, atoms.net_wm_name, atoms.utf8_string, 0, 256)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| reply.value)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                conn.get_property(false, window, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 256)
                    .ok()
                    .and_then(|cookie| cookie.reply().ok())
                    .map(|reply| reply.value)
            })
            .unwrap_or_default();
        String::from_utf8_lossy(&value)
            .trim_end_matches('\0')
            .to_string()
    }

    fn is_manager_window(conn: &RustConnection, window: xproto::Window, atoms: &Atoms) -> bool {
        let title = title(conn, window, atoms);
        title == crate::manager_window::MANAGER_WINDOW_TITLE
            || title == crate::manager_window::MANAGER_WINDOW_TITLE_ZH
    }

    fn find_manager(
        conn: &RustConnection,
        root: xproto::Window,
        atoms: &Atoms,
    ) -> Option<xproto::Window> {
        manager_windows(conn, root, atoms)
            .into_iter()
            .find(|window| is_manager_window(conn, *window, atoms))
    }

    fn send_client_message(
        conn: &RustConnection,
        root: xproto::Window,
        event: ClientMessageEvent,
    ) -> bool {
        let Ok(cookie) = conn.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        ) else {
            return false;
        };
        cookie.check().is_ok()
    }

    /// 该窗口是否由当前 Manager 进程创建。
    pub fn is_hwnd(handle: usize) -> bool {
        let Some((conn, root, atoms)) = connect() else {
            return false;
        };
        is_manager_window(&conn, handle as u32, &atoms)
            && manager_windows(&conn, root, &atoms).contains(&(handle as u32))
    }

    /// Manager 主窗口的顶层 X11 窗口是否存在。
    pub fn is_running() -> bool {
        let Some((conn, root, atoms)) = connect() else {
            return false;
        };
        find_manager(&conn, root, &atoms).is_some()
    }

    /// Manager 主窗口当前是否已映射且可见。
    pub fn is_visible() -> bool {
        let Some((conn, root, atoms)) = connect() else {
            return false;
        };
        let Some(window) = find_manager(&conn, root, &atoms) else {
            return false;
        };
        let Ok(attributes) = conn.get_window_attributes(window) else {
            return false;
        };
        let Ok(attributes) = attributes.reply() else {
            return false;
        };
        if attributes.map_state != MapState::VIEWABLE {
            return false;
        }
        !property_values(&conn, window, atoms.net_wm_state, AtomEnum::ATOM.into())
            .contains(&atoms.net_wm_state_hidden)
    }

    /// 显示并请求窗口管理器激活 Manager 主窗口。
    pub fn show() -> bool {
        let Some((conn, root, atoms)) = connect() else {
            return false;
        };
        let Some(window) = find_manager(&conn, root, &atoms) else {
            return false;
        };
        let remove_hidden = ClientMessageEvent::new(
            32,
            window,
            atoms.net_wm_state,
            ClientMessageData::from([0, atoms.net_wm_state_hidden, 0, 0, 0]),
        );
        let activate = ClientMessageEvent::new(
            32,
            window,
            atoms.net_active_window,
            ClientMessageData::from([
                1,
                0,
                property_values(
                    &conn,
                    root,
                    atoms.net_active_window,
                    AtomEnum::WINDOW.into(),
                )
                .first()
                .copied()
                .unwrap_or(0),
                0,
                0,
            ]),
        );
        let mapped = conn
            .map_window(window)
            .ok()
            .is_some_and(|cookie| cookie.check().is_ok());
        let hidden_sent = send_client_message(&conn, root, remove_hidden);
        let active_sent = send_client_message(&conn, root, activate);
        // 部分轻量窗口管理器不实现 EWMH 激活消息，但仍允许直接映射窗口。
        conn.flush().is_ok() && (mapped || hidden_sent || active_sent)
    }

    /// 取消映射 Manager 主窗口，使其回到托盘隐藏状态。
    pub fn hide() -> bool {
        let Some((conn, root, atoms)) = connect() else {
            return false;
        };
        let Some(window) = find_manager(&conn, root, &atoms) else {
            return false;
        };
        let _ = atoms;
        let Ok(cookie) = conn.unmap_window(window) else {
            return false;
        };
        cookie.check().is_ok() && conn.flush().is_ok()
    }

    /// 在可见与隐藏状态之间切换 Manager。
    pub fn toggle() -> bool {
        if is_visible() { hide() } else { show() }
    }
}

#[cfg(target_os = "linux")]
pub use imp_linux::{hide, is_hwnd, is_running, is_visible, show, toggle};

#[cfg(target_os = "macos")]
mod imp_macos {
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;

    use objc2::rc::autoreleasepool;
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};

    use super::{ManagerVisibilityHeartbeat, recent_manager_visibility};

    static RECENT_MANAGER_VISIBILITY: OnceLock<Mutex<Option<ManagerVisibilityHeartbeat>>> =
        OnceLock::new();

    fn visibility_cache() -> &'static Mutex<Option<ManagerVisibilityHeartbeat>> {
        RECENT_MANAGER_VISIBILITY.get_or_init(|| Mutex::new(None))
    }

    pub(super) fn report_visibility(is_visible: bool) {
        let Ok(mut cache) = visibility_cache().lock() else {
            return;
        };
        *cache = Some(ManagerVisibilityHeartbeat {
            is_visible,
            received_at: Instant::now(),
        });
    }

    fn cached_visibility() -> Option<bool> {
        let heartbeat = visibility_cache().lock().ok().and_then(|cache| *cache);
        recent_manager_visibility(heartbeat, Instant::now())
    }

    fn is_manager_application(application: &NSRunningApplication) -> bool {
        if application.isTerminated() {
            return false;
        }
        application
            .executableURL()
            .and_then(|url| url.lastPathComponent())
            .is_some_and(|name| super::is_manager_executable_name(&name.to_string()))
            || application
                .localizedName()
                .is_some_and(|name| super::is_manager_executable_name(&name.to_string()))
    }

    fn with_manager<T>(operation: impl FnOnce(&NSRunningApplication) -> T) -> Option<T> {
        autoreleasepool(|_| {
            let applications = NSWorkspace::sharedWorkspace().runningApplications();
            applications
                .iter()
                .find(|application| is_manager_application(application))
                .map(|application| operation(&application))
        })
    }

    fn show_application(application: &NSRunningApplication) -> bool {
        let unhidden = !application.isHidden() || application.unhide();
        #[allow(deprecated)]
        let activated = application.activateWithOptions(
            NSApplicationActivationOptions::ActivateAllWindows
                | NSApplicationActivationOptions::ActivateIgnoringOtherApps,
        );
        unhidden || activated
    }

    /// macOS 的活动窗口标识不是可跨进程验证的 NSWindow 指针。
    pub fn is_hwnd(_: usize) -> bool {
        false
    }

    /// Manager 应用是否仍在运行。
    pub fn is_running() -> bool {
        with_manager(|_| ()).is_some()
    }

    /// Manager 窗口是否可见；近期心跳优先于应用级隐藏状态。
    pub fn is_visible() -> bool {
        cached_visibility()
            .unwrap_or_else(|| with_manager(|application| !application.isHidden()).unwrap_or(false))
    }

    /// 恢复并激活 Manager 应用。
    pub fn show() -> bool {
        let shown = with_manager(show_application).unwrap_or(false);
        if shown {
            report_visibility(true);
        }
        shown
    }

    /// 隐藏 Manager 应用。
    pub fn hide() -> bool {
        let hidden = with_manager(|application| application.hide()).unwrap_or(false);
        if hidden {
            report_visibility(false);
        }
        hidden
    }

    /// 在窗口级可见与隐藏状态之间切换 Manager。
    pub fn toggle() -> bool {
        if is_visible() { hide() } else { show() }
    }
}

#[cfg(target_os = "macos")]
pub use imp_macos::{hide, is_hwnd, is_running, is_visible, show, toggle};

/// 记录 Manager 最近上报的窗口可见性。
#[cfg(target_os = "macos")]
pub fn report_visibility(is_visible: bool) {
    imp_macos::report_visibility(is_visible);
}

/// 非 macOS 平台不依赖心跳可见性缓存。
#[cfg(not(target_os = "macos"))]
pub fn report_visibility(_: bool) {}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod imp_stub {
    /// 不支持窗口管理器定位的平台返回明确的未支持结果。
    pub fn is_hwnd(_: usize) -> bool {
        false
    }
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

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub use imp_stub::{hide, is_hwnd, is_running, is_visible, show, toggle};

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{
        ManagerVisibilityHeartbeat, is_manager_executable_name, recent_manager_visibility,
    };

    #[test]
    fn manager_executable_name_accepts_bundle_and_binary_names() {
        assert!(is_manager_executable_name("neurolings_manager"));
        assert!(is_manager_executable_name("Neurolings_Manager.app"));
        assert!(!is_manager_executable_name("neurolings_runtime"));
        assert!(!is_manager_executable_name("neurolings_manager.exe"));
    }

    #[test]
    fn recent_manager_heartbeat_reports_window_visibility() {
        let now = Instant::now();
        let heartbeat = ManagerVisibilityHeartbeat {
            is_visible: false,
            received_at: now.checked_sub(Duration::from_secs(2)).unwrap(),
        };

        assert_eq!(recent_manager_visibility(Some(heartbeat), now), Some(false));
    }

    #[test]
    fn manager_visibility_heartbeat_expires_after_three_seconds() {
        let now = Instant::now();
        let heartbeat = ManagerVisibilityHeartbeat {
            is_visible: true,
            received_at: now.checked_sub(Duration::from_secs(3)).unwrap(),
        };

        assert_eq!(recent_manager_visibility(Some(heartbeat), now), None);
    }
}
