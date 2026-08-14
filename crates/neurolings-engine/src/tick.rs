//! 帧上下文：在一帧内跨动作传递脚本上下文与附加属性，并限制动作初始化次数。

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::scripting::context::ScriptContext;

/// 单帧内允许的动作初始化上限，超过则视为行为死循环。
pub const INIT_LIMIT: i32 = 20;

#[derive(Clone)]
pub struct Tick {
    pub script: Rc<ScriptContext>,
    pub extra_attr: HashMap<String, String>,
    init_count: Rc<Cell<i32>>,
}

impl Tick {
    pub fn new(script: Rc<ScriptContext>, extra_attr: HashMap<String, String>) -> Self {
        Self {
            script,
            extra_attr,
            init_count: Rc::new(Cell::new(0)),
        }
    }

    pub fn will_init(&self) {
        self.init_count.set(self.init_count.get() + 1);
    }

    pub fn reset(&self) {
        self.init_count.set(0);
    }

    pub fn reached_init_limit(&self) -> bool {
        self.init_count.get() >= INIT_LIMIT
    }

    pub fn overlay(&self, new_attr: HashMap<String, String>) -> Tick {
        let mut extra = self.extra_attr.clone();
        extra.extend(new_attr);
        Tick {
            script: self.script.clone(),
            extra_attr: extra,
            init_count: self.init_count.clone(),
        }
    }
}
