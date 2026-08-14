//! 跳跃动作：沿抛物线感直线逼近目标点，距离够近时落点结束。

use crate::error::Result;
use crate::math::Vec2;
use crate::tick::Tick;

use super::animation::{AnimationAction, AnimationBase};
use super::{Action, ActionBase};

#[derive(Default)]
pub struct Jump {
    anim: AnimationBase,
}

impl Action for Jump {
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
        let target = Vec2::new(
            self.anim.base.vars.get_num("TargetX", 0.0),
            self.anim.base.vars.get_num("TargetY", 0.0),
        );
        let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
        let anchor = mascot.borrow().anchor;
        mascot.borrow_mut().looking_right = anchor.x < target.x;
        let distance = Vec2::new(
            target.x - anchor.x,
            target.y - anchor.y - (target.x - anchor.x).abs(),
        );
        let velocity_abs = self.anim.base.vars.get_num("VelocityParam", 20.0);
        // 距离按 f32 精度开方，与老式桌宠包的手感保持一致。
        let distance_abs =
            ((distance.x * distance.x + distance.y * distance.y) as f32).sqrt() as f64;
        if distance_abs != 0.0 {
            let velocity = Vec2::new(
                velocity_abs * distance.x / distance_abs,
                velocity_abs * distance.y / distance_abs,
            );
            let mut m = mascot.borrow_mut();
            m.anchor.x += velocity.x;
            m.anchor.y += velocity.y;
        }
        if distance_abs <= velocity_abs {
            mascot.borrow_mut().anchor = target;
            return Ok(false);
        }
        self.anim
            .tick_impl(|a| a.check_border_type_impl(), |a| a.handle_dragging_impl())
    }
    fn finalize(&mut self) -> Result<()> {
        self.anim.finalize_impl()
    }
    fn hotspot_probe(&mut self, cursor: Vec2) -> String {
        self.anim.hotspot_behavior_at_impl(cursor)
    }
}

impl AnimationAction for Jump {
    fn anim(&self) -> &AnimationBase {
        &self.anim
    }
    fn anim_mut(&mut self) -> &mut AnimationBase {
        &mut self.anim
    }
}
