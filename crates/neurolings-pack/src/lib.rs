//! .mascot 包格式、旧版 Shimeji-ee 压缩包导入与路径安全。
//!
//! ZIP 压缩包完整支持；其他压缩格式返回 [`PackError::Unsupported`]。

pub mod error;
pub mod legacy;
pub mod limits;
pub mod metadata;
pub mod package;
pub mod safepath;
pub mod storage;
mod zipio;

pub use error::{PackError, Result};
pub use legacy::{
    DEFAULT_ACTIONS_XML, DEFAULT_BEHAVIORS_XML, LegacyArchive, LegacyArchiveAnalysis,
    LegacyMascotCandidate, LegacyMascotConversionResult, analyze_legacy_archive, import_archive,
    inspect_legacy_directory, write_legacy_archive_selection_as_packages,
};
pub use metadata::{MascotMetadata, default_metadata, metadata_from_json, metadata_to_json};
pub use package::{
    MascotPackageReport, cache_path_for_name, extract_package, inspect_package, install_package,
    is_valid_package_name, migrate_legacy_directories, package_legacy_directory,
    package_path_for_name, sanitized_package_base_name, validate_package,
    write_package_from_directory, write_package_from_memory,
};
pub use safepath::safe_child_path;
pub use storage::default_storage_path;
