//! 系统托盘（Windows）：显示管理器、召唤模板子菜单、全部关闭、退出。
//! 菜单事件由主循环轮询。

use muda::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use std::cell::RefCell;

thread_local! {
    static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
}

/// 召唤子菜单项的 id 前缀。
const SPAWN_PREFIX: &str = "spawn:";

#[derive(Debug, Clone, PartialEq)]
pub enum TrayCommand {
    None,
    ShowManager,
    Spawn(String),
    CloseAll,
    Quit,
}

fn build_menu(template_names: &[String]) -> Box<Menu> {
    let menu = Menu::new();
    let show = MenuItem::with_id(MenuId::new("show_manager"), "Show manager", true, None);
    let spawn_submenu = Submenu::new("Spawn", true);
    if template_names.is_empty() {
        let empty = MenuItem::with_id(MenuId::new("spawn:none"), "(none)", false, None);
        let _ = spawn_submenu.append(&empty);
    } else {
        for name in template_names {
            let id = format!("{SPAWN_PREFIX}{name}");
            let item = MenuItem::with_id(MenuId::new(&id), name, true, None);
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

pub fn init() {
    let png = include_bytes!("../../../assets/DefaultMascot/img/shime1.png");
    let Ok(img) = image::load_from_memory(png) else {
        return;
    };
    let img = img.to_rgba8();
    let (width, height) = img.dimensions();
    let Ok(icon) = Icon::from_rgba(img.into_raw(), width, height) else {
        return;
    };

    let menu = build_menu(&["Default".to_string()]);
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
