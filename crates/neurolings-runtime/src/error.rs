//! 运行时错误类型。

/// 运行时守护进程产生的错误。
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// 启动/配置问题（存储、模板、CLI 选项）。
    #[error("startup failed: {0}")]
    Startup(String),
    /// 平台后端故障。
    #[error("platform: {0}")]
    Platform(#[from] neurolings_platform::PlatformError),
    /// 引擎故障（XML、tick、召唤）。
    #[error("engine: {0}")]
    Engine(#[from] neurolings_engine::EngineError),
}

/// 运行时通用的 Result 别名。
pub type Result<T> = std::result::Result<T, RuntimeError>;
