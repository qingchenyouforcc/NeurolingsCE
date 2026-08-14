//! 自毁动作：动画播完后标记桌宠死亡，由运行时回收。

use crate::error::Result;
use crate::math::Vec2;
use crate::tick::Tick;

use super::animation::{AnimationAction, AnimationBase};
use super::{Action, ActionBase};

#[derive(Default)]
pub struct SelfDestruct {
    anim: AnimationBase,
}

impl SelfDestruct {
    /// 播放动画；本动作在动画之上追加死亡标记。
    fn animate_tick(&mut self) -> Result<bool> {
        if self.anim.animation_finished()? {
            return Ok(false);
        }
        self.anim
            .tick_impl(|a| a.check_border_type_impl(), |a| a.handle_dragging_impl())
    }
}

impl Action for SelfDestruct {
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
        if !self.animate_tick()? {
            if self.anim.animation_finished()? {
                let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
                mascot.borrow_mut().dead = true;
                return Ok(true);
            }
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

impl AnimationAction for SelfDestruct {
    fn anim(&self) -> &AnimationBase {
        &self.anim
    }
    fn anim_mut(&mut self) -> &mut AnimationBase {
        &mut self.anim
    }
}
