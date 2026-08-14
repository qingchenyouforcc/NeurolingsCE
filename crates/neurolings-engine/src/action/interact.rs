//! 交互动作：响应其他桌宠的广播会合，播放互动动画。

use crate::error::Result;
use crate::math::Vec2;
use crate::tick::Tick;

use super::animation::{AnimationAction, AnimationBase};
use super::{Action, ActionBase};

#[derive(Default)]
pub struct Interact {
    anim: AnimationBase,
}

impl Interact {
    /// 播放动画；本动作在动画之上追加交互状态判定。
    fn animate_tick(&mut self) -> Result<bool> {
        if self.anim.animation_finished()? {
            return Ok(false);
        }
        self.anim
            .tick_impl(|a| a.check_border_type_impl(), |a| a.handle_dragging_impl())
    }
}

impl Action for Interact {
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
        {
            let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
            let m = mascot.borrow();
            if !m.interaction.available() || !m.interaction.started {
                return Ok(false);
            }
        }
        if !self.animate_tick()? {
            if self.anim.animation_finished()? {
                let next = self.anim.base.vars.get_string("Behavior", "");
                if !next.is_empty() {
                    let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
                    mascot.borrow_mut().queued_behavior = next;
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
        Ok(mascot.borrow().interaction.ongoing())
    }
    fn finalize(&mut self) -> Result<()> {
        self.anim.finalize_impl()
    }
    fn hotspot_probe(&mut self, cursor: Vec2) -> String {
        self.anim.hotspot_behavior_at_impl(cursor)
    }
}

impl AnimationAction for Interact {
    fn anim(&self) -> &AnimationBase {
        &self.anim
    }
    fn anim_mut(&mut self) -> &mut AnimationBase {
        &mut self.anim
    }
}
