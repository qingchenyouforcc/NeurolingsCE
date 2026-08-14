//! 每帧的环境更新：显示器几何、任务栏扣除、前台窗口交互区、
//! 光标位移、用户缩放与多屏环境集合。
//!
//! 每只桌宠绑定一个屏幕环境；窗口化（沙盒）模式下改绑沙盒环境。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use neurolings_engine::environment::{Area, DArea, Environment, HBorder};
use neurolings_engine::math::Vec2;
use neurolings_platform::{ActiveWindowInfo, MascotBackend, Point, Rect, ScreenInfo};

use crate::settings::Settings;

/// 活动窗口不可用时使用的屏外哑元区域。
const INACTIVE_IE: [f64; 4] = [-50.0, -50.0, -50.0, -50.0];

/// 单个屏幕及其引擎环境。
pub struct ScreenEnv {
    pub screen: ScreenInfo,
    pub env: Rc<RefCell<Environment>>,
}

/// 多屏环境集合。键为显示器物理矩形。
pub struct EnvironmentSet {
    pub screens: Vec<ScreenEnv>,
    /// 窗口化模式的沙盒环境。
    pub sandbox: Option<Rc<RefCell<Environment>>>,
    /// 当前前台窗口句柄（推窗回调目标）；0 表示无目标。
    pub push_target: u64,
}

impl EnvironmentSet {
    /// 依据当前显示器列表重建环境集合；已有环境按矩形复用（保留随机状态）。
    pub fn refresh(
        &mut self,
        backend: &mut dyn MascotBackend,
        settings: &Settings,
        windowed: bool,
        sandbox_size: (i32, i32),
        sandbox_origin: Option<Point>,
    ) {
        let cursor = backend.cursor_pos();
        let active = if windowed {
            None
        } else {
            backend.active_window()
        };
        let scale = 1.0 / settings.user_scale().sqrt();
        let detach = settings.detach_threshold();
        let allows_pushing = settings.get_bool(crate::settings::KEY_WINDOW_PUSHING, false)
            && backend.supports_window_pushing();

        self.push_target = active.map(|a| a.handle).unwrap_or(0);
        let latest = backend.screens();
        // 复用已有环境，新增显示器创建新环境，消失的显示器丢弃。
        let mut retained: HashMap<(i32, i32, i32, i32), Rc<RefCell<Environment>>> = self
            .screens
            .drain(..)
            .map(|s| {
                (
                    (
                        s.screen.monitor.left,
                        s.screen.monitor.top,
                        s.screen.monitor.right,
                        s.screen.monitor.bottom,
                    ),
                    s.env,
                )
            })
            .collect();

        self.screens = latest
            .into_iter()
            .map(|screen| {
                let key = (
                    screen.monitor.left,
                    screen.monitor.top,
                    screen.monitor.right,
                    screen.monitor.bottom,
                );
                let env = retained
                    .remove(&key)
                    .unwrap_or_else(|| Rc::new(RefCell::new(Environment::default())));
                Self::update_env(&env, &screen, cursor, active, scale, detach, allows_pushing);
                ScreenEnv { screen, env }
            })
            .collect();

        if windowed {
            let sandbox = self
                .sandbox
                .get_or_insert_with(|| Rc::new(RefCell::new(Environment::default())));
            let (w, h) = sandbox_size;
            let rect = ScreenInfo {
                monitor: Rect {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: h,
                },
                work_area: Rect {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: h,
                },
            };
            Self::update_env(sandbox, &rect, cursor, None, scale, detach, false);
            // 沙盒环境的光标使用窗口局部坐标。
            if let Some(origin) = sandbox_origin {
                let local_cursor = Vec2::new(
                    cursor.x as f64 - origin.x as f64,
                    cursor.y as f64 - origin.y as f64,
                );
                let old = sandbox.borrow().cursor.as_vec2();
                sandbox.borrow_mut().cursor = neurolings_engine::environment::DVec2::with_delta(
                    local_cursor.x,
                    local_cursor.y,
                    local_cursor.x - old.x,
                    local_cursor.y - old.y,
                );
            }
        } else {
            self.sandbox = None;
        }
    }

    /// 单个环境的逐帧几何与交互区更新。
    #[allow(clippy::too_many_arguments)]
    fn update_env(
        env: &Rc<RefCell<Environment>>,
        screen: &ScreenInfo,
        cursor: Point,
        active: Option<ActiveWindowInfo>,
        scale: f64,
        detach_threshold: f64,
        allows_pushing: bool,
    ) {
        let mut e = env.borrow_mut();

        // 任务栏高度由显示器矩形与工作区的差值推得（仅处理上/下边缘）。
        let mut taskbar_height = screen.monitor.bottom - screen.work_area.bottom;
        let mut status_bar_height = screen.work_area.top - screen.monitor.top;
        if taskbar_height < 0 {
            taskbar_height = 0;
        }
        if status_bar_height < 0 {
            status_bar_height = 0;
        }

        e.screen = Area::new(
            screen.monitor.top as f64 + status_bar_height as f64,
            screen.monitor.right as f64,
            screen.monitor.bottom as f64,
            screen.monitor.left as f64,
        );
        // 地板扣除任务栏，天花板与工作区仍按完整显示器几何计算。
        e.floor = HBorder::new(
            (screen.monitor.bottom - taskbar_height) as f64,
            screen.monitor.left as f64,
            screen.monitor.right as f64,
        );
        e.work_area = Area::new(
            screen.monitor.top as f64,
            screen.monitor.right as f64,
            (screen.monitor.bottom - taskbar_height) as f64,
            screen.monitor.left as f64,
        );
        e.ceiling = HBorder::new(
            screen.monitor.top as f64,
            screen.monitor.left as f64,
            screen.monitor.right as f64,
        );

        // 前台窗口交互区：位置明显有效才启用，否则放到屏外。
        let valid = active.is_some_and(|a| a.area.left.abs() > 1 && a.area.top.abs() > 1);
        match active {
            Some(info) if valid => {
                let area = info.area;
                let prev = e.active_ie;
                e.active_ie = DArea::new(
                    area.top as f64,
                    area.right as f64,
                    area.bottom as f64,
                    area.left as f64,
                    0.0,
                    0.0,
                );
                // 同一窗口的边位移：区分四条边各自的位移量。
                if prev.visible() {
                    e.active_ie.set_edge_offsets(
                        area.left as f64 - prev.area.left,
                        area.right as f64 - prev.area.right,
                        area.top as f64 - prev.area.top,
                        area.bottom as f64 - prev.area.bottom,
                    );
                    if detach_threshold > 0.0 {
                        let speed = (e.active_ie.dx * e.active_ie.dx
                            + e.active_ie.dy * e.active_ie.dy)
                            .sqrt();
                        let upper = detach_threshold * 3.0;
                        if speed >= upper {
                            // 过快移动视为窗口已消失，避免拖出巨大的交互区。
                            e.active_ie = DArea::new(
                                INACTIVE_IE[0],
                                INACTIVE_IE[1],
                                INACTIVE_IE[2],
                                INACTIVE_IE[3],
                                0.0,
                                0.0,
                            );
                        } else if speed > detach_threshold {
                            // 中速移动按比例衰减交互区的影响。
                            let ratio =
                                1.0 - (speed - detach_threshold) / (upper - detach_threshold);
                            e.active_ie.dx *= ratio;
                            e.active_ie.dy *= ratio;
                            e.active_ie.left_dx *= ratio;
                            e.active_ie.right_dx *= ratio;
                            e.active_ie.top_dy *= ratio;
                            e.active_ie.bottom_dy *= ratio;
                        }
                    }
                }
            }
            _ => {
                e.active_ie = DArea::new(
                    INACTIVE_IE[0],
                    INACTIVE_IE[1],
                    INACTIVE_IE[2],
                    INACTIVE_IE[3],
                    0.0,
                    0.0,
                );
            }
        }

        // 光标按帧增量更新（拖拽抛出等动作依赖每帧位移）。
        let old = e.cursor.as_vec2();
        let now = Vec2::new(cursor.x as f64, cursor.y as f64);
        e.cursor = neurolings_engine::environment::DVec2::with_delta(
            now.x,
            now.y,
            now.x - old.x,
            now.y - old.y,
        );

        e.subtick_count = crate::runtime::session::SUBTICK_COUNT;
        e.allows_window_pushing = allows_pushing;
        // 用户缩放：引擎把大数值视为更小的物理单位，故取倒数开方。
        e.set_scale(scale);
    }

    /// 查找锚点所在屏幕的环境。
    pub fn env_at(&self, point: Vec2) -> Option<&Rc<RefCell<Environment>>> {
        self.screens
            .iter()
            .find(|s| {
                s.screen
                    .monitor
                    .contains(Point::new(point.x as i32, point.y as i32))
            })
            .map(|s| &s.env)
    }

    /// 主屏幕环境。
    pub fn primary(&self) -> Option<&Rc<RefCell<Environment>>> {
        self.screens.first().map(|s| &s.env)
    }

    /// 每帧结束后撤销缩放，避免几何被重复放大。
    pub fn reset_scales(&self) {
        for screen in &self.screens {
            screen.env.borrow_mut().reset_scale();
        }
        if let Some(sandbox) = &self.sandbox {
            sandbox.borrow_mut().reset_scale();
        }
    }
}
