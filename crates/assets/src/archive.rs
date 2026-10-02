use crate::{SafePathError, SecurityLimits, validate_relative_path};

/// 归档读取器返回的单个条目描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub path: String,
    pub is_directory: bool,
    pub compressed_size: Option<u64>,
    pub uncompressed_size: u64,
}

/// 纯 Rust 归档后端的最小抽象；具体 ZIP/7z/RAR 实现可以独立替换。
pub trait ArchiveReader {
    fn entries(&mut self) -> Result<Vec<ArchiveEntry>, ArchiveError>;
    fn read_entry(&mut self, path: &str) -> Result<Vec<u8>, ArchiveError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("archive I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("archive format is unsupported")]
    Unsupported,
    #[error("archive entry is not found: {0}")]
    NotFound(String),
    #[error("unsafe archive entry path: {0}")]
    UnsafePath(String),
    #[error("archive entry exceeds a security limit")]
    LimitExceeded,
    #[error("archive backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveValidationLimits {
    pub max_entries: usize,
    pub max_total_uncompressed_bytes: u64,
    pub max_single_file_bytes: u64,
}

impl Default for ArchiveValidationLimits {
    fn default() -> Self {
        Self {
            max_entries: SecurityLimits::MASCOT_ZIP_ENTRY_MAX_COUNT,
            max_total_uncompressed_bytes: SecurityLimits::MASCOT_EXTRACTED_MAX_BYTES,
            max_single_file_bytes: SecurityLimits::MASCOT_SINGLE_FILE_MAX_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveReport {
    pub entry_count: usize,
    pub file_count: usize,
    pub extracted_bytes: u64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ArchiveValidationError {
    #[error("archive contains too many entries")]
    TooManyEntries,
    #[error("archive extracted data exceeds the limit")]
    ExtractedBytesExceeded,
    #[error("archive entry exceeds the single-file limit: {0}")]
    SingleFileBytesExceeded(String),
    #[error("unsafe archive entry path: {0}")]
    UnsafePath(String),
}

/// 在交给解压器前检查 entry 名称、条目数及展开大小。
pub fn validate_archive_entries(
    entries: &[ArchiveEntry],
    limits: ArchiveValidationLimits,
) -> Result<ArchiveReport, ArchiveValidationError> {
    if entries.len() > limits.max_entries {
        return Err(ArchiveValidationError::TooManyEntries);
    }
    let mut extracted_bytes = 0u64;
    let mut file_count = 0usize;
    for entry in entries {
        let path = entry.path.trim_end_matches(['/', '\\']);
        validate_relative_path(path)
            .map_err(|_| ArchiveValidationError::UnsafePath(entry.path.clone()))?;
        if entry.is_directory {
            continue;
        }
        file_count += 1;
        if entry.uncompressed_size > limits.max_single_file_bytes {
            return Err(ArchiveValidationError::SingleFileBytesExceeded(
                entry.path.clone(),
            ));
        }
        extracted_bytes = extracted_bytes
            .checked_add(entry.uncompressed_size)
            .ok_or(ArchiveValidationError::ExtractedBytesExceeded)?;
        if extracted_bytes > limits.max_total_uncompressed_bytes {
            return Err(ArchiveValidationError::ExtractedBytesExceeded);
        }
    }
    Ok(ArchiveReport {
        entry_count: entries.len(),
        file_count,
        extracted_bytes,
    })
}

impl From<SafePathError> for ArchiveError {
    fn from(_: SafePathError) -> Self {
        Self::UnsafePath("invalid path".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, size: u64) -> ArchiveEntry {
        ArchiveEntry {
            path: path.to_owned(),
            is_directory: false,
            compressed_size: None,
            uncompressed_size: size,
        }
    }

    #[test]
    fn rejects_path_escape_and_nested_overflow() {
        assert!(matches!(
            validate_archive_entries(&[entry("../evil", 1)], Default::default()),
            Err(ArchiveValidationError::UnsafePath(_))
        ));
        let limits = ArchiveValidationLimits {
            max_entries: 2,
            max_total_uncompressed_bytes: 10,
            max_single_file_bytes: 8,
        };
        assert_eq!(
            validate_archive_entries(&[entry("a", 9)], limits),
            Err(ArchiveValidationError::SingleFileBytesExceeded(
                "a".to_owned()
            ))
        );
        assert_eq!(
            validate_archive_entries(&[entry("a", 6), entry("b", 6)], limits),
            Err(ArchiveValidationError::ExtractedBytesExceeded)
        );
    }
}
