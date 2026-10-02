use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek};
use std::path::Path;

use crate::{SafePathError, SecurityLimits, safe_child_path, validate_relative_path};

/// 当前支持的归档格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    /// ZIP 容器。
    Zip,
    /// 未压缩 TAR 容器。
    Tar,
    /// GZIP 压缩的 TAR 容器。
    Gzip,
    /// 7z 容器；当前仅做格式识别，解包后端待补充。
    SevenZip,
    /// RAR 容器；当前仅做格式识别，解包后端待补充。
    Rar,
}

/// 根据文件头识别 mascot 包归档格式。
pub fn detect_archive_format(bytes: &[u8]) -> Option<ArchiveFormat> {
    let is_zip = bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08");
    if is_zip {
        return Some(ArchiveFormat::Zip);
    }
    if bytes.starts_with(b"\x1f\x8b") {
        return Some(ArchiveFormat::Gzip);
    }
    if bytes.starts_with(b"7z\xbc\xaf\x27\x1c") {
        return Some(ArchiveFormat::SevenZip);
    }
    if bytes.starts_with(b"Rar!\x1a\x07") {
        return Some(ArchiveFormat::Rar);
    }
    if bytes.len() >= 262 && &bytes[257..262] == b"ustar" {
        return Some(ArchiveFormat::Tar);
    }
    None
}

/// 根据文件头创建受限的归档读取器。
pub fn open_archive(bytes: &[u8]) -> Result<Box<dyn ArchiveReader>, ArchiveError> {
    match detect_archive_format(bytes) {
        Some(ArchiveFormat::Zip) => Ok(Box::new(ZipArchiveReader::from_bytes(bytes)?)),
        Some(ArchiveFormat::Tar) => Ok(Box::new(TarArchiveReader::from_bytes(bytes)?)),
        Some(ArchiveFormat::Gzip) => Ok(Box::new(TarArchiveReader::from_gzip(bytes)?)),
        Some(ArchiveFormat::SevenZip | ArchiveFormat::Rar) => Err(ArchiveError::Unsupported),
        None => Err(ArchiveError::Unsupported),
    }
}

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

/// 基于纯 Rust `zip` 后端的 mascot 归档读取器。
pub struct ZipArchiveReader<R> {
    archive: zip::ZipArchive<R>,
    limits: ArchiveValidationLimits,
}

impl ZipArchiveReader<Cursor<Vec<u8>>> {
    /// 从内存中的 ZIP 字节创建读取器。
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ArchiveError> {
        if bytes.len() as u64 > SecurityLimits::MASCOT_PACKAGE_MAX_BYTES {
            return Err(ArchiveError::LimitExceeded);
        }
        Self::new(Cursor::new(bytes.to_vec()))
    }
}

impl ZipArchiveReader<BufReader<File>> {
    /// 从磁盘上的 `.mascot` 文件创建读取器，并检查压缩包大小上限。
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ArchiveError> {
        let path = path.as_ref();
        let metadata = std::fs::metadata(path)?;
        if metadata.len() > SecurityLimits::MASCOT_PACKAGE_MAX_BYTES {
            return Err(ArchiveError::LimitExceeded);
        }
        Self::new(BufReader::new(File::open(path)?))
    }
}

impl<R: Read + Seek> ZipArchiveReader<R> {
    /// 从可定位输入创建 ZIP 读取器。
    pub fn new(reader: R) -> Result<Self, ArchiveError> {
        Ok(Self {
            archive: zip::ZipArchive::new(reader)
                .map_err(|error| ArchiveError::Backend(error.to_string()))?,
            limits: ArchiveValidationLimits::default(),
        })
    }

    /// 替换归档展开限制，便于调用方在测试或导入策略中使用更严格的边界。
    pub fn with_limits(mut self, limits: ArchiveValidationLimits) -> Self {
        self.limits = limits;
        self
    }
}

impl<R: Read + Seek> ArchiveReader for ZipArchiveReader<R> {
    fn entries(&mut self) -> Result<Vec<ArchiveEntry>, ArchiveError> {
        let mut entries = Vec::with_capacity(self.archive.len());
        for index in 0..self.archive.len() {
            let file = self
                .archive
                .by_index(index)
                .map_err(|error| ArchiveError::Backend(error.to_string()))?;
            let raw_path = file.name().to_owned();
            let path = raw_path.trim_end_matches(['/', '\\']).to_owned();
            let normalized = validate_relative_path(&path)
                .map_err(|_| ArchiveError::UnsafePath(raw_path.clone()))?;
            entries.push(ArchiveEntry {
                path: normalized,
                is_directory: file.is_dir() || raw_path.ends_with(['/', '\\']),
                compressed_size: Some(file.compressed_size()),
                uncompressed_size: file.size(),
            });
        }
        validate_archive_entries(&entries, self.limits).map_err(ArchiveError::from)?;
        Ok(entries)
    }

    fn read_entry(&mut self, path: &str) -> Result<Vec<u8>, ArchiveError> {
        let normalized =
            validate_relative_path(path).map_err(|_| ArchiveError::UnsafePath(path.to_owned()))?;
        for index in 0..self.archive.len() {
            let file = self
                .archive
                .by_index(index)
                .map_err(|error| ArchiveError::Backend(error.to_string()))?;
            let entry_path = file.name().trim_end_matches(['/', '\\']).to_owned();
            let entry_path = validate_relative_path(&entry_path)
                .map_err(|_| ArchiveError::UnsafePath(file.name().to_owned()))?;
            if entry_path != normalized {
                continue;
            }
            if file.is_dir() {
                return Err(ArchiveError::NotFound(path.to_owned()));
            }
            if file.size() > self.limits.max_single_file_bytes {
                return Err(ArchiveError::LimitExceeded);
            }
            let mut bytes = Vec::with_capacity(file.size() as usize);
            file.take(self.limits.max_single_file_bytes.saturating_add(1))
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > self.limits.max_single_file_bytes {
                return Err(ArchiveError::LimitExceeded);
            }
            return Ok(bytes);
        }
        Err(ArchiveError::NotFound(path.to_owned()))
    }
}

/// 纯 Rust TAR/GZIP 后端；条目在构造时读入受限内存，避免保留外部文件句柄。
pub struct TarArchiveReader {
    entries: BTreeMap<String, (bool, Vec<u8>)>,
    limits: ArchiveValidationLimits,
}

impl TarArchiveReader {
    /// 从未压缩 TAR 字节读取。
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ArchiveError> {
        if bytes.len() as u64 > SecurityLimits::MASCOT_PACKAGE_MAX_BYTES {
            return Err(ArchiveError::LimitExceeded);
        }
        Self::from_reader(Cursor::new(bytes.to_vec()))
    }

    /// 从 GZIP 压缩的 TAR 字节读取。
    pub fn from_gzip(bytes: &[u8]) -> Result<Self, ArchiveError> {
        if bytes.len() as u64 > SecurityLimits::MASCOT_PACKAGE_MAX_BYTES {
            return Err(ArchiveError::LimitExceeded);
        }
        let mut decoder = flate2::read::GzDecoder::new(Cursor::new(bytes));
        let mut tar_bytes = Vec::new();
        decoder.read_to_end(&mut tar_bytes)?;
        Self::from_bytes(&tar_bytes)
    }

    fn from_reader<R: Read>(reader: R) -> Result<Self, ArchiveError> {
        let limits = ArchiveValidationLimits::default();
        let mut archive = tar::Archive::new(reader);
        let mut entries = BTreeMap::new();
        let mut total = 0u64;
        let mut count = 0usize;
        for item in archive
            .entries()
            .map_err(|error| ArchiveError::Backend(error.to_string()))?
        {
            let mut item = item.map_err(|error| ArchiveError::Backend(error.to_string()))?;
            count += 1;
            if count > limits.max_entries {
                return Err(ArchiveError::LimitExceeded);
            }
            let raw_path = item
                .path()
                .map_err(|error| ArchiveError::Backend(error.to_string()))?
                .to_string_lossy()
                .into_owned();
            let path = validate_relative_path(&raw_path)
                .map_err(|_| ArchiveError::UnsafePath(raw_path.clone()))?;
            let is_directory = item.header().entry_type().is_dir();
            if is_directory {
                entries.insert(path, (true, Vec::new()));
                continue;
            }
            let declared = item.header().size().map_err(ArchiveError::Io)?;
            if declared > limits.max_single_file_bytes {
                return Err(ArchiveError::LimitExceeded);
            }
            let mut data = Vec::with_capacity(declared as usize);
            item.read_to_end(&mut data)?;
            total = total
                .checked_add(data.len() as u64)
                .ok_or(ArchiveError::LimitExceeded)?;
            if total > limits.max_total_uncompressed_bytes {
                return Err(ArchiveError::LimitExceeded);
            }
            entries.insert(path, (false, data));
        }
        Ok(Self { entries, limits })
    }
}

impl ArchiveReader for TarArchiveReader {
    fn entries(&mut self) -> Result<Vec<ArchiveEntry>, ArchiveError> {
        let entries = self
            .entries
            .iter()
            .map(|(path, (is_directory, data))| ArchiveEntry {
                path: path.clone(),
                is_directory: *is_directory,
                compressed_size: None,
                uncompressed_size: data.len() as u64,
            })
            .collect::<Vec<_>>();
        validate_archive_entries(&entries, self.limits).map_err(ArchiveError::from)?;
        Ok(entries)
    }

    fn read_entry(&mut self, path: &str) -> Result<Vec<u8>, ArchiveError> {
        let path =
            validate_relative_path(path).map_err(|_| ArchiveError::UnsafePath(path.to_owned()))?;
        let Some((is_directory, data)) = self.entries.get(&path) else {
            return Err(ArchiveError::NotFound(path));
        };
        if *is_directory {
            return Err(ArchiveError::NotFound(path));
        }
        Ok(data.clone())
    }
}

/// 将归档安全展开到指定目录，并返回校验报告。
pub fn extract_archive(
    reader: &mut impl ArchiveReader,
    destination: impl AsRef<Path>,
) -> Result<ArchiveReport, ArchiveError> {
    let destination = destination.as_ref();
    std::fs::create_dir_all(destination)?;
    let entries = reader.entries()?;
    let report = validate_archive_entries(&entries, ArchiveValidationLimits::default())
        .map_err(ArchiveError::from)?;
    for entry in &entries {
        let target = safe_child_path(destination, &entry.path)?;
        if entry.is_directory {
            std::fs::create_dir_all(target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, reader.read_entry(&entry.path)?)?;
    }
    Ok(report)
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

impl From<ArchiveValidationError> for ArchiveError {
    fn from(error: ArchiveValidationError) -> Self {
        match error {
            ArchiveValidationError::UnsafePath(path) => Self::UnsafePath(path),
            ArchiveValidationError::TooManyEntries
            | ArchiveValidationError::ExtractedBytesExceeded
            | ArchiveValidationError::SingleFileBytesExceeded(_) => Self::LimitExceeded,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_bytes() -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("info.json", options).unwrap();
        writer.write_all(br#"{"name":"Fox"}"#).unwrap();
        writer.add_directory("img/", options).unwrap();
        writer.start_file("img/frame.png", options).unwrap();
        writer.write_all(b"png").unwrap();
        writer.finish().unwrap().into_inner()
    }

    fn tar_bytes() -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_path("info.json").unwrap();
        header.set_size(14);
        header.set_cksum();
        builder.append(&header, &br#"{"name":"Fox"}"#[..]).unwrap();
        builder.into_inner().unwrap()
    }

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

    #[test]
    fn detects_and_reads_zip_package() {
        let bytes = zip_bytes();
        assert_eq!(detect_archive_format(&bytes), Some(ArchiveFormat::Zip));
        let mut reader = open_archive(&bytes).unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(
            reader.read_entry("info.json").unwrap(),
            br#"{"name":"Fox"}"#
        );
    }

    #[test]
    fn detects_and_reads_tar_package() {
        let bytes = tar_bytes();
        assert_eq!(detect_archive_format(&bytes), Some(ArchiveFormat::Tar));
        let mut reader = open_archive(&bytes).unwrap();
        assert_eq!(
            reader.read_entry("info.json").unwrap(),
            br#"{"name":"Fox"}"#
        );
    }

    #[test]
    fn extracts_zip_with_safe_paths_and_limits() {
        let temp = tempfile::tempdir().unwrap();
        let mut reader = ZipArchiveReader::from_bytes(&zip_bytes()).unwrap();
        let report = extract_archive(&mut reader, temp.path()).unwrap();
        assert_eq!(report.file_count, 2);
        assert_eq!(
            std::fs::read(temp.path().join("img/frame.png")).unwrap(),
            b"png"
        );
    }

    #[test]
    fn rejects_unsafe_zip_entry() {
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        writer
            .start_file("../escape.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"x").unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let mut reader = ZipArchiveReader::from_bytes(&bytes).unwrap();
        assert!(matches!(reader.entries(), Err(ArchiveError::UnsafePath(_))));
    }

    #[test]
    fn rejects_oversized_single_entry_before_reading() {
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        writer
            .start_file("large.bin", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"12345").unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let limits = ArchiveValidationLimits {
            max_single_file_bytes: 4,
            ..Default::default()
        };
        let mut reader = ZipArchiveReader::from_bytes(&bytes)
            .unwrap()
            .with_limits(limits);
        assert!(matches!(
            reader.read_entry("large.bin"),
            Err(ArchiveError::LimitExceeded)
        ));
    }
}
