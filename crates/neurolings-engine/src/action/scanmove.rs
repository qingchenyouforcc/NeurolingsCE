//! 扫描移动：连接广播中的其他桌宠，移动到其位置后发起交互。

use std::collections::HashMap;

use crate::environment::Border;
use crate::error::Result;
use crate::math::Vec2;
use crate::tick::Tick;

use super::animation::{AnimationAction, AnimationBase};
use super::{Action, ActionBase};

#[derive(Default)]
pub struct ScanMove {
    anim: AnimationBase,
}

impl ScanMove {
    /// 直线移动流程；本动作在其上追加会合判定。
    fn move_tick(&mut self) -> Result<bool> {
        let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
        // 同步日文旧属性名"目的地X/Y"，保持对老式桌宠包的兼容
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
            let (on_left, on_right) = {
                let m = mascot.borrow();
                let anchor = m.anchor;
                let env = m.env.clone().expect("env set before tick");
                let env = env.borrow();
                (
                    env.work_area.left_border().is_on(anchor)
                        || env.active_ie.right_border().is_on(anchor),
                    env.work_area.right_border().is_on(anchor)
                        || env.active_ie.left_border().is_on(anchor),
                )
            };
            if on_left {
                mascot.borrow_mut().looking_right = false;
            }
            if on_right {
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
        // 越过判定沿用历史行为：两个分支都比较水平坐标（见 movement.rs 的说明）。
        if self.anim.base.vars.has("TargetX") {
            let x = self.anim.base.vars.get_num("TargetX", 0.0);
            if (start.x >= x && end.x <= x) || (start.x <= x && end.x >= x) {
                mascot.borrow_mut().anchor.x = x;
                return Ok(false);
            }
        } else if self.anim.base.vars.has("TargetY") {
            let y = self.anim.base.vars.get_num("TargetY", 0.0);
            if (start.x >= y && end.x <= y) || (start.x <= y && end.x >= y) {
                mascot.borrow_mut().anchor.y = y;
                return Ok(false);
            }
        } else {
            // 两个目标都未定义：动作无法执行，直接结束
            return Ok(false);
        }
        Ok(true)
    }
}

impl Action for ScanMove {
    fn base(&self) -> &ActionBase {
        &self.anim.base
    }
    fn base_mut(&mut self) -> &mut ActionBase {
        &mut self.anim.base
    }
    fn requests_broadcast(&self) -> bool {
        false
    }
    fn init(&mut self, ctx: &mut Tick) -> Result<()> {
        // 动画初始化，但不开广播服务端：Affordance 留给下方客户端连接使用。
        self.anim.base.init_impl(ctx, true, false)?;
        self.anim.anim_idx = -1;
        self.anim.current_anim_time = -1;
        self.anim.window_push_requested = false;
        if self.anim.base.vars.has("FixedVelocity") {
            self.anim.has_fixed_velocity = true;
            let s = self.anim.base.vars.get_string("FixedVelocity", "");
            self.anim.fixed_velocity = Vec2::from_str_lenient(&s);
        } else {
            self.anim.has_fixed_velocity = false;
        }

        let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
        let env = mascot.borrow().env.clone();
        let Some(env) = env else { return Ok(()) };
        let anchor = mascot.borrow().anchor;
        let affordance = self.anim.base.vars.get_string("Affordance", "");
        let behavior = self.anim.base.vars.get_string("Behavior", "");
        let target_behavior = self.anim.base.vars.get_string("TargetBehavior", "");
        if let Some(client) = env.borrow_mut().broadcasts.borrow_mut().try_connect(
            anchor,
            &affordance,
            &behavior,
            &target_behavior,
        ) {
            self.anim.base.client = client;
        }
        Ok(())
    }
    fn tick(&mut self) -> Result<bool> {
        if !self.anim.base.client.connected() {
            return Ok(false);
        }
        let Some(target) = self.anim.base.client.get_target() else {
            return Ok(false);
        };
        self.anim
            .base
            .vars
            .add_attr_nums(&HashMap::from([("TargetX".to_string(), target.x)]));
        let ret = self.move_tick()?;
        let Some(target) = self.anim.base.client.get_target() else {
            return Ok(ret);
        };
        let mascot = self.anim.base.mascot.clone().expect("mascot set at init");
        if (mascot.borrow().anchor.x - target.x).abs() < 3.0 {
            mascot.borrow_mut().anchor.x = target.x;
            self.anim.base.client.notify_arrival();
            if let Some(interaction) = self.anim.base.client.get_interaction() {
                let mut m = mascot.borrow_mut();
                m.interaction = interaction;
                m.queued_behavior = m.interaction.behavior().to_string();
            }
            return Ok(true);
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

impl AnimationAction for ScanMove {
    fn anim(&self) -> &AnimationBase {
        &self.anim
    }
    fn anim_mut(&mut self) -> &mut AnimationBase {
        &mut self.anim
    }
}
