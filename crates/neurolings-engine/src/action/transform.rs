//! 变身动作：动画播完后发起繁殖请求变身为新桌宠，自身死亡。

use crate::error::Result;
use crate::math::Vec2;
use crate::tick::Tick;

use super::animation::{AnimationAction, AnimationBase};
use super::{Action, ActionBase};

#[derive(Default)]
pub struct Transform {
    anim: AnimationBase,
}

impl Transform {
    /// 播放动画；本动作在动画之上追加变身逻辑。
    fn animate_tick(&mut self) -> Result<bool> {
        if self.anim.animation_finished()? {
            return Ok(false);
        }
        self.anim
            .tick_impl(|a| a.check_border_type_impl(), |a| a.handle_dragging_impl())
    }
}

impl Action for Transform {
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
        let ret = self.animate_tick()?;
        if self.anim.animation_finished()? {
            // 动画播完：生成繁殖请求，是否兑现由运行时决定。
            let behavior = self.anim.base.vars.get_string("TransformBehavior", "Fall");
            let name = self.anim.base.vars.get_string("TransformMascot", "");
            let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
            let mut m = mascot.borrow_mut();
            let anchor = m.anchor;
            {
                let request = &mut m.breed_request;
                request.available = true;
                request.behavior = behavior;
                request.name = name;
                request.transient = false; // 变身产物视为常驻桌宠
                request.anchor = anchor;
            }
            // 变身意味着当前个体立即消失。
            m.dead = true;
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

impl AnimationAction for Transform {
    fn anim(&self) -> &AnimationBase {
        &self.anim
    }
    fn anim_mut(&mut self) -> &mut AnimationBase {
        &mut self.anim
    }
}
