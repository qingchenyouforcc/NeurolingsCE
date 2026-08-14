//! 转向动作：立即设置朝向并结束；未指定 LookRight 时翻转当前朝向。

use crate::error::Result;

use super::instant::Instant;
use super::{Action, ActionBase};

#[derive(Default)]
pub struct Look {
    instant: Instant,
}

impl Action for Look {
    fn base(&self) -> &ActionBase {
        &self.instant.base
    }
    fn base_mut(&mut self) -> &mut ActionBase {
        &mut self.instant.base
    }
    fn tick(&mut self) -> Result<bool> {
        let current = self
            .base()
            .mascot
            .as_ref()
            .map(|m| m.borrow().looking_right);
        let value = self
            .base()
            .vars
            .get_bool("LookRight", current.unwrap_or(false));
        if let Some(mascot) = &self.base().mascot {
            mascot.borrow_mut().looking_right = value;
        }
        Ok(false)
    }
}
