//! 运行时主循环：帧调度、会话生命周期、事件路由与沙盒窗口模式。

pub mod bubbles;
pub mod environment;
pub mod inspector;
pub mod interaction;
pub mod session;
pub mod sounds;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use neurolings_engine::environment::Environment;
use neurolings_engine::mascot::Factory;
use neurolings_engine::math::Vec2;
use neurolings_platform::{MascotBackend, MascotEvent, MascotEventKind, Point, ScreenInfo};

use crate::combinations::CombinationStore;
use crate::runtime::environment::EnvironmentSet;
use crate::runtime::interaction::{GestureOutcome, MenuAction};
use crate::runtime::session::{Session, TICK_INTERVAL_MS, create_session};
use crate::services::{CommandChannel, RuntimeView};
use crate::settings::Settings;
use crate::templates::TemplateStore;

/// 沙盒（窗口）模式的环境尺寸。
const SANDBOX_WIDTH: i32 = 640;
const SANDBOX_HEIGHT: i32 = 480;
/// 沙盒窗口合成用的底色（浅色画布）。
const SANDBOX_BG: [u8; 4] = [0xF0, 0xF0, 0xF0, 0xFF];
/// 沙盒窗口的事件 id（与桌宠/气泡 id 空间隔开）。
const SANDBOX_WINDOW_ID: u64 = 2_000_000;
/// 窗口模式下会话窗口挪到屏外隐藏的位置。
const OFFSCREEN: Point = Point {
    x: -10_000,
    y: -10_000,
};

pub struct RuntimeOptions {
    pub templates: Vec<crate::templates::LoadedTemplate>,
    pub screen: ScreenInfo,
    pub spawn_name: Option<String>,
    pub tick_limit: Option<u64>,
    pub headless: bool,
    pub enable_ipc: bool,
    pub enable_http: bool,
    /// CLI 自动拉起的运行时模式（不拉起管理器）。
    pub cli_runtime_mode: bool,
    /// 开机自启模式（静默恢复组合）。
    pub startup_mode: bool,
}

/// run 内部的启动参数集合（模板已转入工厂后的剩余项）。
struct RuntimeOpts {
    #[allow(dead_code)]
    screen: ScreenInfo,
    spawn_name: Option<String>,
    headless: bool,
    cli_runtime_mode: bool,
    startup_mode: bool,
}

/// 窗口化模式的沙盒状态。
struct Sandbox {
    window: Box<dyn neurolings_platform::MascotWindow>,
    top_left: Point,
}

struct RunState {
    sessions: Vec<Session>,
    factory: Factory,
    templates: TemplateStore,
    envs: EnvironmentSet,
    settings: Settings,
    app_data_dir: PathBuf,
    labels: HashMap<i64, u64>,
    next_label: i64,
    next_session_id: u64,
    combinations: CombinationStore,
    quit: bool,
    windowed: bool,
    sandbox: Option<Sandbox>,
    bubble_texts: HashMap<String, Vec<String>>,
}

pub fn run(opts: RuntimeOptions) -> Result<u64, String> {
    crate::services::debug_log("run: 运行时启动");
    let RuntimeOptions {
        templates,
        screen,
        spawn_name,
        tick_limit,
        headless,
        enable_ipc,
        enable_http,
        cli_runtime_mode,
        startup_mode,
    } = opts;
    let opts = RuntimeOpts {
        screen,
        spawn_name,
        headless,
        cli_runtime_mode,
        startup_mode,
    };
    let storage = neurolings_pack::storage::default_storage_path();
    let app_data_dir = storage
        .as_ref()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(PathBuf::from))
                .unwrap_or_default()
        });
    let settings = Settings::load(&app_data_dir);

    let mut template_store = TemplateStore::new();
    let mut factory = Factory::new(None);
    for template in templates {
        template_store.register(&template);
        factory
            .register_template(template.engine_template())
            .map_err(|e| e.to_string())?;
    }
    if template_store.names_sorted().is_empty() {
        return Err("no mascot templates found".into());
    }

    let backend: Option<Rc<RefCell<Box<dyn MascotBackend>>>> = if headless {
        None
    } else {
        match neurolings_platform::create_backend() {
            Ok(b) => Some(Rc::new(RefCell::new(b))),
            Err(e) => return Err(e.to_string()),
        }
    };

    // 单实例守卫：绑定失败说明已有运行时实例。
    let ipc_server = if enable_ipc {
        match crate::ipc::IpcServer::bind() {
            Ok(server) => Some(server),
            Err(err) => {
                return Err(format!(
                    "another NeurolingsCE runtime is already running ({err})"
                ));
            }
        }
    } else {
        None
    };

    let commands = CommandChannel::new();
    let command_tx = commands.sender();
    let mut _ipc_thread = None;
    if let Some(server) = ipc_server {
        let tx = command_tx.clone();
        _ipc_thread = Some(std::thread::spawn(move || {
            server.serve(tx);
        }));
    }
    let mut _http_thread = None;
    if enable_http {
        let tx = command_tx.clone();
        _http_thread = Some(std::thread::spawn(move || {
            crate::http::serve(tx);
        }));
    }

    let mut state = RunState {
        sessions: Vec::new(),
        factory,
        templates: template_store,
        envs: EnvironmentSet {
            screens: Vec::new(),
            sandbox: None,
            push_target: 0,
        },
        settings,
        app_data_dir: app_data_dir.clone(),
        labels: HashMap::new(),
        next_label: 0,
        next_session_id: 1,
        combinations: CombinationStore::new(&app_data_dir),
        quit: false,
        windowed: false,
        sandbox: None,
        bubble_texts: HashMap::new(),
    };

    startup_spawn(&mut state, &opts, &backend);

    let mut tick_count: u64 = 0;
    let mut next_tick = Instant::now();

    while !state.quit {
        if let Some(limit) = tick_limit
            && tick_count >= limit
        {
            break;
        }

        // 输入事件与托盘。
        let events = backend
            .as_ref()
            .map(|b| b.borrow_mut().pump_events())
            .unwrap_or_default();
        for event in events {
            handle_event(&mut state, &backend, event);
        }
        #[cfg(windows)]
        match crate::tray::poll() {
            crate::tray::TrayCommand::ShowManager => {
                crate::services::launch_manager();
            }
            crate::tray::TrayCommand::Spawn(name) => {
                let _ = spawn_default(&mut state, &backend, &name, "");
            }
            crate::tray::TrayCommand::CloseAll => {
                state.sessions.clear();
                state.labels.clear();
            }
            crate::tray::TrayCommand::Quit => state.quit = true,
            crate::tray::TrayCommand::None => {}
        }

        // IPC / HTTP 命令（主线程执行）。
        while let Some(cmd) = commands.try_recv() {
            let mut view = build_view(&mut state, &backend);
            let response = crate::services::dispatch(&cmd.request, &mut view);
            let _ = cmd.reply.send(response);
        }
        if state.quit {
            break;
        }

        // 窗口化模式切换：同步沙盒窗口与会话环境。
        if state.windowed && state.sandbox.is_none() {
            enter_sandbox(&mut state, &backend);
        } else if !state.windowed && state.sandbox.is_some() {
            exit_sandbox(&mut state);
        }

        // 固定频率推进一帧（环境每帧更新一次，与原版节拍一致）。
        let now = Instant::now();
        if now >= next_tick {
            next_tick += Duration::from_millis(TICK_INTERVAL_MS);
            tick_count += 1;
            if let Some(backend) = &backend {
                let sandbox_origin = state.sandbox.as_ref().map(|s| s.top_left);
                let mut b = backend.borrow_mut();
                state.envs.refresh(
                    &mut **b,
                    &state.settings,
                    state.windowed,
                    (SANDBOX_WIDTH, SANDBOX_HEIGHT),
                    sandbox_origin,
                );
            }
            install_push_callbacks(&mut state, &backend);
            migrate_removed_screens(&mut state);
            run_tick(&mut state, &backend);
        }

        std::thread::sleep(Duration::from_millis(1));
    }

    // 退出前保存"关闭前组合"，供静默启动恢复。
    save_last_combination(&mut state);
    Ok(tick_count)
}

fn build_view<'a>(
    state: &'a mut RunState,
    backend: &'a Option<Rc<RefCell<Box<dyn MascotBackend>>>>,
) -> RuntimeView<'a> {
    RuntimeView {
        sessions: &mut state.sessions,
        factory: &mut state.factory,
        envs: &mut state.envs,
        templates: &mut state.templates,
        settings: &mut state.settings,
        labels: &mut state.labels,
        next_label: &mut state.next_label,
        next_session_id: &mut state.next_session_id,
        combinations: &state.combinations,
        quit: &mut state.quit,
        backend,
        windowed: &mut state.windowed,
    }
}

/// 启动时的初始行为：普通启动拉起管理器；静默启动恢复组合；
/// CLI 运行时模式保持空闲等待指令。
fn startup_spawn(
    state: &mut RunState,
    opts: &RuntimeOpts,
    backend: &Option<Rc<RefCell<Box<dyn MascotBackend>>>>,
) {
    if opts.headless {
        // 冒烟测试直接召唤一只桌宠。
        let name = opts
            .spawn_name
            .clone()
            .or_else(|| {
                state
                    .templates
                    .names_sorted()
                    .iter()
                    .find(|n| n.as_str() == "Default")
                    .cloned()
            })
            .unwrap_or_else(|| state.templates.names_sorted()[0].clone());
        let env = state
            .envs
            .primary()
            .cloned()
            .or_else(|| Some(Rc::new(RefCell::new(Environment::default()))))
            .unwrap();
        let _ = create_session(
            &mut state.sessions,
            &state.factory,
            &mut None::<&mut Box<dyn MascotBackend>>,
            &env,
            &state.templates,
            &mut state.next_session_id,
            &name,
            None,
            "Fall",
        );
        return;
    }

    if opts.startup_mode {
        restore_startup_combination(state, backend);
        return;
    }
    if opts.cli_runtime_mode {
        return;
    }
    // 普通启动：拉起管理器；桌宠由用户/管理器召唤。
    crate::services::debug_log("startup_spawn: 普通启动分支，准备拉起管理器");
    crate::services::launch_manager();
    if let Some(name) = &opts.spawn_name {
        let _ = spawn_default(state, backend, name, "");
    }
}

fn spawn_default(
    state: &mut RunState,
    backend: &Option<Rc<RefCell<Box<dyn MascotBackend>>>>,
    name: &str,
    behavior: &str,
) -> Result<u64, String> {
    // 窗口化模式在沙盒环境内生成；否则使用主屏幕环境。
    let env = if state.windowed {
        state.envs.sandbox.clone()
    } else {
        state.envs.primary().cloned()
    };
    let env = env.ok_or("no environment")?;
    let mut guard = backend.as_ref().map(|b| b.borrow_mut());
    let mut backend_opt = guard.as_deref_mut();
    create_session(
        &mut state.sessions,
        &state.factory,
        &mut backend_opt,
        &env,
        &state.templates,
        &mut state.next_session_id,
        name,
        None,
        behavior,
    )
}

fn restore_startup_combination(
    state: &mut RunState,
    backend: &Option<Rc<RefCell<Box<dyn MascotBackend>>>>,
) {
    let mode = state
        .settings
        .get_string(crate::settings::KEY_STARTUP_COMBO_MODE, "last:");
    let name = if mode == "last:" {
        crate::combinations::LAST_BEFORE_CLOSE.to_string()
    } else if let Some(id) = mode.strip_prefix("id:") {
        id.to_string()
    } else {
        crate::combinations::LAST_BEFORE_CLOSE.to_string()
    };
    let Some(combo) = state.combinations.get(&name) else {
        return;
    };
    for member in &combo.members {
        let _ = spawn_default(state, backend, &member.template, "");
    }
}

fn save_last_combination(state: &mut RunState) {
    let templates: Vec<String> = state.sessions.iter().map(|s| s.name.clone()).collect();
    if templates.is_empty() {
        return;
    }
    let _ = state
        .combinations
        .save_combination(crate::combinations::LAST_BEFORE_CLOSE, templates);
}

/// 路由一条平台事件到对应会话（桌宠窗口或沙盒窗口）。
fn handle_event(
    state: &mut RunState,
    backend: &Option<Rc<RefCell<Box<dyn MascotBackend>>>>,
    event: MascotEvent,
) {
    if event.mascot_id == SANDBOX_WINDOW_ID {
        handle_sandbox_event(state, backend, event);
        return;
    }
    if event.mascot_id >= bubbles::BUBBLE_ID_BASE {
        return; // 气泡窗口不响应交互
    }
    let id = event.mascot_id;
    match event.kind {
        MascotEventKind::LeftDown => {
            if let Some(session) = state.sessions.iter_mut().find(|s| s.id == id) {
                session.on_left_down(event.screen, event.local);
            }
        }
        MascotEventKind::Move => {
            if let Some(session) = state.sessions.iter_mut().find(|s| s.id == id) {
                session.on_move(event.screen);
            }
        }
        MascotEventKind::LeftUp => {
            let outcome = state
                .sessions
                .iter_mut()
                .find(|s| s.id == id)
                .map(|s| s.on_left_up(event.screen));
            if let Some(GestureOutcome::Click(count)) = outcome.flatten() {
                maybe_show_click_bubble(state, id, count);
            }
        }
        MascotEventKind::LeftDoubleClick => {
            // 双击召唤同款桌宠（需允许繁殖）。
            let name = state.sessions.iter().find(|s| s.id == id).and_then(|s| {
                let allows = s
                    .manager
                    .state
                    .borrow()
                    .env
                    .as_ref()
                    .map(|e| e.borrow().allows_breeding)
                    .unwrap_or(false);
                allows.then(|| s.name.clone())
            });
            if let Some(name) = name {
                let _ = spawn_default(state, backend, &name, "");
            }
        }
        MascotEventKind::RightUp => {
            open_context_menu(state, backend, id, event.screen);
        }
    }
}

fn maybe_show_click_bubble(state: &mut RunState, session_id: u64, count: u32) {
    let enabled = state
        .settings
        .get_bool(crate::settings::KEY_BUBBLE_ENABLED, true);
    if !enabled {
        return;
    }
    let threshold = state
        .settings
        .get_i64(crate::settings::KEY_BUBBLE_CLICKS, 1)
        .max(1) as u32;
    if count != threshold {
        return;
    }
    let Some(session) = state.sessions.iter_mut().find(|s| s.id == session_id) else {
        return;
    };
    let texts = state
        .bubble_texts
        .entry(session.name.clone())
        .or_insert_with(|| bubbles::load_bubble_texts(&session.pack_dir, &state.app_data_dir));
    let roll = state
        .envs
        .primary()
        .map(|e| e.borrow_mut().random_int(i32::MAX) as usize)
        .unwrap_or(0);
    let text = bubbles::random_bubble_text(texts, roll);
    session.pending_bubble = Some(text);
}

fn open_context_menu(
    state: &mut RunState,
    backend: &Option<Rc<RefCell<Box<dyn MascotBackend>>>>,
    session_id: u64,
    at: Point,
) {
    let Some(session) = state.sessions.iter().find(|s| s.id == session_id) else {
        return;
    };
    let locale = state.settings.locale();
    let (entries, behavior_names) = interaction::build_context_menu(session, locale);
    let Some(inner) = backend else {
        return;
    };
    let choice = match inner.borrow_mut().show_menu(at, &entries) {
        Ok(choice) => choice,
        Err(_) => return,
    };
    let Some(choice) = choice else { return };
    let Some(action) = interaction::menu_action(choice, session_id, &behavior_names) else {
        return;
    };
    execute_menu_action(state, backend, action);
}

fn execute_menu_action(
    state: &mut RunState,
    backend: &Option<Rc<RefCell<Box<dyn MascotBackend>>>>,
    action: MenuAction,
) {
    match action {
        MenuAction::PauseToggle(id) => {
            if let Some(session) = state.sessions.iter_mut().find(|s| s.id == id) {
                session.paused = !session.paused;
            }
        }
        MenuAction::CallAnother(id) => {
            if let Some(name) = state
                .sessions
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.name.clone())
            {
                let _ = spawn_default(state, backend, &name, "");
            }
        }
        MenuAction::ShowManager => {
            crate::services::launch_manager();
        }
        MenuAction::Inspect(id) => {
            if let Some(session) = state.sessions.iter().find(|s| s.id == id) {
                let text = crate::runtime::inspector::inspect_text(session);
                if let Some(backend) = backend {
                    backend.borrow_mut().show_text_dialog(&session.name, &text);
                }
            }
        }
        MenuAction::DismissOthers(id) => {
            state.sessions.retain(|s| s.id == id);
            state.labels.retain(|_, mascot_id| *mascot_id == id);
        }
        MenuAction::DismissAll => {
            state.sessions.clear();
            state.labels.clear();
        }
        MenuAction::Dismiss(id) => {
            state.sessions.retain(|s| s.id != id);
            state.labels.retain(|_, mascot_id| *mascot_id != id);
        }
        MenuAction::Behavior(id, behavior) => {
            if let Some(session) = state.sessions.iter_mut().find(|s| s.id == id) {
                session.manager.next_behavior(&behavior);
            }
        }
    }
}

/// 沙盒窗口的事件路由：命中测试找到桌宠并转发。
fn handle_sandbox_event(
    state: &mut RunState,
    backend: &Option<Rc<RefCell<Box<dyn MascotBackend>>>>,
    event: MascotEvent,
) {
    let env_pos = Vec2::new(event.local.x as f64, event.local.y as f64);
    // 从后往前找：最晚加入的桌宠在视觉上层。
    let hit = state
        .sessions
        .iter()
        .rev()
        .find(|s| session_contains(s, env_pos))
        .map(|s| s.id);
    let Some(id) = hit else { return };
    match event.kind {
        MascotEventKind::LeftDown => {
            if let Some(session) = state.sessions.iter_mut().find(|s| s.id == id) {
                session.on_left_down(event.local, event.local);
            }
        }
        MascotEventKind::Move => {
            if let Some(session) = state.sessions.iter_mut().find(|s| s.id == id) {
                session.on_move(event.local);
            }
        }
        MascotEventKind::LeftUp => {
            let outcome = state
                .sessions
                .iter_mut()
                .find(|s| s.id == id)
                .map(|s| s.on_left_up(event.local));
            if let Some(GestureOutcome::Click(count)) = outcome.flatten() {
                maybe_show_click_bubble(state, id, count);
            }
        }
        MascotEventKind::LeftDoubleClick => {
            let name = state.sessions.iter().find(|s| s.id == id).and_then(|s| {
                let allows = s
                    .manager
                    .state
                    .borrow()
                    .env
                    .as_ref()
                    .map(|e| e.borrow().allows_breeding)
                    .unwrap_or(false);
                allows.then(|| s.name.clone())
            });
            if let Some(name) = name {
                let _ = spawn_default(state, backend, &name, "");
            }
        }
        MascotEventKind::RightUp => {
            open_context_menu(state, backend, id, event.screen);
        }
    }
}

fn session_contains(session: &Session, point: Vec2) -> bool {
    let state = session.manager.state.borrow();
    let frame = &state.active_frame;
    let anchor = state.anchor;
    let (w, h) = session.frame_size;
    let left = anchor.x - frame.anchor.x;
    let top = anchor.y - frame.anchor.y;
    point.x >= left && point.x <= left + w as f64 && point.y >= top && point.y <= top + h as f64
}

/// 推进一帧：环境覆盖、行为 tick、繁殖、音效、渲染。
fn run_tick(state: &mut RunState, backend: &Option<Rc<RefCell<Box<dyn MascotBackend>>>>) {
    let windowed = state.windowed;
    let locale = state.settings.locale();
    let _ = locale;

    // 迭代顺序与 C++ 一致：从后往前。
    let mut i = state.sessions.len();
    while i > 0 {
        i -= 1;
        let session = &mut state.sessions[i];
        if session.dead {
            continue;
        }
        if session.paused {
            continue;
        }

        session.maintain_hold();

        let env = session.manager.state.borrow().env.clone();
        let Some(env) = env else { continue };

        // 长下落的桌宠本帧绕过任务栏地板。
        let y_before = session.manager.state.borrow().anchor.y;
        let env_override = {
            let mut e = env.borrow_mut();
            session.fall_tracker.apply_env_override(&mut e)
        };
        let tick_result = session.manager.tick();
        if let Some(saved) = env_override {
            let mut e = env.borrow_mut();
            crate::fallthrough::FallThroughTracker::restore_env_override(&mut e, saved);
        }
        if let Err(err) = tick_result {
            eprintln!("mascot {}: tick failed: {err}", session.name);
            session.dead = true;
            continue;
        }

        // 下落进度观察。
        {
            let s = session.manager.state.borrow();
            session
                .fall_tracker
                .observe(s.on_land(), s.dragging, y_before, s.anchor.y);
        }
        session.fall_tracker.reset_if_dragged(session.dragging);

        // 引擎标记死亡。
        if session.manager.state.borrow().dead {
            session.dead = true;
            continue;
        }

        // 拖拽跨屏：切换会话环境。
        if session.dragging && !windowed {
            let anchor = session.manager.state.borrow().anchor;
            if let Some(new_env) = state.envs.env_at(anchor)
                && !Rc::ptr_eq(new_env, &env)
            {
                session.manager.state.borrow_mut().env = Some(new_env.clone());
            }
        }

        // 音效：active_sound_changed 驱动播放/停止。
        if let Some(sounds) = &mut session.sounds {
            let mut s = session.manager.state.borrow_mut();
            if s.active_sound_changed {
                sounds.stop();
                if !s.active_sound.is_empty() {
                    let name = s.active_sound.clone();
                    sounds.play(&name);
                }
            } else if !sounds.playing() {
                s.active_sound.clear();
            }
        }

        // 渲染。窗口化模式由沙盒合成，跳过单窗口绘制。
        if windowed {
            let _ = session.window.update_frame(&[0, 0, 0, 0], 1, 1, OFFSCREEN);
        } else {
            session.render();
        }
    }

    // 处理繁殖/变身请求（会话循环外执行，避免迭代中修改列表）。
    let mut idx = 0;
    while idx < state.sessions.len() {
        let available = state.sessions[idx]
            .manager
            .state
            .borrow()
            .breed_request
            .available;
        if available && let Err(err) = handle_breed_request(state, backend, idx) {
            eprintln!("breed failed: {err}");
        }
        idx += 1;
    }

    // 沙盒合成绘制。
    if state.sandbox.is_some() {
        render_sandbox(state);
    }

    // 移除死亡会话。
    let before = state.sessions.len();
    state.sessions.retain(|s| !s.dead);
    if state.sessions.len() != before {
        state
            .labels
            .retain(|_, id| state.sessions.iter().any(|s| s.id == *id));
    }

    // 各环境的桌宠计数。
    let count = state.sessions.len() as i64;
    for screen in &state.envs.screens {
        screen.env.borrow_mut().mascot_count = count;
    }
    if let Some(sandbox) = &state.envs.sandbox {
        sandbox.borrow_mut().mascot_count = count;
    }
    state.envs.reset_scales();

    // 气泡生命周期。
    let mut guard = backend.as_ref().map(|b| b.borrow_mut());
    let mut backend_opt = guard.as_deref_mut();
    bubbles::process_bubbles(&mut state.sessions, &mut backend_opt);
}

fn handle_breed_request(
    state: &mut RunState,
    backend: &Option<Rc<RefCell<Box<dyn MascotBackend>>>>,
    parent_index: usize,
) -> Result<(), String> {
    let (available, name_raw, behavior, anchor, looking_right, env) = {
        let session = &state.sessions[parent_index];
        let s = session.manager.state.borrow();
        let req = &s.breed_request;
        (
            req.available,
            req.name.clone(),
            req.behavior.clone(),
            req.anchor,
            req.looking_right,
            s.env.clone(),
        )
    };
    if !available {
        return Ok(());
    }

    // 名称缺省取父桌宠模板名；去掉路径分隔符防穿越。
    let parent_name = state.sessions[parent_index].name.clone();
    let mut name = if name_raw.is_empty() {
        parent_name.clone()
    } else {
        name_raw.clone()
    };
    if let Some(pos) = name.rfind(['/', '\\']) {
        name = name[pos + 1..].to_string();
    }
    if !state.templates.names_sorted().iter().any(|n| n == &name) {
        state.sessions[parent_index]
            .manager
            .state
            .borrow_mut()
            .breed_request
            .available = false;
        return Err(format!("unknown breed template: {name}"));
    }

    let env = env.ok_or("no environment")?;
    let init = neurolings_engine::mascot::Initializer::new(anchor, &behavior, looking_right);
    let product = state
        .factory
        .spawn(&name, init)
        .map_err(|e| e.to_string())?;
    product.manager.state.borrow_mut().env = Some(env.clone());

    let id = state.next_session_id;
    state.next_session_id += 1;
    let window: Box<dyn neurolings_platform::MascotWindow> = match backend {
        Some(b) => b
            .borrow_mut()
            .create_window(id)
            .map_err(|e| e.to_string())?,
        None => Box::new(crate::headless::HeadlessWindow),
    };
    let pack_dir = state.templates.pack_dir(&name).unwrap_or_default();
    let img_dir = pack_dir.join("img");
    let sound_dir = pack_dir.join("sound");
    let mut session = Session {
        id,
        data_id: state
            .templates
            .names_sorted()
            .iter()
            .position(|n| n == &name)
            .unwrap_or(0) as i64,
        name,
        label: None,
        manager: product.manager,
        window,
        img_dir,
        pack_dir,
        frames: HashMap::new(),
        dragging: false,
        fall_tracker: crate::fallthrough::FallThroughTracker::new(),
        gesture: interaction::Gesture::default(),
        sounds: crate::runtime::sounds::SoundPlayer::new(&sound_dir),
        paused: false,
        dead: false,
        window_top_left: Point::new(0, 0),
        frame_size: (0, 0),
        windowed: false,
        bubble_window: None,
        bubble_until: Instant::now(),
        pending_bubble: None,
        pending_codex_bubble: None,
        bubble_bitmap: None,
        bubble_size: (0, 0),
    };
    // 预渲染首帧；失败则丢弃子桌宠。
    {
        let needs_tick = {
            let s = session.manager.state.borrow();
            s.active_frame.get_name(s.looking_right).is_empty()
        };
        if needs_tick && session.manager.tick().is_err() {
            state.sessions[parent_index]
                .manager
                .state
                .borrow_mut()
                .breed_request
                .available = false;
            return Ok(());
        }
    }
    state.sessions.push(session);
    env.borrow_mut().mascot_count = state.sessions.len() as i64;
    state.sessions[parent_index]
        .manager
        .state
        .borrow_mut()
        .breed_request
        .available = false;
    Ok(())
}

/// 沙盒窗口的合成渲染：底色 + 各会话帧。
fn render_sandbox(state: &mut RunState) {
    let Some(Sandbox {
        window, top_left, ..
    }) = state.sandbox.as_mut()
    else {
        return;
    };
    let width = SANDBOX_WIDTH as u32;
    let height = SANDBOX_HEIGHT as u32;
    let mut canvas = vec![0u8; (width * height * 4) as usize];
    for pixel in canvas.chunks_exact_mut(4) {
        pixel.copy_from_slice(&SANDBOX_BG);
    }
    for session in state.sessions.iter_mut() {
        let (frame, looking_right, anchor) = {
            let st = session.manager.state.borrow();
            (st.active_frame.clone(), st.looking_right, st.anchor)
        };
        let name = frame.get_name(looking_right).to_lowercase();
        if name.is_empty() {
            continue;
        }
        let Some(bitmap) = session.sandbox_frame(&name) else {
            continue;
        };
        let mirrored = looking_right && frame.right_name.is_empty();
        let (anchor_x, anchor_y) = if mirrored {
            (bitmap.width as f64 - frame.anchor.x, frame.anchor.y)
        } else {
            (frame.anchor.x, frame.anchor.y)
        };
        let off_x = (anchor.x - anchor_x).round() as i64;
        let off_y = (anchor.y - anchor_y).round() as i64;
        let (bw, bh) = (bitmap.width, bitmap.height);
        let buffer = if mirrored {
            bitmap.mirrored.as_ref().unwrap_or(&bitmap.premul_bgra)
        } else {
            &bitmap.premul_bgra
        };
        for y in 0..bh as i64 {
            let dy = y + off_y;
            if dy < 0 || dy >= height as i64 {
                continue;
            }
            for x in 0..bw as i64 {
                let dx = x + off_x;
                if dx < 0 || dx >= width as i64 {
                    continue;
                }
                let si = ((y as u32 * bw + x as u32) * 4) as usize;
                let di = ((dy as u32 * width + dx as u32) * 4) as usize;
                let a = buffer[si + 3] as u32;
                if a == 0 {
                    continue;
                }
                let inv = 255 - a;
                canvas[di] = (buffer[si] as u32 + canvas[di] as u32 * inv / 255) as u8;
                canvas[di + 1] = (buffer[si + 1] as u32 + canvas[di + 1] as u32 * inv / 255) as u8;
                canvas[di + 2] = (buffer[si + 2] as u32 + canvas[di + 2] as u32 * inv / 255) as u8;
                canvas[di + 3] = 255;
            }
        }
    }
    let _ = window.update_frame(&canvas, width, height, *top_left);
}

/// 进入窗口化模式：创建沙盒窗口，把全部会话迁入沙盒环境。
fn enter_sandbox(state: &mut RunState, backend: &Option<Rc<RefCell<Box<dyn MascotBackend>>>>) {
    let Some(backend) = backend else { return };
    let window = match backend.borrow_mut().create_window(SANDBOX_WINDOW_ID) {
        Ok(w) => w,
        Err(_) => return,
    };
    // 沙盒窗口居中于主屏。
    let top_left = {
        let screens = backend.borrow().screens();
        match screens.first() {
            Some(screen) => Point::new(
                screen.monitor.left + (screen.monitor.width() - SANDBOX_WIDTH) / 2,
                screen.monitor.top + (screen.monitor.height() - SANDBOX_HEIGHT) / 2,
            ),
            None => Point::new(100, 100),
        }
    };
    let env = Rc::new(RefCell::new(Environment::default()));
    state.envs.sandbox = Some(env.clone());
    state.sandbox = Some(Sandbox { window, top_left });
    // 迁移会话环境并重置位置。
    for session in state.sessions.iter_mut() {
        session.manager.state.borrow_mut().env = Some(env.clone());
        session.manager.reset_position();
        session.windowed = true;
    }
}

/// 退出窗口化模式：销毁沙盒窗口，会话回到桌面环境。
fn exit_sandbox(state: &mut RunState) {
    state.sandbox = None;
    state.envs.sandbox = None;
    let env = state.envs.primary().cloned();
    for session in state.sessions.iter_mut() {
        session.windowed = false;
        if let Some(env) = &env {
            session.manager.state.borrow_mut().env = Some(env.clone());
            session.manager.reset_position();
        }
    }
}

/// 为所有环境注入窗口推移回调（ThrowIE 动作由此生效）。
fn install_push_callbacks(
    state: &mut RunState,
    backend: &Option<Rc<RefCell<Box<dyn MascotBackend>>>>,
) {
    let Some(backend_rc) = backend else { return };
    let target = state.envs.push_target;
    let mut envs: Vec<Rc<RefCell<Environment>>> =
        state.envs.screens.iter().map(|s| s.env.clone()).collect();
    if let Some(sandbox) = &state.envs.sandbox {
        envs.push(sandbox.clone());
    }
    for env in envs {
        let backend_rc = backend_rc.clone();
        env.borrow_mut().window_push_callback = Some(Box::new(move |dx, dy| {
            if target == 0 {
                return false;
            }
            backend_rc.borrow_mut().push_window(target, dx, dy)
        }));
    }
}

/// 显示器被移除时，把仍持有失效环境的会话迁回主屏并重置位置。
fn migrate_removed_screens(state: &mut RunState) {
    if state.windowed {
        return;
    }
    let Some(primary) = state.envs.primary().cloned() else {
        return;
    };
    for session in state.sessions.iter_mut() {
        let env = session.manager.state.borrow().env.clone();
        let stale = match &env {
            Some(current) => !state
                .envs
                .screens
                .iter()
                .any(|s| Rc::ptr_eq(&s.env, current)),
            None => true,
        };
        if stale {
            session.manager.state.borrow_mut().env = Some(primary.clone());
            session.manager.reset_position();
        }
    }
}
