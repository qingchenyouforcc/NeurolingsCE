//! CLI 流程集成测试（以函数驱动，不启动二进制）。

use std::path::Path;

use neurolings_cli::{output, parser, run_to_output};
use serde_json::Value;

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../mascot_pack");

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn parse_value(text: &str) -> Value {
    serde_json::from_str(text.trim_end_matches('\n')).expect("single-line JSON output")
}

#[test]
fn version_text() {
    let out = run_to_output(&args(&["NeurolingsCE-cli", "--version"]), None);
    assert_eq!(out.exit_code, 0);
    assert_eq!(
        out.stdout,
        format!("NeurolingsCE {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(out.stderr.is_empty());
}

#[test]
fn version_json() {
    let out = run_to_output(&args(&["NeurolingsCE-cli", "--json", "--version"]), None);
    assert_eq!(out.exit_code, 0);
    let value = parse_value(&out.stdout);
    assert_eq!(value["app"], "NeurolingsCE");
    assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
    // 单行紧凑 JSON，带末尾换行。
    assert!(!out.stdout[..out.stdout.len() - 1].contains('\n'));
    assert!(out.stdout.ends_with('\n'));
}

#[test]
fn version_quiet_produces_no_output() {
    let out = run_to_output(&args(&["NeurolingsCE-cli", "--quiet", "--version"]), None);
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.is_empty());
}

#[test]
fn help_text_and_json() {
    let out = run_to_output(&args(&["NeurolingsCE-cli", "--help"]), None);
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.starts_with("NeurolingsCE CLI\n"));
    assert!(out.stdout.contains("--mascot, -m validate FILE"));
    assert!(
        out.stdout
            .contains("list, list-loaded, spawn, alter, dismiss, dismiss-all")
    );

    let out = run_to_output(&args(&["NeurolingsCE-cli", "--json", "--help"]), None);
    assert_eq!(out.exit_code, 0);
    let value = parse_value(&out.stdout);
    assert_eq!(value["app"], "NeurolingsCE");
    assert_eq!(value["label_scope"], "current_app_run");
    assert!(value["commands"].is_array());
    assert!(value["legacy_commands"].is_array());
    assert!(value["global_options"].is_array());
}

#[test]
fn validate_valid_package_json_and_text() {
    let package = Path::new(FIXTURE_DIR).join("Cerber.mascot");
    let package = package.to_string_lossy().to_string();

    let out = run_to_output(
        &args(&["NeurolingsCE-cli", "--mascot", "validate", &package]),
        None,
    );
    assert_eq!(out.exit_code, 0, "stderr = {}", out.stderr);
    assert!(out.stdout.starts_with("Valid mascot package: Cerber"));
    assert!(out.stdout.contains("files,"));

    let out = run_to_output(
        &args(&[
            "NeurolingsCE-cli",
            "--json",
            "--mascot",
            "validate",
            &package,
        ]),
        None,
    );
    assert_eq!(out.exit_code, 0);
    let value = parse_value(&out.stdout);
    assert_eq!(value["ok"], true);
    assert_eq!(value["mascot"]["name"], "Cerber");
    assert_eq!(value["package_version"], value["mascot"]["version"]);
    assert!(value["entry_count"].as_u64().unwrap() > 0);
    assert!(value["file_count"].as_u64().unwrap() > 0);
    assert!(value["extracted_bytes"].as_u64().unwrap() > 0);
    assert_eq!(value["errors"], Value::Array(vec![]));
}

#[test]
fn validate_missing_package_is_usage_error() {
    // 文件不存在属于参数错误：退出码 2，与原版契约一致。
    let out = run_to_output(
        &args(&[
            "NeurolingsCE-cli",
            "--mascot",
            "validate",
            "/no/such/file.mascot",
        ]),
        None,
    );
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("ERROR: Mascot package does not exist"));

    let out = run_to_output(
        &args(&[
            "NeurolingsCE-cli",
            "--json",
            "--mascot",
            "validate",
            "/no/such/file.mascot",
        ]),
        None,
    );
    assert_eq!(out.exit_code, 2);
    let value = parse_value(&out.stdout);
    assert_eq!(value["code"], "invalid_arguments");
    assert_eq!(value["error"], "Mascot package does not exist");
}

#[test]
fn validate_quiet_produces_no_output() {
    let package = Path::new(FIXTURE_DIR).join("Cerber.mascot");
    let package = package.to_string_lossy().to_string();
    let out = run_to_output(
        &args(&[
            "NeurolingsCE-cli",
            "--quiet",
            "--mascot",
            "validate",
            &package,
        ]),
        None,
    );
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.is_empty());

    // 错误不受 quiet 抑制。
    let out = run_to_output(
        &args(&[
            "NeurolingsCE-cli",
            "--quiet",
            "--mascot",
            "validate",
            "/no/such/file.mascot",
        ]),
        None,
    );
    assert_eq!(out.exit_code, 2);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("ERROR: Mascot package does not exist"));
}

#[test]
fn mascot_list_on_temp_storage() {
    let temp = tempfile::tempdir().unwrap();
    let storage = temp.path().join("storage");
    std::fs::create_dir_all(&storage).unwrap();

    // 安装两个模板：一个 .mascot 文件、一个解压目录。
    // 既定契约：迁移后只列 *.mascot 文件，
    // 带 info.json 的散开目录不再列出。
    std::fs::copy(
        Path::new(FIXTURE_DIR).join("Eviling.mascot"),
        storage.join("Eviling.mascot"),
    )
    .unwrap();
    let zebra_dir = storage.join("Zebra");
    std::fs::create_dir_all(&zebra_dir).unwrap();
    std::fs::write(
        zebra_dir.join("info.json"),
        r#"{"name": "Zebra", "version": "2.0", "description": "d", "author": "a"}"#,
    )
    .unwrap();

    // JSON 输出：内置默认模板 id 0 在前，其余按名称排序 1..n。
    let out = run_to_output(
        &args(&["NeurolingsCE-cli", "--json", "--mascot", "list"]),
        Some(storage.clone()),
    );
    assert_eq!(out.exit_code, 0, "stderr = {}", out.stderr);
    let value = parse_value(&out.stdout);
    let templates = value["templates"].as_array().expect("templates array");
    assert_eq!(templates.len(), 2);
    assert_eq!(templates[0]["id"], 0);
    assert_eq!(templates[0]["name"], "Default");
    assert_eq!(templates[1]["id"], 1);
    assert_eq!(templates[1]["name"], "Eviling");

    // 文本输出。
    let out = run_to_output(
        &args(&["NeurolingsCE-cli", "--mascot", "list"]),
        Some(storage.clone()),
    );
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.contains("[0] Default"));
    assert!(out.stdout.contains("[1] Eviling"));
    assert!(!out.stdout.contains("Zebra"));
}

#[test]
fn mascot_add_and_remove_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let storage = temp.path().join("storage");
    let archive = Path::new(FIXTURE_DIR).join("Cerber.zip");
    let archive = archive.to_string_lossy().to_string();

    let out = run_to_output(
        &args(&["NeurolingsCE-cli", "--json", "--mascot", "add", &archive]),
        Some(storage.clone()),
    );
    assert_eq!(out.exit_code, 0, "stderr = {}", out.stderr);
    let value = parse_value(&out.stdout);
    assert_eq!(value["templates"][0]["name"], "Cerber");

    // 移除它。
    let out = run_to_output(
        &args(&["NeurolingsCE-cli", "--json", "--mascot", "remove", "Cerber"]),
        Some(storage.clone()),
    );
    assert_eq!(out.exit_code, 0, "stderr = {}", out.stderr);
    let value = parse_value(&out.stdout);
    assert_eq!(value["removed"], "Cerber");

    // 再次移除失败，退出码 1。
    let out = run_to_output(
        &args(&["NeurolingsCE-cli", "--mascot", "remove", "Cerber"]),
        Some(storage.clone()),
    );
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("ERROR: No such mascot template"));

    // 含路径穿越的名称以退出码 2 拒绝。
    let out = run_to_output(
        &args(&["NeurolingsCE-cli", "--mascot", "remove", "../evil"]),
        Some(storage),
    );
    assert_eq!(out.exit_code, 2);
}

#[test]
fn mascot_remove_does_not_delete_non_package_directory() {
    let temp = tempfile::tempdir().unwrap();
    let storage = temp.path().join("storage");
    let unrelated = storage.join("Cerber");
    std::fs::create_dir_all(&unrelated).unwrap();
    std::fs::write(unrelated.join("keep.txt"), "keep").unwrap();

    let out = run_to_output(
        &args(&["NeurolingsCE-cli", "--mascot", "remove", "Cerber"]),
        Some(storage),
    );
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("ERROR: No such mascot template"));
    assert!(unrelated.join("keep.txt").is_file());
}

#[test]
fn mascot_add_missing_archive_is_usage_error() {
    let out = run_to_output(
        &args(&[
            "NeurolingsCE-cli",
            "--mascot",
            "add",
            "/no/such/archive.zip",
        ]),
        None,
    );
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("ERROR: Mascot archive does not exist"));
}

#[test]
fn rejects_host_and_port_globals() {
    for option in ["--host", "--port"] {
        let out = run_to_output(
            &args(&["NeurolingsCE-cli", option, "localhost", "--version"]),
            None,
        );
        assert_eq!(out.exit_code, 2, "{option}");
        assert!(
            out.stderr.contains(&format!(
                "ERROR: {option} is not supported by the local IPC CLI"
            )),
            "{option}: stderr = {}",
            out.stderr
        );
        assert!(
            out.stderr
                .contains("Use the local running instance instead of host/port routing.")
        );
    }

    // JSON 模式改为输出错误对象。
    let out = run_to_output(
        &args(&[
            "NeurolingsCE-cli",
            "--json",
            "--host",
            "localhost",
            "--version",
        ]),
        None,
    );
    assert_eq!(out.exit_code, 2);
    let value = parse_value(&out.stdout);
    assert_eq!(value["code"], "invalid_arguments");
    assert!(value["error"].as_str().unwrap().contains("--host"));
}

#[test]
fn usage_errors_exit_two() {
    let cases: &[&[&str]] = &[
        &["NeurolingsCE-cli"],
        &["NeurolingsCE-cli", "--unknown-option"],
        &["NeurolingsCE-cli", "--mascot"],
        &["NeurolingsCE-cli", "--mascot", "frobnicate"],
        &["NeurolingsCE-cli", "--mascot", "add"],
        &["NeurolingsCE-cli", "--close"],
        &["NeurolingsCE-cli", "--close", "abc"],
    ];
    for case in cases {
        let out = run_to_output(&args(case), None);
        assert_eq!(out.exit_code, 2, "case = {case:?}");
        assert!(out.stderr.starts_with("ERROR: "), "case = {case:?}");
    }
}

#[test]
fn numeric_options_reject_values_outside_i32_range() {
    let cases: &[&[&str]] = &[
        &[
            "NeurolingsCE-cli",
            "--connect-timeout-ms",
            "2147483648",
            "--version",
        ],
        &[
            "NeurolingsCE-cli",
            "--summon",
            "mascot",
            "--data-id",
            "2147483648",
        ],
        &["NeurolingsCE-cli", "spawn", "--data-id", "2147483648"],
    ];
    for case in cases {
        let out = run_to_output(&args(case), None);
        assert_eq!(out.exit_code, 2, "case = {case:?}");
        assert!(out.stderr.starts_with("ERROR: "), "case = {case:?}");
    }
}

#[test]
fn stop_is_idempotent_without_a_running_runtime() {
    // --stop 从不自动拉起运行时；没有运行时也视为成功。
    // 若恰好有运行时在跑则将其停止；两种情况的契约输出都是
    // {"stopped":true} 且退出码 0。
    let out = run_to_output(&args(&["NeurolingsCE-cli", "--json", "--stop"]), None);
    assert_eq!(out.exit_code, 0, "stderr = {}", out.stderr);
    assert!(
        out.stdout.contains("\"stopped\":true"),
        "stdout = {}",
        out.stdout
    );
}

#[test]
fn parse_errors_respect_json_flag_seen_so_far() {
    let outcome =
        parser::parse_cli_arguments(&args(&["NeurolingsCE-cli", "--json", "--host", "x"]));
    match outcome {
        parser::ParseOutcome::Failure { global, error } => {
            assert!(global.json);
            assert_eq!(error.exit_code, 2);
            let out = output::write_cli_error(&global, &error);
            let value = parse_value(&out.stdout);
            assert_eq!(value["code"], "invalid_arguments");
        }
        _ => panic!("expected parse failure"),
    }
}

#[test]
fn codex_notify_rejects_invalid_activity_during_parse() {
    let out = run_to_output(
        &args(&[
            "NeurolingsCE-cli",
            "--codex-notify",
            r#"{"type":"session-title-updated"}"#,
        ]),
        None,
    );
    assert_eq!(out.exit_code, 2);
    assert!(
        out.stderr.contains(
            "ERROR: Invalid Codex notification: new session notification requires a title"
        ),
        "stderr = {}",
        out.stderr
    );
}
