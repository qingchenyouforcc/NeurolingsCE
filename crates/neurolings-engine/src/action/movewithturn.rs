//! 带转身动画的移动：需要反向时先播放转身动画再继续移动。

use std::collections::HashMap;
use std::rc::Rc;

use crate::animation::Animation;
use crate::environment::Border;
use crate::error::{EngineError, Result};
use crate::math::Vec2;
use crate::pose::Pose;
use crate::tick::Tick;

use super::animation::{AnimationAction, AnimationBase};
use super::{Action, ActionBase};

/// 判断从 start 到 end 的移动是否越过了 v（含方向无关的双向判定）。
fn passed(start: f64, end: f64, v: f64) -> bool {
    (start >= v && end <= v) || (start <= v && end >= v)
}

#[derive(Default)]
pub struct MoveWithTurn {
    anim: AnimationBase,
    is_turning: bool,
}

impl MoveWithTurn {
    fn headed_right(&self) -> bool {
        let target_x = self.anim.base.vars.get_num("TargetX", 0.0);
        let anchor_x = self
            .anim
            .base
            .mascot
            .as_ref()
            .map(|m| m.borrow().anchor.x)
            .unwrap_or(0.0);
        anchor_x <= target_x
    }

    fn needs_turn(&self) -> bool {
        let looking_right = self
            .anim
            .base
            .mascot
            .as_ref()
            .is_some_and(|m| m.borrow().looking_right);
        (looking_right && !self.headed_right()) || (!looking_right && self.headed_right())
    }

    /// 取速度，动画选择走本动作的转身逻辑。
    fn velocity(&mut self) -> Result<Vec2> {
        if self.anim.has_fixed_velocity {
            Ok(self.anim.fixed_velocity)
        } else {
            Ok(self.pose()?.velocity)
        }
    }

    /// 取当前姿态，动画选择走本动作的转身逻辑。
    fn pose(&mut self) -> Result<Pose> {
        let elapsed = self.anim.base.elapsed() as i32;
        let anim = <Self as AnimationAction>::get_animation(self)?;
        Ok(anim.get_pose(elapsed).clone())
    }
}

impl Action for MoveWithTurn {
    fn base(&self) -> &ActionBase {
        &self.anim.base
    }
    fn base_mut(&mut self) -> &mut ActionBase {
        &mut self.anim.base
    }
    fn requests_broadcast(&self) -> bool {
        true
    }
    fn init(&mut self, ctx: &mut Tick) -> Result<()> {
        // 动画数不为 2 时退化为普通移动，不视为错误。
        self.anim.init_impl(ctx)
    }
    fn tick(&mut self) -> Result<bool> {
        // 直线移动流程，但动画选择分派到本动作的转身版本。
        let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
        if self.anim.base.vars.has("TargetX") {
            let x = self.anim.base.vars.get_num("TargetX", 0.0);
            self.anim
                .base
                .vars
                .add_attr_nums(&HashMap::from([("目的地X".to_string(), x)]));
            let velocity = self.velocity()?;
            let anchor_x = mascot.borrow().anchor.x;
            if velocity.x > 0.0 {
                mascot.borrow_mut().looking_right = x < anchor_x;
            } else if velocity.x < 0.0 {
                mascot.borrow_mut().looking_right = x > anchor_x;
            }
        }
        if self.anim.base.vars.has("TargetY") {
            let y = self.anim.base.vars.get_num("TargetY", 0.0);
            self.anim
                .base
                .vars
                .add_attr_nums(&HashMap::from([("目的地Y".to_string(), y)]));
            let (anchor, env) = {
                let m = mascot.borrow();
                (m.anchor, m.env.clone().expect("env set before tick"))
            };
            let env = env.borrow();
            if env.work_area.left_border().is_on(anchor)
                || env.active_ie.right_border().is_on(anchor)
            {
                mascot.borrow_mut().looking_right = false;
            }
            if env.work_area.right_border().is_on(anchor)
                || env.active_ie.left_border().is_on(anchor)
            {
                mascot.borrow_mut().looking_right = true;
            }
        }

        let start = mascot.borrow().anchor;
        // 动画帧推进（手写以便接入转身动画选择）。
        if !self.anim.base.tick_impl(true)? {
            return Ok(false);
        }
        if !self.anim.check_border_type_impl()? {
            return Ok(false);
        }
        if !self.anim.handle_dragging_impl()? {
            return Ok(false);
        }
        if self.anim.is_window_push_action() && !self.anim.handle_window_push()? {
            return Ok(false);
        }
        let velocity = self.velocity()?;
        let dx = self.anim.base.dx(velocity.x);
        let dy = self.anim.base.dy(velocity.y);
        {
            let mut m = mascot.borrow_mut();
            m.anchor.x += dx;
            m.anchor.y += dy;
        }
        // 姿态在锚点更新后再取：动画选择条件可能引用 mascot.anchor。
        let pose = self.pose()?;
        mascot.borrow_mut().active_frame = pose.frame;
        let end = mascot.borrow().anchor;

        if self.anim.base.vars.has("TargetX") {
            let x = self.anim.base.vars.get_num("TargetX", 0.0);
            if passed(start.x, end.x, x) {
                mascot.borrow_mut().anchor.x = x;
                return Ok(false);
            }
        } else if self.anim.base.vars.has("TargetY") {
            let y = self.anim.base.vars.get_num("TargetY", 0.0);
            // 行为基准：越过判定恒用水平坐标（见 movement.rs 的说明）。
            if passed(start.x, end.x, y) {
                mascot.borrow_mut().anchor.y = y;
                return Ok(false);
            }
        } else {
            // 两个目标都未定义：动作无法执行，直接结束
            return Ok(false);
        }
        Ok(true)
    }
    fn finalize(&mut self) -> Result<()> {
        self.anim.finalize_impl()
    }
    fn hotspot_probe(&mut self, cursor: Vec2) -> String {
        self.anim.hotspot_behavior_at_impl(cursor)
    }
}

impl AnimationAction for MoveWithTurn {
    fn anim(&self) -> &AnimationBase {
        &self.anim
    }
    fn anim_mut(&mut self) -> &mut AnimationBase {
        &mut self.anim
    }
    fn get_animation(&mut self) -> Result<Rc<Animation>> {
        if self.anim.animations.len() < 2 {
            return self.anim.get_animation();
        }
        let mascot = self
            .anim
            .base
            .mascot
            .clone()
            .ok_or_else(|| EngineError::Logic("animation action without mascot".into()))?;
        if !self.is_turning && self.needs_turn() {
            self.is_turning = true;
            let headed_right = self.headed_right();
            mascot.borrow_mut().looking_right = headed_right;
            self.anim.base.reset_elapsed();
        } else if self.is_turning
            && self.anim.base.elapsed() >= self.anim.animations[1].duration as i64
        {
            self.is_turning = false;
            self.anim.base.reset_elapsed();
        }
        Ok(self.anim.animations[usize::from(self.is_turning)].clone())
    }
}
