//! 挣脱动作：被拖拽时播放挣脱动画，动画播完则挣脱成功。

use crate::error::Result;
use crate::math::Vec2;
use crate::tick::Tick;

use super::animation::{AnimationAction, AnimationBase};
use super::{Action, ActionBase};

#[derive(Default)]
pub struct Resist {
    anim: AnimationBase,
}

impl Resist {
    /// 播放动画；因本动作覆盖了拖拽检查，不能直接复用基座的 tick_impl。
    fn animate_tick(&mut self) -> Result<bool> {
        if self.anim.animation_finished()? {
            return Ok(false);
        }
        if !self.anim.base.tick_impl(true)? {
            return Ok(false);
        }
        if !self.anim.check_border_type_impl()? {
            return Ok(false);
        }
        if !self.handle_dragging()? {
            return Ok(false);
        }
        if self.anim.is_window_push_action() && !self.anim.handle_window_push()? {
            return Ok(false);
        }
        let velocity = self.anim.get_velocity()?;
        let dx = self.anim.base.dx(velocity.x);
        let dy = self.anim.base.dy(velocity.y);
        let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
        {
            let mut m = mascot.borrow_mut();
            m.anchor.x += dx;
            m.anchor.y += dy;
        }
        // 姿态在锚点更新后再取：动画选择条件可能引用 mascot.anchor。
        let pose = self.anim.get_pose()?;
        mascot.borrow_mut().active_frame = pose.frame;
        Ok(true)
    }
}

impl Action for Resist {
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
        let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
        let (cursor_x, anchor_x) = {
            let m = mascot.borrow();
            (m.get_cursor().x, m.anchor.x)
        };
        if (cursor_x - anchor_x).abs() >= 5.0 {
            // 光标被拖走：挣脱失败，回到拖拽状态
            let mut m = mascot.borrow_mut();
            m.queued_behavior = "Dragged".to_string();
            m.was_on_ie = false;
            return Ok(false);
        }
        let ret = self.animate_tick()?;
        if self.anim.animation_finished()? {
            // 动画播完：挣脱成功
            mascot.borrow_mut().dragging = false;
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

impl AnimationAction for Resist {
    fn anim(&self) -> &AnimationBase {
        &self.anim
    }
    fn anim_mut(&mut self) -> &mut AnimationBase {
        &mut self.anim
    }
    fn handle_dragging(&mut self) -> Result<bool> {
        let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
        if !mascot.borrow().dragging {
            // 用户松手：切换为抛出行为
            let mut m = mascot.borrow_mut();
            m.queued_behavior = "Thrown".to_string();
            m.was_on_ie = false;
            return Ok(false);
        }
        Ok(true)
    }
}
