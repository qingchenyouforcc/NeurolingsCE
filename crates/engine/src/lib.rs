//! Shijima mascot 模拟引擎的纯 Rust 表达式执行核心。

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use roxmltree::Node;

/// 表达式运行时可见的值。
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// 双精度浮点数。
    Number(f64),
    /// 布尔值。
    Bool(bool),
    /// UTF-8 字符串。
    String(String),
    /// 没有值的结果。
    Null,
    /// 只读对象属性集合。
    Object(BTreeMap<String, Value>),
}

impl Value {
    /// 返回值的脚本类型名称。
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Number(_) => "number",
            Self::Bool(_) => "boolean",
            Self::String(_) => "string",
            Self::Null => "null",
            Self::Object(_) => "object",
        }
    }

    /// 将数值转换为数字，其他类型返回 `None`。
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    fn is_truthy(&self) -> bool {
        match self {
            Self::Bool(value) => *value,
            Self::Number(value) => *value != 0.0 && !value.is_nan(),
            Self::String(value) => !value.is_empty(),
            Self::Null => false,
            Self::Object(_) => true,
        }
    }

    fn to_script_string(&self) -> String {
        match self {
            Self::Number(value) => {
                if value.fract() == 0.0 {
                    format!("{value:.0}")
                } else {
                    value.to_string()
                }
            }
            Self::Bool(value) => value.to_string(),
            Self::String(value) => value.clone(),
            Self::Null => "null".to_owned(),
            Self::Object(_) => "[object Object]".to_owned(),
        }
    }
}

/// 注册到表达式上下文中的只读函数。
pub type NativeFunction = Arc<dyn Fn(&[Value]) -> Result<Value, EvalError> + Send + Sync + 'static>;

/// 表达式执行时可读取的变量和函数集合。
pub struct EvalContext {
    variables: BTreeMap<String, Value>,
    functions: BTreeMap<String, NativeFunction>,
    random_source: Box<dyn FnMut() -> f64 + Send + 'static>,
}

impl Default for EvalContext {
    fn default() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15);
        let mut state = seed | 1;
        Self {
            variables: BTreeMap::new(),
            functions: BTreeMap::new(),
            random_source: Box::new(move || {
                // xorshift64 只用于提供非安全随机性；可测试场景通过 set_random_source 注入确定性源。
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state as f64) / (u64::MAX as f64)
            }),
        }
    }
}

impl EvalContext {
    /// 写入或覆盖一个脚本变量。
    pub fn set_variable(&mut self, name: impl Into<String>, value: Value) {
        self.variables.insert(name.into(), value);
    }

    /// 注册一个按名称调用的纯 Rust 函数。
    pub fn register_function<F>(&mut self, name: impl Into<String>, function: F)
    where
        F: Fn(&[Value]) -> Result<Value, EvalError> + Send + Sync + 'static,
    {
        self.functions.insert(name.into(), Arc::new(function));
    }

    /// 替换 `Math.random()` 使用的随机源。
    pub fn set_random_source<F>(&mut self, source: F)
    where
        F: FnMut() -> f64 + Send + 'static,
    {
        self.random_source = Box::new(source);
    }

    fn variable(&self, name: &str) -> Result<Value, EvalError> {
        self.variables
            .get(name)
            .cloned()
            .ok_or_else(|| EvalError::UndefinedVariable(name.to_owned()))
    }

    fn call_function(&self, name: &str, args: &[Value]) -> Result<Value, EvalError> {
        let function = self
            .functions
            .get(name)
            .ok_or_else(|| EvalError::UnknownFunction(name.to_owned()))?;
        function(args)
    }

    fn random(&mut self) -> f64 {
        let value = (self.random_source)();
        if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// 解析阶段错误。
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ParseError {
    /// 输入包含无法识别的字符。
    #[error("unexpected character '{character}' at byte {position}")]
    UnexpectedCharacter { character: char, position: usize },
    /// 字符串没有闭合。
    #[error("unterminated string at byte {position}")]
    UnterminatedString { position: usize },
    /// 数值字面量格式不正确。
    #[error("invalid number at byte {position}")]
    InvalidNumber { position: usize },
    /// 语法位置缺少表达式或标点。
    #[error("unexpected token at byte {position}: expected {expected}")]
    UnexpectedToken { position: usize, expected: String },
    /// 输入末尾缺少必要的 token。
    #[error("unexpected end of expression; expected {expected}")]
    UnexpectedEnd { expected: String },
}

/// 求值阶段错误。
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum EvalError {
    /// 引用了未定义的变量。
    #[error("undefined variable: {0}")]
    UndefinedVariable(String),
    /// 调用了未注册的函数。
    #[error("unknown function: {0}")]
    UnknownFunction(String),
    /// 函数参数数量不匹配。
    #[error("function {name} expected {expected} arguments, got {actual}")]
    Arity {
        name: String,
        expected: usize,
        actual: usize,
    },
    /// 运算需要的值类型不匹配。
    #[error("expected {expected}, got {actual}")]
    TypeMismatch {
        expected: &'static str,
        actual: &'static str,
    },
    /// 对象上不存在请求的成员。
    #[error("member {member} is missing on {object}")]
    MissingMember {
        member: String,
        object: &'static str,
    },
    /// 目标值不能作为函数调用。
    #[error("value of type {0} is not callable")]
    NotCallable(&'static str),
    /// 除数为零。
    #[error("division by zero")]
    DivisionByZero,
    /// 内置函数调用参数错误。
    #[error("invalid call to {name}: expected {expected} arguments, got {actual}")]
    InvalidCall {
        name: String,
        expected: usize,
        actual: usize,
    },
}

/// 统一的源代码解析和执行错误。
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum EngineError {
    /// 源码无法解析。
    #[error("{0}")]
    Parse(#[from] ParseError),
    /// 表达式执行失败。
    #[error("{0}")]
    Eval(#[from] EvalError),
}

/// 一个已解析、可重复执行的表达式。
pub struct Expression {
    root: Expr,
}

impl Expression {
    /// 将表达式源码解析为 AST。
    pub fn parse(source: &str) -> Result<Self, ParseError> {
        let tokens = Lexer::new(source).tokenize()?;
        let root = Parser::new(tokens).parse()?;
        Ok(Self { root })
    }

    /// 在给定上下文中执行表达式。
    pub fn evaluate(&self, context: &mut EvalContext) -> Result<Value, EvalError> {
        self.root.evaluate(context)
    }
}

/// 轻量级表达式引擎，负责复用上下文并提供源码入口。
#[derive(Default)]
pub struct Engine {
    context: EvalContext,
}

/// 引擎动作种类；解析后不再依赖 XML 节点。
#[derive(Clone, Debug, PartialEq)]
pub enum ActionKind {
    /// 一次性改变锚点。
    Offset { dx: f64, dy: f64 },
    /// 持续按位移移动。
    Move { dx: f64, dy: f64 },
    /// 等待指定 subtick。
    Stay { duration: u64 },
    /// 标记 mascot 自毁。
    SelfDestruct,
    /// 立即完成的空动作。
    Instant,
    /// 依次执行子动作。
    Sequence(Vec<Action>),
    /// 延迟引用已注册动作。
    Reference(String),
}

/// 已解析动作。
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    /// 可选动作名。
    pub name: Option<String>,
    /// 动作实现。
    pub kind: ActionKind,
}

/// 已解析行为。
#[derive(Clone, Debug, PartialEq)]
pub struct Behavior {
    /// 行为名称。
    pub name: String,
    /// 选择权重。
    pub frequency: f64,
    /// 可选条件表达式。
    pub condition: Option<String>,
    /// 行为根动作。
    pub action: Action,
}

/// XML 解析错误。
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ModelError {
    /// XML 语法错误。
    #[error("invalid engine XML: {0}")]
    Xml(String),
    /// 缺少必要字段。
    #[error("engine XML is missing {0}")]
    Missing(String),
    /// 属性数值无效。
    #[error("invalid engine attribute {0}")]
    Attribute(String),
}

/// actions.xml 与 behaviors.xml 的纯 Rust 解析结果。
#[derive(Clone, Debug, Default)]
pub struct EngineModel {
    actions: BTreeMap<String, Action>,
    behaviors: Vec<Behavior>,
}

impl EngineModel {
    /// 解析两个 Shijima 风格 XML 文档。
    pub fn from_xml(actions_xml: &str, behaviors_xml: &str) -> Result<Self, ModelError> {
        let actions_doc = roxmltree::Document::parse(actions_xml)
            .map_err(|error| ModelError::Xml(error.to_string()))?;
        let behaviors_doc = roxmltree::Document::parse(behaviors_xml)
            .map_err(|error| ModelError::Xml(error.to_string()))?;
        let mut actions = BTreeMap::new();
        let root = actions_doc.root_element();
        for node in root.children().filter(|node| {
            node.is_element() && node.tag_name().name().eq_ignore_ascii_case("action")
        }) {
            let action = parse_action(node)?;
            let name = action
                .name
                .clone()
                .ok_or_else(|| ModelError::Missing("action name".into()))?;
            actions.insert(name, action);
        }
        let mut behaviors = Vec::new();
        for node in behaviors_doc.root_element().children().filter(|node| {
            node.is_element() && node.tag_name().name().eq_ignore_ascii_case("behavior")
        }) {
            let name = attribute(node, "name")
                .ok_or_else(|| ModelError::Missing("behavior name".into()))?;
            let frequency = attribute(node, "frequency")
                .unwrap_or("1")
                .parse()
                .map_err(|_| ModelError::Attribute("frequency".into()))?;
            let condition = attribute(node, "condition").map(str::to_owned);
            let action_node = node
                .children()
                .find(|child| {
                    child.is_element() && child.tag_name().name().eq_ignore_ascii_case("action")
                })
                .ok_or_else(|| ModelError::Missing("behavior action".into()))?;
            behaviors.push(Behavior {
                name: name.to_owned(),
                frequency,
                condition,
                action: parse_action(action_node)?,
            });
        }
        Ok(Self { actions, behaviors })
    }

    /// 返回行为列表。
    pub fn behaviors(&self) -> &[Behavior] {
        &self.behaviors
    }

    /// 按名称查找动作。
    pub fn action(&self, name: &str) -> Option<&Action> {
        self.actions.get(name)
    }
}

fn attribute<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| attribute.name().eq_ignore_ascii_case(name))
        .map(|attribute| attribute.value())
}

fn parse_action<'a, 'input>(node: Node<'a, 'input>) -> Result<Action, ModelError> {
    let name = attribute(node, "name").map(str::to_owned);
    let typ = attribute(node, "type").map(|value| value.to_ascii_lowercase());
    let children = node
        .children()
        .filter(|child| {
            child.is_element() && child.tag_name().name().eq_ignore_ascii_case("action")
        })
        .collect::<Vec<_>>();
    let kind = match typ.as_deref() {
        Some("offset") => ActionKind::Offset {
            dx: parse_attr(node, "dx")?,
            dy: parse_attr(node, "dy")?,
        },
        Some("move") | Some("movewithturn") => ActionKind::Move {
            dx: parse_attr(node, "dx")?,
            dy: parse_attr(node, "dy")?,
        },
        Some("stay") => ActionKind::Stay {
            duration: parse_attr::<u64>(node, "duration")?,
        },
        Some("selfdestruct") | Some("self_destruct") => ActionKind::SelfDestruct,
        Some("instant") => ActionKind::Instant,
        Some("sequence") => ActionKind::Sequence(
            children
                .iter()
                .map(|child| parse_action(*child))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        None if !children.is_empty() => ActionKind::Sequence(
            children
                .iter()
                .map(|child| parse_action(*child))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        None => ActionKind::Reference(
            name.clone()
                .ok_or_else(|| ModelError::Missing("action type or name".into()))?,
        ),
        Some(other) => return Err(ModelError::Attribute(format!("type={other}"))),
    };
    Ok(Action { name, kind })
}

fn parse_attr<T: std::str::FromStr>(node: Node<'_, '_>, name: &str) -> Result<T, ModelError> {
    attribute(node, name)
        .ok_or_else(|| ModelError::Missing(name.into()))?
        .parse()
        .map_err(|_| ModelError::Attribute(name.into()))
}

/// 运行时的单个 mascot 状态。
#[derive(Clone, Debug)]
pub struct EngineMascot {
    id: u64,
    behavior: String,
    x: f64,
    y: f64,
    dead: bool,
    variables: BTreeMap<String, Value>,
    running: Option<RunningAction>,
}

impl EngineMascot {
    /// 返回运行时 ID。
    pub fn id(&self) -> u64 {
        self.id
    }
    /// 返回当前行为。
    pub fn behavior(&self) -> &str {
        &self.behavior
    }
    /// 返回锚点位置。
    pub fn position(&self) -> (f64, f64) {
        (self.x, self.y)
    }
    /// 判断是否已标记自毁。
    pub fn is_dead(&self) -> bool {
        self.dead
    }
}

#[derive(Clone, Debug)]
struct RunningAction {
    action: Action,
    child_index: usize,
    remaining: Option<u64>,
}

/// 纯 Rust 行为/动作运行时。
#[derive(Clone, Debug)]
pub struct EngineRuntime {
    model: EngineModel,
    mascots: Vec<EngineMascot>,
    next_id: u64,
}

impl EngineRuntime {
    /// 创建空运行时。
    pub fn new(model: EngineModel) -> Self {
        Self {
            model,
            mascots: Vec::new(),
            next_id: 0,
        }
    }

    /// 创建指定行为的 mascot。
    pub fn spawn(
        &mut self,
        behavior: impl Into<String>,
        x: f64,
        y: f64,
    ) -> Result<u64, ModelError> {
        let behavior = behavior.into();
        if !self
            .model
            .behaviors
            .iter()
            .any(|item| item.name == behavior)
        {
            return Err(ModelError::Missing(format!("behavior {behavior}")));
        }
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| ModelError::Attribute("mascot id".into()))?;
        self.mascots.push(EngineMascot {
            id,
            behavior,
            x,
            y,
            dead: false,
            variables: BTreeMap::new(),
            running: None,
        });
        Ok(id)
    }

    /// 设置 mascot 脚本变量。
    pub fn set_variable(
        &mut self,
        id: u64,
        name: impl Into<String>,
        value: Value,
    ) -> Result<(), ModelError> {
        let mascot = self
            .mascots
            .iter_mut()
            .find(|mascot| mascot.id == id)
            .ok_or_else(|| ModelError::Missing(format!("mascot {id}")))?;
        mascot.variables.insert(name.into(), value);
        Ok(())
    }

    /// 执行指定数量的 subtick。
    pub fn tick(&mut self, subticks: u32) {
        for _ in 0..subticks {
            let model = self.model.clone();
            for mascot in &mut self.mascots {
                if mascot.dead {
                    continue;
                }
                let behavior = model
                    .behaviors
                    .iter()
                    .find(|behavior| behavior.name == mascot.behavior);
                let Some(behavior) = behavior else { continue };
                if mascot.running.is_none() {
                    if let Some(condition) = behavior.condition.as_deref() {
                        let mut engine = Engine::new();
                        for (name, value) in &mascot.variables {
                            engine
                                .context_mut()
                                .set_variable(name.clone(), value.clone());
                        }
                        if !matches!(engine.evaluate_source(condition), Ok(Value::Bool(true))) {
                            continue;
                        }
                    }
                    mascot.running = Some(RunningAction {
                        action: behavior.action.clone(),
                        child_index: 0,
                        remaining: None,
                    });
                }
                let mut running = mascot.running.take().expect("running action initialized");
                let done = execute_running(&model, &mut running, mascot);
                if !done {
                    mascot.running = Some(running);
                }
            }
        }
    }

    /// 返回所有 mascot 状态。
    pub fn mascots(&self) -> &[EngineMascot] {
        &self.mascots
    }
}

fn execute_running(
    model: &EngineModel,
    running: &mut RunningAction,
    mascot: &mut EngineMascot,
) -> bool {
    loop {
        match &running.action.kind {
            ActionKind::Sequence(children) => {
                if running.child_index >= children.len() {
                    return true;
                }
                let child = children[running.child_index].clone();
                let mut child_running = RunningAction {
                    action: child,
                    child_index: 0,
                    remaining: None,
                };
                if execute_running(model, &mut child_running, mascot) {
                    running.child_index += 1;
                    continue;
                }
                return false;
            }
            ActionKind::Reference(name) => {
                let Some(action) = model.action(name).cloned() else {
                    return true;
                };
                running.action = action;
            }
            ActionKind::Offset { dx, dy } => {
                mascot.x += dx;
                mascot.y += dy;
                return true;
            }
            ActionKind::Move { dx, dy } => {
                mascot.x += dx;
                mascot.y += dy;
                return true;
            }
            ActionKind::Stay { duration } => {
                let remaining = running.remaining.get_or_insert(*duration);
                if *remaining == 0 {
                    return true;
                }
                *remaining -= 1;
                return *remaining == 0;
            }
            ActionKind::SelfDestruct => {
                mascot.dead = true;
                return true;
            }
            ActionKind::Instant => return true,
        }
    }
}

impl Engine {
    /// 使用默认上下文创建引擎。
    pub fn new() -> Self {
        Self::default()
    }

    /// 使用调用方提供的上下文创建引擎。
    pub fn with_context(context: EvalContext) -> Self {
        Self { context }
    }

    /// 获取可修改的求值上下文。
    pub fn context_mut(&mut self) -> &mut EvalContext {
        &mut self.context
    }

    /// 执行已解析表达式。
    pub fn evaluate(&mut self, expression: &Expression) -> Result<Value, EvalError> {
        expression.evaluate(&mut self.context)
    }

    /// 解析并执行源码。
    pub fn evaluate_source(&mut self, source: &str) -> Result<Value, EngineError> {
        let expression = Expression::parse(source)?;
        Ok(self.evaluate(&expression)?)
    }
}

#[derive(Debug, Clone)]
enum Expr {
    Literal(Value),
    Identifier(String),
    Member {
        object: Box<Expr>,
        property: String,
    },
    Call {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },
    Unary {
        operator: UnaryOperator,
        operand: Box<Expr>,
    },
    Binary {
        operator: BinaryOperator,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Conditional {
        condition: Box<Expr>,
        on_true: Box<Expr>,
        on_false: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy)]
enum UnaryOperator {
    Not,
    Positive,
    Negative,
}

#[derive(Debug, Clone, Copy)]
enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}

impl Expr {
    fn evaluate(&self, context: &mut EvalContext) -> Result<Value, EvalError> {
        match self {
            Self::Literal(value) => Ok(value.clone()),
            Self::Identifier(name) => context.variable(name),
            Self::Member { object, property } => {
                if matches!(object.as_ref(), Self::Identifier(name) if name == "Math")
                    && property == "random"
                {
                    return Err(EvalError::NotCallable("function"));
                }
                let value = object.evaluate(context)?;
                match value {
                    Value::Object(properties) => {
                        properties
                            .get(property)
                            .cloned()
                            .ok_or_else(|| EvalError::MissingMember {
                                member: property.clone(),
                                object: "object",
                            })
                    }
                    other => Err(EvalError::MissingMember {
                        member: property.clone(),
                        object: other.type_name(),
                    }),
                }
            }
            Self::Call { callee, arguments } => {
                let values = arguments
                    .iter()
                    .map(|argument| argument.evaluate(context))
                    .collect::<Result<Vec<_>, _>>()?;
                if let Self::Identifier(name) = callee.as_ref() {
                    return context.call_function(name, &values);
                }
                if let Self::Member { object, property } = callee.as_ref()
                    && matches!(object.as_ref(), Self::Identifier(name) if name == "Math")
                    && property == "random"
                {
                    if !values.is_empty() {
                        return Err(EvalError::InvalidCall {
                            name: "Math.random".to_owned(),
                            expected: 0,
                            actual: values.len(),
                        });
                    }
                    return Ok(Value::Number(context.random()));
                }
                let value = callee.evaluate(context)?;
                Err(EvalError::NotCallable(value.type_name()))
            }
            Self::Unary { operator, operand } => {
                let value = operand.evaluate(context)?;
                match operator {
                    UnaryOperator::Not => Ok(Value::Bool(!value.is_truthy())),
                    UnaryOperator::Positive => number(value, "unary +").map(Value::Number),
                    UnaryOperator::Negative => {
                        number(value, "unary -").map(|value| Value::Number(-value))
                    }
                }
            }
            Self::Binary {
                operator,
                left,
                right,
            } => {
                let left_value = left.evaluate(context)?;
                match operator {
                    BinaryOperator::And if !left_value.is_truthy() => Ok(left_value),
                    BinaryOperator::Or if left_value.is_truthy() => Ok(left_value),
                    BinaryOperator::And | BinaryOperator::Or => right.evaluate(context),
                    _ => {
                        let right_value = right.evaluate(context)?;
                        evaluate_binary(*operator, left_value, right_value)
                    }
                }
            }
            Self::Conditional {
                condition,
                on_true,
                on_false,
            } => {
                if condition.evaluate(context)?.is_truthy() {
                    on_true.evaluate(context)
                } else {
                    on_false.evaluate(context)
                }
            }
        }
    }
}

fn number(value: Value, _operation: &str) -> Result<f64, EvalError> {
    value.as_number().ok_or_else(|| EvalError::TypeMismatch {
        expected: "number",
        actual: value.type_name(),
    })
}

fn evaluate_binary(
    operator: BinaryOperator,
    left: Value,
    right: Value,
) -> Result<Value, EvalError> {
    match operator {
        BinaryOperator::Add => match (&left, &right) {
            (Value::String(_), _) | (_, Value::String(_)) => Ok(Value::String(format!(
                "{}{}",
                left.to_script_string(),
                right.to_script_string()
            ))),
            _ => numeric_pair(left, right, |a, b| a + b),
        },
        BinaryOperator::Subtract => numeric_pair(left, right, |a, b| a - b),
        BinaryOperator::Multiply => numeric_pair(left, right, |a, b| a * b),
        BinaryOperator::Divide => {
            let (left, right) = numbers(left, right)?;
            if right == 0.0 {
                Err(EvalError::DivisionByZero)
            } else {
                Ok(Value::Number(left / right))
            }
        }
        BinaryOperator::Remainder => {
            let (left, right) = numbers(left, right)?;
            if right == 0.0 {
                Err(EvalError::DivisionByZero)
            } else {
                Ok(Value::Number(left % right))
            }
        }
        BinaryOperator::Equal => Ok(Value::Bool(left == right)),
        BinaryOperator::NotEqual => Ok(Value::Bool(left != right)),
        BinaryOperator::Less => compare_pair(left, right, |ordering| ordering.is_lt()),
        BinaryOperator::LessEqual => compare_pair(left, right, |ordering| ordering.is_le()),
        BinaryOperator::Greater => compare_pair(left, right, |ordering| ordering.is_gt()),
        BinaryOperator::GreaterEqual => compare_pair(left, right, |ordering| ordering.is_ge()),
        BinaryOperator::And | BinaryOperator::Or => {
            unreachable!("short circuit operators are evaluated before this branch")
        }
    }
}

fn numbers(left: Value, right: Value) -> Result<(f64, f64), EvalError> {
    let left = left.as_number().ok_or_else(|| EvalError::TypeMismatch {
        expected: "number",
        actual: left.type_name(),
    })?;
    let right = right.as_number().ok_or_else(|| EvalError::TypeMismatch {
        expected: "number",
        actual: right.type_name(),
    })?;
    Ok((left, right))
}

fn numeric_pair(
    left: Value,
    right: Value,
    operation: impl FnOnce(f64, f64) -> f64,
) -> Result<Value, EvalError> {
    let (left, right) = numbers(left, right)?;
    Ok(Value::Number(operation(left, right)))
}

fn compare_pair(
    left: Value,
    right: Value,
    operation: impl FnOnce(std::cmp::Ordering) -> bool,
) -> Result<Value, EvalError> {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => {
            Ok(Value::Bool(operation(left.total_cmp(&right))))
        }
        (Value::String(left), Value::String(right)) => Ok(Value::Bool(operation(left.cmp(&right)))),
        (left, _) => Err(EvalError::TypeMismatch {
            expected: "number or string",
            actual: left.type_name(),
        }),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum TokenKind {
    Number(f64),
    String(String),
    Identifier(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqualEqual,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    AndAnd,
    OrOr,
    Bang,
    Dot,
    Comma,
    LeftParen,
    RightParen,
    Question,
    Colon,
    End,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    position: usize,
}

struct Lexer<'a> {
    source: &'a str,
    cursor: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, cursor: 0 }
    }

    fn tokenize(mut self) -> Result<Vec<Token>, ParseError> {
        let mut tokens = Vec::new();
        while let Some(character) = self.peek() {
            if character.is_whitespace() {
                self.advance();
                continue;
            }
            let position = self.cursor;
            let kind = match character {
                '0'..='9' => self.number()?,
                '\'' | '"' => self.string()?,
                'a'..='z' | 'A'..='Z' | '_' | '$' => self.identifier(),
                '+' => {
                    self.advance();
                    TokenKind::Plus
                }
                '-' => {
                    self.advance();
                    TokenKind::Minus
                }
                '*' => {
                    self.advance();
                    TokenKind::Star
                }
                '/' => {
                    self.advance();
                    TokenKind::Slash
                }
                '%' => {
                    self.advance();
                    TokenKind::Percent
                }
                '.' => {
                    self.advance();
                    TokenKind::Dot
                }
                ',' => {
                    self.advance();
                    TokenKind::Comma
                }
                '(' => {
                    self.advance();
                    TokenKind::LeftParen
                }
                ')' => {
                    self.advance();
                    TokenKind::RightParen
                }
                '?' => {
                    self.advance();
                    TokenKind::Question
                }
                ':' => {
                    self.advance();
                    TokenKind::Colon
                }
                '!' => {
                    self.advance();
                    if self.consume('=') {
                        TokenKind::BangEqual
                    } else {
                        TokenKind::Bang
                    }
                }
                '=' => {
                    self.advance();
                    if self.consume('=') {
                        TokenKind::EqualEqual
                    } else {
                        return Err(ParseError::UnexpectedCharacter {
                            character,
                            position,
                        });
                    }
                }
                '<' => {
                    self.advance();
                    if self.consume('=') {
                        TokenKind::LessEqual
                    } else {
                        TokenKind::Less
                    }
                }
                '>' => {
                    self.advance();
                    if self.consume('=') {
                        TokenKind::GreaterEqual
                    } else {
                        TokenKind::Greater
                    }
                }
                '&' => {
                    self.advance();
                    if self.consume('&') {
                        TokenKind::AndAnd
                    } else {
                        return Err(ParseError::UnexpectedCharacter {
                            character,
                            position,
                        });
                    }
                }
                '|' => {
                    self.advance();
                    if self.consume('|') {
                        TokenKind::OrOr
                    } else {
                        return Err(ParseError::UnexpectedCharacter {
                            character,
                            position,
                        });
                    }
                }
                _ => {
                    return Err(ParseError::UnexpectedCharacter {
                        character,
                        position,
                    });
                }
            };
            tokens.push(Token { kind, position });
        }
        tokens.push(Token {
            kind: TokenKind::End,
            position: self.cursor,
        });
        Ok(tokens)
    }

    fn number(&mut self) -> Result<TokenKind, ParseError> {
        let start = self.cursor;
        while matches!(self.peek(), Some('0'..='9')) {
            self.advance();
        }
        if self.peek() == Some('.') {
            self.advance();
            while matches!(self.peek(), Some('0'..='9')) {
                self.advance();
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.advance();
            if matches!(self.peek(), Some('+' | '-')) {
                self.advance();
            }
            let exponent_start = self.cursor;
            while matches!(self.peek(), Some('0'..='9')) {
                self.advance();
            }
            if exponent_start == self.cursor {
                return Err(ParseError::InvalidNumber { position: start });
            }
        }
        self.source[start..self.cursor]
            .parse::<f64>()
            .map(TokenKind::Number)
            .map_err(|_| ParseError::InvalidNumber { position: start })
    }

    fn string(&mut self) -> Result<TokenKind, ParseError> {
        let position = self.cursor;
        let quote = self.advance().expect("string starts with a quote");
        let mut value = String::new();
        while let Some(character) = self.advance() {
            if character == quote {
                return Ok(TokenKind::String(value));
            }
            if character == '\\' {
                let escaped = self
                    .advance()
                    .ok_or(ParseError::UnterminatedString { position })?;
                value.push(match escaped {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '0' => '\0',
                    '\\' => '\\',
                    '\'' => '\'',
                    '"' => '"',
                    other => other,
                });
            } else {
                value.push(character);
            }
        }
        Err(ParseError::UnterminatedString { position })
    }

    fn identifier(&mut self) -> TokenKind {
        let start = self.cursor;
        self.advance();
        while matches!(
            self.peek(),
            Some('a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '$')
        ) {
            self.advance();
        }
        TokenKind::Identifier(self.source[start..self.cursor].to_owned())
    }

    fn peek(&self) -> Option<char> {
        self.source[self.cursor..].chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.cursor += character.len_utf8();
        Some(character)
    }

    fn consume(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, cursor: 0 }
    }

    fn parse(mut self) -> Result<Expr, ParseError> {
        let expression = self.expression(0)?;
        if !matches!(self.current(), TokenKind::End) {
            return Err(ParseError::UnexpectedToken {
                position: self.current_token().position,
                expected: "end of expression".to_owned(),
            });
        }
        Ok(expression)
    }

    fn expression(&mut self, minimum_precedence: u8) -> Result<Expr, ParseError> {
        let mut left = self.prefix()?;
        while let Some((operator, precedence)) = self.binary_operator() {
            if precedence < minimum_precedence {
                break;
            }
            self.advance();
            let right = self.expression(precedence + 1)?;
            left = Expr::Binary {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        if minimum_precedence == 0 && self.consume_kind(&TokenKind::Question) {
            let on_true = self.expression(0)?;
            self.expect(TokenKind::Colon, ":")?;
            let on_false = self.expression(0)?;
            left = Expr::Conditional {
                condition: Box::new(left),
                on_true: Box::new(on_true),
                on_false: Box::new(on_false),
            };
        }
        Ok(left)
    }

    fn prefix(&mut self) -> Result<Expr, ParseError> {
        let mut expression = match self.advance_kind() {
            TokenKind::Number(value) => Expr::Literal(Value::Number(value)),
            TokenKind::String(value) => Expr::Literal(Value::String(value)),
            TokenKind::Identifier(name) => match name.as_str() {
                "true" => Expr::Literal(Value::Bool(true)),
                "false" => Expr::Literal(Value::Bool(false)),
                "null" => Expr::Literal(Value::Null),
                _ => Expr::Identifier(name),
            },
            TokenKind::Bang => Expr::Unary {
                operator: UnaryOperator::Not,
                operand: Box::new(self.expression(7)?),
            },
            TokenKind::Plus => Expr::Unary {
                operator: UnaryOperator::Positive,
                operand: Box::new(self.expression(7)?),
            },
            TokenKind::Minus => Expr::Unary {
                operator: UnaryOperator::Negative,
                operand: Box::new(self.expression(7)?),
            },
            TokenKind::LeftParen => {
                let expression = self.expression(0)?;
                self.expect(TokenKind::RightParen, ")")?;
                expression
            }
            TokenKind::End => {
                return Err(ParseError::UnexpectedEnd {
                    expected: "expression".to_owned(),
                });
            }
            other => {
                return Err(ParseError::UnexpectedToken {
                    position: self.previous_position(),
                    expected: format!("expression, got {other:?}"),
                });
            }
        };
        loop {
            expression = match self.current() {
                TokenKind::Dot => {
                    self.advance();
                    let position = self.current_token().position;
                    let property = match self.advance_kind() {
                        TokenKind::Identifier(name) => name,
                        _ => {
                            return Err(ParseError::UnexpectedToken {
                                position,
                                expected: "member name".to_owned(),
                            });
                        }
                    };
                    Expr::Member {
                        object: Box::new(expression),
                        property,
                    }
                }
                TokenKind::LeftParen => {
                    self.advance();
                    let mut arguments = Vec::new();
                    if !self.consume_kind(&TokenKind::RightParen) {
                        loop {
                            arguments.push(self.expression(0)?);
                            if self.consume_kind(&TokenKind::RightParen) {
                                break;
                            }
                            self.expect(TokenKind::Comma, ",")?;
                        }
                    }
                    Expr::Call {
                        callee: Box::new(expression),
                        arguments,
                    }
                }
                _ => break,
            };
        }
        Ok(expression)
    }

    fn binary_operator(&self) -> Option<(BinaryOperator, u8)> {
        Some(match self.current() {
            TokenKind::OrOr => (BinaryOperator::Or, 1),
            TokenKind::AndAnd => (BinaryOperator::And, 2),
            TokenKind::EqualEqual => (BinaryOperator::Equal, 3),
            TokenKind::BangEqual => (BinaryOperator::NotEqual, 3),
            TokenKind::Less => (BinaryOperator::Less, 4),
            TokenKind::LessEqual => (BinaryOperator::LessEqual, 4),
            TokenKind::Greater => (BinaryOperator::Greater, 4),
            TokenKind::GreaterEqual => (BinaryOperator::GreaterEqual, 4),
            TokenKind::Plus => (BinaryOperator::Add, 5),
            TokenKind::Minus => (BinaryOperator::Subtract, 5),
            TokenKind::Star => (BinaryOperator::Multiply, 6),
            TokenKind::Slash => (BinaryOperator::Divide, 6),
            TokenKind::Percent => (BinaryOperator::Remainder, 6),
            _ => return None,
        })
    }

    fn current_token(&self) -> &Token {
        &self.tokens[self.cursor]
    }
    fn current(&self) -> &TokenKind {
        &self.current_token().kind
    }

    fn advance(&mut self) {
        if !matches!(self.current(), TokenKind::End) {
            self.cursor += 1;
        }
    }

    fn advance_kind(&mut self) -> TokenKind {
        let kind = self.current().clone();
        self.advance();
        kind
    }

    fn previous_position(&self) -> usize {
        self.tokens[self.cursor.saturating_sub(1)].position
    }

    fn consume_kind(&mut self, expected: &TokenKind) -> bool {
        if self.current() == expected {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: TokenKind, label: &str) -> Result<(), ParseError> {
        if self.consume_kind(&expected) {
            Ok(())
        } else if matches!(self.current(), TokenKind::End) {
            Err(ParseError::UnexpectedEnd {
                expected: label.to_owned(),
            })
        } else {
            Err(ParseError::UnexpectedToken {
                position: self.current_token().position,
                expected: label.to_owned(),
            })
        }
    }
}

impl fmt::Debug for EvalContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EvalContext")
            .field("variables", &self.variables.keys().collect::<Vec<_>>())
            .field("functions", &self.functions.keys().collect::<Vec<_>>())
            .finish()
    }
}
