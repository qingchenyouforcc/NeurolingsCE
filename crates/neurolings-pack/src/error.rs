//! `.mascot` 包子系统的错误类型。

use thiserror::Error;

/// 包检查、校验、解压与导入过程中产生的错误。
#[derive(Debug, Error)]
pub enum PackError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("walk error: {0}")]
    Walk(#[from] walkdir::Error),
    #[error("unsupported archive format: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Msg(String),
}

/// pack crate 的便捷 Result 别名。
pub type Result<T> = std::result::Result<T, PackError>;

impl PackError {
    /// 构造携带纯文本消息的错误。
    pub fn msg(message: impl Into<String>) -> Self {
        PackError::Msg(message.into())
    }
}
