//! 动画驱动动作的共用机制：帧序列选择、边界检查、拖拽手势、hotspot 与抛窗。
//!
//! stay/move/fall/jump/breed 等绝大多数动作都建立在此基座之上。

use std::rc::Rc;

use crate::animation::Animation;
use crate::environment::Border;
use crate::error::{EngineError, Result};
use crate::math::Vec2;
use crate::pose::Pose;
use crate::tick::Tick;

use super::{Action, ActionBase};

pub struct AnimationBase {
    pub base: ActionBase,
    pub animations: Vec<Rc<Animation>>,
    pub has_fixed_velocity: bool,
    pub fixed_velocity: Vec2,
    pub current_anim_time: i64,
    pub current_anim: Option<Rc<Animation>>,
    pub anim_idx: i32,
    pub window_push_requested: bool,
}

impl Default for AnimationBase {
    fn default() -> Self {
        Self {
            base: ActionBase::default(),
            animations: Vec::new(),
            has_fixed_velocity: false,
            fixed_velocity: Vec2::ZERO,
            current_anim_time: -1,
            current_anim: None,
            anim_idx: -1,
            window_push_requested: false,
        }
    }
}

impl AnimationBase {
    pub fn is_window_push_action(&self) -> bool {
        if let Some(cls) = self.base.init_attr.get("Class")
            && cls.contains("ThrowIE")
        {
            return true;
        }
        matches!(
            self.base.init_attr.get("Name").map(String::as_str),
            Some("ThrowIE" | "ThrowIe")
        )
    }

    /// 选取第一个条件成立的动画序列；选取变化时重置已播放时间。
    pub fn get_animation(&mut self) -> Result<Rc<Animation>> {
        let mascot = self
            .base
            .mascot
            .clone()
            .ok_or_else(|| EngineError::Logic("animation action without mascot".into()))?;
        let time = mascot.borrow().time;
        if self.current_anim_time == time
            && let Some(anim) = &self.current_anim
        {
            return Ok(anim.clone());
        }
        for (i, anim) in self.animations.iter().enumerate() {
            let cond = anim.condition.clone();
            if self.base.vars.eval_condition(&cond) {
                if self.anim_idx != i as i32 {
                    self.anim_idx = i as i32;
                    self.base.reset_elapsed();
                }
                self.current_anim_time = time;
                self.current_anim = Some(anim.clone());
                return Ok(anim.clone());
            }
        }
        Err(EngineError::NoAnimationAvailable)
    }

    pub fn get_pose(&mut self) -> Result<Pose> {
        let elapsed = self.base.elapsed() as i32;
        let anim = self.get_animation()?;
        Ok(anim.get_pose(elapsed).clone())
    }

    pub fn get_velocity(&mut self) -> Result<Vec2> {
        if self.has_fixed_velocity {
            Ok(self.fixed_velocity)
        } else {
            Ok(self.get_pose()?.velocity)
        }
    }

    pub fn animation_finished(&mut self) -> Result<bool> {
        let anim = self.get_animation()?;
        Ok(self.base.elapsed() >= anim.duration as i64)
    }

    /// 检查锚点是否仍贴附于 BorderType 指定的边界，脱离则排队 Fall。
    pub fn check_border_type_impl(&mut self) -> Result<bool> {
        let border_type = self.base.vars.get_string("BorderType", "");
        let mascot = self.base.mascot.clone().expect("mascot set at init");
        let on_border = {
            let m = mascot.borrow();
            let env = m.env.clone().expect("env set before tick");
            let env = env.borrow();
            let anchor = m.anchor;
            match border_type.as_str() {
                "Floor" => env.floor.is_on(anchor) || env.active_ie.top_border().is_on(anchor),
                "Wall" => {
                    env.work_area.left_border().is_on(anchor)
                        || env.work_area.right_border().is_on(anchor)
                        || env.active_ie.left_border().is_on(anchor)
                        || env.active_ie.right_border().is_on(anchor)
                }
                "Ceiling" => {
                    env.work_area.top_border().is_on(anchor)
                        || env.active_ie.bottom_border().is_on(anchor)
                }
                "" => true,
                other => {
                    return Err(EngineError::Logic(format!("Unknown border: {other}")));
                }
            }
        };
        if !on_border {
            mascot.borrow_mut().queued_behavior = "Fall".to_string();
            return Ok(false);
        }
        Ok(true)
    }

    /// 处理按下状态：命中 hotspot 则重启动画或切换行为，否则进入拖拽。
    pub fn handle_dragging_impl(&mut self) -> Result<bool> {
        let mascot = self.base.mascot.clone().expect("mascot set at init");
        if !mascot.borrow().dragging {
            return Ok(true);
        }
        let draggable = self.base.vars.get_bool("Draggable", true);
        let (cursor, topleft, allows_hotspots) = {
            let m = mascot.borrow();
            let cursor = m.get_cursor().as_vec2();
            let topleft = if m.looking_right {
                // 注意：镜像时按 128px 标准帧宽反推左上角。
                m.anchor - Vec2::new(128.0 - m.active_frame.anchor.x, m.active_frame.anchor.y)
            } else {
                m.anchor - m.active_frame.anchor
            };
            let allows = m
                .env
                .as_ref()
                .map(|e| e.borrow().allows_hotspots)
                .unwrap_or(false);
            (cursor, topleft, allows)
        };
        let cursor_rel = cursor - topleft;
        let anim = self.get_animation()?;
        let hotspot = if allows_hotspots {
            anim.hotspot_at(cursor_rel).cloned()
        } else {
            None
        };
        if let Some(hotspot) = hotspot {
            if hotspot.behavior.is_empty() {
                // 空行为的 hotspot：重新播放动画。
                self.base.reset_elapsed();
            } else {
                // 切换到 hotspot 指定的行为。
                {
                    let mut m = mascot.borrow_mut();
                    m.queued_behavior = hotspot.behavior.clone();
                    m.dragging = false;
                }
                return Ok(false);
            }
        } else if draggable {
            // 进入拖拽状态。
            {
                let mut m = mascot.borrow_mut();
                m.was_on_ie = false;
                m.interaction.finalize();
                m.queued_behavior = "Dragged".to_string();
            }
            return Ok(false);
        } else {
            mascot.borrow_mut().dragging = false;
        }
        Ok(true)
    }

    /// 查询光标位置命中的 hotspot 对应的行为名；未命中返回空串。
    pub fn hotspot_behavior_at_impl(&mut self, cursor: Vec2) -> String {
        let Some(mascot) = self.base.mascot.clone() else {
            return String::new();
        };
        let (topleft, allows_hotspots) = {
            let m = mascot.borrow();
            let topleft = if m.looking_right {
                m.anchor - Vec2::new(128.0 - m.active_frame.anchor.x, m.active_frame.anchor.y)
            } else {
                m.anchor - m.active_frame.anchor
            };
            let allows = m
                .env
                .as_ref()
                .map(|e| e.borrow().allows_hotspots)
                .unwrap_or(false);
            (topleft, allows)
        };
        if !allows_hotspots {
            return String::new();
        }
        let Ok(anim) = self.get_animation() else {
            return String::new();
        };
        anim.hotspot_at(cursor - topleft)
            .filter(|h| h.valid())
            .map(|h| h.behavior.clone())
            .unwrap_or_default()
    }

    /// 初始化：复位动画选择状态并解析 FixedVelocity。
    pub fn init_impl(&mut self, ctx: &mut Tick) -> Result<()> {
        let (rv, rb) = (true, true);
        self.base.init_impl(ctx, rv, rb)?;
        self.anim_idx = -1;
        self.current_anim_time = -1;
        self.window_push_requested = false;
        if self.base.vars.has("FixedVelocity") {
            self.has_fixed_velocity = true;
            let s = self.base.vars.get_string("FixedVelocity", "");
            self.fixed_velocity = Vec2::from_str_lenient(&s);
        } else {
            self.has_fixed_velocity = false;
        }
        Ok(())
    }

    /// 收尾：清空当前动画缓存并释放基座资源。
    pub fn finalize_impl(&mut self) -> Result<()> {
        self.current_anim = None;
        self.base.finalize_impl(true)
    }

    /// 抛窗动作（ThrowIE）的处理：目标窗口失效时立即切换 Fall。
    pub fn handle_window_push(&mut self) -> Result<bool> {
        let mascot = self.base.mascot.clone().expect("mascot set at init");
        let anchor = mascot.borrow().anchor;
        let env = mascot.borrow().env.clone().expect("env set");
        let (allows, visible) = {
            let e = env.borrow();
            (e.allows_window_pushing, e.active_ie.visible())
        };
        if !allows || !visible {
            mascot.borrow_mut().queued_behavior = "Fall".to_string();
            return Ok(false);
        }
        if !self.window_push_requested {
            self.window_push_requested = true;
            let on_ie = env.borrow().active_ie.is_on(anchor);
            if on_ie {
                let initial_vx = self.base.vars.get_num("InitialVX", 0.0);
                let initial_vy = self.base.vars.get_num("InitialVY", 0.0);
                if initial_vx.is_finite() && initial_vy.is_finite() {
                    let mut push_dx = 0.0;
                    let mut push_dy = 0.0;
                    {
                        let e = env.borrow();
                        if e.active_ie.left_border().is_on(anchor) {
                            push_dx = initial_vx.abs();
                        } else if e.active_ie.right_border().is_on(anchor) {
                            push_dx = -initial_vx.abs();
                        } else if e.active_ie.top_border().is_on(anchor) {
                            push_dy = initial_vy.abs();
                        } else if e.active_ie.bottom_border().is_on(anchor) {
                            push_dy = -initial_vy.abs();
                        }
                    }
                    if push_dx != 0.0 || push_dy != 0.0 {
                        env.borrow_mut().request_window_push(push_dx, push_dy);
                    }
                }
            }
        }
        Ok(true)
    }

    /// 动画驱动动作的帧推进。边界与拖拽检查由具体动作提供，
    /// 以便 fall/dragged 等子类覆盖。
    pub fn tick_impl<C, D>(&mut self, check_border_type: C, handle_dragging: D) -> Result<bool>
    where
        C: FnOnce(&mut Self) -> Result<bool>,
        D: FnOnce(&mut Self) -> Result<bool>,
    {
        if !self.base.tick_impl(true)? {
            return Ok(false);
        }
        if !check_border_type(self)? {
            return Ok(false);
        }
        if !handle_dragging(self)? {
            return Ok(false);
        }
        if self.is_window_push_action() && !self.handle_window_push()? {
            return Ok(false);
        }
        let velocity = self.get_velocity()?;
        let dx = self.base.dx(velocity.x);
        let dy = self.base.dy(velocity.y);
        let mascot = self.base.mascot.clone().expect("mascot set at init");
        {
            let mut m = mascot.borrow_mut();
            m.anchor.x += dx;
            m.anchor.y += dy;
        }
        // 姿态在锚点更新后再取：动画选择条件可能引用 mascot.anchor。
        let pose = self.get_pose()?;
        mascot.borrow_mut().active_frame = pose.frame;
        Ok(true)
    }
}

/// 动画驱动动作的公共接口。具体动作内嵌 AnimationBase，
/// 并可覆盖边界/拖拽检查。
pub trait AnimationAction: Action {
    fn anim(&self) -> &AnimationBase;
    fn anim_mut(&mut self) -> &mut AnimationBase;

    fn check_border_type(&mut self) -> Result<bool> {
        self.anim_mut().check_border_type_impl()
    }

    fn handle_dragging(&mut self) -> Result<bool> {
        self.anim_mut().handle_dragging_impl()
    }

    fn hotspot_behavior_at(&mut self, cursor: Vec2) -> String {
        self.anim_mut().hotspot_behavior_at_impl(cursor)
    }

    fn get_animation(&mut self) -> Result<Rc<Animation>> {
        self.anim_mut().get_animation()
    }

    fn animation_finished(&mut self) -> Result<bool> {
        self.anim_mut().animation_finished()
    }
}
