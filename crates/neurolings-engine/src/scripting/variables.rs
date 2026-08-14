//! 动作作用域变量：来自 actions.xml 的属性绑定。
//!
//! 三种求值形式：静态值仅设置一次；`${...}` 在初始化时求值一次；
//! `#{...}` 每帧重新求值。所有变量都写入本动作独享的隔离作用域，
//! 动作结束时作用域整体销毁，不会污染其他动作或其他桌宠。

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::condition::Condition;
use super::context::{ScopeHandle, ScriptContext};

/// 识别 `${...}` / `#{...}` 包装，返回前缀字符。
fn dynamic_prefix(s: &str) -> Option<u8> {
    let b = s.as_bytes();
    if b.len() >= 3 && b[1] == b'{' && b[b.len() - 1] == b'}' && (b[0] == b'$' || b[0] == b'#') {
        Some(b[0])
    } else {
        None
    }
}

fn is_num(s: &str) -> bool {
    !s.is_empty() && s.parse::<f64>().is_ok()
}

#[derive(Default)]
pub struct Variables {
    /// 每帧重新求值的表达式（`#{...}` 形式），键为变量名。
    dynamic_attr: HashMap<String, String>,
    /// 本动作声明过的全部变量名。
    attr_keys: HashSet<String>,
    /// 本动作独享的变量作用域；None 表示尚未初始化。
    scope: Option<ScopeHandle>,
}

impl Variables {
    pub fn new() -> Self {
        Self::default()
    }

    fn ctx(&self) -> Option<Rc<ScriptContext>> {
        self.scope.as_ref().map(|s| s.context().clone())
    }

    fn scope_id(&self) -> u64 {
        self.scope.as_ref().map_or(0, |s| s.id())
    }

    pub fn add_attr_nums(&mut self, attr: &HashMap<String, f64>) {
        let Some(ctx) = self.ctx() else { return };
        let scope = self.scope_id();
        for (key, val) in attr {
            self.dynamic_attr.remove(key);
            self.attr_keys.insert(key.clone());
            ctx.set_scope_num(scope, key, *val);
        }
    }

    pub fn add_attr(&mut self, attr: &HashMap<String, String>) {
        let Some(ctx) = self.ctx() else { return };
        let scope = self.scope_id();
        for (key, raw) in attr {
            let mut val = raw.clone();
            if val == "null" || val == "true" || val == "false" {
                val = format!("${{{val}}}");
            }
            self.dynamic_attr.remove(key);
            self.attr_keys.insert(key.clone());
            match dynamic_prefix(&val) {
                None => {
                    // 静态值：数字直接写入，否则按字符串写入。
                    if is_num(&val) {
                        ctx.set_scope_num(scope, key, val.parse().unwrap());
                    } else {
                        ctx.set_scope_str(scope, key, &val);
                    }
                }
                Some(b'$') => {
                    // 初始化时求值一次。
                    ctx.set_scope_eval(scope, key, &val[2..val.len() - 1]);
                }
                Some(b'#') => {
                    // 每帧重新求值。
                    self.dynamic_attr
                        .insert(key.clone(), val[2..val.len() - 1].to_string());
                }
                _ => unreachable!(),
            }
        }
    }

    pub fn init(&mut self, ctx: Rc<ScriptContext>, attr: &HashMap<String, String>) {
        self.scope = Some(ctx.make_scope());
        self.add_attr(attr);
    }

    pub fn tick(&mut self) {
        let Some(ctx) = self.ctx() else { return };
        let scope = self.scope_id();
        for (key, js) in &self.dynamic_attr {
            ctx.set_scope_eval(scope, key, js);
        }
    }

    pub fn finalize(&mut self) {
        self.dynamic_attr.clear();
        self.attr_keys.clear();
        // 释放句柄即销毁 JS 侧的整个作用域。
        self.scope = None;
    }

    pub fn get_num(&self, key: &str, fallback: f64) -> f64 {
        self.ctx().map_or(fallback, |c| {
            c.get_scope_num(self.scope_id(), key, fallback)
        })
    }

    pub fn get_bool(&self, key: &str, fallback: bool) -> bool {
        self.ctx().map_or(fallback, |c| {
            c.get_scope_bool(self.scope_id(), key, fallback)
        })
    }

    pub fn get_string(&self, key: &str, fallback: &str) -> String {
        self.ctx().map_or_else(
            || fallback.to_string(),
            |c| c.get_scope_string(self.scope_id(), key, fallback),
        )
    }

    /// 在本动作的作用域内求值条件。
    pub fn eval_condition(&self, cond: &Condition) -> bool {
        match self.ctx() {
            Some(ctx) => {
                let _guard = ctx.enter_scope(self.scope_id());
                cond.eval(&ctx)
            }
            None => false,
        }
    }

    pub fn has(&self, key: &str) -> bool {
        self.attr_keys.contains(key)
    }
}
