//! Neurolings-rs 构建/运行任务（`cargo xtask <子命令>`）。
//!
//! 子命令：
//! - `build`   构建 Rust 运行时 + CLI（release）、Flutter 管理器，并组装 dist 目录
//! - `run`     构建后启动 dist 中的运行时（自动拉起管理器），并召唤一只默认桌宠
//! - `package` 仅组装 dist 目录（假定已构建）
//! - `dist`    打印 dist 输出目录路径
//!
//! 约定：所有产物集中到 `<项目根>/dist/NeurolingsCE-windows-x64/`，
//! 运行时、CLI、管理器与插件同目录部署，保证 show manager 等功能可用。

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn project_root() -> PathBuf {
    // xtask 位于 <root>/xtask，向上一级即项目根。
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .expect("xtask 应位于项目根之下")
        .to_path_buf()
}

fn dist_dir(root: &Path) -> PathBuf {
    root.join("dist").join("NeurolingsCE-windows-x64")
}

fn flutter_cmd() -> &'static str {
    if cfg!(windows) {
        "flutter.bat"
    } else {
        "flutter"
    }
}

fn run_or_exit(cmd: &mut Command, what: &str) {
    println!("▶ {what}");
    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("无法启动 {what}: {e}"));
    if !status.success() {
        panic!("{what} 失败（退出码 {:?}）", status.code());
    }
}

/// 构建 Rust 运行时与 CLI（release）。
fn build_rust(root: &Path) {
    run_or_exit(
        Command::new("cargo")
            .args([
                "build",
                "--release",
                "-p",
                "neurolings-runtime",
                "-p",
                "neurolings-cli",
            ])
            .current_dir(root),
        "cargo build --release (runtime + cli)",
    );
}

/// 构建 Flutter 管理器（Windows release）。若 flutter 不在 PATH 则跳过并提示。
fn build_manager(root: &Path) -> bool {
    let manager = root.join("manager");
    if !manager.join("pubspec.yaml").exists() {
        println!("⚠ 未找到 manager/pubspec.yaml，跳过管理器构建");
        return false;
    }
    let probe = Command::new(flutter_cmd())
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if probe.is_err() || !probe.unwrap().success() {
        println!("⚠ 未检测到 flutter，可复用已有 manager 构建或先安装 Flutter");
        return false;
    }
    run_or_exit(
        Command::new(flutter_cmd())
            .args(["build", "windows", "--release"])
            .current_dir(&manager),
        "flutter build windows --release (manager)",
    );
    true
}

/// 把运行时、CLI、管理器与插件组装到 dist 目录。
fn assemble_dist(root: &Path) -> PathBuf {
    // 组装前先结束可能在运行的旧进程，避免产物文件被占用导致复制失败。
    kill_running();
    let out = dist_dir(root);
    if out.exists() {
        std::fs::remove_dir_all(&out).expect("清理旧 dist");
    }
    std::fs::create_dir_all(&out).expect("创建 dist 目录");

    let copy_file = |src: PathBuf| -> bool {
        if src.exists() {
            let dest = out.join(src.file_name().unwrap());
            // 先删除已存在的目标，避免文件被占用或复制不覆盖。
            if dest.exists() {
                let _ = std::fs::remove_file(&dest);
            }
            std::fs::copy(&src, &dest).expect("复制文件");
            println!("  复制 {}", src.display());
            true
        } else {
            println!("  ⚠ 缺失 {}", src.display());
            false
        }
    };

    let mut copied = 0usize;

    // 先复制 Flutter 管理器整个 runner 目录。
    let manager_runner = root.join("manager/build/windows/x64/runner/Release");
    if manager_runner.exists() {
        copy_dir_recursive(&manager_runner, &out);
        println!("  复制 manager runner");
        copied += 1;
    } else {
        println!(
            "  ⚠ 缺失 {}（先运行 flutter build windows --release）",
            manager_runner.display()
        );
    }

    // 再复制新鲜的 Rust 运行时与 CLI，覆盖 manager runner 里可能残留的旧副本。
    if copy_file(root.join("target/release/NeurolingsCE.exe")) {
        copied += 1;
    }
    if copy_file(root.join("target/release/NeurolingsCE-cli.exe")) {
        copied += 1;
    }

    if copied == 0 {
        panic!("dist 组装失败：没有任何产物可复制");
    }
    println!("✔ dist 就绪：{}", out.display());
    out
}

/// 结束正在运行的运行时/管理器/CLI，保证产物文件可被覆盖。
fn kill_running() {
    #[cfg(windows)]
    for im in [
        "NeurolingsCE.exe",
        "neurolings_manager.exe",
        "NeurolingsCE-cli.exe",
    ] {
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", im])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    // 给系统一点时间释放文件句柄。
    std::thread::sleep(std::time::Duration::from_millis(300));
}

fn copy_dir_recursive(src: &Path, dest: &Path) {
    std::fs::create_dir_all(dest).expect("创建目录");
    for entry in std::fs::read_dir(src).expect("读取目录") {
        let entry = entry.expect("目录项");
        let path = entry.path();
        let target = dest.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("复制文件");
        }
    }
}

/// 启动 dist 中的运行时（会自动拉起同目录的管理器），并召唤一只默认桌宠。
fn run_app(root: &Path) {
    let out = dist_dir(root);
    let runtime = out.join("NeurolingsCE.exe");
    let cli = out.join("NeurolingsCE-cli.exe");
    if !runtime.exists() {
        panic!("未找到 {}，请先运行 cargo xtask build", runtime.display());
    }

    println!("▶ 启动运行时 {}", runtime.display());
    // 以普通模式启动：运行时会拉起同目录的管理器。
    // NEUROLINGS_DEBUG=1 使鼠标按下/松开事件写入 neurolings_mouse_debug.log，便于诊断拖拽。
    // 彻底脱离父进程，cargo xtask run 立即返回。
    #[cfg(windows)]
    {
        // 用 shell 的 start 启动，完全脱离当前控制台，避免拖住父进程管道。
        let status = Command::new("cmd")
            .args([
                "/c",
                "start",
                "",
                "/D",
                &out.display().to_string(),
                &runtime.display().to_string(),
            ])
            .env("NEUROLINGS_DEBUG", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if status.is_err() || !status.unwrap().success() {
            panic!("启动运行时失败");
        }
    }
    #[cfg(not(windows))]
    {
        // 此子进程需要脱离 xtask 持续运行，等待它会让 `cargo xtask run` 无法返回。
        #[allow(clippy::zombie_processes)]
        Command::new(&runtime)
            .current_dir(&out)
            .env("NEUROLINGS_DEBUG", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("启动运行时失败");
    }

    // 等待运行时就绪后召唤一只默认桌宠，保证桌面上立即可见。
    std::thread::sleep(std::time::Duration::from_secs(3));
    if cli.exists() {
        println!("▶ 召唤默认桌宠");
        let _ = Command::new(&cli)
            .args(["--summon", "mascot", "--name", "Default", "1"])
            .current_dir(&out)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    println!("✔ 已启动。管理器与桌宠应已出现在桌面/任务栏。");
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let root = project_root();
    let sub = args.first().map(String::as_str).unwrap_or("run");

    match sub {
        "build" => {
            build_rust(&root);
            build_manager(&root);
            assemble_dist(&root);
        }
        "package" => {
            assemble_dist(&root);
        }
        "run" => {
            build_rust(&root);
            build_manager(&root);
            assemble_dist(&root);
            run_app(&root);
        }
        "dist" => {
            println!("{}", dist_dir(&root).display());
        }
        other => {
            eprintln!("未知子命令：{other}");
            eprintln!("用法：cargo xtask [build|run|package|dist]");
            std::process::exit(2);
        }
    }
}
