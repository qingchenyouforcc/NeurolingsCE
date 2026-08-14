//! 位移动作：立即按整数偏移量移动锚点并结束。

use crate::error::Result;

use super::instant::Instant;
use super::{Action, ActionBase};

#[derive(Default)]
pub struct Offset {
    instant: Instant,
}

impl Action for Offset {
    fn base(&self) -> &ActionBase {
        &self.instant.base
    }
    fn base_mut(&mut self) -> &mut ActionBase {
        &mut self.instant.base
    }
    fn tick(&mut self) -> Result<bool> {
        let dx = self.base().vars.get_num("X", 0.0) as i64;
        let dy = self.base().vars.get_num("Y", 0.0) as i64;
        if let Some(mascot) = &self.base().mascot {
            let mut m = mascot.borrow_mut();
            m.anchor.x += dx as f64;
            m.anchor.y += dy as f64;
        }
        Ok(false)
    }
}
