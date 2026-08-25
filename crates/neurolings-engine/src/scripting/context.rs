//! 脚本上下文：基于 QuickJS 的行为条件与动作变量求值引擎。
//!
//! # 架构
//!
//! 求值采用"快照 + 回写"模型：每次求值前把桌宠状态生成为 `mascot` 全局对象，
//! 求值结束后把可写字段同步回 Rust 状态。可写范围遵循 C++ setter 的既定契约：
//! anchor、lookRight、activeBehavior、bounds（x/y/width/height），以及
//! environment 的 workArea/screen/activeIE 三个区域的 left/right/top/bottom。
//! 单次求值内读取一致，这正是行为条件所依赖的全部语义。
//!
//! # 作用域隔离
//!
//! 每个动作实例与每个行为管理器都持有独立的变量作用域（[`ScopeHandle`]），
//! 存放于 JS 侧的 `globalThis.__scopes` 中。求值时通过 `with` 链把当前
//! 活动作用域注入名字解析：同名变量互不串扰，嵌套动作各自独立，
//! 多只桌宠共享同一上下文也不会互相污染。
//!
//! 脚本中的新变量赋值（含 `var` 声明）按 C++ proxy set 的既定语义落入当前
//! 活动作用域：求值前快照 `globalThis` 的自有属性名，求值结束后（含出错
//! 路径）把新增属性转移到活动作用域并从 `globalThis` 删除。无活动作用域时
//! 使用兜底的 0 号作用域，对齐 C++ context 构造函数创建的 initial global。
//!
//! # 常量
//!
//! 桌宠包 `<Constant>` 定义的常量以惰性 getter 注入名字解析链，
//! 每次访问时重新求值，且优先级高于作用域变量。
//!
//! 名字解析顺序：常量 → 活动作用域 → 全局（`mascot`、`Math` 等）。
//!
//! # 超时与错误
//!
//! 所有桌宠包脚本入口均使用 100ms 内部执行预算；selector 等外部表达式
//! 通过 [`ScriptContext::eval_bool_timed`] 使用调用方提供的独立预算。
//! 求值失败不杀桌宠，但会按表达式去重后输出一次警告日志，避免逐帧刷屏。

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::time::{Duration, Instant};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rquickjs::context::EvalOptions;
use rquickjs::{CatchResultExt, Context, Runtime};

use crate::environment::{Area, DArea, HBorder};
use crate::state::SharedState;

thread_local! {
    /// 无环境上下文（测试/独立求值）时的兜底随机流。
    static FALLBACK_RNG: RefCell<Option<StdRng>> = const { RefCell::new(None) };
}

/// 从当前环境的 RNG 取 [0,1) 随机数——与原版一致，
/// Math.random 与行为选择共用同一条真随机流；无环境时用系统熵兜底。
fn rust_random_value(state: &Rc<RefCell<Option<SharedState>>>) -> f64 {
    if let Some(state_rc) = state.borrow().clone() {
        let s = state_rc.borrow();
        if let Some(env) = &s.env {
            return env.borrow_mut().random();
        }
    }
    FALLBACK_RNG.with(|cell| {
        let mut guard = cell.borrow_mut();
        guard.get_or_insert_with(StdRng::from_os_rng).random()
    })
}

/// 非严格模式求值选项：桌宠脚本依赖 `with` 语句实现作用域链，必须关闭严格模式。
fn sloppy() -> EvalOptions {
    let mut options = EvalOptions::default();
    options.strict = false;
    options
}

/// 桌宠包脚本的单次执行上限，避免条件或变量表达式阻塞运行时主循环。
const INTERNAL_EVAL_TIMEOUT: Duration = Duration::from_millis(100);

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
        "{{isOn:function(p){{return Math.abs(p.y-({y}))<1.0&&p.x>={a}&&p.x<={bnd};}},\
         faces:function(p){{return p.x>={a}&&p.x<={bnd};}}}}",
        y = js_num(b.y),
        a = js_num(b.xstart),
        bnd = js_num(b.xend),
    )
}

fn vborder_js(x: f64, ystart: f64, yend: f64) -> String {
    format!(
        "{{isOn:function(p){{return Math.abs(p.x-({x}))<1.0&&p.y>={a}&&p.y<={b};}},\
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
        "(Math.abs(p.x-({x}))<1.0&&p.y>={a}&&p.y<={b})",
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

struct DeadlineGuard<'a> {
    deadline: &'a Cell<Option<Instant>>,
    previous: Option<Instant>,
}

impl Drop for DeadlineGuard<'_> {
    fn drop(&mut self) {
        self.deadline.set(self.previous);
    }
}

pub struct ScriptContext {
    _runtime: Runtime,
    context: Context,
    pub state: Rc<RefCell<Option<SharedState>>>,
    deadline: Rc<Cell<Option<Instant>>>,
    scope_counter: Cell<u64>,
    /// 当前活动作用域编号；0 表示无活动作用域（使用兜底的初始全局）。
    active_scope: Cell<u64>,
    /// 已报告过的求值错误（按规范化后的表达式去重），用于日志限频。
    reported_errors: RefCell<HashSet<String>>,
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
        let state: Rc<RefCell<Option<SharedState>>> = Rc::new(RefCell::new(None));
        {
            // Math.random 走环境 RNG（真随机、与行为选择同源），
            // 并支持 `Math.random * x` 写法（部分桌宠包依赖 valueOf 求值）。
            let state_for_rng = state.clone();
            context.with(|ctx| {
                let rust_random = rquickjs::Function::new(ctx.clone(), move || -> f64 {
                    rust_random_value(&state_for_rng)
                })
                .expect("注册随机函数失败");
                ctx.globals()
                    .set("__rustRandom", rust_random)
                    .expect("安装随机函数失败");
                ctx.eval_with_options::<(), _>(
                    "globalThis.__scopes=Object.create(null);\
                     globalThis.__scopes[0]=Object.create(null);\
                     var console={log:function(){},error:function(){}};\
                     (function(){var r=function(){return __rustRandom();};\
                     r.valueOf=r;Math.random=r;})();\
                     globalThis.__snapshotGlobals=function(){\
                     var names=Object.getOwnPropertyNames(globalThis);\
                     var snap=Object.create(null);\
                     for(var i=0;i<names.length;i++)snap[names[i]]=true;\
                     return snap;};\
                     globalThis.__sweepGlobals=function(snap,scope){\
                     if(scope==null)return;\
                     var names=Object.getOwnPropertyNames(globalThis);\
                     for(var i=0;i<names.length;i++){var k=names[i];\
                     if(!(k in snap)){\
                     try{scope[k]=globalThis[k];}catch(e){}\
                     try{delete globalThis[k];}catch(e){}\
                     }}};\
                     globalThis.__addAccessorFns=function(obj,names){\
                     for(var i=0;i<names.length;i++){(function(p){\
                     var s=p.charAt(0).toUpperCase()+p.slice(1);\
                     obj[\"get\"+s]=function(){return this[p];};\
                     obj[\"set\"+s]=function(v){this[p]=v;};\
                     })(names[i]);}};\
                     globalThis.__addAccessorFns(console,[\"log\",\"error\"]);",
                    sloppy(),
                )
                .ok();
            });
        }
        Rc::new(Self {
            _runtime: runtime,
            context,
            state,
            deadline,
            scope_counter: Cell::new(0),
            active_scope: Cell::new(0),
            reported_errors: RefCell::new(HashSet::new()),
        })
    }

    pub fn set_state(&self, state: SharedState) {
        *self.state.borrow_mut() = Some(state);
    }

    fn with_ctx<R>(&self, f: impl FnOnce(rquickjs::Ctx) -> R) -> R {
        self.context.with(f)
    }

    fn enter_deadline(&self, timeout: Duration) -> DeadlineGuard<'_> {
        let previous = self.deadline.replace(Some(Instant::now() + timeout));
        DeadlineGuard {
            deadline: &self.deadline,
            previous,
        }
    }

    fn raw_eval(&self, src: String) {
        self.with_ctx(|ctx| {
            ctx.eval_with_options::<(), _>(src, sloppy()).ok();
        });
    }

    /// 脚本求值失败时输出警告日志；同一表达式（规范化后）只报告一次，
    /// 防止每帧执行的 `#{...}` 变量或条件在持续出错时刷屏。
    fn report_eval_error(&self, expr: &str, err: &impl std::fmt::Display) {
        let mut summary: String = expr.replace(['\r', '\t', '\n'], " ");
        const MAX_SUMMARY: usize = 160;
        if summary.chars().count() > MAX_SUMMARY {
            summary = summary.chars().take(MAX_SUMMARY).collect::<String>() + "...";
        }
        if self.reported_errors.borrow_mut().insert(summary.clone()) {
            eprintln!("[neurolings-engine] warning: script eval failed: {summary}: {err}");
        }
    }

    #[cfg(test)]
    pub(crate) fn reported_error_count(&self) -> usize {
        self.reported_errors.borrow().len()
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
        {
            let _deadline = self.enter_deadline(INTERNAL_EVAL_TIMEOUT);
            let _guard = self.enter_scope(scope);
            let inner = self.wrap_source(js);
            let src = format!(
                "globalThis.__scopes[{scope}][{k}]={inner};",
                k = js_str(key)
            );
            self.with_ctx(|ctx| {
                if let Err(err) = ctx.eval_with_options::<(), _>(src, sloppy()).catch(&ctx) {
                    self.report_eval_error(js, &err);
                }
            });
        }
        self.sync_back();
    }

    /// 求值常量表达式并按类型取出结果，对齐 C++ proxy get 的"常量优先"语义。
    /// 返回 None 表示常量不存在，或其求值结果为 null/undefined
    /// （此时应落回作用域变量，与 context.cc 的 get 回调一致）。
    fn eval_constant(&self, scope: u64, key: &str) -> Option<serde_json::Value> {
        let expr = {
            let state_rc = self.state.borrow().clone()?;
            state_rc.borrow().constants.get(key).cloned()
        }?;
        self.install_mascot();
        let out = {
            let _deadline = self.enter_deadline(INTERNAL_EVAL_TIMEOUT);
            let _guard = self.enter_scope(scope);
            let src = format!(
                "(function(){{var v={};\
                 return JSON.stringify({{\
                 isNull:v===null||v===undefined,\
                 num:typeof v===\"number\"?v:null,\
                 bool:typeof v===\"boolean\"?v:null,\
                 str:typeof v===\"string\"?v:null}});}})()",
                self.wrap_source(&expr)
            );
            self.with_ctx(|ctx| {
                match ctx
                    .eval_with_options::<String, _>(src, sloppy())
                    .catch(&ctx)
                {
                    Ok(s) => Some(s),
                    Err(err) => {
                        self.report_eval_error(&expr, &err);
                        None
                    }
                }
            })
        };
        self.sync_back();
        let parsed: serde_json::Value = serde_json::from_str(&out?).ok()?;
        // 注意：JSON 无法表示 NaN/Infinity，它们会以 null 出现并按类型不符处理；
        // 常量表达式产生 NaN 属于病态输入，可接受。
        if parsed["isNull"].as_bool().unwrap_or(true) {
            None
        } else {
            Some(parsed)
        }
    }

    pub fn get_scope_num(&self, scope: u64, key: &str, fallback: f64) -> f64 {
        if let Some(v) = self.eval_constant(scope, key) {
            // 常量命中但类型不符时使用回退值（与 C++ 一致，不落回作用域变量）。
            return v["num"].as_f64().unwrap_or(fallback);
        }
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
        if let Some(v) = self.eval_constant(scope, key) {
            return v["bool"].as_bool().unwrap_or(fallback);
        }
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
        if let Some(v) = self.eval_constant(scope, key) {
            return v["str"]
                .as_str()
                .map_or_else(|| fallback.to_string(), str::to_string);
        }
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
    ///
    /// `with` 链写在待 eval 的源码内部，并用间接 eval `(0,eval)` 让脚本在
    /// 全局作用域中求值：这样脚本里的新赋值与 `var`/函数声明都会成为
    /// `globalThis` 的新属性，再由快照/清扫逻辑转移到当前活动作用域——
    /// 语义上将全局对象替换为 Proxy，并将 set/defineProperty 全部转发到
    /// `_activeGlobal` 的行为。
    fn wrap_source(&self, js: &str) -> String {
        let consts = self.constants_js();
        let scope = self.active_scope.get();
        let inner = format!("with(globalThis.__scopes[{scope}]) with({consts}) {{\n{js}\n}}");
        format!(
            "(function(){{\
             var __snap=globalThis.__snapshotGlobals();\
             try{{return (0,eval)({inner});}}\
             finally{{globalThis.__sweepGlobals(__snap,globalThis.__scopes[{scope}]);}}\
             }})()",
            inner = js_str(&inner)
        )
    }

    pub fn eval_bool(&self, js: &str) -> bool {
        self.install_mascot();
        let src = format!("!!{}", self.wrap_source(js));
        let result = {
            let _deadline = self.enter_deadline(INTERNAL_EVAL_TIMEOUT);
            self.with_ctx(
                |ctx| match ctx.eval_with_options::<bool, _>(src, sloppy()).catch(&ctx) {
                    Ok(v) => v,
                    Err(err) => {
                        self.report_eval_error(js, &err);
                        false
                    }
                },
            )
        };
        self.sync_back();
        result
    }

    /// 带自定义超时的布尔求值（selector 等外部表达式用）。
    ///
    /// selector 使用调用方给定的更严格预算；其余脚本入口使用统一内部预算。
    pub fn eval_bool_timed(&self, js: &str, timeout: Duration) -> bool {
        self.install_mascot();
        let src = format!("!!{}", self.wrap_source(js));
        let result = {
            let _deadline = self.enter_deadline(timeout);
            self.with_ctx(
                |ctx| match ctx.eval_with_options::<bool, _>(src, sloppy()).catch(&ctx) {
                    Ok(v) => v,
                    Err(err) => {
                        self.report_eval_error(js, &err);
                        false
                    }
                },
            )
        };
        self.sync_back();
        result
    }

    pub fn eval_number(&self, js: &str) -> f64 {
        self.install_mascot();
        let src = format!("Number({})", self.wrap_source(js));
        let result = {
            let _deadline = self.enter_deadline(INTERNAL_EVAL_TIMEOUT);
            self.with_ctx(
                |ctx| match ctx.eval_with_options::<f64, _>(src, sloppy()).catch(&ctx) {
                    Ok(v) => v,
                    Err(err) => {
                        self.report_eval_error(js, &err);
                        f64::NAN
                    }
                },
            )
        };
        self.sync_back();
        result
    }

    pub fn eval_string(&self, js: &str) -> String {
        self.install_mascot();
        let src = format!("String({})", self.wrap_source(js));
        let result = {
            let _deadline = self.enter_deadline(INTERNAL_EVAL_TIMEOUT);
            self.with_ctx(|ctx| {
                match ctx
                    .eval_with_options::<String, _>(src, sloppy())
                    .catch(&ctx)
                {
                    Ok(v) => v,
                    Err(err) => {
                        self.report_eval_error(js, &err);
                        String::new()
                    }
                }
            })
        };
        self.sync_back();
        result
    }

    pub fn eval(&self, js: &str) {
        self.install_mascot();
        let src = self.wrap_source(js);
        {
            let _deadline = self.enter_deadline(INTERNAL_EVAL_TIMEOUT);
            self.with_ctx(|ctx| {
                if let Err(err) = ctx.eval_with_options::<(), _>(src, sloppy()).catch(&ctx) {
                    self.report_eval_error(js, &err);
                }
            });
        }
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
    ///
    /// 每个对象同时附带 `getXxx()/setXxx()` 访问器函数
    /// （context.cc 的 put_prop_functions）；`activeBehavior` 生成为
    /// get/set 访问器属性，只有脚本显式赋值时才置脏标记，回写阶段据此
    /// 无条件写入 queued_behavior（同名也强制重新排队，对齐 C++ setter）。
    fn install_mascot(&self) {
        let Some(state_rc) = self.state.borrow().clone() else {
            return;
        };
        let state = state_rc.borrow();
        let Some(env_rc) = state.env.clone() else {
            return;
        };
        let env = env_rc.borrow();

        // C++ 的 activeBehavior getter：queued 非空时返回 queued，否则返回
        // 当前行为名，两者皆无返回 null。
        let active_behavior = if !state.queued_behavior.is_empty() {
            js_str(&state.queued_behavior)
        } else if let Some(b) = &state.behavior {
            js_str(&b.name)
        } else {
            "null".to_string()
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

        // 注意：局部变量一律包在 IIFE 内，避免在 globalThis 上留下常驻属性。
        let script = format!(
            "globalThis.__abAssigned=false;\
             globalThis.__abValue=undefined;\
             globalThis.mascot={{\
             bounds:{{x:{bx},y:{by},width:{bw},height:{bh}}},\
             anchor:{{x:{ax},y:{ay}}},\
             lookRight:{lr},\
             totalCount:{tc},\
             environment:{{\
             floor:{floor},ceiling:{ceiling},wall:{wall},\
             workArea:{wa},screen:{screen},activeIE:{ie},\
             allowsWindowPushing:{awp},\
             cursor:{{x:{cx},y:{cy},dx:{cdx},dy:{cdy}}}\
             }}}};\
             Object.defineProperty(globalThis.mascot,\"activeBehavior\",{{\
             get:function(){{var v=globalThis.__abValue;\
             return (globalThis.__abAssigned&&typeof v===\"string\"&&v!==\"\")?v:{ab};}},\
             set:function(v){{globalThis.__abAssigned=true;globalThis.__abValue=v;}},\
             enumerable:true,configurable:true}});\
             (function(m){{\
             globalThis.__addAccessorFns(m,[\"bounds\",\"activeBehavior\",\"anchor\",\
             \"lookRight\",\"totalCount\",\"environment\"]);\
             globalThis.__addAccessorFns(m.bounds,[\"x\",\"y\",\"width\",\"height\"]);\
             globalThis.__addAccessorFns(m.anchor,[\"x\",\"y\"]);\
             var e=m.environment;\
             globalThis.__addAccessorFns(e,[\"floor\",\"ceiling\",\"wall\",\"workArea\",\
             \"screen\",\"activeIE\",\"allowsWindowPushing\",\"cursor\"]);\
             globalThis.__addAccessorFns(e.cursor,[\"x\",\"y\",\"dx\",\"dy\"]);\
             var ap=[\"rightBorder\",\"leftBorder\",\"topBorder\",\"bottomBorder\",\
             \"width\",\"height\",\"visible\",\"left\",\"right\",\"top\",\"bottom\"];\
             globalThis.__addAccessorFns(e.workArea,ap);\
             globalThis.__addAccessorFns(e.screen,ap);\
             globalThis.__addAccessorFns(e.activeIE,ap);\
             }})(globalThis.mascot);",
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
        self.with_ctx(|ctx| {
            if let Err(err) = ctx.eval_with_options::<(), _>(script, sloppy()).catch(&ctx) {
                self.report_eval_error("<mascot snapshot>", &err);
            }
        });
    }

    /// 求值结束后，把可写字段同步回 Rust 状态。
    ///
    /// 回写范围与 C++ 的 setter 覆盖范围一致：anchor、lookRight、bounds、
    /// environment 的 workArea/screen/activeIE 四边；activeBehavior 仅在
    /// 脚本显式赋值后（脏标记置位）无条件写入 queued_behavior。
    /// 非有限数（NaN/Infinity）与非数字类型一律忽略，保留原状态。
    fn sync_back(&self) {
        let Some(state_rc) = self.state.borrow().clone() else {
            return;
        };
        let (nums, lr, ab) = self.with_ctx(|ctx| {
            let nums = ctx
                .eval_with_options::<String, _>(
                    "(function(){function g(f){try{var v=f();\
                     return typeof v===\"number\"&&isFinite(v)?v:null;}catch(e){return null;}}\
                     return JSON.stringify([\
                     g(function(){return mascot.anchor.x;}),\
                     g(function(){return mascot.anchor.y;}),\
                     g(function(){return mascot.bounds.x;}),\
                     g(function(){return mascot.bounds.y;}),\
                     g(function(){return mascot.bounds.width;}),\
                     g(function(){return mascot.bounds.height;}),\
                     g(function(){return mascot.environment.workArea.left;}),\
                     g(function(){return mascot.environment.workArea.right;}),\
                     g(function(){return mascot.environment.workArea.top;}),\
                     g(function(){return mascot.environment.workArea.bottom;}),\
                     g(function(){return mascot.environment.screen.left;}),\
                     g(function(){return mascot.environment.screen.right;}),\
                     g(function(){return mascot.environment.screen.top;}),\
                     g(function(){return mascot.environment.screen.bottom;}),\
                     g(function(){return mascot.environment.activeIE.left;}),\
                     g(function(){return mascot.environment.activeIE.right;}),\
                     g(function(){return mascot.environment.activeIE.top;}),\
                     g(function(){return mascot.environment.activeIE.bottom;})\
                     ]);})()"
                        .to_string(),
                    sloppy(),
                )
                .ok();
            let lr = ctx
                .eval_with_options::<bool, _>("mascot.lookRight", sloppy())
                .ok();
            let ab = ctx
                .eval_with_options::<Option<String>, _>(
                    "globalThis.__abAssigned\
                     ?(typeof globalThis.__abValue===\"string\"?globalThis.__abValue:\"\")\
                     :null",
                    sloppy(),
                )
                .ok()
                .flatten();
            (nums, lr, ab)
        });

        let vals: Vec<Option<f64>> = nums
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let get = |i: usize| vals.get(i).copied().flatten();

        let mut state = state_rc.borrow_mut();
        if let (Some(x), Some(y)) = (get(0), get(1)) {
            state.anchor = crate::math::Vec2::new(x, y);
        }
        if let Some(v) = get(2) {
            state.bounds.x = v;
        }
        if let Some(v) = get(3) {
            state.bounds.y = v;
        }
        if let Some(v) = get(4) {
            state.bounds.width = v;
        }
        if let Some(v) = get(5) {
            state.bounds.height = v;
        }
        if let Some(env_rc) = state.env.clone() {
            let mut env = env_rc.borrow_mut();
            if let Some(v) = get(6) {
                env.work_area.left = v;
            }
            if let Some(v) = get(7) {
                env.work_area.right = v;
            }
            if let Some(v) = get(8) {
                env.work_area.top = v;
            }
            if let Some(v) = get(9) {
                env.work_area.bottom = v;
            }
            if let Some(v) = get(10) {
                env.screen.left = v;
            }
            if let Some(v) = get(11) {
                env.screen.right = v;
            }
            if let Some(v) = get(12) {
                env.screen.top = v;
            }
            if let Some(v) = get(13) {
                env.screen.bottom = v;
            }
            if let Some(v) = get(14) {
                env.active_ie.area.left = v;
            }
            if let Some(v) = get(15) {
                env.active_ie.area.right = v;
            }
            if let Some(v) = get(16) {
                env.active_ie.area.top = v;
            }
            if let Some(v) = get(17) {
                env.active_ie.area.bottom = v;
            }
        }
        if let Some(lr) = lr {
            state.looking_right = lr;
        }
        // 仅当脚本显式给 mascot.activeBehavior 赋过值（含同名赋值）才写队列，
        // 与 C++ setter 无条件写 queued_behavior 的语义一致。
        if let Some(ab) = ab {
            state.queued_behavior = ab;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::behavior::Behavior;
    use crate::environment::Environment;
    use crate::scripting::condition::Condition;
    use crate::state::{State, shared_state};

    /// 搭建一个带默认环境的活动状态，供 mascot 快照类测试使用。
    fn ctx_with_state() -> (Rc<ScriptContext>, SharedState) {
        let ctx = ScriptContext::new();
        let mut state = State::default();
        state.env = Some(Rc::new(RefCell::new(Environment::default())));
        let shared = shared_state(state);
        ctx.set_state(shared.clone());
        (ctx, shared)
    }

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

    #[test]
    fn math_random_is_not_deterministic_across_contexts() {
        // 与原版一致：无环境时走系统熵，两个上下文的序列不应逐值相同。
        let a = ScriptContext::new();
        let b = ScriptContext::new();
        let seq_a: Vec<f64> = (0..16).map(|_| a.eval_number("Math.random()")).collect();
        let seq_b: Vec<f64> = (0..16).map(|_| b.eval_number("Math.random()")).collect();
        assert_ne!(seq_a, seq_b, "Math.random 不应是固定种子序列");
        for v in &seq_a {
            assert!((0.0..1.0).contains(v), "Math.random 应返回 [0,1): {v}");
        }
    }

    #[test]
    fn new_assignments_land_in_scope_without_global_residue() {
        let ctx = ScriptContext::new();
        let a = ctx.make_scope();
        let b = ctx.make_scope();
        {
            let _g = ctx.enter_scope(a.id());
            ctx.eval("NewVar = 42;");
            ctx.eval("var VarDecl = 7;");
        }
        // 未声明赋值与 var 声明都应落入当前活动作用域。
        assert_eq!(ctx.get_scope_num(a.id(), "NewVar", -1.0), 42.0);
        assert_eq!(ctx.get_scope_num(a.id(), "VarDecl", -1.0), 7.0);
        // 求值结束后 globalThis 无残留。
        assert!(ctx.eval_bool("typeof NewVar === 'undefined' && typeof VarDecl === 'undefined'"));
        assert!(ctx.eval_bool("!Object.prototype.hasOwnProperty.call(globalThis, 'NewVar')"));
        // 其他作用域不可见。
        {
            let _g = ctx.enter_scope(b.id());
            assert!(ctx.eval_bool("typeof NewVar === 'undefined'"));
        }
        // 求值中途抛错时，出错前的赋值仍即时生效（对齐 C++ proxy set）。
        {
            let _g = ctx.enter_scope(a.id());
            ctx.eval("PartialVar = 1; throw new Error('boom');");
        }
        assert_eq!(ctx.get_scope_num(a.id(), "PartialVar", -1.0), 1.0);
        assert!(ctx.eval_bool("typeof PartialVar === 'undefined'"));
    }

    #[test]
    fn mascot_writes_sync_back_to_state() {
        let (ctx, shared) = ctx_with_state();
        ctx.eval(
            "mascot.anchor.x = 5; mascot.anchor.y = 6;\
             mascot.bounds.x = 50; mascot.bounds.width = 128;\
             mascot.lookRight = true;\
             mascot.environment.workArea.left = 10;\
             mascot.environment.screen.bottom = 900;\
             mascot.environment.activeIE.top = 20;",
        );
        {
            let s = shared.borrow();
            assert_eq!(s.anchor.x, 5.0);
            assert_eq!(s.anchor.y, 6.0);
            assert_eq!(s.bounds.x, 50.0);
            assert_eq!(s.bounds.width, 128.0);
            assert!(s.looking_right);
        }
        {
            let env = shared.borrow().env.as_ref().unwrap().clone();
            let env = env.borrow();
            assert_eq!(env.work_area.left, 10.0);
            assert_eq!(env.screen.bottom, 900.0);
            assert_eq!(env.active_ie.area.top, 20.0);
        }
    }

    #[test]
    fn mascot_accessor_functions_follow_cpp_put_prop_functions() {
        let (ctx, shared) = ctx_with_state();
        ctx.eval(
            "mascot.setLookRight(true);\
             mascot.setAnchor({x: 9, y: 8});\
             mascot.environment.workArea.setLeft(33);\
             mascot.setActiveBehavior('Jump');",
        );
        assert!(shared.borrow().looking_right);
        assert_eq!(shared.borrow().anchor.x, 9.0);
        assert_eq!(shared.borrow().queued_behavior, "Jump");
        assert_eq!(ctx.eval_number("mascot.getAnchor().y"), 8.0);
        assert_eq!(
            ctx.eval_number("mascot.environment.workArea.getLeft()"),
            33.0
        );
        assert_eq!(ctx.eval_string("mascot.getActiveBehavior()"), "Jump");
        let env = shared.borrow().env.as_ref().unwrap().clone();
        assert_eq!(env.borrow().work_area.left, 33.0);
    }

    #[test]
    fn mascot_boundary_conditions_expose_callable_border_methods() {
        let (ctx, shared) = ctx_with_state();
        {
            let mut state = shared.borrow_mut();
            state.anchor = crate::math::Vec2::new(0.0, 100.0);
            let env = state.env.as_ref().unwrap().clone();
            let mut env = env.borrow_mut();
            env.work_area = Area::new(0.0, 100.0, 100.0, 0.0);
            env.floor = HBorder::new(100.0, 0.0, 100.0);
            env.active_ie = DArea::new(10.0, 90.0, 90.0, 10.0, 0.0, 0.0);
        }

        assert!(ctx.eval_bool("mascot.environment.workArea.leftBorder.isOn(mascot.anchor)"));
        assert!(ctx.eval_bool("mascot.environment.floor.isOn(mascot.anchor)"));
        assert!(ctx.eval_bool("mascot.environment.activeIE.visible"));
        assert_eq!(ctx.reported_error_count(), 0);
    }

    #[test]
    fn negative_desktop_coordinates_generate_valid_snapshot() {
        let (ctx, shared) = ctx_with_state();
        {
            let mut state = shared.borrow_mut();
            state.anchor = crate::math::Vec2::new(-1920.0, 0.0);
            let env = state.env.as_ref().unwrap().clone();
            let mut env = env.borrow_mut();
            env.screen = Area::new(-1080.0, 0.0, 0.0, -1920.0);
            env.work_area = env.screen;
            env.floor = HBorder::new(0.0, -1920.0, 0.0);
        }

        assert!(ctx.eval_bool("mascot.environment.workArea.leftBorder.isOn(mascot.anchor)"));
        assert!(ctx.eval_bool("mascot.environment.floor.isOn(mascot.anchor)"));
        assert_eq!(ctx.reported_error_count(), 0);
    }

    #[test]
    fn same_name_active_behavior_assignment_requeues() {
        let (ctx, shared) = ctx_with_state();
        shared.borrow_mut().behavior = Some(Rc::new(Behavior::new(
            "Walk".to_string(),
            1,
            false,
            Condition::Constant(true),
        )));
        // 未显式赋值时不应凭空制造行为队列。
        ctx.eval_bool("true");
        assert!(shared.borrow().queued_behavior.is_empty());
        // 同名赋值也必须写入队列（C++ setter 无条件写，强制重新 init）。
        ctx.eval("mascot.activeBehavior = 'Walk';");
        assert_eq!(shared.borrow().queued_behavior, "Walk");
    }

    #[test]
    fn constants_take_priority_over_scope_variables() {
        let (ctx, shared) = ctx_with_state();
        {
            let mut s = shared.borrow_mut();
            s.constants.insert("K".to_string(), "123".to_string());
            s.constants.insert("S".to_string(), "'abc'".to_string());
            s.constants.insert("N".to_string(), "null".to_string());
            s.constants.insert("B".to_string(), "true".to_string());
        }
        let scope = ctx.make_scope();
        ctx.set_scope_num(scope.id(), "K", 1.0);
        ctx.set_scope_num(scope.id(), "N", 9.0);
        ctx.set_scope_eval(scope.id(), "B", "false");
        // 常量优先于同名作用域变量。
        assert_eq!(ctx.get_scope_num(scope.id(), "K", -1.0), 123.0);
        assert!(ctx.get_scope_bool(scope.id(), "B", false));
        // 常量命中但类型不符时使用回退值，不落回作用域变量。
        assert_eq!(ctx.get_scope_num(scope.id(), "S", -1.0), -1.0);
        // 常量求值为 null 时落回作用域变量（对齐 C++ proxy get）。
        assert_eq!(ctx.get_scope_num(scope.id(), "N", -1.0), 9.0);
        // 无常量的键维持原有作用域读取行为。
        ctx.set_scope_num(scope.id(), "Plain", 5.0);
        assert_eq!(ctx.get_scope_num(scope.id(), "Plain", -1.0), 5.0);
        ctx.set_scope_str(scope.id(), "T", "hello");
        assert_eq!(ctx.get_scope_string(scope.id(), "T", "fallback"), "hello");
    }

    #[test]
    fn constant_expressions_can_read_the_active_scope() {
        let (ctx, shared) = ctx_with_state();
        shared
            .borrow_mut()
            .constants
            .insert("Offset".to_string(), "Base + 1".to_string());
        let scope = ctx.make_scope();
        ctx.set_scope_num(scope.id(), "Base", 4.0);

        let _guard = ctx.enter_scope(scope.id());
        assert!(ctx.eval_bool("Offset === 5"));
    }

    #[test]
    fn untrusted_eval_timeout_recovers_for_later_scripts() {
        let ctx = ScriptContext::new();
        let started = Instant::now();
        assert!(!ctx.eval_bool("while (true) {}"));
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(!ctx.eval_bool_timed("while (true) {}", Duration::from_millis(50)));
        assert!(ctx.eval_bool("true"));
    }

    #[test]
    fn eval_errors_are_reported_once_per_expression() {
        let ctx = ScriptContext::new();
        // 语法错误：回退为 false，不杀桌宠。
        assert!(!ctx.eval_bool("1 +* 2"));
        assert!(!ctx.eval_bool("1 +* 2"));
        assert!(!ctx.eval_bool("2 +* 3"));
        // 同一表达式只报告一次。
        assert_eq!(ctx.reported_error_count(), 2);
    }
}
