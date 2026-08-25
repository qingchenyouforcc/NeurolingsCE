//! NeurolingsCE 运行时入口。
//!
//! 启动模式：
//! - 普通启动：拉起管理器进程，运行时驻留（无桌宠直到召唤）；
//! - `--neurolingsce-cli-runtime`：CLI 自动拉起，不拉起管理器；
//! - `--neurolingsce-startup`：开机自启，按设置静默恢复组合；
//! - `--smoke [ticks]`：无窗口自检（CI 用）。

mod codex;
mod codex_appserver;
mod combinations;
mod fallthrough;
mod headless;
mod http;
mod ipc;
mod log;
mod migrate;
mod runtime;
mod services;
mod settings;
mod templates;
#[cfg(any(windows, target_os = "macos"))]
mod tray;
mod update;

use std::path::PathBuf;
use std::process::ExitCode;

use neurolings_platform::{Rect, ScreenInfo};
use runtime::RuntimeOptions;
use settings::Settings;

const HELP: &str = "\
NeurolingsCE desktop runtime

Usage:
  NeurolingsCE                     Start the runtime (launches the manager).
  NeurolingsCE --neurolingsce-cli-runtime
                                 Runtime-only mode (started by the CLI).
  NeurolingsCE --neurolingsce-startup
                                 Startup mode (silent, restores combination).
  NeurolingsCE --smoke [ticks]   Headless self-test (no windows).
  NeurolingsCE --mascot-pack-dir <dir>
                                 Load templates from an unpacked mascot dir.
  NeurolingsCE --mascot <name>   Spawn this template at startup.
  NeurolingsCE --version         Show version and exit.
";

fn fake_screen() -> ScreenInfo {
    ScreenInfo {
        monitor: Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
        work_area: Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        },
        scale: 1.0,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let mut smoke_ticks: Option<u64> = None;
    let mut pack_dir: Option<PathBuf> = None;
    let mut spawn_name: Option<String> = None;
    let mut cli_runtime_mode = false;
    let mut startup_mode = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--smoke" => {
                // 可选的帧数参数，默认 200。
                let ticks = args
                    .get(i + 1)
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(200);
                if args.get(i + 1).is_some_and(|s| s.parse::<u64>().is_ok()) {
                    i += 1;
                }
                smoke_ticks = Some(ticks);
            }
            "--mascot-pack-dir" => {
                i += 1;
                pack_dir = args.get(i).map(PathBuf::from);
            }
            "--mascot" => {
                i += 1;
                spawn_name = args.get(i).cloned();
            }
            "--neurolingsce-cli-runtime" => cli_runtime_mode = true,
            "--neurolingsce-startup" => startup_mode = true,
            "--version" | "-v" => {
                println!("NeurolingsCE {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--help" | "-h" => {
                print!("{HELP}");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown option: {other}");
                eprintln!("{HELP}");
                return ExitCode::from(2);
            }
        }
        i += 1;
    }

    let headless = smoke_ticks.is_some();

    // 冒烟测试默认使用内置 fixture，避免触碰用户存储。
    if headless && pack_dir.is_none() {
        let bundled = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mascot_pack"));
        if bundled.is_dir() {
            pack_dir = Some(bundled);
        }
    }

    let mut loaded = if let Some(dir) = &pack_dir {
        templates::load_from_dir(dir)
    } else {
        let storage = match neurolings_pack::storage::default_storage_path() {
            Some(path) => path,
            None => {
                eprintln!("could not resolve mascot storage path");
                return ExitCode::FAILURE;
            }
        };
        let app_data = storage.parent().map(PathBuf::from).unwrap_or_default();
        if !headless {
            // 会话日志尽早初始化，覆盖迁移与模板加载阶段。
            log::init(&app_data);
            // 一次性迁移 CE 旧版注册表数据（设置/组合），并清洗历史键值。
            migrate::run_once(&app_data);
            // 写入存储目录 README 并清理早期版本的落盘默认模板。
            templates::prepare_storage(&storage);
        }
        let cache = storage
            .parent()
            .map(|p| p.join("mascot-cache"))
            .unwrap_or_else(|| storage.join("mascot-cache"));
        let _ = std::fs::create_dir_all(&cache);
        templates::load_from_storage(&storage, &cache)
    };

    // 默认模板为内嵌虚拟模板 Default：不落盘、不可删除。
    // 磁盘上若还留着同名包，丢掉以免工厂重复登记导致启动失败。
    if let Some(default) = templates::load_default_virtual() {
        loaded.retain(|t| !templates::is_default_template(&t.name));
        loaded.insert(0, default);
    }

    if loaded.is_empty() {
        eprintln!("no mascot templates found");
        return ExitCode::FAILURE;
    }
    let count = loaded.len();

    let screen = if headless {
        fake_screen()
    } else {
        match neurolings_platform::create_backend() {
            Ok(backend) => backend
                .screens()
                .into_iter()
                .next()
                .unwrap_or_else(fake_screen),
            Err(_) => fake_screen(),
        }
    };

    // HTTP 由设置项 http/enabled 控制（默认关闭）；托盘菜单语言同源读取。
    let runtime_settings = if headless {
        None
    } else {
        let app_data_dir = neurolings_pack::storage::default_storage_path()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_default();
        Some(Settings::load(&app_data_dir))
    };
    let enable_http = runtime_settings
        .as_ref()
        .is_some_and(|settings| settings.get_bool(settings::KEY_HTTP_ENABLED, false));

    #[cfg(any(windows, target_os = "macos"))]
    let locale = runtime_settings
        .as_ref()
        .map_or(settings::Locale::En, Settings::locale);

    #[cfg(any(windows, target_os = "macos"))]
    if !headless {
        let names: Vec<String> = loaded.iter().map(|t| t.name.clone()).collect();
        tray::init(&names, locale);
    }

    let opts = RuntimeOptions {
        templates: loaded,
        screen,
        spawn_name,
        tick_limit: smoke_ticks,
        headless,
        enable_ipc: !headless,
        enable_http,
        cli_runtime_mode,
        startup_mode,
    };

    // 单实例预检（对齐原版三段逻辑）：已有实例时，
    // 自启/CLI 模式静默退出；普通启动请已有实例弹管理器后退出。
    if !headless {
        let running = ipc::client_call(
            &serde_json::json!({ "command": "ping" }),
            std::time::Duration::from_millis(500),
        )
        .is_ok();
        if running {
            if cli_runtime_mode || startup_mode {
                return ExitCode::SUCCESS;
            }
            let _ = ipc::client_call(
                &serde_json::json!({ "command": "show_manager" }),
                std::time::Duration::from_millis(500),
            );
            return ExitCode::SUCCESS;
        }
    }

    // 捆绑的 agent 技能同步安装（Windows，对齐原版 15 秒超时语义）。
    if !headless {
        sync_bundled_skills();
    }

    // 启动自动更新检查（1500ms 延迟，对齐原版）。
    if !headless {
        let app_data = neurolings_pack::storage::default_storage_path()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_default();
        crate::update::start_startup_check(app_data);
    }

    let result = runtime::run(opts);
    if !headless {
        crate::log::shutdown();
    }
    match result {
        Ok(ticks) => {
            if headless {
                println!("smoke ok: {count} template(s), {ticks} ticks");
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            if err.contains("already running") {
                // 竞态兜底（预检后瞬间被占用）：按启动模式决定静默或弹管理器。
                if cli_runtime_mode || startup_mode {
                    return ExitCode::SUCCESS;
                }
                let _ = ipc::client_call(
                    &serde_json::json!({ "command": "show_manager" }),
                    std::time::Duration::from_millis(500),
                );
                return ExitCode::SUCCESS;
            }
            eprintln!("runtime error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// 安装捆绑的 agent 技能到 Codex home（对齐原版 syncBundledSkillsForCurrentUser）：
/// 找到 neurolingsce-skill/scripts/install_to_codex_home.ps1 后以
/// powershell 运行，启动 5 秒、完成 15 秒超时，超时即终止。
#[cfg(windows)]
fn sync_bundled_skills() {
    use std::process::Command;

    let app_root = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|d| d.to_path_buf()))
        .unwrap_or_default();
    // 打包分发时脚本位于 exe 旁；开发环境回退到仓库内的捆绑目录。
    let mut candidates = vec![
        app_root.join("neurolingsce-skill/scripts/install_to_codex_home.ps1"),
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../neurolingsce-skill/scripts/install_to_codex_home.ps1"
        )),
    ];
    // AppRoot 需指向同时包含 neurolingsce-skill 与 neurolingsce-companion 的根。
    let app_roots = [
        &app_root,
        &PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")),
    ];
    let Some(index) = candidates.iter().position(|c| c.is_file()) else {
        return;
    };
    let script = candidates.remove(index);
    // 打包分发时脚本与 AppRoot 均为 exe 旁；开发环境均为仓库根。
    let selected_root = if index == 0 { &app_root } else { app_roots[1] };
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
    command.arg(&script);
    command.arg("-AppRoot");
    command.arg(selected_root);
    match command.spawn() {
        Ok(mut child) => {
            // 完成超时 15 秒（对齐原版）；启动失败即放弃。
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => {
                        crate::log::info("startup", "bundled skills install finished");
                        break;
                    }
                    Ok(None) if std::time::Instant::now() >= deadline => {
                        let _ = child.kill();
                        crate::log::warn("startup", "bundled skills install timed out");
                        break;
                    }
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
                    Err(e) => {
                        crate::log::warn("startup", &format!("bundled skills install failed: {e}"));
                        break;
                    }
                }
            }
        }
        Err(e) => {
            crate::log::warn(
                "startup",
                &format!("could not start bundled skill install script: {e}"),
            );
        }
    }
}

#[cfg(not(windows))]
fn sync_bundled_skills() {}
