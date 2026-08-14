//! 行为与动画的触发条件：常量布尔值或内嵌 JS 表达式。

use super::context::ScriptContext;

/// 条件表达式：常量布尔值，或以 `${...}` / `#{...}` 书写的 JS 表达式。
#[derive(Debug, Clone)]
pub enum Condition {
    Constant(bool),
    Js(String),
}

impl Default for Condition {
    fn default() -> Self {
        Condition::Constant(true)
    }
}

impl From<bool> for Condition {
    fn from(value: bool) -> Self {
        Condition::Constant(value)
    }
}

impl From<&str> for Condition {
    fn from(s: &str) -> Self {
        let bytes = s.as_bytes();
        if s.len() > 3
            && (bytes[0] == b'$' || bytes[0] == b'#')
            && bytes[1] == b'{'
            && bytes[s.len() - 1] == b'}'
        {
            Condition::Js(s[2..s.len() - 1].to_string())
        } else {
            Condition::Constant(s == "true")
        }
    }
}

impl From<String> for Condition {
    fn from(s: String) -> Self {
        Condition::from(s.as_str())
    }
}

impl Condition {
    pub fn eval(&self, ctx: &ScriptContext) -> bool {
        match self {
            Condition::Constant(value) => *value,
            Condition::Js(js) => ctx.eval_bool(js),
        }
    }
}
