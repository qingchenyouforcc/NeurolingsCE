//! 包格式与旧版导入共用的底层 ZIP 工具。

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use zip::ZipArchive;

use crate::error::{PackError, Result};

/// 所有包/旧版 zip 读取共用的读取器类型。
pub type ZipReader = BufReader<File>;

/// zip 压缩包内的原始条目。
#[derive(Debug, Clone)]
pub struct RawArchiveEntry {
    pub name: String,
    pub is_directory: bool,
    pub uncompressed_size: u64,
}

/// 构建旧版分析目录树时接受的扩展名。
pub const ALLOWED_LEGACY_EXTENSIONS: &[&str] = &["wav", "png", "xml", "json", "txt"];

/// 从磁盘打开 zip 压缩包。
pub fn open_zip(path: &Path) -> Result<ZipArchive<ZipReader>> {
    let file = File::open(path)?;
    let archive = ZipArchive::new(BufReader::new(file)).map_err(|error| match error {
        zip::result::ZipError::InvalidArchive(_) => {
            PackError::msg("Package is not a valid ZIP archive")
        }
        other => PackError::from(other),
    })?;
    Ok(archive)
}

/// 列出 zip 压缩包的原始条目，强制桌宠条目数量上限。
pub fn read_raw_archive_entries(
    archive: &mut ZipArchive<ZipReader>,
) -> Result<Vec<RawArchiveEntry>> {
    let mut entries = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        if entries.len() >= crate::limits::MASCOT_ZIP_ENTRY_MAX_COUNT {
            return Err(PackError::msg(format!(
                "Archive contains too many entries ({}, maximum {})",
                crate::limits::MASCOT_ZIP_ENTRY_MAX_COUNT + 1,
                crate::limits::MASCOT_ZIP_ENTRY_MAX_COUNT
            )));
        }
        let file = archive.by_index(index)?;
        let name = file.name().to_string();
        let is_directory = name.ends_with('/') || name.ends_with('\\');
        entries.push(RawArchiveEntry {
            name,
            is_directory,
            uncompressed_size: file.size(),
        });
    }
    Ok(entries)
}

/// 按索引把单个 zip 条目读入内存，强制字节数预算。
pub fn read_zip_entry_bytes(
    archive: &mut ZipArchive<ZipReader>,
    index: usize,
    max_bytes: u64,
) -> Result<Vec<u8>> {
    let file = archive.by_index(index)?;
    let declared = file.size();
    if declared > max_bytes {
        return Err(PackError::msg("Mascot package entry is too large"));
    }
    let mut buffer = Vec::new();
    file.take(max_bytes + 1).read_to_end(&mut buffer)?;
    if buffer.len() as u64 > max_bytes {
        return Err(PackError::msg("Mascot package entry is too large"));
    }
    Ok(buffer)
}

/// 只读取 zip 条目的前 limit 字节（用于不解压整图检查 PNG 头）。
pub fn read_zip_entry_header(
    archive: &mut ZipArchive<ZipReader>,
    index: usize,
    limit: u64,
) -> Result<Vec<u8>> {
    let file = archive.by_index(index)?;
    let mut buffer = Vec::new();
    file.take(limit).read_to_end(&mut buffer)?;
    Ok(buffer)
}

/// 查找归一化小写路径与 wanted 匹配的条目索引。
pub fn find_entry_by_normalized_path(entries: &[RawArchiveEntry], wanted: &str) -> Option<usize> {
    let wanted = crate::package::normalized_archive_path(wanted)?.to_ascii_lowercase();
    entries.iter().position(|entry| {
        crate::package::normalized_archive_path(&entry.name)
            .map(|normalized| normalized.to_ascii_lowercase() == wanted)
            .unwrap_or(false)
    })
}
