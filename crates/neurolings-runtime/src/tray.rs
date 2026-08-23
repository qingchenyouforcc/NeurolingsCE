//! 系统托盘：显示/隐藏管理器、召唤模板子菜单、全部关闭、退出。
//! Windows 使用 tray-icon/muda，macOS 使用 NSStatusItem，Linux 为占位。
//!
//! 与原版对齐：图标首选应用 ico；菜单文案随 Locale 本地化；
//! 菜单项与左键单击都是"切换管理器可见性"。

use crate::settings::Locale;

/// 托盘文案（译文对齐原版 translations/shijima-qt_zh_CN.ts）。
pub(crate) struct TrayTexts {
    pub hide: &'static str,
    pub show: &'static str,
    pub spawn: &'static str,
    pub none: &'static str,
    pub kill_all: &'static str,
    pub quit: &'static str,
}

/// 按语言设置取托盘文案。
pub(crate) fn texts(locale: Locale) -> TrayTexts {
    match locale {
        Locale::En => TrayTexts {
            hide: "Hide",
            show: "Show",
            spawn: "Spawn",
            none: "(none)",
            kill_all: "Kill all",
            quit: "Quit",
        },
        Locale::ZhCn => TrayTexts {
            hide: "隐藏",
            show: "显示",
            spawn: "生成",
            none: "(无)",
            kill_all: "全部关闭",
            quit: "退出",
        },
    }
}

#[cfg(windows)]
mod imp {
    use muda::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
    use tray_icon::{TrayIcon, TrayIconBuilder};

    use std::cell::RefCell;

    use super::{Locale, texts};

    thread_local! {
        static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
        static CURRENT_LOCALE: RefCell<Locale> = const { RefCell::new(Locale::En) };
        static LAST_VISIBLE: RefCell<bool> = const { RefCell::new(false) };
    }

    const SPAWN_PREFIX: &str = "spawn:";

    #[derive(Debug, Clone, PartialEq)]
    pub enum TrayCommand {
        None,
        /// 切换管理器可见性（菜单项与左键单击共用，与原版一致）。
        ToggleManager,
        Spawn(String),
        CloseAll,
        Quit,
    }

    fn build_menu(template_names: &[String], locale: Locale) -> Box<Menu> {
        let t = texts(locale);
        let manager_visible = neurolings_platform::manager_window::is_visible();
        let toggle_text = if manager_visible { t.hide } else { t.show };
        let toggle = MenuItem::with_id(MenuId::new("toggle_manager"), toggle_text, true, None);
        let spawn_submenu = Submenu::new(t.spawn, true);
        if template_names.is_empty() {
            let empty = MenuItem::with_id(MenuId::new("spawn:none"), t.none, false, None);
            let _ = spawn_submenu.append(&empty);
        } else {
            // 排序与原版一致：大小写不敏感
            let mut sorted = template_names.to_vec();
            sorted.sort_by_key(|name| name.to_lowercase());
            for name in sorted {
                let id = format!("{SPAWN_PREFIX}{name}");
                let item = MenuItem::with_id(MenuId::new(&id), &name, true, None);
                let _ = spawn_submenu.append(&item);
            }
        }
        let kill_all = MenuItem::with_id(MenuId::new("close_all"), t.kill_all, true, None);
        let quit = MenuItem::with_id(MenuId::new("quit"), t.quit, true, None);
        let menu = Menu::new();
        let _ = menu.append(&toggle);
        let _ = menu.append(&spawn_submenu);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&kill_all);
        let _ = menu.append(&quit);
        Box::new(menu)
    }

    /// 图标回退链与原版一致：应用 ico → 内嵌默认桌宠图。
    fn icon_fallback() -> Option<tray_icon::Icon> {
        let candidates: &[&[u8]] = &[
            include_bytes!("../../../assets/neurolingsce.ico"),
            include_bytes!("../../../assets/DefaultMascot/img/shime1.png"),
        ];
        for bytes in candidates {
            if let Ok(img) = image::load_from_memory(bytes) {
                let img = img.to_rgba8();
                let (w, h) = img.dimensions();
                if let Ok(icon) = tray_icon::Icon::from_rgba(img.into_raw(), w, h) {
                    return Some(icon);
                }
            }
        }
        None
    }

    pub fn init(template_names: &[String], locale: Locale) {
        let Some(icon) = icon_fallback() else { return };
        let menu = build_menu(template_names, locale);
        let Ok(tray) = TrayIconBuilder::new()
            .with_menu(menu)
            .with_icon(icon)
            .with_tooltip(neurolings_common::version::APP_NAME)
            .build()
        else {
            return;
        };
        CURRENT_LOCALE.with(|slot| *slot.borrow_mut() = locale);
        LAST_VISIBLE
            .with(|slot| *slot.borrow_mut() = neurolings_platform::manager_window::is_visible());
        TRAY.with(|slot| *slot.borrow_mut() = Some(tray));
    }

    /// 刷新菜单（模板增删或语言变化后调用）。
    pub fn refresh(template_names: &[String]) {
        TRAY.with(|slot| {
            if let Some(tray) = slot.borrow().as_ref() {
                let locale = CURRENT_LOCALE.with(|l| *l.borrow());
                let menu = build_menu(template_names, locale);
                tray.set_menu(Some(menu));
            }
        });
    }

    /// 语言变化后刷新菜单文案。
    pub fn set_locale(locale: Locale) {
        CURRENT_LOCALE.with(|slot| *slot.borrow_mut() = locale);
    }

    /// 主循环低频调用：管理器可见性变化时重建菜单（同步 Show/Hide 文案）。
    pub fn sync_visibility(template_names: &[String]) {
        let visible = neurolings_platform::manager_window::is_visible();
        let changed = LAST_VISIBLE.with(|slot| {
            let mut guard = slot.borrow_mut();
            std::mem::replace(&mut *guard, visible) != visible
        });
        if changed {
            refresh(template_names);
        }
    }

    pub fn poll() -> TrayCommand {
        let mut command = TrayCommand::None;
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let id = event.id().0.as_str();
            if let Some(name) = id.strip_prefix(SPAWN_PREFIX) {
                if name != "none" {
                    command = TrayCommand::Spawn(name.to_string());
                }
                continue;
            }
            match id {
                "toggle_manager" => command = TrayCommand::ToggleManager,
                "close_all" => command = TrayCommand::CloseAll,
                "quit" => command = TrayCommand::Quit,
                _ => {}
            }
        }
        // 左键单击/双击与菜单切换语义一致（与原版 trayIconActivated 对齐）。
        while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            let is_toggle = matches!(
                event,
                tray_icon::TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    button_state: tray_icon::MouseButtonState::Up,
                    ..
                } | tray_icon::TrayIconEvent::DoubleClick {
                    button: tray_icon::MouseButton::Left,
                    ..
                }
            );
            if is_toggle {
                command = TrayCommand::ToggleManager;
            }
        }
        command
    }
}

#[cfg(windows)]
pub use imp::{TrayCommand, init, poll, refresh, set_locale, sync_visibility};

#[cfg(target_os = "macos")]
mod imp_macos {
    use std::cell::RefCell;

    use super::{Locale, texts};

    thread_local! {
        static CURRENT_LOCALE: RefCell<Locale> = const { RefCell::new(Locale::En) };
        static LAST_VISIBLE: RefCell<bool> = const { RefCell::new(false) };
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum TrayCommand {
        None,
        ToggleManager,
        Spawn(String),
        CloseAll,
        Quit,
    }

    fn icon_rgba() -> Option<(Vec<u8>, u32, u32)> {
        let candidates: &[&[u8]] = &[
            include_bytes!("../../../assets/neurolingsce.ico"),
            include_bytes!("../../../assets/DefaultMascot/img/shime1.png"),
        ];
        for bytes in candidates {
            if let Ok(img) = image::load_from_memory(bytes) {
                let img = img.to_rgba8();
                let (w, h) = img.dimensions();
                return Some((img.into_raw(), w, h));
            }
        }
        None
    }

    fn apply_menu(template_names: &[String]) {
        let locale = CURRENT_LOCALE.with(|l| *l.borrow());
        let t = texts(locale);
        let visible = neurolings_platform::manager_window::is_visible();
        let toggle = if visible { t.hide } else { t.show };
        neurolings_platform::macos::tray::tray_set_menu(
            toggle,
            t.spawn,
            t.none,
            t.kill_all,
            t.quit,
            template_names,
        );
    }

    pub fn init(template_names: &[String], locale: Locale) {
        let Some((rgba, w, h)) = icon_rgba() else {
            return;
        };
        if !neurolings_platform::macos::tray::tray_init(
            neurolings_common::version::APP_NAME,
            &rgba,
            w,
            h,
        ) {
            return;
        }
        CURRENT_LOCALE.with(|slot| *slot.borrow_mut() = locale);
        apply_menu(template_names);
    }

    pub fn refresh(template_names: &[String]) {
        apply_menu(template_names);
    }

    pub fn set_locale(locale: Locale) {
        CURRENT_LOCALE.with(|slot| *slot.borrow_mut() = locale);
    }

    pub fn sync_visibility(template_names: &[String]) {
        let visible = neurolings_platform::manager_window::is_visible();
        let changed = LAST_VISIBLE.with(|slot| {
            let mut guard = slot.borrow_mut();
            std::mem::replace(&mut *guard, visible) != visible
        });
        if changed {
            apply_menu(template_names);
        }
    }

    pub fn poll() -> TrayCommand {
        let Some(command) = neurolings_platform::macos::tray::tray_poll() else {
            return TrayCommand::None;
        };
        match command.as_str() {
            "toggle_manager" => TrayCommand::ToggleManager,
            "close_all" => TrayCommand::CloseAll,
            "quit" => TrayCommand::Quit,
            other if other.starts_with("spawn:") => {
                let name = &other[6..];
                if name == "none" {
                    TrayCommand::None
                } else {
                    TrayCommand::Spawn(name.to_string())
                }
            }
            _ => TrayCommand::None,
        }
    }
}

#[cfg(target_os = "macos")]
pub use imp_macos::{TrayCommand, init, poll, refresh, set_locale, sync_visibility};

#[cfg(target_os = "linux")]
mod imp_stub {
    /// 非 Windows 平台的占位实现（阶段 7 补 Linux/macOS 托盘）。
    #[derive(Debug, Clone, PartialEq)]
    pub enum TrayCommand {
        None,
        ToggleManager,
        Spawn(String),
        CloseAll,
        Quit,
    }
    pub fn init(_: &[String], _: crate::settings::Locale) {}
    pub fn poll() -> TrayCommand {
        TrayCommand::None
    }
    pub fn refresh(_: &[String]) {}
    pub fn set_locale(_: crate::settings::Locale) {}
    pub fn sync_visibility(_: &[String]) {}
}

#[cfg(target_os = "linux")]
pub use imp_stub::{TrayCommand, init, poll, refresh, set_locale, sync_visibility};
