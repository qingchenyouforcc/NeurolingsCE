//! 选择动作：以 select 模式运行的顺序容器（只执行一个子动作）。

use super::sequence::Sequence;
use super::{Action, ActionBase};

pub struct Select {
    seq: Sequence,
}

impl Default for Select {
    fn default() -> Self {
        Self::new()
    }
}

impl Select {
    pub fn new() -> Self {
        Self {
            seq: Sequence::new_select(),
        }
    }
    pub fn sequence_mut(&mut self) -> &mut Sequence {
        &mut self.seq
    }
}

impl Action for Select {
    fn base(&self) -> &ActionBase {
        &self.seq.base
    }
    fn base_mut(&mut self) -> &mut ActionBase {
        &mut self.seq.base
    }
    fn requests_interpolation(&self) -> bool {
        false
    }
    fn init(&mut self, ctx: &mut crate::tick::Tick) -> crate::error::Result<()> {
        self.seq.init(ctx)
    }
    fn subtick(&mut self, idx: i32) -> crate::error::Result<bool> {
        self.seq.subtick(idx)
    }
    fn finalize(&mut self) -> crate::error::Result<()> {
        self.seq.finalize()
    }

    fn chain_next(&self) -> Option<crate::action::SharedAction> {
        self.seq.chain_next()
    }
}
