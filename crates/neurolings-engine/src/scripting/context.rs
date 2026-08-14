//! 脚本上下文：基于 QuickJS 的行为条件与动作变量求值引擎。
//!
//! # 架构
//!
//! 求值采用"快照 + 回写"模型：每次求值前把桌宠状态生成为 `mascot` 全局对象，
//! 求值结束后把可写字段（anchor、lookRight、activeBehavior）同步回 Rust 状态。
//! 单次求值内读取一致，这正是行为条件所依赖的全部语义。
//!
//! # 作用域隔离
//!
//! 每个动作实例与每个行为管理器都持有独立的变量作用域（[`ScopeHandle`]），
//! 存放于 JS 侧的 `globalThis.__scopes` 中。求值时通过 `with` 链把当前
//! 活动作用域注入名字解析：同名变量互不串扰，嵌套动作各自独立，
//! 多只桌宠共享同一上下文也不会互相污染。
//!
//! # 常量
//!
//! 桌宠包 `<Constant>` 定义的常量以惰性 getter 注入名字解析链，
//! 每次访问时重新求值，且优先级高于作用域变量。
//!
//! 名字解析顺序：常量 → 活动作用域 → 全局（`mascot`、`Math` 等）。

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use rquickjs::context::EvalOptions;
use rquickjs::{Context, Runtime};

use crate::environment::{Area, DArea, HBorder};
use crate::state::SharedState;

/// 非严格模式求值选项：桌宠脚本依赖 `with` 语句实现作用域链，必须关闭严格模式。
fn sloppy() -> EvalOptions {
    let mut options = EvalOptions::default();
    options.strict = false;
    options
}

/// 单次脚本求值的超时上限，防止恶意或损坏的桌宠包脚本卡死引擎。
const EVAL_TIMEOUT: Duration = Duration::from_millis(100);

/// 把浮点数格式化为合法的 JS 数字字面量。
fn js_num(v: f64) -> String {
    if v.is_finite() {
        format!("{v:?}")
    } else if v.is_nan() {
        "NaN".to_string()
    } else if v > 0.0 {
        "Infinity".to_string()
    } else {
        "-Infinity".to_string()
    }
}

/// 把任意字符串转义为带双引号的 JS 字符串字面量。
fn js_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn hborder_js(b: &HBorder) -> String {
    format!(
        "{{isOn:function(p){{return Math.abs(p.y-{y})<1.0&&p.x>={a}&&p.x<={bnd};}},\
         faces:function(p){{return p.x>={a}&&p.x<={bnd};}}}}",
        y = js_num(b.y),
        a = js_num(b.xstart),
        bnd = js_num(b.xend),
    )
}

fn vborder_js(x: f64, ystart: f64, yend: f64) -> String {
    format!(
        "{{isOn:function(p){{return Math.abs(p.x-{x})<1.0&&p.y>={a}&&p.y<={b};}},\
         faces:function(p){{return p.y>={a}&&p.y<={b};}}}}",
        x = js_num(x),
        a = js_num(ystart),
        b = js_num(yend),
    )
}

fn area_js(area: &Area) -> String {
    let tb = area.top_border();
    let bb = area.bottom_border();
    let lb = area.left_border();
    let rb = area.right_border();
    format!(
        "{{topBorder:{tb},bottomBorder:{bb},leftBorder:{lb},rightBorder:{rb},\
         width:{w},height:{h},visible:{vis},left:{l},right:{r},top:{t},bottom:{btm}}}",
        tb = hborder_js(&tb),
        bb = hborder_js(&bb),
        lb = vborder_js(lb.x, lb.ystart, lb.yend),
        rb = vborder_js(rb.x, rb.ystart, rb.yend),
        w = js_num(area.width()),
        h = js_num(area.height()),
        vis = area.visible(),
        l = js_num(area.left),
        r = js_num(area.right),
        t = js_num(area.top),
        btm = js_num(area.bottom),
    )
}

fn darea_js(area: &DArea) -> String {
    area_js(&area.area)
}

fn vb_on(x: f64, ystart: f64, yend: f64) -> String {
    format!(
        "(Math.abs(p.x-{x})<1.0&&p.y>={a}&&p.y<={b})",
        x = js_num(x),
        a = js_num(ystart),
        b = js_num(yend)
    )
}

fn vb_faces(ystart: f64, yend: f64) -> String {
    format!("(p.y>={a}&&p.y<={b})", a = js_num(ystart), b = js_num(yend))
}

/// 变量作用域句柄。持有期间作用域在 JS 侧存活，释放时整个作用域随之销毁。
pub struct ScopeHandle {
    ctx: Rc<ScriptContext>,
    id: u64,
}

impl ScopeHandle {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn context(&self) -> &Rc<ScriptContext> {
        &self.ctx
    }
}

impl Drop for ScopeHandle {
    fn drop(&mut self) {
        self.ctx.destroy_scope(self.id);
    }
}

/// 活动作用域守卫：构造时切换当前作用域，析构时恢复先前作用域。
pub struct ScopeGuard<'a> {
    ctx: &'a ScriptContext,
    prev: u64,
}

impl Drop for ScopeGuard<'_> {
    fn drop(&mut self) {
        self.ctx.active_scope.set(self.prev);
    }
}

pub struct ScriptContext {
    _runtime: Runtime,
    context: Context,
    pub state: RefCell<Option<SharedState>>,
    deadline: Rc<Cell<Option<Instant>>>,
    scope_counter: Cell<u64>,
    /// 当前活动作用域编号；0 表示无活动作用域。
    active_scope: Cell<u64>,
}

impl ScriptContext {
    pub fn new() -> Rc<Self> {
        let runtime = Runtime::new().expect("创建 QuickJS 运行时失败");
        let deadline = Rc::new(Cell::new(None::<Instant>));
        {
            let deadline = deadline.clone();
            runtime.set_interrupt_handler(Some(Box::new(move || {
                deadline.get().is_some_and(|d| Instant::now() >= d)
            })));
        }
        let context = Context::full(&runtime).expect("创建 QuickJS 上下文失败");
        context.with(|ctx| {
            // 作用域容器；console 输出直接丢弃；Math.random 用确定性序列，
            // 并支持 `Math.random * x` 写法（部分桌宠包依赖 valueOf 求值）。
            ctx.eval_with_options::<(), _>(
                "globalThis.__scopes=Object.create(null);\
                 var console={log:function(){},error:function(){}};\
                 (function(){var s=123456789;\
                 var r=function(){s=(s*1103515245+12345)>>>0;return (s>>>8)/16777216;};\
                 r.valueOf=r;Math.random=r;})();",
                sloppy(),
            )
            .ok();
        });
        Rc::new(Self {
            _runtime: runtime,
            context,
            state: RefCell::new(None),
            deadline,
            scope_counter: Cell::new(0),
            active_scope: Cell::new(0),
        })
    }

    pub fn set_state(&self, state: SharedState) {
        *self.state.borrow_mut() = Some(state);
    }

    fn with_ctx<R>(&self, f: impl FnOnce(rquickjs::Ctx) -> R) -> R {
        self.context.with(f)
    }

    fn raw_eval(&self, src: String) {
        self.with_ctx(|ctx| {
            ctx.eval_with_options::<(), _>(src, sloppy()).ok();
        });
    }

    fn begin_eval(&self) {
        self.deadline.set(Some(Instant::now() + EVAL_TIMEOUT));
    }

    fn end_eval(&self) {
        self.deadline.set(None);
    }

    // ---- 作用域管理 ----

    /// 创建一个新的隔离作用域。
    pub fn make_scope(self: &Rc<Self>) -> ScopeHandle {
        let id = self.scope_counter.get() + 1;
        self.scope_counter.set(id);
        self.raw_eval(format!("globalThis.__scopes[{id}]=Object.create(null);"));
        ScopeHandle {
            ctx: self.clone(),
            id,
        }
    }

    fn destroy_scope(&self, id: u64) {
        self.raw_eval(format!("delete globalThis.__scopes[{id}];"));
    }

    /// 把指定作用域设为活动作用域，返回守卫；守卫析构时恢复。
    pub fn enter_scope(&self, id: u64) -> ScopeGuard<'_> {
        let prev = self.active_scope.get();
        self.active_scope.set(id);
        ScopeGuard { ctx: self, prev }
    }

    // ---- 作用域变量读写 ----

    pub fn set_scope_num(&self, scope: u64, key: &str, value: f64) {
        self.raw_eval(format!(
            "globalThis.__scopes[{scope}][{k}]={v};",
            k = js_str(key),
            v = js_num(value)
        ));
    }

    pub fn set_scope_str(&self, scope: u64, key: &str, value: &str) {
        self.raw_eval(format!(
            "globalThis.__scopes[{scope}][{k}]={v};",
            k = js_str(key),
            v = js_str(value)
        ));
    }

    /// 在指定作用域内求值表达式，并把结果存入该作用域。
    pub fn set_scope_eval(&self, scope: u64, key: &str, js: &str) {
        self.install_mascot();
        self.begin_eval();
        {
            let _guard = self.enter_scope(scope);
            let inner = self.wrap_source(js);
            self.raw_eval(format!(
                "globalThis.__scopes[{scope}][{k}]={inner};",
                k = js_str(key)
            ));
        }
        self.end_eval();
        self.sync_back();
    }

    pub fn get_scope_num(&self, scope: u64, key: &str, fallback: f64) -> f64 {
        let src = format!(
            "(function(){{var v=globalThis.__scopes[{scope}][{k}];\
             return typeof v===\"number\"?v:null;}})()",
            k = js_str(key)
        );
        self.with_ctx(|ctx| {
            ctx.eval_with_options::<Option<f64>, _>(src, sloppy())
                .ok()
                .flatten()
                .unwrap_or(fallback)
        })
    }

    pub fn get_scope_bool(&self, scope: u64, key: &str, fallback: bool) -> bool {
        let src = format!(
            "(function(){{var v=globalThis.__scopes[{scope}][{k}];\
             return typeof v===\"boolean\"?v:null;}})()",
            k = js_str(key)
        );
        self.with_ctx(|ctx| {
            ctx.eval_with_options::<Option<bool>, _>(src, sloppy())
                .ok()
                .flatten()
                .unwrap_or(fallback)
        })
    }

    pub fn get_scope_string(&self, scope: u64, key: &str, fallback: &str) -> String {
        let src = format!(
            "(function(){{var v=globalThis.__scopes[{scope}][{k}];\
             return typeof v===\"string\"?v:null;}})()",
            k = js_str(key)
        );
        self.with_ctx(|ctx| {
            ctx.eval_with_options::<Option<String>, _>(src, sloppy())
                .ok()
                .flatten()
                .unwrap_or_else(|| fallback.to_string())
        })
    }

    // ---- 求值 ----

    /// 生成常量注入对象：每个常量是惰性 getter，访问时重新求值。
    fn constants_js(&self) -> String {
        let Some(state_rc) = self.state.borrow().clone() else {
            return "{}".to_string();
        };
        let state = state_rc.borrow();
        if state.constants.is_empty() {
            return "{}".to_string();
        }
        let mut out = String::from("{");
        for (key, value) in &state.constants {
            out.push_str(&format!(
                "get [{k}](){{return eval({v});}},",
                k = js_str(key),
                v = js_str(value)
            ));
        }
        out.push('}');
        out
    }

    /// 把用户脚本包进名字解析链：常量 → 活动作用域 → 全局。
    fn wrap_source(&self, js: &str) -> String {
        let consts = self.constants_js();
        let body = js_str(js);
        let scope = self.active_scope.get();
        if scope != 0 {
            format!(
                "(function(){{with(globalThis.__scopes[{scope}]){{\
                 with({consts}){{return eval({body});}}}}}})()"
            )
        } else {
            format!("(function(){{with({consts}){{return eval({body});}}}})()")
        }
    }

    pub fn eval_bool(&self, js: &str) -> bool {
        self.install_mascot();
        self.begin_eval();
        let src = format!("!!{}", self.wrap_source(js));
        let result = self.with_ctx(|ctx| {
            ctx.eval_with_options::<bool, _>(src, sloppy())
                .unwrap_or(false)
        });
        self.end_eval();
        self.sync_back();
        result
    }

    /// 带自定义超时的布尔求值（selector 等外部表达式用）。
    pub fn eval_bool_timed(&self, js: &str, timeout: Duration) -> bool {
        self.install_mascot();
        self.deadline.set(Some(Instant::now() + timeout));
        let src = format!("!!{}", self.wrap_source(js));
        let result = self.with_ctx(|ctx| {
            ctx.eval_with_options::<bool, _>(src, sloppy())
                .unwrap_or(false)
        });
        self.end_eval();
        self.sync_back();
        result
    }

    pub fn eval_number(&self, js: &str) -> f64 {
        self.install_mascot();
        self.begin_eval();
        let src = format!("Number({})", self.wrap_source(js));
        let result = self.with_ctx(|ctx| {
            ctx.eval_with_options::<f64, _>(src, sloppy())
                .unwrap_or(f64::NAN)
        });
        self.end_eval();
        self.sync_back();
        result
    }

    pub fn eval_string(&self, js: &str) -> String {
        self.install_mascot();
        self.begin_eval();
        let src = format!("String({})", self.wrap_source(js));
        let result = self.with_ctx(|ctx| {
            ctx.eval_with_options::<String, _>(src, sloppy())
                .unwrap_or_default()
        });
        self.end_eval();
        self.sync_back();
        result
    }

    pub fn eval(&self, js: &str) {
        self.install_mascot();
        self.begin_eval();
        let src = self.wrap_source(js);
        self.raw_eval(src);
        self.end_eval();
        self.sync_back();
    }

    /// 导出 `JSON.stringify(mascot)`，用于状态检查器。
    pub fn export_state_json(&self) -> String {
        self.eval_string("JSON.stringify(mascot)")
    }

    /// 行为抽选使用环境的随机数发生器，保证可播种复现。
    pub fn random_int(&self, upper_range: i32) -> i32 {
        if let Some(state_rc) = self.state.borrow().clone() {
            let state = state_rc.borrow();
            if let Some(env) = &state.env {
                return env.borrow_mut().random_int(upper_range);
            }
        }
        0
    }

    // ---- 状态快照与回写 ----

    /// 把当前桌宠状态生成为 JS 侧的 `mascot` 全局对象。
    fn install_mascot(&self) {
        let Some(state_rc) = self.state.borrow().clone() else {
            return;
        };
        let state = state_rc.borrow();
        let Some(env_rc) = state.env.clone() else {
            return;
        };
        let env = env_rc.borrow();

        let active_behavior = if !state.queued_behavior.is_empty() {
            js_str(&state.queued_behavior)
        } else if let Some(b) = &state.behavior {
            js_str(&b.name)
        } else {
            "undefined".to_string()
        };

        let cursor = state.get_cursor();
        let wa = &env.work_area;
        let ie = &env.active_ie;
        let wall = format!(
            "{{isOn:function(p){{return {wl}||{wr}||{il}||{ir};}},\
             faces:function(p){{return {fwl}||{fwr}||{fil}||{fir};}}}}",
            wl = vb_on(wa.left, wa.top, wa.bottom),
            wr = vb_on(wa.right, wa.top, wa.bottom),
            il = vb_on(ie.area.left, ie.area.top, ie.area.bottom),
            ir = vb_on(ie.area.right, ie.area.top, ie.area.bottom),
            fwl = vb_faces(wa.top, wa.bottom),
            fwr = vb_faces(wa.top, wa.bottom),
            fil = vb_faces(ie.area.top, ie.area.bottom),
            fir = vb_faces(ie.area.top, ie.area.bottom),
        );

        let script = format!(
            "globalThis.mascot={{\
             bounds:{{x:{bx},y:{by},width:{bw},height:{bh}}},\
             activeBehavior:{ab},\
             anchor:{{x:{ax},y:{ay}}},\
             lookRight:{lr},\
             totalCount:{tc},\
             environment:{{\
             floor:{floor},ceiling:{ceiling},wall:{wall},\
             workArea:{wa},screen:{screen},activeIE:{ie},\
             allowsWindowPushing:{awp},\
             cursor:{{x:{cx},y:{cy},dx:{cdx},dy:{cdy}}}\
             }}}};",
            bx = js_num(state.bounds.x),
            by = js_num(state.bounds.y),
            bw = js_num(state.bounds.width),
            bh = js_num(state.bounds.height),
            ab = active_behavior,
            ax = js_num(state.anchor.x),
            ay = js_num(state.anchor.y),
            lr = state.looking_right,
            tc = env.mascot_count,
            floor = hborder_js(&env.floor),
            ceiling = hborder_js(&env.ceiling),
            wall = wall,
            wa = area_js(wa),
            screen = area_js(&env.screen),
            ie = darea_js(ie),
            awp = env.allows_window_pushing,
            cx = js_num(cursor.x),
            cy = js_num(cursor.y),
            cdx = js_num(cursor.dx),
            cdy = js_num(cursor.dy),
        );
        drop(env);
        drop(state);
        self.raw_eval(script);
    }

    /// 求值结束后，把可写字段同步回 Rust 状态。
    fn sync_back(&self) {
        let Some(state_rc) = self.state.borrow().clone() else {
            return;
        };
        let (ax, ay, lr, ab) = self.with_ctx(|ctx| {
            let read = |expr: &str| {
                ctx.eval_with_options::<f64, _>(expr.to_string(), sloppy())
                    .ok()
            };
            let ax = read("mascot.anchor.x");
            let ay = read("mascot.anchor.y");
            let lr = ctx
                .eval_with_options::<bool, _>("mascot.lookRight", sloppy())
                .ok();
            let ab = ctx
                .eval_with_options::<String, _>(
                    "typeof mascot.activeBehavior==='string'?mascot.activeBehavior:''",
                    sloppy(),
                )
                .ok();
            (ax, ay, lr, ab)
        });
        let mut state = state_rc.borrow_mut();
        if let (Some(x), Some(y)) = (ax, ay) {
            state.anchor = crate::math::Vec2::new(x, y);
        }
        if let Some(lr) = lr {
            state.looking_right = lr;
        }
        if let Some(ab) = ab {
            let current = if !state.queued_behavior.is_empty() {
                state.queued_behavior.clone()
            } else if let Some(b) = &state.behavior {
                b.name.clone()
            } else {
                String::new()
            };
            if ab != current {
                state.queued_behavior = ab;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_bool_without_state() {
        let ctx = ScriptContext::new();
        assert!(ctx.eval_bool("1 < 2"));
        assert!(!ctx.eval_bool("1 > 2"));
    }

    #[test]
    fn scope_variables_are_isolated() {
        let ctx = ScriptContext::new();
        let a = ctx.make_scope();
        let b = ctx.make_scope();
        ctx.set_scope_num(a.id(), "X", 1.0);
        ctx.set_scope_num(b.id(), "X", 2.0);
        assert_eq!(ctx.get_scope_num(a.id(), "X", -1.0), 1.0);
        assert_eq!(ctx.get_scope_num(b.id(), "X", -1.0), 2.0);
        {
            let _g = ctx.enter_scope(a.id());
            assert!(ctx.eval_bool("X === 1"));
        }
        {
            let _g = ctx.enter_scope(b.id());
            assert!(ctx.eval_bool("X === 2"));
        }
        // 无活动作用域时变量不可见。
        assert!(ctx.eval_bool("typeof X === 'undefined'"));
    }

    #[test]
    fn scope_is_destroyed_on_drop() {
        let ctx = ScriptContext::new();
        let a = ctx.make_scope();
        let id = a.id();
        ctx.set_scope_num(id, "Y", 5.0);
        drop(a);
        assert_eq!(ctx.get_scope_num(id, "Y", -1.0), -1.0);
    }

    #[test]
    fn math_random_supports_valueof() {
        let ctx = ScriptContext::new();
        let n = ctx.eval_number("Math.random * 10");
        assert!((0.0..10.0).contains(&n), "valueOf 应支持算术运算: {n}");
    }
}
