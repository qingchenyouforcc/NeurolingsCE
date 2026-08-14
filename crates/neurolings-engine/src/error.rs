use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("could not find behavior: {0}")]
    NoSuchBehavior(String),
    #[error("no animation available")]
    NoAnimationAvailable,
    #[error("action finalized twice")]
    FinalizeTwice,
    #[error("tick() failed after multiple attempts")]
    TickFailed,
    #[error("{0}")]
    Logic(String),
}

pub type Result<T> = std::result::Result<T, EngineError>;
