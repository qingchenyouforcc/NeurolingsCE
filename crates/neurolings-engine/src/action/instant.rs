//! 瞬时动作的基座：offset/look 等只改属性、无动画的动作共用。

use super::{Action, ActionBase};

#[derive(Default)]
pub struct Instant {
    pub base: ActionBase,
}

impl Action for Instant {
    fn base(&self) -> &ActionBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut ActionBase {
        &mut self.base
    }
}
