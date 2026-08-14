//! 繁殖动作：动画播完后发起繁殖请求，由运行时生成新桌宠。

use crate::error::Result;
use crate::math::Vec2;
use crate::tick::Tick;

use super::animation::{AnimationAction, AnimationBase};
use super::{Action, ActionBase};

#[derive(Default)]
pub struct Breed {
    anim: AnimationBase,
}

impl Breed {
    /// 播放动画；本动作在动画之上追加繁殖请求逻辑。
    fn animate_tick(&mut self) -> Result<bool> {
        if self.anim.animation_finished()? {
            return Ok(false);
        }
        self.anim
            .tick_impl(|a| a.check_border_type_impl(), |a| a.handle_dragging_impl())
    }
}

impl Action for Breed {
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
        let transient = self.anim.base.vars.get_bool("BornTransient", false);
        {
            let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
            let m = mascot.borrow();
            let allows_breeding = m
                .env
                .as_ref()
                .is_some_and(|env| env.borrow().allows_breeding);
            if !transient && (!allows_breeding || !m.can_breed) {
                return Ok(false);
            }
        }
        let ret = self.animate_tick()?;
        if self.anim.animation_finished()? {
            // 动画播完：生成繁殖请求，是否兑现由运行时决定。
            let born_x = self.anim.base.vars.get_num("BornX", 0.0);
            let born_y = self.anim.base.vars.get_num("BornY", 0.0);
            let behavior = self.anim.base.vars.get_string("BornBehavior", "Fall");
            let name = self.anim.base.vars.get_string("BornMascot", "");
            let dx = self.anim.base.dx(born_x);
            let dy = self.anim.base.dy(born_y);
            let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
            let mut m = mascot.borrow_mut();
            let anchor = Vec2::new(m.anchor.x + dx, m.anchor.y + dy);
            let request = &mut m.breed_request;
            request.available = true;
            request.behavior = behavior;
            request.name = name;
            request.transient = transient;
            request.anchor = anchor;
            return Ok(false);
        }
        Ok(ret)
    }
    fn finalize(&mut self) -> Result<()> {
        self.anim.finalize_impl()
    }
    fn hotspot_probe(&mut self, cursor: Vec2) -> String {
        self.anim.hotspot_behavior_at_impl(cursor)
    }
}

impl AnimationAction for Breed {
    fn anim(&self) -> &AnimationBase {
        &self.anim
    }
    fn anim_mut(&mut self) -> &mut AnimationBase {
        &mut self.anim
    }
}
