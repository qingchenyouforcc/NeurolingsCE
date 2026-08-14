//! 桌宠会话与模板：Manager 驱动单只桌宠，Factory 按模板生成新个体。

pub mod factory;
pub mod manager;

pub use factory::{Factory, Product, Template};
pub use manager::{Initializer, Manager};
