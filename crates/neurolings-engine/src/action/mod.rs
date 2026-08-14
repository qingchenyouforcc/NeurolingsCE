//! 动作系统：动作 trait 与所有动作共享的基座机制。

pub mod animate;
pub mod animation;
pub mod breed;
pub mod dragged;
pub mod fall;
pub mod instant;
pub mod interact;
pub mod jump;
pub mod look;
pub mod movement;
pub mod movewithturn;
pub mod offset;
pub mod reference;
pub mod resist;
pub mod scanmove;
pub mod select;
pub mod selfdestruct;
pub mod sequence;
pub mod stay;
pub mod transform;
pub mod turn;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::broadcast::{Client, Server};
use crate::error::{EngineError, Result};
use crate::math::Vec2;
use crate::scripting::variables::Variables;
use crate::state::SharedState;
use crate::tick::Tick;

pub type SharedAction = Rc<RefCell<dyn Action>>;

pub fn shared<A: Action + 'static>(action: A) -> SharedAction {
    let rc: Rc<RefCell<A>> = Rc::new(RefCell::new(action));
    rc
}

/// 所有动作共享的基座：生命周期、变量、广播与插值。
/// 每个动作内嵌此结构，并通过 Action trait 的 base()/base_mut() 暴露。
pub struct ActionBase {
    pub active: bool,
    pub target_offset: Vec2,
    pub mascot: Option<SharedState>,
    pub vars: Variables,
    pub server: Server,
    pub client: Client,
    pub start_time: i64,
    pub init_attr: HashMap<String, String>,
}

impl Default for ActionBase {
    fn default() -> Self {
        Self {
            active: false,
            target_offset: Vec2::ZERO,
            mascot: None,
            vars: Variables::new(),
            server: Server::default(),
            client: Client::default(),
            start_time: 0,
            init_attr: HashMap::new(),
        }
    }
}

impl ActionBase {
    pub fn reset_elapsed(&mut self) {
        if let Some(mascot) = &self.mascot {
            self.start_time = mascot.borrow().time;
        }
    }

    pub fn elapsed(&self) -> i64 {
        self.mascot
            .as_ref()
            .map_or(0, |m| m.borrow().time - self.start_time)
    }

    /// 水平翻转适配：动作参数按朝左素材书写，朝右时取反。
    pub fn dx(&self, dx: f64) -> f64 {
        let looking_right = self
            .mascot
            .as_ref()
            .is_some_and(|m| m.borrow().looking_right);
        (if looking_right { -1.0 } else { 1.0 }) * dx
    }

    pub fn dy(&self, dy: f64) -> f64 {
        dy
    }

    /// 动作初始化。requests_vars/requests_broadcast 由具体动作声明。
    pub fn init_impl(
        &mut self,
        ctx: &mut Tick,
        requests_vars: bool,
        requests_broadcast: bool,
    ) -> Result<()> {
        ctx.will_init();
        if self.active {
            return Err(EngineError::Logic("init() called twice".into()));
        }
        self.active = true;
        let mascot = ctx
            .script
            .state
            .borrow()
            .clone()
            .ok_or_else(|| EngineError::Logic("script context has no state".into()))?;
        self.start_time = mascot.borrow().time;
        let mut attr = self.init_attr.clone();
        for (k, v) in &ctx.extra_attr {
            attr.insert(k.clone(), v.clone());
        }
        self.mascot = Some(mascot.clone());
        if requests_vars {
            self.vars.init(ctx.script.clone(), &attr);
            if requests_broadcast {
                let affordance = self.vars.get_string("Affordance", "");
                if !affordance.is_empty() {
                    let anchor = mascot.borrow().anchor;
                    let env = mascot.borrow().env.clone();
                    if let Some(env) = env {
                        self.server = env
                            .borrow_mut()
                            .broadcasts
                            .borrow_mut()
                            .start_broadcast(&affordance, anchor);
                        self.vars
                            .add_attr(&HashMap::from([("Affordance".to_string(), String::new())]));
                    }
                }
            }
        }
        Ok(())
    }

    /// 帧推进：更新广播锚点、处理会合、刷新动态变量并检查 Condition。
    pub fn tick_impl(&mut self, requests_vars: bool) -> Result<bool> {
        if !requests_vars {
            return Ok(true);
        }
        if self.server.active() {
            let anchor = self
                .mascot
                .as_ref()
                .map(|m| m.borrow().anchor)
                .unwrap_or(Vec2::ZERO);
            self.server.update_anchor(anchor);
        }
        if self.server.did_meet_up()
            && let Some(interaction) = self.server.get_interaction()
        {
            if let Some(mascot) = &self.mascot {
                let mut m = mascot.borrow_mut();
                m.interaction = interaction;
                m.queued_behavior = m.interaction.behavior().to_string();
            }
            return Ok(true);
        }
        self.vars.tick();
        if !self.vars.get_bool("Condition", true) {
            return Ok(false);
        }
        Ok(true)
    }

    /// 子帧推进：在两次整帧位置之间做线性插值。
    pub fn subtick_impl(
        &mut self,
        idx: i32,
        requests_vars: bool,
        requests_interpolation: bool,
    ) -> Result<bool> {
        if requests_interpolation {
            if idx == 0 {
                let Some(mascot) = self.mascot.clone() else {
                    return Ok(false);
                };
                let start_anchor = mascot.borrow().anchor;
                if !self.tick_impl(requests_vars)? {
                    return Ok(false);
                }
                self.target_offset = mascot.borrow().anchor - start_anchor;
                mascot.borrow_mut().anchor = start_anchor;
            }
            let Some(mascot) = self.mascot.clone() else {
                return Ok(false);
            };
            let subticks = {
                let m = mascot.borrow();
                m.env
                    .as_ref()
                    .map(|e| e.borrow_mut().sanitized_subtick_count())
                    .unwrap_or(1)
            };
            let step = self.target_offset * (1.0 / subticks as f64);
            mascot.borrow_mut().anchor += step;
            Ok(true)
        } else {
            if idx != 0 {
                return Ok(true);
            }
            self.tick_impl(requests_vars)
        }
    }

    /// 收尾：停播广播、销毁变量作用域并解除与桌宠状态的绑定。
    pub fn finalize_impl(&mut self, requests_vars: bool) -> Result<()> {
        if !self.active {
            return Err(EngineError::FinalizeTwice);
        }
        if let Some(mascot) = &self.mascot
            && mascot.borrow().next_subtick != 0
        {
            return Err(EngineError::Logic(
                "finalize() called at non-zero subtick".into(),
            ));
        }
        if requests_vars {
            self.server.finalize();
            self.client.finalize();
            self.vars.finalize();
        }
        self.mascot = None;
        self.active = false;
        Ok(())
    }
}

/// 动作接口。默认实现提供通用生命周期；具体动作按需覆盖。
pub trait Action {
    fn base(&self) -> &ActionBase;
    fn base_mut(&mut self) -> &mut ActionBase;

    fn requests_vars(&self) -> bool {
        true
    }
    fn requests_broadcast(&self) -> bool {
        false
    }
    fn requests_interpolation(&self) -> bool {
        true
    }

    fn set_init_attr(&mut self, attr: HashMap<String, String>) {
        self.base_mut().init_attr = attr;
    }

    fn init(&mut self, ctx: &mut Tick) -> Result<()> {
        let rv = self.requests_vars();
        let rb = self.requests_broadcast();
        self.base_mut().init_impl(ctx, rv, rb)
    }

    fn tick(&mut self) -> Result<bool> {
        let rv = self.requests_vars();
        self.base_mut().tick_impl(rv)
    }

    fn subtick(&mut self, idx: i32) -> Result<bool> {
        let rv = self.requests_vars();
        let ri = self.requests_interpolation();
        self.base_mut().subtick_impl(idx, rv, ri)
    }

    fn finalize(&mut self) -> Result<()> {
        let rv = self.requests_vars();
        self.base_mut().finalize_impl(rv)
    }

    /// 查询光标命中的 hotspot 行为；仅动画驱动动作需要实现。
    fn hotspot_probe(&mut self, _cursor: Vec2) -> String {
        String::new()
    }

    /// 动作链的下一环（引用目标或序列当前子动作），供 hotspot 穿透查询。
    fn chain_next(&self) -> Option<SharedAction> {
        None
    }
}
