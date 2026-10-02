//! `NeurolingsCE-cli` 可执行程序入口。

fn main() {
    let invocation = match cli::parse_args(std::env::args_os()) {
        Ok(invocation) => invocation,
        Err(error) => {
            eprintln!("{}", error.message);
            std::process::exit(error.exit_code);
        }
    };

    if invocation.global.json {
        let output = match invocation.command {
            cli::CliCommand::Help => serde_json::json!({"ok": true, "command": "help"}),
            cli::CliCommand::Version => {
                serde_json::json!({"ok": true, "version": env!("CARGO_PKG_VERSION")})
            }
            _ => serde_json::json!({"ok": true, "command": format!("{:?}", invocation.command)}),
        };
        println!(
            "{}",
            serde_json::to_string(&output).expect("JSON 输出可序列化")
        );
    } else if matches!(invocation.command, cli::CliCommand::Help) {
        println!(
            "NeurolingsCE-cli --list | --summon mascot --name NAME | --close LABEL | --version"
        );
    } else if matches!(invocation.command, cli::CliCommand::Version) {
        println!("NeurolingsCE-cli {}", env!("CARGO_PKG_VERSION"));
    }
}
