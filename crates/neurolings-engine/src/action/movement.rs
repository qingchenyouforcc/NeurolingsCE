//! 直线移动动作：朝 TargetX/TargetY 移动，越过目标即结束。

use std::collections::HashMap;

use crate::environment::Border;
use crate::error::Result;
use crate::math::Vec2;
use crate::tick::Tick;

use super::animation::{AnimationAction, AnimationBase};
use super::{Action, ActionBase};

/// 判断从 start 到 end 的移动是否越过了 v（含方向无关的双向判定）。
fn passed(start: f64, end: f64, v: f64) -> bool {
    (start >= v && end <= v) || (start <= v && end >= v)
}

#[derive(Default)]
pub struct Move {
    anim: AnimationBase,
}

impl Action for Move {
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
        self.anim.init_impl(ctx)
    }
    fn tick(&mut self) -> Result<bool> {
        // 同步日文旧属性名"目的地X/Y"，保持对老式桌宠包的兼容
        let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
        if self.anim.base.vars.has("TargetX") {
            let x = self.anim.base.vars.get_num("TargetX", 0.0);
            self.anim
                .base
                .vars
                .add_attr_nums(&HashMap::from([("目的地X".to_string(), x)]));
            let velocity = self.anim.get_velocity()?;
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
        if !self
            .anim
            .tick_impl(|a| a.check_border_type_impl(), |a| a.handle_dragging_impl())?
        {
            return Ok(false);
        }
        let end = mascot.borrow().anchor;

        if self.anim.base.vars.has("TargetX") {
            let x = self.anim.base.vars.get_num("TargetX", 0.0);
            if passed(start.x, end.x, x) {
                mascot.borrow_mut().anchor.x = x;
                return Ok(false);
            }
        } else if self.anim.base.vars.has("TargetY") {
            let y = self.anim.base.vars.get_num("TargetY", 0.0);
            // 修复原版 move.cc 历史 bug：TargetY 应比较 y 坐标而非 x
            if passed(start.y, end.y, y) {
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

impl AnimationAction for Move {
    fn anim(&self) -> &AnimationBase {
        &self.anim
    }
    fn anim_mut(&mut self) -> &mut AnimationBase {
        &mut self.anim
    }
}
