//! 会话日志：按日期分目录的单文件日志，行格式与目录命名对齐原版 AppLog。
//!
//! - 路径：`<应用数据目录>/log/<yyyy-MM-dd>/<neurolingsce>-<HH-mm-ss-zzz>.log`
//! - 行格式：`[yyyy-MM-dd HH:mm:ss.zzz] [LEVEL] [category] [tid:HEX] message`
//! - 级别：`NEUROLINGSCE_LOG_LEVEL`（debug/info/warning/error/critical，默认 info）
//! - `NEUROLINGSCE_LOG_STDERR=1` 时镜像到 stderr

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use chrono::Local;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

impl Level {
    fn name(self) -> &'static str {
        match self {
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Warning => "warning",
            Level::Error => "error",
            Level::Critical => "critical",
        }
    }

    fn parse(text: &str) -> Option<Level> {
        match text.trim().to_ascii_lowercase().as_str() {
            "debug" => Some(Level::Debug),
            "info" => Some(Level::Info),
            "warning" | "warn" => Some(Level::Warning),
            "error" => Some(Level::Error),
            "critical" | "fatal" => Some(Level::Critical),
            _ => None,
        }
    }
}

struct LoggerState {
    file: Option<File>,
    min_level: Level,
    mirror_stderr: bool,
}

static LOGGER: OnceLock<Mutex<LoggerState>> = OnceLock::new();
static THREAD_COUNTER: AtomicU32 = AtomicU32::new(1);

thread_local! {
    /// 线程登记号：首次写日志时分配，用于 [tid:HEX] 字段。
    static THREAD_ID: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn thread_id_hex() -> String {
    THREAD_ID.with(|cell| {
        let mut id = cell.get();
        if id == 0 {
            id = THREAD_COUNTER.fetch_add(1, Ordering::Relaxed);
            cell.set(id);
        }
        format!("{id:x}")
    })
}

fn logger() -> &'static Mutex<LoggerState> {
    LOGGER.get_or_init(|| {
        Mutex::new(LoggerState {
            file: None,
            min_level: Level::Info,
            mirror_stderr: false,
        })
    })
}

/// 应用名小写连字符形式（对齐原版 applicationNameForFileSystem）。
fn app_name_for_fs() -> String {
    neurolings_common::version::APP_NAME
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// 初始化会话日志（进程内调用一次；重复调用安全，仅刷新配置）。
pub fn init(app_data_dir: &std::path::Path) {
    let min_level = std::env::var("NEUROLINGSCE_LOG_LEVEL")
        .ok()
        .and_then(|v| Level::parse(&v))
        .unwrap_or(Level::Info);
    let mirror_stderr = std::env::var("NEUROLINGSCE_LOG_STDERR")
        .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));

    // 目录与文件名对齐原版：log/<yyyy-MM-dd>/<app>-<HH-mm-ss-zzz>.log
    let dir = app_data_dir
        .join("log")
        .join(Local::now().format("%Y-%m-%d").to_string());
    let file_name = format!(
        "{}-{}.log",
        app_name_for_fs(),
        Local::now().format("%H-%M-%S-%3f")
    );
    let path: PathBuf = dir.join(file_name);
    let file = fs::create_dir_all(&dir).ok().and_then(|_| {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()
    });

    let mut state = logger().lock().unwrap();
    state.min_level = min_level;
    state.mirror_stderr = mirror_stderr;
    state.file = file;
    let session_path = path.to_string_lossy().into_owned();
    drop(state);

    log(
        Level::Info,
        "app",
        &format!(
            "Logging initialized. app={} version={} min_level={} stderr={} session_log={}",
            neurolings_common::version::APP_NAME,
            neurolings_common::version::VERSION,
            min_level.name(),
            mirror_stderr,
            session_path
        ),
    );
}

/// 退出前写入关闭标记（对齐原版 Logging shutdown）。
pub fn shutdown() {
    log(Level::Info, "app", "Logging shutdown");
}

/// 写一条日志：级别过滤后单行落盘（可选镜像 stderr）。
pub fn log(level: Level, category: &str, message: &str) {
    let mut state = logger().lock().unwrap();
    if level < state.min_level {
        return;
    }
    let line = format!(
        "[{}] [{}] [{}] [tid:{}] {}",
        Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
        level.name(),
        if category.is_empty() { "app" } else { category },
        thread_id_hex(),
        message
    );
    if let Some(file) = state.file.as_mut() {
        let _ = writeln!(file, "{line}");
    }
    if state.mirror_stderr {
        eprintln!("{line}");
    }
}

pub fn debug(category: &str, message: &str) {
    log(Level::Debug, category, message);
}

pub fn info(category: &str, message: &str) {
    log(Level::Info, category, message);
}

pub fn warn(category: &str, message: &str) {
    log(Level::Warning, category, message);
}

pub fn error(category: &str, message: &str) {
    log(Level::Error, category, message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_session_log_under_dated_directory() {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path());
        info("startup", "hello from test");
        shutdown();
        // 找到日期目录下的日志文件并校验内容与行格式。
        let log_dir = dir.path().join("log");
        let date_dir = fs::read_dir(&log_dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.is_dir())
            .expect("dated log directory");
        let log_file = fs::read_dir(&date_dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "log"))
            .expect("session log file");
        let content = fs::read_to_string(&log_file).unwrap();
        assert!(content.contains("[info] [startup] [tid:"));
        assert!(content.contains("hello from test"));
        assert!(content.contains("Logging shutdown"));
    }
}
