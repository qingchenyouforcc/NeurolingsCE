//! 被拖拽动作：跟随光标悬挂，松手后切换 Thrown，挣脱超时自动结束。

use std::collections::HashMap;

use crate::error::Result;
use crate::math::Vec2;
use crate::tick::Tick;

use super::animation::{AnimationAction, AnimationBase};
use super::{Action, ActionBase};

// footX/FootDX 等小写别名用于兼容老式桌宠包。

#[derive(Default)]
pub struct Dragged {
    anim: AnimationBase,
    foot_x: f64,
    foot_dx: f64,
    time_to_resist: i64,
}

impl Dragged {
    /// 动画帧推进；因本动作覆盖了拖拽检查，不能直接复用基座的 tick_impl。
    fn animation_tick(&mut self) -> Result<bool> {
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

impl Action for Dragged {
    fn base(&self) -> &ActionBase {
        &self.anim.base
    }
    fn base_mut(&mut self) -> &mut ActionBase {
        &mut self.anim.base
    }
    fn requests_broadcast(&self) -> bool {
        true
    }
    fn requests_interpolation(&self) -> bool {
        false
    }
    fn init(&mut self, ctx: &mut Tick) -> Result<()> {
        self.anim.init_impl(ctx)?;
        let offset_x = self.anim.base.vars.get_num("OffsetX", 0.0);
        self.foot_dx = 0.0;
        let cursor_x = self
            .anim
            .base
            .mascot
            .as_ref()
            .expect("mascot set at init")
            .borrow()
            .get_cursor()
            .x;
        self.foot_x = cursor_x + offset_x;
        self.time_to_resist = 250;
        self.anim.base.vars.add_attr_nums(&HashMap::from([
            ("FootX".to_string(), self.foot_x),
            ("footX".to_string(), self.foot_x),
        ]));
        Ok(())
    }
    fn tick(&mut self) -> Result<bool> {
        // 整帧逻辑即动画基座流程（含本动作的拖拽检查覆盖）。
        self.animation_tick()
    }
    fn subtick(&mut self, idx: i32) -> Result<bool> {
        let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
        mascot.borrow_mut().looking_right = false;
        let cursor = mascot.borrow().get_cursor();
        if idx == 0 {
            self.foot_dx = (self.foot_dx + ((cursor.x - self.foot_x) * 0.1)) * 0.8;
            self.foot_x += self.foot_dx;
            self.anim.base.vars.add_attr_nums(&HashMap::from([
                ("FootX".to_string(), self.foot_x),
                ("footX".to_string(), self.foot_x),
                ("FootDX".to_string(), self.foot_dx),
                ("footDX".to_string(), self.foot_dx),
            ]));
        }
        let offset_x = self.anim.base.vars.get_num("OffsetX", 0.0);
        let offset_y = self.anim.base.vars.get_num("OffsetY", 120.0);
        let env = {
            let m = mascot.borrow();
            m.env.clone().expect("env set before tick")
        };
        let subtick_count = env.borrow_mut().sanitized_subtick_count() as f64;
        if (cursor.x - mascot.borrow().anchor.x + offset_x).abs() >= 5.0 / subtick_count {
            self.anim.base.reset_elapsed();
        }
        // 无插值：仅子帧 0 执行整帧逻辑，其余子帧直接视为成功。
        if idx == 0 && !self.animation_tick()? {
            return Ok(false);
        }
        if mascot.borrow().dragging {
            let mut m = mascot.borrow_mut();
            m.anchor.x = cursor.x + offset_x;
            m.anchor.y = cursor.y + offset_y;
        }
        if self.anim.base.elapsed() >= self.time_to_resist {
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

impl AnimationAction for Dragged {
    fn anim(&self) -> &AnimationBase {
        &self.anim
    }
    fn anim_mut(&mut self) -> &mut AnimationBase {
        &mut self.anim
    }
    fn handle_dragging(&mut self) -> Result<bool> {
        let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
        if !mascot.borrow().dragging {
            // 用户松手：切换为抛出行为。
            let mut m = mascot.borrow_mut();
            m.queued_behavior = "Thrown".to_string();
            m.was_on_ie = false;
            return Ok(false);
        }
        Ok(true)
    }
}
