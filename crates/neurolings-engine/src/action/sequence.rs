//! 顺序执行动作：依次执行子动作列表，可循环；select 变体随机/按条件选一个。
//!
//! select 通过 `select_mode` 标志实现：next_action() 的所有调用点
//! 都遵循"只执行一个子动作"的语义。

use std::collections::HashMap;
use std::rc::Rc;

use crate::error::Result;
use crate::scripting::context::ScriptContext;
use crate::tick::Tick;

use super::{Action, ActionBase, SharedAction};

pub struct Sequence {
    pub base: ActionBase,
    pub actions: Vec<SharedAction>,
    action_idx: i32,
    current: Option<SharedAction>,
    script_ctx: Option<Rc<ScriptContext>>,
    select_mode: bool,
    did_execute: bool,
}

impl Default for Sequence {
    fn default() -> Self {
        Self {
            base: ActionBase::default(),
            actions: Vec::new(),
            action_idx: -1,
            current: None,
            script_ctx: None,
            select_mode: false,
            did_execute: false,
        }
    }
}

impl Sequence {
    pub fn new_select() -> Self {
        Self {
            select_mode: true,
            ..Default::default()
        }
    }

    pub fn current_action(&self) -> Option<SharedAction> {
        self.current.clone()
    }

    fn next_action(&mut self) -> Result<Option<SharedAction>> {
        if self.select_mode && self.did_execute {
            if let Some(action) = self.current.take() {
                action.borrow_mut().finalize()?;
            }
            return Ok(None);
        }
        if self.action_idx >= self.actions.len() as i32 && !self.base.vars.get_bool("Loops", false)
        {
            return Ok(None);
        }
        if let Some(action) = self.current.take() {
            action.borrow_mut().finalize()?;
        }
        self.action_idx += 1;
        if self.action_idx >= self.actions.len() as i32 {
            if self.base.vars.get_bool("Loops", false) {
                self.action_idx = 0;
            } else {
                return Ok(None);
            }
        }
        let action = self.actions[self.action_idx as usize].clone();
        let script = self.script_ctx.clone().expect("script ctx set at init");
        let mut ctx = Tick::new(script, HashMap::new());
        action.borrow_mut().init(&mut ctx)?;
        self.current = Some(action.clone());
        if self.select_mode {
            self.did_execute = true;
        }
        Ok(Some(action))
    }
}

impl Action for Sequence {
    fn base(&self) -> &ActionBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut ActionBase {
        &mut self.base
    }
    fn requests_interpolation(&self) -> bool {
        false
    }

    fn init(&mut self, ctx: &mut Tick) -> Result<()> {
        // select 变体强制 Loops=false；注意叠加值带尾随空格，
        // 按非布尔值处理同样落到 false，必须原样保留。
        if self.select_mode {
            self.did_execute = false;
            let mut overlay =
                ctx.overlay(HashMap::from([("Loops".to_string(), "false ".to_string())]));
            let (rv, rb) = (self.requests_vars(), self.requests_broadcast());
            self.base.init_impl(&mut overlay, rv, rb)?;
            self.script_ctx = Some(overlay.script.clone());
            self.action_idx = -1;
            self.next_action()?;
            return Ok(());
        }
        let (rv, rb) = (self.requests_vars(), self.requests_broadcast());
        self.base.init_impl(ctx, rv, rb)?;
        self.script_ctx = Some(ctx.script.clone());
        self.action_idx = -1;
        self.next_action()?;
        Ok(())
    }

    fn subtick(&mut self, idx: i32) -> Result<bool> {
        if idx == 0 && !self.base.tick_impl(self.requests_vars())? {
            return Ok(false);
        }
        if self.current.is_none() {
            return Ok(false);
        }
        loop {
            let Some(action) = self.current.clone() else {
                break;
            };
            let child_ok = action.borrow_mut().subtick(idx)?;
            let queued_empty = self
                .base
                .mascot
                .as_ref()
                .is_none_or(|m| m.borrow().queued_behavior.is_empty());
            if child_ok || idx != 0 || !queued_empty {
                break;
            }
            if self.next_action()?.is_none() {
                break;
            }
        }
        Ok(self.current.is_some())
    }

    fn finalize(&mut self) -> Result<()> {
        if let Some(action) = self.current.take() {
            action.borrow_mut().finalize()?;
        }
        self.script_ctx = None;
        self.base.finalize_impl(true)
    }

    fn chain_next(&self) -> Option<SharedAction> {
        self.current.clone()
    }
}
