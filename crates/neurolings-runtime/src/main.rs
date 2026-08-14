//! NeurolingsCE 运行时入口。
//!
//! 启动模式：
//! - 普通启动：拉起管理器进程，运行时驻留（无桌宠直到召唤）；
//! - `--neurolingsce-cli-runtime`：CLI 自动拉起，不拉起管理器；
//! - `--neurolingsce-startup`：开机自启，按设置静默恢复组合；
//! - `--smoke [ticks]`：无窗口自检（CI 用）。

mod codex;
mod combinations;
mod fallthrough;
mod headless;
mod http;
mod ipc;
mod runtime;
mod services;
mod settings;
mod templates;
#[cfg(windows)]
mod tray;

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

    let loaded = if let Some(dir) = &pack_dir {
        templates::load_from_dir(dir)
    } else {
        let storage = match neurolings_pack::storage::default_storage_path() {
            Some(path) => path,
            None => {
                eprintln!("could not resolve mascot storage path");
                return ExitCode::FAILURE;
            }
        };
        if !headless {
            templates::install_default_if_missing(&storage);
        }
        let cache = storage
            .parent()
            .map(|p| p.join("mascot-cache"))
            .unwrap_or_else(|| storage.join("mascot-cache"));
        let _ = std::fs::create_dir_all(&cache);
        templates::load_from_storage(&storage, &cache)
    };

    if loaded.is_empty() {
        eprintln!("no mascot templates found");
        return ExitCode::FAILURE;
    }
    let count = loaded.len();

    let screen = if headless {
        fake_screen()
    } else {
        #[cfg(windows)]
        tray::init();
        match neurolings_platform::create_backend() {
            Ok(backend) => backend
                .screens()
                .into_iter()
                .next()
                .unwrap_or_else(fake_screen),
            Err(_) => fake_screen(),
        }
    };

    // HTTP 由设置项 http/enabled 控制（默认关闭）。
    let enable_http = if headless {
        false
    } else {
        let app_data_dir = neurolings_pack::storage::default_storage_path()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_default();
        let settings = Settings::load(&app_data_dir);
        settings.get_bool(settings::KEY_HTTP_ENABLED, false)
    };

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

    match runtime::run(opts) {
        Ok(ticks) => {
            if headless {
                println!("smoke ok: {count} template(s), {ticks} ticks");
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            if err.contains("already running") {
                // 单实例行为：请已有实例拉起管理器后安静退出。
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
