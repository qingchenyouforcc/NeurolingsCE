//! 转身动作：首帧判断是否需要转身，之后播放转身动画。

use crate::error::Result;
use crate::math::Vec2;
use crate::tick::Tick;

use super::animation::{AnimationAction, AnimationBase};
use super::{Action, ActionBase};

#[derive(Default)]
pub struct Turn {
    anim: AnimationBase,
}

impl Action for Turn {
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
        let will_look_right = self.anim.base.vars.get_bool("LookRight", false);
        if self.anim.base.elapsed() == 0 {
            // 首帧判定
            let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
            let looking_right = mascot.borrow().looking_right;
            if will_look_right == looking_right {
                // 已朝目标方向，无需转身
                return Ok(false);
            } else {
                mascot.borrow_mut().looking_right = will_look_right;
            }
        }
        if self.anim.animation_finished()? {
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

impl AnimationAction for Turn {
    fn anim(&self) -> &AnimationBase {
        &self.anim
    }
    fn anim_mut(&mut self) -> &mut AnimationBase {
        &mut self.anim
    }
}
