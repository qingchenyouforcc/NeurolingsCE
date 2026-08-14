//! 行为定义与选择：行为节点、条件化行为列表与按频率加权抽选的管理器。

use std::cell::RefCell;
use std::rc::Rc;

use crate::action::SharedAction;
use crate::error::{EngineError, Result};
use crate::scripting::condition::Condition;
use crate::scripting::context::{ScopeHandle, ScriptContext};
use crate::state::SharedState;

/// 单个行为节点，对应 behaviors.xml 中的 `<Behavior>`。
pub struct Behavior {
    /// 来自 `<NextBehaviorList>`：本行为结束后的候选行为列表。
    pub next_list: Option<BehaviorList>,
    /// 为真时下一次选择合并全局列表；为假时仅从 next_list 中选择。
    pub add_next: bool,

    pub name: String,
    pub frequency: i32,
    pub hidden: bool,
    pub condition: Condition,

    /// `<BehaviorReference>` 指向的真实行为，由解析器在构建后回填。
    pub referenced: RefCell<Option<Rc<Behavior>>>,

    /// 行为绑定的动作。
    pub action: RefCell<Option<SharedAction>>,
}

impl Behavior {
    pub fn new(name: String, frequency: i32, hidden: bool, condition: Condition) -> Self {
        Self {
            next_list: None,
            add_next: true,
            name,
            frequency,
            hidden,
            condition,
            referenced: RefCell::new(None),
            action: RefCell::new(None),
        }
    }

    /// 返回实际生效的行为：若为引用节点则追踪到被引用的原始行为。
    pub fn dereferenced(self: &Rc<Behavior>) -> Rc<Behavior> {
        match &*self.referenced.borrow() {
            Some(target) => target.clone(),
            None => self.clone(),
        }
    }
}

/// 条件化的行为列表，对应 behaviors.xml 的 `<Condition>` 嵌套结构。
#[derive(Default)]
pub struct BehaviorList {
    pub children: Vec<Rc<Behavior>>,
    pub condition: Condition,
    pub sublists: Vec<BehaviorList>,
}

impl BehaviorList {
    pub fn from_children(children: Vec<Rc<Behavior>>) -> Self {
        Self {
            children,
            condition: Condition::Constant(true),
            sublists: Vec::new(),
        }
    }

    pub fn flatten_unconditional(&self) -> Vec<Rc<Behavior>> {
        let mut flat = Vec::new();
        for behavior in &self.children {
            flat.push(behavior.clone());
        }
        for sub in &self.sublists {
            flat.extend(sub.flatten_unconditional());
        }
        flat
    }

    pub fn flatten(&self, ctx: &ScriptContext) -> Vec<Rc<Behavior>> {
        let mut flat = Vec::new();
        if self.condition.eval(ctx) {
            for behavior in &self.children {
                if behavior.condition.eval(ctx) {
                    flat.push(behavior.clone());
                }
            }
        }
        for sub in &self.sublists {
            flat.extend(sub.flatten(ctx));
        }
        flat
    }

    /// 按名称递归查找行为；不追踪各行为自带的 next_list。
    pub fn find(&self, name: &str) -> Option<Rc<Behavior>> {
        for child in &self.children {
            if child.name == name {
                return Some(child.clone());
            }
        }
        for sublist in &self.sublists {
            if let Some(found) = sublist.find(name) {
                return Some(found);
            }
        }
        None
    }
}

/// 行为选择管理器：维护全局行为列表与"下一行为"候选列表，按频率加权随机抽选。
pub struct BehaviorManager {
    initial_list: BehaviorList,
    next_list: BehaviorList,
    /// 行为条件求值专用的隔离作用域，与动作变量互不可见。
    scope: ScopeHandle,
}

impl BehaviorManager {
    pub fn new(
        initial_list: BehaviorList,
        first_behavior: &str,
        ctx: &Rc<ScriptContext>,
    ) -> Result<Self> {
        let mut manager = Self {
            initial_list,
            next_list: BehaviorList::default(),
            scope: ctx.make_scope(),
        };
        manager.set_next(first_behavior)?;
        Ok(manager)
    }

    pub fn initial_list(&self) -> &BehaviorList {
        &self.initial_list
    }

    pub fn set_next(&mut self, next_name: &str) -> Result<()> {
        self.next_list = BehaviorList::default();
        if !next_name.is_empty() {
            let behavior = self
                .initial_list
                .find(next_name)
                .ok_or_else(|| EngineError::NoSuchBehavior(next_name.to_string()))?;
            self.next_list.children.push(behavior);
        } else {
            self.next_list
                .sublists
                .push(self.initial_list.shallow_clone());
        }
        Ok(())
    }

    /// 清除临时偏好后，恢复由当前活动行为决定的候选列表。
    pub fn restore_next(&mut self, behavior: Option<&Rc<Behavior>>) {
        let Some(behavior) = behavior.cloned() else {
            // 无活动行为时退回全局列表，等价于 set_next("")。
            self.next_list = BehaviorList::default();
            self.next_list
                .sublists
                .push(self.initial_list.shallow_clone());
            return;
        };
        let behavior = behavior.dereferenced();
        if !behavior.add_next {
            self.next_list = match &behavior.next_list {
                Some(list) => list.shallow_clone(),
                None => BehaviorList::default(),
            };
        } else {
            self.next_list = BehaviorList::default();
            self.next_list
                .sublists
                .push(self.initial_list.shallow_clone());
            if let Some(list) = &behavior.next_list {
                self.next_list.sublists.push(list.shallow_clone());
            }
        }
    }

    pub fn next(&mut self, ctx: &ScriptContext, state: &SharedState) -> Option<Rc<Behavior>> {
        ctx.set_state(state.clone());
        let flat = {
            let _guard = ctx.enter_scope(self.scope.id());
            self.next_list.flatten(ctx)
        };

        let behavior = if flat.len() == 1 {
            flat.into_iter().next().unwrap()
        } else {
            let freq_sum: i32 = flat.iter().map(|b| b.frequency).sum();
            if freq_sum == 0 {
                return None;
            }
            let dice = ctx.random_int(freq_sum);
            let mut counter = 0;
            let mut picked = None;
            for option in flat {
                counter += option.frequency;
                if counter > dice {
                    picked = Some(option);
                    break;
                }
            }
            picked?
        };

        let behavior = behavior.dereferenced();
        self.restore_next(Some(&behavior));
        Some(behavior)
    }
}

impl BehaviorList {
    /// 复制列表结构，行为节点本身仍为共享引用。
    pub fn shallow_clone(&self) -> BehaviorList {
        BehaviorList {
            children: self.children.clone(),
            condition: self.condition.clone(),
            sublists: self.sublists.iter().map(|s| s.shallow_clone()).collect(),
        }
    }
}
