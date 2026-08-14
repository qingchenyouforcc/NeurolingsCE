//! 单只桌宠的会话管理器：负责行为选择、动作执行与逐帧状态推进。

use std::rc::Rc;

use crate::action::SharedAction;
use crate::behavior::{Behavior, BehaviorManager};
use crate::environment::{Border, MulScale};
use crate::error::{EngineError, Result};
use crate::math::Vec2;
use crate::parser::Parser;
use crate::scripting::context::ScriptContext;
use crate::state::{BreedRequest, SharedState, State, shared_state};
use crate::tick::Tick;

/// 桌宠的初始状态：出生锚点、初始行为名与朝向。
#[derive(Clone, Default)]
pub struct Initializer {
    pub anchor: Vec2,
    pub behavior: String,
    pub looking_right: bool,
}

impl Initializer {
    pub fn new(anchor: Vec2, behavior: &str, looking_right: bool) -> Self {
        Self {
            anchor,
            behavior: behavior.to_string(),
            looking_right,
        }
    }
}

impl From<&BreedRequest> for Initializer {
    fn from(data: &BreedRequest) -> Self {
        Self {
            anchor: data.anchor,
            behavior: data.behavior.clone(),
            looking_right: data.looking_right,
        }
    }
}

pub struct Manager {
    behaviors: BehaviorManager,
    tick_ctx: Tick,
    action: Option<SharedAction>,
    pub script_ctx: Rc<ScriptContext>,
    pub state: SharedState,
}

impl Manager {
    pub fn new(
        actions_xml: &str,
        behaviors_xml: &str,
        init: Initializer,
        script_ctx: Option<Rc<ScriptContext>>,
    ) -> Result<Self> {
        let mut parser = Parser::new();
        parser.parse(actions_xml, behaviors_xml)?;
        let script_ctx = match script_ctx {
            Some(ctx) => ctx,
            None => ScriptContext::new(),
        };
        let tick_ctx = Tick::new(script_ctx.clone(), Default::default());
        let mut state = State::default();
        state.anchor = init.anchor;
        state.constants = parser.constants.clone();
        state.looking_right = init.looking_right;
        let state = shared_state(state);
        script_ctx.set_state(state.clone());
        let behaviors = BehaviorManager::new(parser.behavior_list, &init.behavior, &script_ctx)?;
        Ok(Self {
            behaviors,
            tick_ctx,
            action: None,
            script_ctx,
            state,
        })
    }

    pub fn initial_behavior_list(&self) -> &crate::behavior::BehaviorList {
        self.behaviors.initial_list()
    }

    fn next_behavior_named(&mut self, name: &str) -> Result<()> {
        if !name.is_empty() {
            self.behaviors.set_next(name)?;
        }
        if let Some(action) = self.action.take() {
            action.borrow_mut().finalize()?;
        }

        let behavior = self.behaviors.next(&self.script_ctx, &self.state);
        let behavior = match behavior {
            Some(b) => b,
            None => {
                self.behaviors.set_next("Fall")?;
                match self.behaviors.next(&self.script_ctx, &self.state) {
                    Some(b) => b,
                    None => return Err(EngineError::Logic("no next behavior".into())),
                }
            }
        };

        // 切换行为前先停止当前音效。
        {
            let mut s = self.state.borrow_mut();
            s.active_sound.clear();
            s.active_sound_changed = true;
            s.behavior = Some(behavior.clone());
        }

        let action = behavior
            .action
            .borrow()
            .clone()
            .ok_or_else(|| EngineError::Logic("behavior without action".into()))?;
        action.borrow_mut().init(&mut self.tick_ctx)?;
        self.action = Some(action);
        Ok(())
    }

    fn action_tick(&mut self) -> Result<bool> {
        // 仅子帧 0 的执行结果有意义，其余子帧一律视为成功。
        let action = self.action.clone().expect("action set before tick");
        let next_subtick = self.state.borrow().next_subtick;
        let ignore_did_tick = next_subtick != 0;
        let did_tick = action.borrow_mut().subtick(next_subtick)?;
        let ret = ignore_did_tick || did_tick;
        if ret {
            let subticks = {
                let s = self.state.borrow();
                s.env
                    .as_ref()
                    .map(|e| e.borrow_mut().sanitized_subtick_count())
                    .unwrap_or(1)
            };
            let mut s = self.state.borrow_mut();
            s.next_subtick = (s.next_subtick + 1) % subticks;
        }
        Ok(ret)
    }

    pub fn reset_position(&self) {
        let Some(env) = self.state.borrow().env.clone() else {
            return;
        };
        let new_anchor = {
            let mut e = env.borrow_mut();
            let screen = e.screen;
            if screen.width() >= 100.0 && screen.height() >= 100.0 {
                let new_x = screen.left + 50.0 + e.random_int(screen.width() as i32 - 50) as f64;
                let new_y = screen.top + 50.0 + e.random_int(screen.height() as i32 - 50) as f64;
                Vec2::new(new_x, new_y)
            } else {
                Vec2::new(screen.width() / 2.0, screen.height() / 2.0)
            }
        };
        self.state.borrow_mut().anchor = new_anchor;
    }

    pub fn detach_from_borders(&self) {
        let mut s = self.state.borrow_mut();
        let Some(env) = s.env.clone() else { return };
        let env = env.borrow();
        let anchor = s.anchor;
        if env.active_ie.right_border().is_on(anchor) || env.work_area.left_border().is_on(anchor) {
            s.anchor.x += 1.0;
        } else if env.active_ie.left_border().is_on(anchor)
            || env.work_area.right_border().is_on(anchor)
        {
            s.anchor.x -= 1.0;
        }
        if env.active_ie.bottom_border().is_on(anchor) || env.work_area.top_border().is_on(anchor) {
            s.anchor.y += 1.0;
        } else if env.active_ie.top_border().is_on(anchor)
            || env.work_area.bottom_border().is_on(anchor)
        {
            s.anchor.y -= 1.0;
        }
    }

    /// 外部请求切换行为：重置帧上下文并结束交互后排队，待下一帧生效。
    pub fn next_behavior(&mut self, name: &str) {
        self.tick_ctx.reset();
        self.state.borrow_mut().interaction.finalize();
        self.state.borrow_mut().queued_behavior = name.to_string();
    }

    pub fn prefer_next_behavior(&mut self, name: &str) {
        let _ = self.behaviors.set_next(name);
    }

    pub fn clear_preferred_next_behavior(&mut self) {
        let current = self.state.borrow().behavior.clone();
        self.behaviors.restore_next(current.as_ref());
    }

    fn hotspot_behavior_at(action: Option<&SharedAction>, cursor: Vec2) -> String {
        let Some(action) = action else {
            return String::new();
        };
        let mut current = action.clone();
        loop {
            let probe = current.borrow_mut().hotspot_probe(cursor);
            if !probe.is_empty() {
                return probe;
            }
            let next = current.borrow().chain_next();
            match next {
                Some(n) => current = n,
                None => return String::new(),
            }
        }
    }

    pub fn hotspot_behavior(&self, cursor: Vec2) -> String {
        Self::hotspot_behavior_at(self.action.as_ref(), cursor)
    }

    pub fn trigger_hotspot(&mut self, cursor: Vec2) -> bool {
        let behavior = self.hotspot_behavior(cursor);
        if behavior.is_empty() {
            return false;
        }
        self.next_behavior(&behavior);
        true
    }

    pub fn active_behavior(&self) -> Option<Rc<Behavior>> {
        self.state.borrow().behavior.clone()
    }

    pub fn export_state(&self) -> String {
        self.script_ctx.set_state(self.state.clone());
        self.script_ctx.export_state_json()
    }

    fn has_queued_behavior(&self) -> bool {
        !self.state.borrow().queued_behavior.is_empty()
    }

    fn activate_queued_behavior(&mut self) -> Result<()> {
        if self.has_queued_behavior() {
            self.state.borrow_mut().next_subtick = 0;
            let behavior = self.state.borrow().queued_behavior.clone();
            {
                let mut s = self.state.borrow_mut();
                if s.interaction.available()
                    && !s.interaction.started
                    && s.interaction.behavior() == behavior
                {
                    // 启动 ScanMove/Broadcast 桌宠间交互。
                    s.interaction.started = true;
                }
            }
            self.next_behavior_named(&behavior)?;
            self.state.borrow_mut().queued_behavior.clear();
        }
        Ok(())
    }

    fn pre_tick(&mut self) -> Result<()> {
        self.state.borrow_mut().active_sound_changed = false;
        self.tick_ctx.reset();
        self.script_ctx.set_state(self.state.clone());
        let scale = {
            let s = self.state.borrow();
            s.env
                .as_ref()
                .map(|e| e.borrow().get_scale())
                .unwrap_or(1.0)
        };
        if scale != 1.0 {
            let mut s = self.state.borrow_mut();
            s.local_cursor = s.local_cursor.mul_scale(scale);
            s.anchor *= scale;
        }
        {
            let mut s = self.state.borrow_mut();
            s.roll_dcursor();
            if let Some(env) = s.env.clone() {
                s.active_ie_offset.x += env.borrow().active_ie.dx;
                s.active_ie_offset.y += env.borrow().active_ie.dy;
            }
        }
        if self.state.borrow().next_subtick == 0 {
            self.state.borrow_mut().time += 1;
            if self.state.borrow().behavior.is_none() {
                // 首帧：先确定初始行为。
                self.next_behavior_named("")?;
            }
            self.activate_queued_behavior()?;
            {
                let mut s = self.state.borrow_mut();
                if let Some(env) = s.env.clone() {
                    let (sticky, was_on_ie, floor_above, ie) = {
                        let e = env.borrow();
                        (
                            e.sticky_ie,
                            s.was_on_ie,
                            e.floor.y > s.anchor.y,
                            e.active_ie,
                        )
                    };
                    if sticky && was_on_ie && floor_above {
                        // 逐个尝试各移动边：窗口缩放/最大化时，对边的位移量可能不同。
                        let anchor = s.anchor;
                        let candidates = [
                            Vec2::new(anchor.x + ie.left_dx, anchor.y),
                            Vec2::new(anchor.x + ie.right_dx, anchor.y),
                            Vec2::new(anchor.x, anchor.y + ie.top_dy),
                            Vec2::new(anchor.x, anchor.y + ie.bottom_dy),
                            anchor + s.active_ie_offset,
                        ];
                        for candidate in candidates {
                            if ie.is_on(candidate) {
                                s.anchor = candidate;
                                break;
                            }
                        }
                    }
                }
                s.active_ie_offset = Vec2::ZERO;
            }
        } else if self.state.borrow().behavior.is_none() {
            return Err(EngineError::Logic(
                "cannot determine first behavior on non-zero subtick".into(),
            ));
        }
        Ok(())
    }

    fn post_tick(&mut self) {
        let mut s = self.state.borrow_mut();
        let Some(env) = s.env.clone() else { return };
        let subticks = env.borrow_mut().sanitized_subtick_count();
        if s.next_subtick == 1 || subticks == 1 {
            // 子帧 0 刚执行完毕，此时记录桌宠是否站在活动窗口上。
            let e = env.borrow();
            s.was_on_ie = e.active_ie.is_on(s.anchor) && !e.floor.is_on(s.anchor);
        }
        if !s.active_frame.sound.is_empty() && s.active_sound != s.active_frame.sound {
            s.active_sound_changed = true;
            s.active_sound = s.active_frame.sound.clone();
        }
        let scale = env.borrow().get_scale();
        if scale != 1.0 {
            s.local_cursor = s.local_cursor.mul_scale(1.0 / scale);
            s.anchor /= scale;
        }
    }

    fn tick_inner(&mut self) -> Result<bool> {
        loop {
            let did_tick = self.action_tick()?;
            if self.has_queued_behavior() {
                self.activate_queued_behavior()?;
                continue;
            }
            if did_tick {
                break;
            }
            if self.tick_ctx.reached_init_limit() {
                return Ok(false);
            }
            self.state.borrow_mut().interaction.finalize();
            self.next_behavior_named("")?;
        }
        self.post_tick();
        Ok(true)
    }

    pub fn tick(&mut self) -> Result<()> {
        if self.state.borrow().dead {
            return Ok(());
        }

        self.pre_tick()?;

        // 第 1 次尝试：正常推进。
        if self.tick_inner()? {
            return Ok(());
        }

        // 第 2 次尝试：强制切换到 Fall 行为后重试。
        self.tick_ctx.reset();
        self.next_behavior_named("Fall")?;
        if self.tick_inner()? {
            return Ok(());
        }

        // 第 3 次尝试：Fall + 脱离边界。
        self.tick_ctx.reset();
        self.detach_from_borders();
        self.next_behavior_named("Fall")?;
        if self.tick_inner()? {
            return Ok(());
        }

        // 第 4 次尝试：Fall + 重置位置 + 脱离边界。
        self.tick_ctx.reset();
        self.reset_position();
        self.detach_from_borders();
        self.next_behavior_named("Fall")?;
        if self.tick_inner()? {
            return Ok(());
        }

        // 四次尝试全部失败，桌宠数据极可能已损坏。
        Err(EngineError::TickFailed)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::environment::{Area, Environment, HBorder};

    fn mascot_pack_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mascot_pack")
    }

    fn default_env() -> Rc<RefCell<Environment>> {
        let mut env = Environment::default();
        env.screen = Area::from_vec2(Vec2::new(1920.0, 1080.0));
        env.work_area = Area::from_vec2(Vec2::new(1920.0, 1080.0));
        env.floor = HBorder::new(1080.0, 0.0, 1920.0);
        env.ceiling = HBorder::new(0.0, 0.0, 1920.0);
        env.seed(42);
        Rc::new(RefCell::new(env))
    }

    fn read_pack(name: &str) -> (String, String) {
        let base = mascot_pack_path().join(name);
        let actions = std::fs::read_to_string(base.join("actions.xml")).unwrap();
        let behaviors = std::fs::read_to_string(base.join("behaviors.xml")).unwrap();
        (actions, behaviors)
    }

    fn run_sequence(name: &str, ticks: usize) -> Vec<(i64, f64, f64, String)> {
        let (actions, behaviors) = read_pack(name);
        let mut manager = Manager::new(
            &actions,
            &behaviors,
            Initializer::new(Vec2::new(200.0, 0.0), "Fall", false),
            None,
        )
        .expect("manager created");
        manager.state.borrow_mut().env = Some(default_env());
        let mut trace = Vec::new();
        for _ in 0..ticks {
            manager.tick().expect("tick");
            let s = manager.state.borrow();
            let behavior = s
                .behavior
                .as_ref()
                .map(|b| b.dereferenced().name.clone())
                .unwrap_or_default();
            trace.push((s.time, s.anchor.x, s.anchor.y, behavior));
        }
        trace
    }

    #[test]
    fn tick_sequence_is_deterministic() {
        let a = run_sequence("Default", 300);
        let b = run_sequence("Default", 300);
        assert_eq!(a, b, "same seed must reproduce the tick sequence");
        assert!(
            a.iter().any(|(_, _, _, b)| b != "Fall"),
            "mascot leaves Fall"
        );
        assert!(
            a.iter().all(|(_, x, y, _)| x.is_finite() && y.is_finite()),
            "positions stay finite"
        );
    }

    #[test]
    fn every_pack_ticks_without_failure() {
        for pack in [
            "Default", "Cerber", "Eviling", "Neuron", "Tuteling", "Vedaling", "Weuron",
        ] {
            let trace = run_sequence(pack, 120);
            assert_eq!(trace.len(), 120, "{pack} ran 120 ticks");
        }
    }
}
