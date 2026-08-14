//! neurolings-engine：Shimeji-ee 兼容的桌宠行为引擎。
//!
//! 职责：解析桌宠包的 actions/behaviors XML，驱动行为选择与动作状态机，
//! 处理重力/边界物理、桌宠间广播交互与脚本条件求值。

pub mod action;
pub mod animation;
pub mod behavior;
pub mod broadcast;
pub mod environment;
pub mod error;
pub mod hotspot;
pub mod mascot;
pub mod math;
pub mod parser;
pub mod pose;
pub mod scripting;
pub mod state;
pub mod tick;
pub mod translator;

pub use error::{EngineError, Result};
