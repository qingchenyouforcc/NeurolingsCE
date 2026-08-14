//! 动作引用：把自身属性叠加到帧上下文后，转发给被引用的目标动作。

use crate::error::Result;
use crate::tick::Tick;

use super::{Action, ActionBase, SharedAction};

#[derive(Default)]
pub struct Reference {
    base: ActionBase,
    pub target: Option<SharedAction>,
}

impl Reference {
    pub fn target(&self) -> Option<SharedAction> {
        self.target.clone()
    }
}

impl Action for Reference {
    fn base(&self) -> &ActionBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut ActionBase {
        &mut self.base
    }
    fn requests_vars(&self) -> bool {
        false
    }
    fn requests_interpolation(&self) -> bool {
        false
    }

    fn init(&mut self, ctx: &mut Tick) -> Result<()> {
        self.base.init_impl(ctx, false, false)?;
        let mut target_ctx = ctx.overlay(self.base.init_attr.clone());
        let target = self.target.clone().expect("reference linked by parser");
        if let Err(err) = target.borrow_mut().init(&mut target_ctx) {
            let _ = self.base.finalize_impl(false);
            return Err(err);
        }
        Ok(())
    }

    fn subtick(&mut self, idx: i32) -> Result<bool> {
        if !self.base.subtick_impl(idx, false, false)? {
            return Ok(false);
        }
        let target = self.target.clone().expect("reference linked by parser");
        target.borrow_mut().subtick(idx)
    }

    fn finalize(&mut self) -> Result<()> {
        self.base.finalize_impl(false)?;
        let target = self.target.clone().expect("reference linked by parser");
        target.borrow_mut().finalize()
    }

    fn chain_next(&self) -> Option<SharedAction> {
        self.target.clone()
    }
}
