//! Mascot 资源包的路径、元数据和归档安全边界。

mod archive;
mod metadata;
mod safe_path;
mod security_limits;

pub use archive::{
    ArchiveEntry, ArchiveError, ArchiveFormat, ArchiveReader, ArchiveReport,
    ArchiveValidationError, ArchiveValidationLimits, ZipArchiveReader, detect_archive_format,
    extract_archive, open_archive, validate_archive_entries,
};
pub use metadata::{
    MascotMetadata, MetadataError, cache_path_for_name, is_valid_package_name, metadata_from_json,
    metadata_to_json, package_path_for_name, sanitized_package_base_name,
};
pub use safe_path::{SafePathError, safe_child_path, validate_relative_path};
pub use security_limits::SecurityLimits;
