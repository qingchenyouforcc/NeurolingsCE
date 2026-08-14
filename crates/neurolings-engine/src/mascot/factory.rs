//! 桌宠工厂：按模板生成引擎管理器，并维护模板注册表。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::environment::Environment;
use crate::error::{EngineError, Result};
use crate::scripting::context::ScriptContext;
use crate::state::BreedRequest;

use super::manager::{Initializer, Manager};

#[derive(Debug, Clone)]
pub struct Template {
    pub name: String,
    pub actions_xml: String,
    pub behaviors_xml: String,
    pub path: String,
}

pub struct Product {
    pub template: Rc<Template>,
    pub manager: Manager,
}

pub struct Factory {
    templates: HashMap<String, Rc<Template>>,
    pub script_ctx: Rc<ScriptContext>,
    pub env: Option<Rc<RefCell<Environment>>>,
}

impl Default for Factory {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Factory {
    pub fn new(script_ctx: Option<Rc<ScriptContext>>) -> Self {
        Self {
            templates: HashMap::new(),
            script_ctx: script_ctx.unwrap_or_else(ScriptContext::new),
            env: None,
        }
    }

    pub fn spawn(&self, name: &str, init: Initializer) -> Result<Product> {
        let template = self
            .templates
            .get(name)
            .cloned()
            .ok_or_else(|| EngineError::Logic(format!("no such template: {name}")))?;
        let manager = Manager::new(
            &template.actions_xml,
            &template.behaviors_xml,
            init,
            Some(self.script_ctx.clone()),
        )?;
        manager.state.borrow_mut().env = self.env.clone();
        Ok(Product { template, manager })
    }

    pub fn spawn_breed(&self, breed_request: &BreedRequest) -> Result<Product> {
        self.spawn(&breed_request.name, Initializer::from(breed_request))
    }

    pub fn clear(&mut self) {
        self.templates.clear();
    }

    pub fn register_template(&mut self, template: Template) -> Result<()> {
        if self.templates.contains_key(&template.name) {
            return Err(EngineError::Logic(
                "cannot register same template twice".into(),
            ));
        }
        self.templates
            .insert(template.name.clone(), Rc::new(template));
        Ok(())
    }

    pub fn deregister_template(&mut self, name: &str) -> Result<()> {
        self.templates
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| EngineError::Logic("no such template".into()))
    }

    pub fn all_templates(&self) -> &HashMap<String, Rc<Template>> {
        &self.templates
    }

    pub fn get_template(&self, name: &str) -> Option<Rc<Template>> {
        self.templates.get(name).cloned()
    }
}
