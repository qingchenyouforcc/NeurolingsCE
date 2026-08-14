//! 下落动作：受重力与空气阻力影响，落地或贴上活动窗口边缘时结束。

use std::collections::HashMap;

use crate::environment::Border;
use crate::error::Result;
use crate::math::Vec2;
use crate::tick::Tick;

use super::animation::{AnimationAction, AnimationBase};
use super::{Action, ActionBase};

#[derive(Default)]
pub struct Fall {
    anim: AnimationBase,
    velocity: Vec2,
}

impl Action for Fall {
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
        self.velocity.x = self.anim.base.vars.get_num("InitialVX", 0.0);
        self.velocity.y = self.anim.base.vars.get_num("InitialVY", 0.0);
        Ok(())
    }
    fn tick(&mut self) -> Result<bool> {
        // Fall 不覆盖整帧逻辑，直接走动画基座的 tick。
        self.anim
            .tick_impl(|a| a.check_border_type_impl(), |a| a.handle_dragging_impl())
    }
    fn subtick(&mut self, idx: i32) -> Result<bool> {
        let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
        // 无插值：仅子帧 0 执行整帧逻辑，其余子帧直接视为成功。
        if idx == 0
            && !self
                .anim
                .tick_impl(|a| a.check_border_type_impl(), |a| a.handle_dragging_impl())?
        {
            return Ok(false);
        }
        let env = {
            let m = mascot.borrow();
            m.env.clone().expect("env set before tick")
        };
        let anchor = mascot.borrow().anchor;
        let mut on_land = {
            let e = env.borrow();
            e.floor.is_on(anchor) || e.ceiling.is_on(anchor) || e.work_area.is_on(anchor)
        };
        if self.anim.base.elapsed() > 0 {
            // 首帧不把活动窗口视为落点，避免刚起跳就吸附
            on_land = on_land || env.borrow().active_ie.is_on(anchor);
        }
        if on_land {
            return Ok(false);
        }

        if self.velocity.x != 0.0 {
            mascot.borrow_mut().looking_right = self.velocity.x > 0.0;
        }

        let subtick_count = env.borrow_mut().sanitized_subtick_count() as f64;

        let resistance_x = self.anim.base.vars.get_num("RegistanceX", 0.05);
        let resistance_y = self.anim.base.vars.get_num("RegistanceY", 0.1);
        let gravity = self.anim.base.vars.get_num("Gravity", 2.0);
        self.velocity.x -= (self.velocity.x * resistance_x) / subtick_count;
        self.velocity.y += (gravity - self.velocity.y * resistance_y) / subtick_count;

        self.anim.base.vars.add_attr_nums(&HashMap::from([
            ("VelocityX".to_string(), self.velocity.x),
            ("VelocityY".to_string(), self.velocity.y),
        ]));

        let before = mascot.borrow().anchor;

        {
            let mut m = mascot.borrow_mut();
            m.anchor.x += self.velocity.x / subtick_count;
            m.anchor.y += self.velocity.y / subtick_count;
        }

        let (work_area, ceiling_y, floor_y) = {
            let e = env.borrow();
            (e.work_area, e.ceiling.y, e.floor.y)
        };
        {
            let mut m = mascot.borrow_mut();
            if m.anchor.x > work_area.right {
                m.anchor.x = work_area.right;
            } else if m.anchor.x < work_area.left {
                m.anchor.x = work_area.left;
            }
            if m.anchor.y < ceiling_y {
                m.anchor.y = ceiling_y;
            } else if m.anchor.y > floor_y {
                m.anchor.y = floor_y;
            }
        }

        let after = mascot.borrow().anchor;
        let active_ie = env.borrow().active_ie;

        // 越过活动窗口边缘时吸附到该边缘。
        {
            let mut m = mascot.borrow_mut();
            if active_ie.visible()
                && active_ie.left_border().faces(after)
                && before.x <= active_ie.area.left
                && after.x >= active_ie.area.left
            {
                m.anchor.x = active_ie.area.left;
            } else if active_ie.visible()
                && active_ie.right_border().faces(after)
                && before.x >= active_ie.area.right
                && after.x <= active_ie.area.right
            {
                m.anchor.x = active_ie.area.right;
            } else if active_ie.visible()
                && active_ie.top_border().faces(after)
                && before.y <= active_ie.area.top
                && after.y >= active_ie.area.top
            {
                m.anchor.y = active_ie.area.top;
            } else if active_ie.visible()
                && active_ie.bottom_border().faces(after)
                && before.y >= active_ie.area.bottom
                && after.y <= active_ie.area.bottom
            {
                m.anchor.y = active_ie.area.bottom;
            }
        }

        // 全局边界优先于活动窗口吸附：窗口矩形与工作区几何可能相差一像素，
        // 吸附可能把锚点推出地板之下，必须最后再钳制一次，否则会永远重复下落。
        {
            let mut m = mascot.borrow_mut();
            if m.anchor.x > work_area.right {
                m.anchor.x = work_area.right;
            } else if m.anchor.x < work_area.left {
                m.anchor.x = work_area.left;
            }
            if m.anchor.y < ceiling_y {
                m.anchor.y = ceiling_y;
            } else if m.anchor.y > floor_y {
                m.anchor.y = floor_y;
            }
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

impl AnimationAction for Fall {
    fn anim(&self) -> &AnimationBase {
        &self.anim
    }
    fn anim_mut(&mut self) -> &mut AnimationBase {
        &mut self.anim
    }
}
