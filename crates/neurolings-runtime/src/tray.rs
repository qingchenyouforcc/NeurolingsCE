//! 系统托盘：显示管理器、召唤模板子菜单、全部关闭、退出。
//! Windows 使用 tray-icon/muda，Linux/macOS 为占位（返回 None）。
//! 菜单由主循环轮询，支持动态刷新（模板增删后重建）。

#[cfg(windows)]
mod imp {
    use muda::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    use std::cell::RefCell;

    thread_local! {
        static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
        static CURRENT_SHOW: RefCell<String> = RefCell::new("Show manager".to_string());
    }

    const SPAWN_PREFIX: &str = "spawn:";

    #[derive(Debug, Clone, PartialEq)]
    pub enum TrayCommand {
        None,
        ShowManager,
        Spawn(String),
        CloseAll,
        Quit,
    }

    fn build_menu(template_names: &[String], show_text: &str) -> Box<Menu> {
        let menu = Menu::new();
        let show = MenuItem::with_id(MenuId::new("show_manager"), show_text, true, None);
        let spawn_submenu = Submenu::new("Spawn", true);
        if template_names.is_empty() {
            let empty = MenuItem::with_id(MenuId::new("spawn:none"), "(none)", false, None);
            let _ = spawn_submenu.append(&empty);
        } else {
            // 排序与原版 ManagerTrayIcon 排序一致：大小写不敏感
            let mut sorted = template_names.to_vec();
            sorted.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
            for name in sorted {
                let id = format!("{SPAWN_PREFIX}{name}");
                let item = MenuItem::with_id(MenuId::new(&id), &name, true, None);
                let _ = spawn_submenu.append(&item);
            }
        }
        let kill_all = MenuItem::with_id(MenuId::new("close_all"), "Kill all", true, None);
        let quit = MenuItem::with_id(MenuId::new("quit"), "Quit", true, None);
        let _ = menu.append_items(&[&show, &spawn_submenu]);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append_items(&[&kill_all, &quit]);
        Box::new(menu)
    }

    fn icon_fallback() -> Option<Icon> {
        // 尝试顺序与原版 makeTrayIconFallback 对齐：shime1.png -> neurolingsce.ico
        let candidates: &[&[u8]] = &[
            include_bytes!("../../../assets/DefaultMascot/img/shime1.png"),
        ];
        for png in candidates {
            if let Ok(img) = image::load_from_memory(png) {
                let img = img.to_rgba8();
                let (w, h) = img.dimensions();
                if let Ok(icon) = Icon::from_rgba(img.into_raw(), w, h) {
                    return Some(icon);
                }
            }
        }
        None
    }

    pub fn init() {
        let Some(icon) = icon_fallback() else { return; };
        let menu = build_menu(&["Default".to_string()], "Show manager");
        let Ok(tray) = TrayIconBuilder::new()
            .with_menu(menu)
            .with_icon(icon)
            .with_tooltip("NeurolingsCE")
            .build()
        else {
            return;
        };
        TRAY.with(|slot| *slot.borrow_mut() = Some(tray));
    }

    /// 刷新 Spawn 子菜单（模板增删后调用）。
    pub fn refresh(template_names: &[String]) {
        TRAY.with(|slot| {
            if let Some(tray) = slot.borrow().as_ref() {
                let show_text = CURRENT_SHOW.with(|s| s.borrow().clone());
                let menu = build_menu(template_names, &show_text);
                let _ = tray.set_menu(Some(menu));
            }
        });
    }

    /// 更新 Show/Hide 文本（与窗口显隐同步）。
    pub fn set_show_text(visible: bool) {
        let text = if visible { "Hide manager" } else { "Show manager" };
        CURRENT_SHOW.with(|s| *s.borrow_mut() = text.to_string());
        // 触发一次刷新以更新菜单文本（需当前模板列表，调用方会随后 refresh）
    }

    pub fn poll() -> TrayCommand {
        let mut command = TrayCommand::None;
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let id = event.id().0.as_str();
            if let Some(name) = id.strip_prefix(SPAWN_PREFIX) {
                command = TrayCommand::Spawn(name.to_string());
                continue;
            }
            match id {
                "show_manager" => command = TrayCommand::ShowManager,
                "close_all" => command = TrayCommand::CloseAll,
                "quit" => command = TrayCommand::Quit,
                _ => {}
            }
        }
        command
    }
}

#[cfg(windows)]
pub use imp::{TrayCommand, init, poll, refresh, set_show_text};

#[cfg(not(windows))]
mod imp_stub {
    #[derive(Debug, Clone, PartialEq)]
    pub enum TrayCommand {
        None,
        ShowManager,
        Spawn(String),
        CloseAll,
        Quit,
    }
    pub fn init() {}
    pub fn poll() -> TrayCommand { TrayCommand::None }
    pub fn refresh(_: &[String]) {}
    pub fn set_show_text(_: bool) {}
}

#[cfg(not(windows))]
pub use imp_stub::{TrayCommand, init, poll, refresh, set_show_text};
