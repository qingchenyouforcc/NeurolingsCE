//! 桌宠包的安全上限。

/// `.mascot` 包或可导入压缩包的最大尺寸（100 MiB）。
pub const MASCOT_PACKAGE_MAX_BYTES: u64 = 100 * 1024 * 1024;
/// 解压后全部内容的最大总尺寸（100 MiB）。
pub const MASCOT_EXTRACTED_MAX_BYTES: u64 = 100 * 1024 * 1024;
/// 单个非音频包文件的最大尺寸（16 MiB）。
pub const MASCOT_SINGLE_FILE_MAX_BYTES: u64 = 16 * 1024 * 1024;
/// `sound/` 目录下单个音频文件的最大尺寸（16 MiB）。
pub const MASCOT_AUDIO_FILE_MAX_BYTES: u64 = 16 * 1024 * 1024;
/// 单张 PNG 图像的最大像素数。
pub const MASCOT_IMAGE_MAX_PIXELS: u64 = 4096 * 4096;
/// 单个包内全部 PNG 图像的最大总像素数。
pub const MASCOT_IMAGE_TOTAL_MAX_PIXELS: u64 = 256 * 1024 * 1024;
/// 桌宠 zip 压缩包的最大条目数。
pub const MASCOT_ZIP_ENTRY_MAX_COUNT: usize = 4096;
/// 清理后的包基础名的最大 UTF-8 字节长度。
pub const PORTABLE_PACKAGE_BASE_NAME_MAX_UTF8_BYTES: usize = 200;
