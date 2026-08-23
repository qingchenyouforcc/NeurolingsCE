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

/// 从设置读取更新流量代理配置。
fn proxy_from_settings(
    settings: &crate::settings::Settings,
) -> neurolings_store::network::ProxySpec {
    neurolings_store::network::ProxySpec {
        mode: settings.get_string("update/proxyMode", "system"),
        host: settings.get_string("update/proxyHost", ""),
        port: settings.get_i64("update/proxyPort", 8080).clamp(1, 65535) as u16,
        username: settings.get_string("update/proxyUsername", ""),
        password: settings.get_string("update/proxyPassword", ""),
    }
}

/// 指定版本是否被忽略或处于稍后提醒抑制期。
fn is_suppressed(settings: &crate::settings::Settings, version: &str) -> bool {
    if settings.get_string("update/ignoredVersion", "") == version {
        return true;
    }
    if settings.get_string("update/remindVersion", "") == version {
        let remind_at = settings.get_string("update/remindAt", "");
        let remind_epoch = remind_at
            .parse::<i64>()
            .ok()
            .or_else(|| {
                chrono::DateTime::parse_from_rfc3339(&remind_at)
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
    let proxy = proxy_from_settings(settings);
    let manifest = match neurolings_store::updater::fetch_manifest_with_proxy(
        MANIFEST_URL,
        15_000,
        Some(&proxy),
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
            let suppressed = is_suppressed(settings, &manifest.version);
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

/// 下载当前清单版本的平台资产（SHA-256 校验）并登记为可安装。
pub fn download(
    app_data_dir: &std::path::Path,
    settings: &crate::settings::Settings,
) -> Result<Value, String> {
    {
        let mut s = status().lock().unwrap();
        if s.downloading {
            return Err("A download is already in progress".into());
        }
        s.downloading = true;
    }
    let proxy = proxy_from_settings(settings);
    let manifest =
        neurolings_store::updater::fetch_manifest_with_proxy(MANIFEST_URL, 15_000, Some(&proxy))
            .inspect_err(|_e| {
                status().lock().unwrap().downloading = false;
            })?;
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
    let result =
        neurolings_store::updater::download_update_with_proxy(&asset, &dest, 300_000, Some(&proxy));
    let mut s = status().lock().unwrap();
    s.downloading = false;
    result.inspect_err(|e| {
        s.error = e.clone();
    })?;
    s.downloaded_version = manifest.version.clone();
    s.downloaded_path = dest.to_string_lossy().into_owned();
    s.downloaded_sha256 = asset.sha256.clone();
    Ok(json!({
        "downloaded": true,
        "version": s.downloaded_version,
        "path": s.downloaded_path,
    }))
}

/// 安装已下载的更新：SHA-256 复检后启动安装器（MSI/EXE）。
pub fn install(settings: &crate::settings::Settings) -> Result<Value, String> {
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
    let _ = settings;
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
    Ok(json!({ "launched": true, "version": version }))
}
