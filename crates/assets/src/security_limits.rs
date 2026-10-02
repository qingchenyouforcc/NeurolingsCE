/// 资源、归档和传输的统一安全上限。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecurityLimits;

impl SecurityLimits {
    /// Local IPC 单条消息上限。
    pub const IPC_MESSAGE_MAX_BYTES: usize = 1024 * 1024;
    /// HTTP JSON 请求体上限。
    pub const HTTP_JSON_BODY_MAX_BYTES: usize = 1024 * 1024;
    /// `.mascot` 压缩包文件上限。
    pub const MASCOT_PACKAGE_MAX_BYTES: u64 = 100 * 1024 * 1024;
    /// 解包后所有文件的累计大小上限。
    pub const MASCOT_EXTRACTED_MAX_BYTES: u64 = 100 * 1024 * 1024;
    /// 单个 mascot 文件上限。
    pub const MASCOT_SINGLE_FILE_MAX_BYTES: u64 = 16 * 1024 * 1024;
    /// 单个音频文件上限。
    pub const MASCOT_AUDIO_FILE_MAX_BYTES: u64 = 16 * 1024 * 1024;
    /// 单张图像像素数上限。
    pub const MASCOT_IMAGE_MAX_PIXELS: u64 = 4096 * 4096;
    /// 所有图像像素数累计上限。
    pub const MASCOT_IMAGE_TOTAL_MAX_PIXELS: u64 = 256 * 1024 * 1024;
    /// 单个归档最多允许的 entry 数量。
    pub const MASCOT_ZIP_ENTRY_MAX_COUNT: usize = 4096;
}

#[cfg(test)]
mod tests {
    use super::SecurityLimits;

    #[test]
    fn matches_documented_limits() {
        assert_eq!(SecurityLimits::MASCOT_PACKAGE_MAX_BYTES, 100 * 1024 * 1024);
        assert_eq!(
            SecurityLimits::MASCOT_SINGLE_FILE_MAX_BYTES,
            16 * 1024 * 1024
        );
        assert_eq!(SecurityLimits::MASCOT_ZIP_ENTRY_MAX_COUNT, 4096);
    }
}
