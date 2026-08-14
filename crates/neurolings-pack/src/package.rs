//! .mascot 包格式。
//!
//! .mascot 文件是 ZIP 压缩包，内含 info.json、actions.xml、behaviors.xml、
//! img/*.png、可选的 sound/ 音效与可选的 bubble_context.txt。

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{PackError, Result};
use crate::limits;
use crate::metadata::{MascotMetadata, metadata_from_json, metadata_to_json};
use crate::safepath::{absolute_path, clean_path, safe_child_path};
use crate::zipio::{self, RawArchiveEntry};

const FORBIDDEN_EXTENSIONS: &[&str] = &[
    ".exe", ".dll", ".com", ".bat", ".cmd", ".ps1", ".sh", ".js", ".vbs", ".lnk", ".scr", ".pif",
    ".msi", ".msp", ".hta", ".jar",
];
const NESTED_ARCHIVE_EXTENSIONS: &[&str] = &[
    ".zip", ".mascot", ".rar", ".7z", ".tar", ".gz", ".bz2", ".xz", ".tgz", ".cab", ".iso", ".apk",
    ".war", ".ear",
];
const ALLOWED_SOUND_EXTENSIONS: &[&str] =
    &[".wav", ".mp3", ".ogg", ".flac", ".m4a", ".aac", ".opus"];
const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/// .mascot 包的校验报告。
///
/// 序列化为 snake_case 字段；CLI 输出时把 metadata 包在契约的
/// mascot 键下。
#[derive(Debug, Clone, Default, Serialize)]
pub struct MascotPackageReport {
    pub ok: bool,
    pub metadata: MascotMetadata,
    pub entry_count: u64,
    pub file_count: u64,
    pub extracted_bytes: u64,
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// 压缩包路径工具
// ---------------------------------------------------------------------------

/// 归一化压缩包条目路径：反斜杠转为 `/`，空组件、`.`/`..` 组件
/// 与含 `:` 的组件一律拒绝。
pub fn normalized_archive_path(path: &str) -> Option<String> {
    let replaced: String = path
        .chars()
        .map(|c| if c == '\\' { '/' } else { c })
        .collect();
    let mut clean_parts: Vec<&str> = Vec::new();
    for part in replaced.split('/') {
        if part.is_empty() {
            continue;
        }
        if part == "." || part == ".." || part.contains(':') {
            return None;
        }
        clean_parts.push(part);
    }
    Some(clean_parts.join("/"))
}

/// 判断归一化后的路径是否属于受支持的载荷内容。
pub fn is_supported_package_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower == "info.json"
        || lower == "bubble_context.txt"
        || lower == "actions.xml"
        || lower == "behaviors.xml"
        || (lower.starts_with("img/") && lower.ends_with(".png"))
        || lower.starts_with("sound/")
}

/// 判断小写路径是否以禁止的可执行文件或嵌套压缩包扩展名结尾。
pub fn is_forbidden_payload_path(lower_path: &str) -> bool {
    FORBIDDEN_EXTENSIONS
        .iter()
        .chain(NESTED_ARCHIVE_EXTENSIONS)
        .any(|extension| lower_path.ends_with(extension))
}

fn is_allowed_sound_extension(lower_path: &str) -> bool {
    ALLOWED_SOUND_EXTENSIONS
        .iter()
        .any(|extension| lower_path.ends_with(extension))
}

/// 检查原始条目路径是否为允许的包条目，并返回其归一化形式。
pub fn is_allowed_package_entry_path(raw_path: &str) -> Option<(String, bool)> {
    let is_directory = raw_path.ends_with('/') || raw_path.ends_with('\\');
    let normalized = normalized_archive_path(raw_path)?;
    if is_directory {
        return Some((normalized, true));
    }
    let lower = normalized.to_ascii_lowercase();
    if !is_supported_package_path(&normalized) {
        return None;
    }
    if lower.starts_with("sound/") && !is_allowed_sound_extension(&lower) {
        return None;
    }
    Some((normalized, false))
}

/// 替换控制字符并截断过长的条目名，确保可安全嵌入错误消息。
pub fn sanitize_entry_name_for_report(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| {
            if (c as u32) < 32 || c as u32 == 0x7f {
                '_'
            } else {
                c
            }
        })
        .collect();
    if sanitized.len() > 200 {
        let mut cut = 197;
        while cut > 0 && !sanitized.is_char_boundary(cut) {
            cut -= 1;
        }
        format!("{}...", &sanitized[..cut])
    } else {
        sanitized
    }
}

/// 从文件前 24 字节解析 PNG 尺寸。
pub fn png_header_dimensions(header: &[u8]) -> Option<(u64, u64)> {
    if header.len() < 24 || header[..8] != PNG_SIGNATURE || &header[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([header[16], header[17], header[18], header[19]]) as u64;
    let height = u32::from_be_bytes([header[20], header[21], header[22], header[23]]) as u64;
    Some((width, height))
}

/// 按归一化路径返回对应的字节预算上限。
pub fn max_size_for_package_path(path: &str) -> u64 {
    if path.to_ascii_lowercase().starts_with("sound/") {
        limits::MASCOT_AUDIO_FILE_MAX_BYTES
    } else {
        limits::MASCOT_SINGLE_FILE_MAX_BYTES
    }
}

fn validate_png_dimensions(label: &str, header: &[u8]) -> Result<()> {
    let Some((width, height)) = png_header_dimensions(header) else {
        return Err(PackError::msg(format!("Image {label} is not a valid PNG")));
    };
    if width == 0 || height == 0 {
        return Err(PackError::msg(format!(
            "Image {label} has invalid dimensions"
        )));
    }
    if width > limits::MASCOT_IMAGE_MAX_PIXELS / height {
        return Err(PackError::msg(format!(
            "Image {label} exceeds the maximum pixel count of {}",
            limits::MASCOT_IMAGE_MAX_PIXELS
        )));
    }
    let pixels = width * height;
    if pixels > limits::MASCOT_IMAGE_MAX_PIXELS {
        return Err(PackError::msg(format!(
            "Image {label} exceeds the maximum pixel count of {}",
            limits::MASCOT_IMAGE_MAX_PIXELS
        )));
    }
    Ok(())
}

pub(crate) fn validate_image_file(file_path: &Path) -> Result<()> {
    let mut file = File::open(file_path).map_err(|_| {
        PackError::msg(format!(
            "Could not inspect image dimensions for {}",
            file_path.display()
        ))
    })?;
    let mut header = [0u8; 24];
    let read = file.read(&mut header).unwrap_or(0);
    validate_png_dimensions(&file_path.display().to_string(), &header[..read])
}

fn ensure_file_size_at_most(path: &Path, max_bytes: u64, label: &str) -> Result<()> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => metadata,
        _ => return Err(PackError::msg(format!("{label} does not exist"))),
    };
    if metadata.len() > max_bytes {
        return Err(PackError::msg(format!(
            "{label} exceeds the maximum size of {max_bytes} bytes"
        )));
    }
    Ok(())
}

/// 检查包文件存在且不超过包尺寸上限。
pub fn ensure_package_file_acceptable(package_path: &Path) -> Result<()> {
    ensure_file_size_at_most(
        package_path,
        limits::MASCOT_PACKAGE_MAX_BYTES,
        "Mascot package",
    )
}

// ---------------------------------------------------------------------------
// 包命名
// ---------------------------------------------------------------------------

/// 把模板名清理为可移植的包基础名。
pub fn sanitized_package_base_name(name: &str) -> String {
    let mut result: String = name.trim().chars().collect();
    const INVALID: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    for ch in INVALID {
        result = result.replace(*ch, "_");
    }
    result = result
        .chars()
        .map(|c| if (c as u32) < 32 { '_' } else { c })
        .collect();
    let mut result = result.trim().to_string();
    while result.ends_with('.') {
        result.pop();
    }
    if result.is_empty() {
        result = "Mascot".to_string();
    }
    result
}

/// 判断模板名是否可作包名（含 Windows 设备名限制）。
pub fn is_valid_package_name(name: &str) -> bool {
    if name.trim().is_empty() {
        return false;
    }
    let base_name = sanitized_package_base_name(name);
    if base_name.len() > limits::PORTABLE_PACKAGE_BASE_NAME_MAX_UTF8_BYTES {
        return false;
    }
    // 取第一个 `.` 之前的部分；无 `.` 时取整串。
    let device_name = base_name
        .split('.')
        .next()
        .unwrap_or(&base_name)
        .to_ascii_uppercase();
    if matches!(device_name.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return false;
    }
    if device_name.len() == 4
        && (device_name.starts_with("COM") || device_name.starts_with("LPT"))
        && device_name.as_bytes()[3].is_ascii_digit()
        && device_name.as_bytes()[3] != b'0'
    {
        return false;
    }
    true
}

/// 模板名对应的存储包路径。
pub fn package_path_for_name(storage_path: &Path, name: &str) -> PathBuf {
    storage_path.join(format!("{}.mascot", sanitized_package_base_name(name)))
}

/// 模板解压后的缓存目录。
pub fn cache_path_for_name(cache_root_path: &Path, name: &str) -> PathBuf {
    cache_root_path.join(sanitized_package_base_name(name))
}

// ---------------------------------------------------------------------------
// Zip 打开与读取工具
// ---------------------------------------------------------------------------

/// 打开包文件，强制执行尺寸与条目数量上限。
fn open_package(
    package_path: &Path,
) -> Result<(zip::ZipArchive<zipio::ZipReader>, Vec<RawArchiveEntry>)> {
    ensure_package_file_acceptable(package_path)?;
    let mut archive = zipio::open_zip(package_path)?;
    if archive.len() > limits::MASCOT_ZIP_ENTRY_MAX_COUNT {
        return Err(PackError::msg(format!(
            "Archive contains too many entries ({}, maximum {})",
            archive.len(),
            limits::MASCOT_ZIP_ENTRY_MAX_COUNT
        )));
    }
    let entries = zipio::read_raw_archive_entries(&mut archive)?;
    Ok((archive, entries))
}

/// 按归一化路径（不区分大小写）读取单个包内文件。
fn read_package_file(package_path: &Path, wanted_path: &str) -> Result<Vec<u8>> {
    let (mut archive, entries) = open_package(package_path)?;
    let Some(wanted_normalized) = normalized_archive_path(wanted_path) else {
        return Err(PackError::msg(format!("Package is missing {wanted_path}")));
    };
    let Some(index) = zipio::find_entry_by_normalized_path(&entries, &wanted_normalized) else {
        return Err(PackError::msg(format!("Package is missing {wanted_path}")));
    };
    let max_bytes = max_size_for_package_path(&wanted_normalized);
    let bytes = zipio::read_zip_entry_bytes(&mut archive, index, max_bytes)
        .map_err(|_| PackError::msg(format!("Could not read {wanted_path}")))?;
    Ok(bytes)
}

/// 读取包内文件的前几个字节（检查 PNG 头用）。
fn read_package_file_header(
    package_path: &Path,
    wanted_path: &str,
    byte_limit: u64,
) -> Result<Vec<u8>> {
    let (mut archive, entries) = open_package(package_path)?;
    let wanted_normalized = normalized_archive_path(wanted_path)
        .ok_or_else(|| PackError::msg(format!("Package is missing {wanted_path}")))?;
    let index = zipio::find_entry_by_normalized_path(&entries, &wanted_normalized)
        .ok_or_else(|| PackError::msg(format!("Package is missing {wanted_path}")))?;
    zipio::read_zip_entry_header(&mut archive, index, byte_limit)
        .map_err(|_| PackError::msg(format!("Could not read {wanted_path}")))
}

// ---------------------------------------------------------------------------
// 检查 / 校验 / 解压 / 写入 / 安装
// ---------------------------------------------------------------------------

/// 检查 .mascot 包并返回其元数据。
pub fn inspect_package(package_path: &Path) -> Result<MascotMetadata> {
    let info_json = read_package_file(package_path, "info.json")?;
    let metadata = metadata_from_json(&info_json)?;

    let (_, entries) = open_package(package_path)?;
    let mut has_actions = false;
    let mut has_behaviors = false;
    let mut has_image = false;
    let mut image_paths = Vec::new();
    for entry in &entries {
        let Some(path) = normalized_archive_path(&entry.name) else {
            continue;
        };
        let lower = path.to_ascii_lowercase();
        has_actions |= lower == "actions.xml";
        has_behaviors |= lower == "behaviors.xml";
        let is_image = lower.starts_with("img/") && lower.ends_with(".png");
        has_image |= is_image;
        if is_image {
            image_paths.push(path);
        }
    }
    if !has_actions || !has_behaviors || !has_image {
        return Err(PackError::msg(
            "Package must contain actions.xml, behaviors.xml, and img/*.png",
        ));
    }
    for image_path in image_paths {
        let header = read_package_file_header(package_path, &image_path, 24)?;
        validate_png_dimensions(&image_path, &header)?;
    }
    Ok(metadata)
}

/// 校验 .mascot 包并生成完整报告。不会硬性失败；问题收集在
/// report.errors 中。
pub fn validate_package(package_path: &Path) -> MascotPackageReport {
    let mut report = MascotPackageReport::default();

    if !package_path.is_file() {
        report
            .errors
            .push("Mascot package does not exist".to_string());
        return report;
    }
    if let Err(error) = ensure_package_file_acceptable(package_path) {
        report.errors.push(error.to_string());
        return report;
    }

    let raw_entries = match zipio::open_zip(package_path)
        .and_then(|mut archive| zipio::read_raw_archive_entries(&mut archive))
    {
        Ok(entries) => entries,
        Err(error) => {
            report.errors.push(error.to_string());
            return report;
        }
    };
    report.entry_count = raw_entries.len() as u64;

    let mut has_info = false;
    let mut has_actions = false;
    let mut has_behaviors = false;
    let mut has_image = false;
    let mut image_paths: Vec<String> = Vec::new();
    let mut total_image_pixels: u64 = 0;
    let mut total_uncompressed_bytes: u64 = 0;
    let mut file_count: u64 = 0;

    for raw_entry in &raw_entries {
        let Some(normalized) = normalized_archive_path(&raw_entry.name) else {
            report.errors.push(format!(
                "Unsupported or unsafe package entry: {}",
                sanitize_entry_name_for_report(&raw_entry.name)
            ));
            continue;
        };
        if raw_entry.is_directory {
            continue;
        }
        file_count += 1;
        if raw_entry.uncompressed_size
            > limits::MASCOT_EXTRACTED_MAX_BYTES - total_uncompressed_bytes
        {
            report
                .errors
                .push("Package extracted data is too large".to_string());
            break;
        }
        total_uncompressed_bytes += raw_entry.uncompressed_size;
        let max_entry_bytes = max_size_for_package_path(&normalized);
        if raw_entry.uncompressed_size > max_entry_bytes {
            report.errors.push(format!(
                "Package entry {} exceeds size limits",
                sanitize_entry_name_for_report(&normalized)
            ));
        }
        let lower = normalized.to_ascii_lowercase();
        if is_forbidden_payload_path(&lower) {
            report.errors.push(format!(
                "Package contains a forbidden payload entry: {}",
                sanitize_entry_name_for_report(&normalized)
            ));
            continue;
        }
        let sound_rejected =
            lower.starts_with("sound/") && is_allowed_package_entry_path(&raw_entry.name).is_none();
        if !is_supported_package_path(&normalized) || sound_rejected {
            report.errors.push(format!(
                "Unsupported or unsafe package entry: {}",
                sanitize_entry_name_for_report(&normalized)
            ));
            continue;
        }
        has_info |= lower == "info.json";
        has_actions |= lower == "actions.xml";
        has_behaviors |= lower == "behaviors.xml";
        let is_image = lower.starts_with("img/") && lower.ends_with(".png");
        has_image |= is_image;
        if is_image {
            image_paths.push(normalized);
        }
    }
    report.file_count = file_count;
    report.extracted_bytes = total_uncompressed_bytes;

    if !has_info {
        report
            .errors
            .push("Package must contain info.json".to_string());
    }
    if !has_actions {
        report
            .errors
            .push("Package must contain actions.xml".to_string());
    }
    if !has_behaviors {
        report
            .errors
            .push("Package must contain behaviors.xml".to_string());
    }
    if !has_image {
        report
            .errors
            .push("Package must contain img/*.png".to_string());
    }

    if has_info {
        match read_package_file(package_path, "info.json") {
            Ok(info_json) => match metadata_from_json(&info_json) {
                Ok(metadata) => report.metadata = metadata,
                Err(error) => report.errors.push(error.to_string()),
            },
            Err(error) => report.errors.push(error.to_string()),
        }
    }

    for image_path in image_paths {
        let header = match read_package_file_header(package_path, &image_path, 24) {
            Ok(header) => header,
            Err(error) => {
                report.errors.push(error.to_string());
                continue;
            }
        };
        let Some((width, height)) = png_header_dimensions(&header) else {
            report
                .errors
                .push(format!("Image {image_path} is not a valid PNG"));
            continue;
        };
        if width == 0 || height == 0 {
            report
                .errors
                .push(format!("Image {image_path} has invalid dimensions"));
            continue;
        }
        if width > limits::MASCOT_IMAGE_MAX_PIXELS / height {
            report.errors.push(format!(
                "Image {image_path} exceeds the maximum pixel count of {}",
                limits::MASCOT_IMAGE_MAX_PIXELS
            ));
            continue;
        }
        let pixels = width * height;
        if pixels > limits::MASCOT_IMAGE_MAX_PIXELS {
            report.errors.push(format!(
                "Image {image_path} exceeds the maximum pixel count of {}",
                limits::MASCOT_IMAGE_MAX_PIXELS
            ));
            continue;
        }
        total_image_pixels += pixels;
        if total_image_pixels > limits::MASCOT_IMAGE_TOTAL_MAX_PIXELS {
            report.errors.push(format!(
                "Package image data exceeds the total pixel budget of {}",
                limits::MASCOT_IMAGE_TOTAL_MAX_PIXELS
            ));
            break;
        }
    }

    if report.errors.is_empty() {
        let temp_dir = match tempfile::tempdir() {
            Ok(temp_dir) => temp_dir,
            Err(_) => {
                report
                    .errors
                    .push("Could not create temporary extraction directory".to_string());
                return report;
            }
        };
        match open_package(package_path) {
            Ok((mut archive, entries)) => {
                let mut added = false;
                let mut supported: Vec<usize> = Vec::new();
                for (index, entry) in entries.iter().enumerate() {
                    let Some(path) = normalized_archive_path(&entry.name) else {
                        continue;
                    };
                    if !is_supported_package_path(&path) {
                        continue;
                    }
                    supported.push(index);
                    added = true;
                }
                if !added {
                    report
                        .errors
                        .push("Package does not contain any supported files".to_string());
                } else if let Err(error) =
                    extract_zip_entries(&mut archive, &entries, &supported, temp_dir.path())
                {
                    report.errors.push(error.to_string());
                } else if let Err(error) =
                    crate::legacy::validate_extracted_directory(temp_dir.path())
                {
                    report.errors.push(error.to_string());
                }
            }
            Err(error) => report.errors.push(error.to_string()),
        }
    }

    report.ok = report.errors.is_empty();
    report
}

/// 把 .mascot 包中受支持的载荷解压到 output_path；输出目录已存在时
/// 会被替换。
pub fn extract_package(package_path: &Path, output_path: &Path) -> Result<()> {
    let (mut archive, entries) = open_package(package_path)?;

    if output_path.exists() {
        fs::remove_dir_all(output_path)?;
    }
    fs::create_dir_all(output_path)?;

    let mut supported: Vec<usize> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let Some(path) = normalized_archive_path(&entry.name) else {
            continue;
        };
        if !is_supported_package_path(&path) {
            continue;
        }
        supported.push(index);
    }
    if supported.is_empty() {
        return Err(PackError::msg(
            "Package does not contain any supported files",
        ));
    }
    extract_zip_entries(&mut archive, &entries, &supported, output_path)
}

/// 把 zip 条目写入 output，带 zip-slip 防护与桌宠尺寸限额。
fn extract_zip_entries(
    archive: &mut zip::ZipArchive<zipio::ZipReader>,
    entries: &[RawArchiveEntry],
    indices: &[usize],
    output: &Path,
) -> Result<()> {
    let mut total_bytes: u64 = 0;
    for index in indices {
        let entry = &entries[*index];
        let Some(normalized) = normalized_archive_path(&entry.name) else {
            continue;
        };
        if entry.is_directory {
            continue;
        }
        let data =
            zipio::read_zip_entry_bytes(archive, *index, limits::MASCOT_SINGLE_FILE_MAX_BYTES)
                .map_err(|_| PackError::msg("Mascot package entry is too large"))?;
        let chunk = data.len() as u64;
        if total_bytes > limits::MASCOT_EXTRACTED_MAX_BYTES
            || chunk > limits::MASCOT_EXTRACTED_MAX_BYTES - total_bytes
        {
            return Err(PackError::msg("Mascot package extracted data is too large"));
        }
        total_bytes += chunk;
        let Some(target) = safe_child_path(output, &normalized) else {
            return Err(PackError::msg("Unsafe package extraction path"));
        };
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target, &data)?;
    }
    Ok(())
}

/// 从解压的桌宠目录创建 .mascot 包。
pub fn write_package_from_directory(source_path: &Path, package_path: &Path) -> Result<()> {
    let source_info = match fs::metadata(source_path) {
        Ok(metadata) if metadata.is_dir() => metadata,
        _ => return Err(PackError::msg("Source mascot directory does not exist")),
    };
    let _ = source_info;

    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let source_root = clean_path(&absolute_path(source_path));
    for dir_entry in walkdir::WalkDir::new(&source_root) {
        let dir_entry = dir_entry?;
        if !dir_entry.file_type().is_file() {
            continue;
        }
        let file_path = dir_entry.path();
        let relative = file_path
            .strip_prefix(&source_root)
            .map_err(|_| PackError::msg("Could not read package source"))?
            .to_string_lossy()
            .replace('\\', "/");
        let Some(relative) = normalized_archive_path(&relative) else {
            continue;
        };
        if !is_supported_package_path(&relative) {
            continue;
        }

        if entries.len() >= limits::MASCOT_ZIP_ENTRY_MAX_COUNT {
            return Err(PackError::msg("Mascot package contains too many entries"));
        }
        ensure_file_size_at_most(
            file_path,
            max_size_for_package_path(&relative),
            "Mascot package file",
        )?;
        if relative.to_ascii_lowercase().starts_with("img/") {
            validate_image_file(file_path)?;
        }
        let data = fs::read(file_path)
            .map_err(|_| PackError::msg(format!("Could not read {}", file_path.display())))?;
        entries.push((relative, data));
    }
    write_validated_package_entries(&mut entries, package_path)
}

/// 用内存中的条目（归一化相对路径 → 文件字节）在 package_path 处创建
/// .mascot 包。校验规则与目录打包一致：路径安全、尺寸限额、img/ 载荷的
/// PNG 头检查与必需文件存在性。
pub fn write_package_from_memory(entries: &[(String, Vec<u8>)], package_path: &Path) -> Result<()> {
    let mut collected: Vec<(String, Vec<u8>)> = Vec::with_capacity(entries.len());
    for (raw_name, data) in entries {
        let Some(relative) = normalized_archive_path(raw_name) else {
            continue;
        };
        if !is_supported_package_path(&relative) {
            continue;
        }
        if collected.len() >= limits::MASCOT_ZIP_ENTRY_MAX_COUNT {
            return Err(PackError::msg("Mascot package contains too many entries"));
        }
        let max_bytes = max_size_for_package_path(&relative);
        if data.len() as u64 > max_bytes {
            return Err(PackError::msg(format!(
                "Mascot package file {relative} exceeds the maximum size of {max_bytes} bytes"
            )));
        }
        if relative.to_ascii_lowercase().starts_with("img/") {
            let _ = validate_png_dimensions(&relative, data);
        }
        collected.push((relative, data.clone()));
    }
    write_validated_package_entries(&mut collected, package_path)
}

/// 打包流程的公共收尾：排序条目、校验范围与必需文件，然后写出 zip。
fn write_validated_package_entries(
    entries: &mut [(String, Vec<u8>)],
    package_path: &Path,
) -> Result<()> {
    entries.sort_by(|lhs, rhs| lhs.0.cmp(&rhs.0));
    validate_zip_entry_ranges(entries)?;

    let mut has_info = false;
    let mut has_actions = false;
    let mut has_behaviors = false;
    let mut has_image = false;
    for (name, _) in entries.iter() {
        let lower = name.to_ascii_lowercase();
        has_info |= lower == "info.json";
        has_actions |= lower == "actions.xml";
        has_behaviors |= lower == "behaviors.xml";
        has_image |= lower.starts_with("img/");
    }
    if !has_info || !has_actions || !has_behaviors || !has_image {
        return Err(PackError::msg(
            "Mascot package source is missing required files",
        ));
    }

    if let Some(parent) = package_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(package_path)
        .map_err(|_| PackError::msg(format!("Could not write {}", package_path.display())))?;
    let mut zip_writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in entries.iter() {
        zip_writer
            .start_file(name, options)
            .map_err(|error| PackError::msg(format!("Could not write package entry: {error}")))?;
        zip_writer.write_all(data)?;
    }
    zip_writer
        .finish()
        .map_err(|error| PackError::msg(format!("Could not write package entry: {error}")))?;
    Ok(())
}

fn validate_zip_entry_ranges(entries: &[(String, Vec<u8>)]) -> Result<()> {
    if entries.len() > limits::MASCOT_ZIP_ENTRY_MAX_COUNT || entries.len() > u16::MAX as usize {
        return Err(PackError::msg("Mascot package contains too many entries"));
    }
    let mut total_size: u64 = 0;
    for (name, data) in entries {
        if name.len() > u16::MAX as usize {
            return Err(PackError::msg("Mascot package entry path is too long"));
        }
        if data.len() as u64 > u32::MAX as u64 {
            return Err(PackError::msg(
                "Mascot package entry is too large for ZIP32",
            ));
        }
        total_size += data.len() as u64;
        if total_size > limits::MASCOT_PACKAGE_MAX_BYTES {
            return Err(PackError::msg(format!(
                "Mascot package exceeds the maximum size of {} bytes",
                limits::MASCOT_PACKAGE_MAX_BYTES
            )));
        }
    }
    Ok(())
}

/// 把 .mascot 包安装到存储并返回安装的模板名。
pub fn install_package(package_path: &Path, storage_path: &Path) -> Result<String> {
    ensure_package_file_acceptable(package_path)?;
    let metadata = inspect_package(package_path)?;
    fs::create_dir_all(storage_path)?;
    let target_path = package_path_for_name(storage_path, &metadata.name);
    let source_absolute = clean_path(&absolute_path(package_path));
    let target_absolute = clean_path(&absolute_path(&target_path));
    if source_absolute != target_absolute {
        let _ = fs::remove_file(&target_path);
        fs::copy(&source_absolute, &target_path)
            .map_err(|_| PackError::msg("Could not copy package into mascot storage"))?;
    }
    Ok(metadata.name)
}

/// 目录缺少 info.json 时以兜底元数据补写。
pub(crate) fn ensure_legacy_metadata(source_path: &Path, fallback_name: &str) {
    let info_path = source_path.join("info.json");
    if info_path.exists() {
        return;
    }
    let metadata = MascotMetadata {
        name: fallback_name.to_string(),
        ..Default::default()
    };
    let _ = fs::write(&info_path, metadata_to_json(&metadata));
}

/// 把旧版（已解压的）桌宠目录打包进存储并返回安装的模板名。
pub fn package_legacy_directory(
    source_path: &Path,
    storage_path: &Path,
    fallback_name: &str,
) -> Result<String> {
    ensure_legacy_metadata(source_path, fallback_name);
    let info_path = source_path.join("info.json");
    let info_bytes =
        fs::read(&info_path).map_err(|_| PackError::msg("Could not read generated info.json"))?;
    let metadata = match metadata_from_json(&info_bytes) {
        Ok(metadata) => metadata,
        Err(_) => {
            let metadata = MascotMetadata {
                name: fallback_name.to_string(),
                ..Default::default()
            };
            let _ = fs::write(&info_path, metadata_to_json(&metadata));
            metadata
        }
    };
    fs::create_dir_all(storage_path)?;
    let target_path = package_path_for_name(storage_path, &metadata.name);
    let _ = fs::remove_file(&target_path);
    write_package_from_directory(source_path, &target_path)?;
    Ok(metadata.name)
}

/// 把存储中旧版解压的 *.mascot 目录迁移为包文件；尽力而为，失败忽略。
pub fn migrate_legacy_directories(storage_path: &Path) {
    let Ok(read) = fs::read_dir(storage_path) else {
        return;
    };
    for entry in read.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let dirname = entry.file_name().to_string_lossy().to_string();
        if dirname.len() <= ".mascot".len() || !dirname.to_ascii_lowercase().ends_with(".mascot") {
            continue;
        }
        let fallback_name = &dirname[..dirname.len() - ".mascot".len()];
        let dir_path = entry.path();
        let temp_package = storage_path.join(format!("{fallback_name}.mascot.migrating"));
        ensure_legacy_metadata(&dir_path, fallback_name);
        if write_package_from_directory(&dir_path, &temp_package).is_err() {
            let _ = fs::remove_file(&temp_package);
            continue;
        }
        if fs::remove_dir_all(&dir_path).is_err() {
            let _ = fs::remove_file(&temp_package);
            continue;
        }
        let _ = fs::rename(&temp_package, &dir_path);
    }
}
