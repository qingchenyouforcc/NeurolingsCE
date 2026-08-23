//! 桌宠的鼠标交互：点按 / 长按 / 拖拽手势识别、双击召唤、右键菜单。
//!
//! 手势规则：
//! - 按下落在 hotspot 上时不立即拖拽，进入长按候选；长按 260ms 内不移动
//!   超过 12px 则触发"摸头"行为并保持循环，直到松开或移出容差；
//! - 松开时全程未超过 6px 且时长不超过 400ms 视为单击：命中 hotspot 则
//!   切换行为，否则累计点击计数，达到阈值弹出随机语气泡；
//! - 双击召唤一只同款桌宠（需允许繁殖）。

use std::time::{Duration, Instant};

use neurolings_engine::mascot::Manager;
use neurolings_engine::math::Vec2;
use neurolings_platform::{MenuEntry, Point};

use crate::runtime::session::Session;
use crate::settings::Locale;

/// 长按触发时长。
pub const HOLD_TRIGGER: Duration = Duration::from_millis(260);
/// 长按期间的位移容差（曼哈顿距离，像素）。
pub const HOLD_MOVE_TOLERANCE: i32 = 12;
/// 单击判定的位移容差（曼哈顿距离，像素）。
pub const CLICK_MOVE_TOLERANCE: i32 = 6;
/// 单击判定的最大时长。
pub const CLICK_MAX_DURATION: Duration = Duration::from_millis(400);
/// 无操作后重置点击计数的间隔。
pub const CLICK_RESET: Duration = Duration::from_millis(500);

/// 右键菜单项的命令编号。
pub mod menu_id {
    pub const BEHAVIOR_BASE: u32 = 1000;
    pub const PAUSE: u32 = 1;
    pub const CALL_ANOTHER: u32 = 2;
    pub const SHOW_MANAGER: u32 = 3;
    pub const INSPECT: u32 = 4;
    pub const DISMISS_OTHERS: u32 = 5;
    pub const DISMISS_ALL: u32 = 6;
    pub const DISMISS: u32 = 7;
}

/// 菜单动作（由主循环执行）。
#[derive(Debug, Clone, PartialEq)]
pub enum MenuAction {
    PauseToggle(u64),
    CallAnother(u64),
    ShowManager,
    Inspect(u64),
    DismissOthers(u64),
    DismissAll,
    Dismiss(u64),
    Behavior(u64, String),
}

/// 一次按住手势的全部状态。
#[derive(Default)]
pub struct Gesture {
    press_active: bool,
    press_screen_pos: Option<Point>,
    press_max_movement: i32,
    press_started: Option<Instant>,
    hotspot_behavior: String,
    hotspot_triggered: bool,
    hotspot_preferred: bool,
    click_count: u32,
    click_deadline: Option<Instant>,
}

fn manhattan(a: Point, b: Point) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

impl Session {
    /// 鼠标按下：命中 hotspot 进入长按候选，否则立即开始拖拽。
    pub fn on_left_down(&mut self, screen: Point, _local: Point) {
        self.gesture.cancel(&mut self.manager);
        self.gesture.press_active = true;
        self.gesture.press_screen_pos = Some(screen);
        self.gesture.press_max_movement = 0;
        self.gesture.press_started = Some(Instant::now());
        let cursor = Vec2::new(screen.x as f64, screen.y as f64);
        self.gesture.hotspot_behavior = self.manager.hotspot_behavior(cursor);
        self.gesture.hotspot_triggered = false;
        self.gesture.hotspot_preferred = false;
        // hotspot 候选在长按阈值前不走引擎的拖拽路径，否则短按会立刻触发。
        let dragging = self.gesture.hotspot_behavior.is_empty();
        self.dragging = dragging;
        let mut s = self.manager.state.borrow_mut();
        s.dragging = dragging;
        if dragging {
            self.fall_tracker.reset_if_dragged(true);
        }
    }

    pub fn on_move(&mut self, screen: Point) {
        if !self.gesture.press_active {
            return;
        }
        let Some(press_pos) = self.gesture.press_screen_pos else {
            return;
        };
        let distance = manhattan(screen, press_pos);
        self.gesture.press_max_movement = self.gesture.press_max_movement.max(distance);
        if distance > HOLD_MOVE_TOLERANCE {
            self.gesture.stop_hold(&mut self.manager);
            self.dragging = true;
            let mut s = self.manager.state.borrow_mut();
            if !s.dragging {
                s.dragging = true;
                self.fall_tracker.reset_if_dragged(true);
            }
        }
    }

    /// 鼠标松开：区分单击与拖拽结束，返回是否需要弹气泡。
    pub fn on_left_up(&mut self, screen: Point) -> Option<GestureOutcome> {
        if !self.gesture.press_active {
            return None;
        }
        let press_pos = self.gesture.press_screen_pos.unwrap_or(screen);
        let distance = manhattan(screen, press_pos);
        self.gesture.press_max_movement = self.gesture.press_max_movement.max(distance);
        let elapsed = self
            .gesture
            .press_started
            .map(|t| t.elapsed())
            .unwrap_or_default();
        let hold_triggered = self.gesture.stop_hold(&mut self.manager);
        self.gesture.press_active = false;
        self.dragging = false;
        self.manager.state.borrow_mut().dragging = false;

        if hold_triggered {
            return None;
        }
        let is_click = elapsed <= CLICK_MAX_DURATION
            && self.gesture.press_max_movement <= CLICK_MOVE_TOLERANCE;
        if !is_click {
            return None;
        }
        // 单击：优先 hotspot 行为，否则累计点击计数。
        let cursor = Vec2::new(screen.x as f64, screen.y as f64);
        if self.manager.trigger_hotspot(cursor) {
            return None;
        }
        let now = Instant::now();
        if self.gesture.click_deadline.is_none_or(|d| now > d) {
            self.gesture.click_count = 0;
        }
        self.gesture.click_count += 1;
        self.gesture.click_deadline = Some(now + CLICK_RESET);
        Some(GestureOutcome::Click(self.gesture.click_count))
    }

    /// 每帧维护长按状态：到阈值后触发/保持"摸头"循环。
    pub fn maintain_hold(&mut self) {
        if !self.gesture.press_active || self.gesture.hotspot_behavior.is_empty() {
            return;
        }
        let Some(started) = self.gesture.press_started else {
            return;
        };
        if started.elapsed() < HOLD_TRIGGER || self.gesture.press_max_movement > HOLD_MOVE_TOLERANCE
        {
            return;
        }
        let behavior = self.gesture.hotspot_behavior.clone();
        let active = self.manager.active_behavior();
        let queued = self.manager.state.borrow().queued_behavior.clone();
        if active.as_ref().is_some_and(|b| b.name == behavior) {
            // 当前摸头动作播完自动再来一轮；松开或取消时清除该偏好。
            self.manager.prefer_next_behavior(&behavior);
            self.gesture.hotspot_preferred = true;
        } else if queued != behavior {
            // 动作切换中只排一次队，避免每帧重复请求。
            self.manager.next_behavior(&behavior);
        }
        self.gesture.hotspot_triggered = true;
    }
}

impl Gesture {
    /// 取消按住状态；返回长按是否已触发过。
    fn stop_hold(&mut self, manager: &mut Manager) -> bool {
        let triggered = self.hotspot_triggered;
        let behavior = std::mem::take(&mut self.hotspot_behavior);
        if !behavior.is_empty() {
            // 松开可能发生在下一帧之前：只撤销本次长按排队的请求。
            {
                let mut s = manager.state.borrow_mut();
                if s.queued_behavior == behavior {
                    s.queued_behavior.clear();
                }
            }
            if self.hotspot_preferred {
                // 恢复活动行为自身的 Add/NextBehaviorList 规则。
                manager.clear_preferred_next_behavior();
            }
        }
        self.hotspot_triggered = false;
        self.hotspot_preferred = false;
        triggered
    }

    /// 彻底取消交互（窗口失焦等场景）。
    pub fn cancel(&mut self, manager: &mut Manager) {
        self.stop_hold(manager);
        self.press_active = false;
        manager.state.borrow_mut().dragging = false;
    }
}

/// 单击结果：携带累计点击次数。
pub enum GestureOutcome {
    Click(u32),
}

/// 构建右键菜单。行为子菜单列出模板全部非隐藏行为。
pub fn build_context_menu(session: &Session, locale: Locale) -> (Vec<MenuEntry>, Vec<String>) {
    let labels = MenuLabels::for_locale(locale);
    let mut behavior_names = Vec::new();
    let mut behavior_entries = Vec::new();
    for behavior in session
        .manager
        .initial_behavior_list()
        .flatten_unconditional()
    {
        if behavior.hidden {
            continue;
        }
        let id = menu_id::BEHAVIOR_BASE + behavior_names.len() as u32;
        behavior_entries.push(MenuEntry::Item {
            id,
            label: behavior.name.clone(),
            checked: false,
        });
        behavior_names.push(behavior.name.clone());
    }

    let entries = vec![
        MenuEntry::Submenu {
            label: format!("\u{1F3AD} {}", labels.behaviors),
            entries: behavior_entries,
        },
        MenuEntry::Separator,
        MenuEntry::Item {
            id: menu_id::PAUSE,
            label: format!("\u{23F8} {}", labels.pause),
            checked: session.paused,
        },
        MenuEntry::Item {
            id: menu_id::CALL_ANOTHER,
            label: format!("\u{2728} {}", labels.call_another),
            checked: false,
        },
        MenuEntry::Separator,
        MenuEntry::Item {
            id: menu_id::SHOW_MANAGER,
            label: format!("\u{1F4CB} {}", labels.show_manager),
            checked: false,
        },
        MenuEntry::Item {
            id: menu_id::INSPECT,
            label: format!("\u{1F50D} {}", labels.inspect),
            checked: false,
        },
        MenuEntry::Separator,
        MenuEntry::Item {
            id: menu_id::DISMISS_OTHERS,
            label: labels.dismiss_others.to_string(),
            checked: false,
        },
        MenuEntry::Item {
            id: menu_id::DISMISS_ALL,
            label: labels.dismiss_all.to_string(),
            checked: false,
        },
        MenuEntry::Item {
            id: menu_id::DISMISS,
            label: format!("\u{00D7} {}", labels.dismiss),
            checked: false,
        },
    ];
    (entries, behavior_names)
}

/// 菜单选择映射为动作。
pub fn menu_action(choice: u32, session_id: u64, behavior_names: &[String]) -> Option<MenuAction> {
    match choice {
        menu_id::PAUSE => Some(MenuAction::PauseToggle(session_id)),
        menu_id::CALL_ANOTHER => Some(MenuAction::CallAnother(session_id)),
        menu_id::SHOW_MANAGER => Some(MenuAction::ShowManager),
        menu_id::INSPECT => Some(MenuAction::Inspect(session_id)),
        menu_id::DISMISS_OTHERS => Some(MenuAction::DismissOthers(session_id)),
        menu_id::DISMISS_ALL => Some(MenuAction::DismissAll),
        menu_id::DISMISS => Some(MenuAction::Dismiss(session_id)),
        id if id >= menu_id::BEHAVIOR_BASE => behavior_names
            .get((id - menu_id::BEHAVIOR_BASE) as usize)
            .map(|name| MenuAction::Behavior(session_id, name.clone())),
        _ => None,
    }
}

struct MenuLabels {
    behaviors: &'static str,
    pause: &'static str,
    call_another: &'static str,
    show_manager: &'static str,
    inspect: &'static str,
    dismiss_others: &'static str,
    dismiss_all: &'static str,
    dismiss: &'static str,
}

impl MenuLabels {
    fn for_locale(locale: Locale) -> Self {
        match locale {
            Locale::En => Self {
                behaviors: "Behaviors",
                pause: "Pause",
                call_another: "Call another",
                show_manager: "Show manager",
                inspect: "Inspect",
                dismiss_others: "Dismiss all but one",
                dismiss_all: "Dismiss all",
                dismiss: "Dismiss",
            },
            Locale::ZhCn => Self {
                // 译文与原版 translations/shijima-qt_zh_CN.ts 逐条一致。
                behaviors: "行为",
                pause: "暂停",
                call_another: "召唤同伴",
                show_manager: "显示管理器",
                inspect: "检查",
                dismiss_others: "只保留一个",
                dismiss_all: "全部清除",
                dismiss: "关闭",
            },
        }
    }
}
