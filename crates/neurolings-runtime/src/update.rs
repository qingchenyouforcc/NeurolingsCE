//! 更新驱动：启动自动检查、手动检查、下载/校验/安装、忽略与稍后提醒。
//! 行为对齐原版 GitHubUpdateManager：启动 1500ms 检查（update/checkOnStartup
//! 默认 true）、代理仅作用于更新流量、ignore/remind 抑制、安装前 SHA-256 复检。

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde_json::{Value, json};

/// 发布清单地址（与原版一致）。
pub const MANIFEST_URL: &str = "https://blog.qingchenyou.asia/NeurolingsCE/update/latest.json";

#[derive(Debug, Clone, Default)]
pub struct UpdateStatus {
    pub checked: bool,
    pub latest_version: String,
    pub notes: String,
    pub release_page: String,
    pub notify: bool,
    pub downloading: bool,
    pub downloaded_version: String,
    pub downloaded_path: String,
    pub downloaded_sha256: String,
    pub error: String,
}

static STATUS: OnceLock<Mutex<UpdateStatus>> = OnceLock::new();

fn status() -> &'static Mutex<UpdateStatus> {
    STATUS.get_or_init(|| Mutex::new(UpdateStatus::default()))
}

/// 后台更新任务所需的设置快照。
///
/// 该值只在线程间传递，不会写入日志、响应或持久化设置。
#[derive(Clone)]
pub struct UpdateRequestConfig {
    proxy: neurolings_store::network::ProxySpec,
    ignored_version: String,
    remind_version: String,
    remind_at: String,
}

/// 下载完成后回到主线程持久化的结果。
pub struct DownloadedUpdate {
    pub version: String,
    pub path: String,
    pub sha256: String,
}

/// 从主线程设置创建后台更新任务快照。
pub fn request_config(settings: &crate::settings::Settings) -> UpdateRequestConfig {
    UpdateRequestConfig {
        proxy: neurolings_store::network::ProxySpec {
            mode: settings.get_string("update/proxyMode", "system"),
            host: settings.get_string("update/proxyHost", ""),
            port: settings.get_i64("update/proxyPort", 8080).clamp(1, 65535) as u16,
            username: settings.get_string("update/proxyUsername", ""),
            password: settings.get_string("update/proxyPassword", ""),
        },
        ignored_version: settings.get_string("update/ignoredVersion", ""),
        remind_version: settings.get_string("update/remindVersion", ""),
        remind_at: settings.get_string("update/remindAt", ""),
    }
}

/// 指定版本是否被忽略或处于稍后提醒抑制期。
fn is_suppressed(config: &UpdateRequestConfig, version: &str) -> bool {
    if config.ignored_version == version {
        return true;
    }
    if config.remind_version == version {
        let remind_epoch = config
            .remind_at
            .parse::<i64>()
            .ok()
            .or_else(|| {
                chrono::DateTime::parse_from_rfc3339(&config.remind_at)
                    .ok()
                    .map(|d| d.timestamp())
            })
            .unwrap_or(0);
        if remind_epoch > chrono::Utc::now().timestamp() {
            return true;
        }
    }
    false
}

/// 执行一次检查；返回是否有需要提示的新版本。
pub fn run_check(settings: &crate::settings::Settings) -> bool {
    run_check_with_config(&request_config(settings))
}

/// 使用启动时快照执行更新检查，可安全在线程中调用。
pub fn run_check_with_config(config: &UpdateRequestConfig) -> bool {
    let manifest = match neurolings_store::updater::fetch_manifest_with_proxy(
        MANIFEST_URL,
        15_000,
        Some(&config.proxy),
    ) {
        Ok(manifest) => manifest,
        Err(e) => {
            let mut s = status().lock().unwrap();
            s.checked = true;
            s.error = e;
            return false;
        }
    };
    let decision =
        neurolings_store::updater::decide(neurolings_common::version::VERSION, &manifest);
    let mut s = status().lock().unwrap();
    s.checked = true;
    s.error.clear();
    match decision {
        neurolings_store::updater::UpdateDecision::UpToDate => {
            s.latest_version.clear();
            s.notify = false;
            false
        }
        neurolings_store::updater::UpdateDecision::Available(_)
        | neurolings_store::updater::UpdateDecision::Mandatory(_) => {
            s.latest_version = manifest.version.clone();
            s.notes = manifest.notes.clone();
            s.release_page = if manifest.release_page.is_empty() {
                "https://github.com/qingchenyouforcc/NeurolingsCE/releases/latest".to_string()
            } else {
                manifest.release_page.clone()
            };
            let suppressed = is_suppressed(config, &manifest.version);
            s.notify = !suppressed;
            !suppressed
        }
    }
}

/// 启动后 1500ms 自动检查（update/checkOnStartup 默认 true）。
pub fn start_startup_check(app_data_dir: PathBuf) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let settings = crate::settings::Settings::load(&app_data_dir);
        if !settings.get_bool("update/checkOnStartup", true) {
            return;
        }
        let notify = run_check(&settings);
        crate::log::info("update", &format!("startup update check: notify={notify}"));
        if notify {
            // 拉起管理器并请求跳转 About 页（原版为托盘气泡通知）。
            crate::services::launch_manager();
            crate::services::request_update_navigate();
        }
    });
}

pub fn status_json() -> Value {
    let s = status().lock().unwrap();
    json!({
        "checked": s.checked,
        "latest_version": s.latest_version,
        "notes": s.notes,
        "release_page": s.release_page,
        "notify": s.notify,
        "downloading": s.downloading,
        "downloaded_version": s.downloaded_version,
        "downloaded_path": s.downloaded_path,
        "error": s.error,
    })
}

/// 标记下载开始。调用方应立即将实际网络和文件 I/O 转入后台 worker。
pub fn begin_download() -> Result<(), String> {
    {
        let mut s = status().lock().unwrap();
        if s.downloading {
            return Err("A download is already in progress".into());
        }
        s.downloading = true;
    }
    Ok(())
}

/// 撤销尚未进入后台队列的下载标记。
///
/// 调度器可能在 `begin_download` 后拒绝任务；此时必须恢复状态，避免前端永久显示
/// 下载进行中。已开始执行的下载不调用本函数。
pub(crate) fn cancel_download_before_start() {
    status().lock().unwrap().downloading = false;
}

/// 在后台下载当前清单版本的平台资产并校验 SHA-256。
pub fn download_with_config(
    app_data_dir: &std::path::Path,
    config: &UpdateRequestConfig,
) -> Result<DownloadedUpdate, String> {
    let manifest = match neurolings_store::updater::fetch_manifest_with_proxy(
        MANIFEST_URL,
        15_000,
        Some(&config.proxy),
    ) {
        Ok(manifest) => manifest,
        Err(error) => {
            let mut s = status().lock().unwrap();
            s.downloading = false;
            s.error = error.clone();
            return Err(error);
        }
    };
    let key = neurolings_store::updater::current_asset_key();
    let Some(asset) = manifest.assets.get(key).cloned() else {
        status().lock().unwrap().downloading = false;
        return Err("No asset for this platform".into());
    };
    let dest_dir = app_data_dir.join("downloads");
    let _ = std::fs::create_dir_all(&dest_dir);
    let file_name = if asset.name.is_empty() {
        "update.bin".to_string()
    } else {
        asset.name.clone()
    };
    let dest = dest_dir.join(file_name);
    let result = neurolings_store::updater::download_update_with_proxy(
        &asset,
        &dest,
        300_000,
        Some(&config.proxy),
    );
    let mut s = status().lock().unwrap();
    s.downloading = false;
    result.inspect_err(|e| {
        s.error = e.clone();
    })?;
    let downloaded = DownloadedUpdate {
        version: manifest.version,
        path: dest.to_string_lossy().into_owned(),
        sha256: asset.sha256,
    };
    s.downloaded_version = downloaded.version.clone();
    s.downloaded_path = downloaded.path.clone();
    s.downloaded_sha256 = downloaded.sha256.clone();
    Ok(downloaded)
}

/// 把已下载安装包信息写入设置（键名与 migrate.rs GROUP_KEYS 一致）。
pub fn persist_downloaded(
    settings: &mut crate::settings::Settings,
    version: &str,
    path: &str,
    sha256: &str,
) -> Result<(), String> {
    settings.set(
        crate::settings::KEY_UPDATE_DOWNLOADED_VERSION,
        json!(version),
    )?;
    settings.set(crate::settings::KEY_UPDATE_DOWNLOADED_PATH, json!(path))?;
    settings.set(crate::settings::KEY_UPDATE_DOWNLOADED_SHA256, json!(sha256))
}

/// 启动时从设置恢复已下载安装包信息（参考实现 GitHubUpdateManager 构造函数）：
/// 安装包文件已不在磁盘上时不恢复，避免指向失效路径。
pub fn restore_downloaded(settings: &crate::settings::Settings) {
    let path = settings.get_string(crate::settings::KEY_UPDATE_DOWNLOADED_PATH, "");
    if path.is_empty() || !std::path::Path::new(&path).is_file() {
        return;
    }
    let mut s = status().lock().unwrap();
    s.downloaded_version = settings.get_string(crate::settings::KEY_UPDATE_DOWNLOADED_VERSION, "");
    s.downloaded_path = path;
    s.downloaded_sha256 = settings.get_string(crate::settings::KEY_UPDATE_DOWNLOADED_SHA256, "");
}

/// 安装已下载的更新：SHA-256 复检后启动安装器（MSI/EXE）。
/// 该函数只读取全局下载状态，因此可在线程中执行。
pub fn install() -> Result<Value, String> {
    let (version, path, sha256) = {
        let s = status().lock().unwrap();
        (
            s.downloaded_version.clone(),
            s.downloaded_path.clone(),
            s.downloaded_sha256.clone(),
        )
    };
    if path.is_empty() {
        return Err("No downloaded installer".into());
    }
    let file = PathBuf::from(&path);
    if !sha256.is_empty() {
        let bytes = std::fs::read(&file).map_err(|e| format!("read_failed: {e}"))?;
        let hex = neurolings_store::network::sha256_bytes(&bytes);
        if !hex.eq_ignore_ascii_case(&sha256) {
            return Err(format!("sha256_mismatch: expected {sha256}, got {hex}"));
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        let lower = path.to_lowercase();
        let spawned = if lower.ends_with(".msi") {
            std::process::Command::new("msiexec")
                .args(["/i", &path])
                .creation_flags(DETACHED_PROCESS)
                .spawn()
        } else {
            std::process::Command::new(&path)
                .creation_flags(DETACHED_PROCESS)
                .spawn()
        };
        spawned.map_err(|e| format!("launch_failed: {e}"))?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new(&path)
            .spawn()
            .map_err(|e| format!("launch_failed: {e}"))?;
    }
    // 安装器已启动：请求运行时优雅退出（停止本地/HTTP API 并退出应用），
    // 否则 exe/dll 被占用会导致安装失败。延迟置位，确保本响应先发回调用方。
    crate::services::request_exit_after_install();
    Ok(json!({ "launched": true, "version": version }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 下载结果写入设置后，重新加载设置可正确恢复到内存状态；
    /// 安装包文件已不在磁盘上时不恢复（恢复函数的既定语义）。
    /// 两个场景共享全局 STATUS，故放在同一测试内顺序执行。
    #[test]
    fn downloaded_installer_persists_and_restores() {
        let dir = tempfile::tempdir().unwrap();
        // 伪造已下载安装包文件（恢复时校验文件存在性）。
        let installer = dir.path().join("update.msi");
        std::fs::write(&installer, b"fake").unwrap();
        let path = installer.to_string_lossy().into_owned();

        let mut settings = crate::settings::Settings::load(dir.path());
        persist_downloaded(&mut settings, "1.2.3", &path, "abc123").unwrap();

        // 模拟重启：重新加载设置并恢复。
        let settings = crate::settings::Settings::load(dir.path());
        restore_downloaded(&settings);
        {
            let s = status().lock().unwrap();
            assert_eq!(s.downloaded_version, "1.2.3");
            assert_eq!(s.downloaded_path, path);
            assert_eq!(s.downloaded_sha256, "abc123");
        }

        // 文件缺失场景：清空内存状态后恢复，应保持为空。
        let dir2 = tempfile::tempdir().unwrap();
        let mut settings2 = crate::settings::Settings::load(dir2.path());
        let missing = dir2.path().join("gone.msi").to_string_lossy().into_owned();
        persist_downloaded(&mut settings2, "9.9.9", &missing, "deadbeef").unwrap();
        {
            let mut s = status().lock().unwrap();
            s.downloaded_version.clear();
            s.downloaded_path.clear();
            s.downloaded_sha256.clear();
        }
        let settings2 = crate::settings::Settings::load(dir2.path());
        restore_downloaded(&settings2);
        let s = status().lock().unwrap();
        assert!(s.downloaded_path.is_empty());
        assert!(s.downloaded_version.is_empty());
        assert!(s.downloaded_sha256.is_empty());
    }
}
