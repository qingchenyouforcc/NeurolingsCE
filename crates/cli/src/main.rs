//! `NeurolingsCE-cli` 可执行程序入口。

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

fn main() {
    let invocation = match cli::parse_args(std::env::args_os()) {
        Ok(invocation) => invocation,
        Err(error) => {
            eprintln!("{}", error.message);
            std::process::exit(error.exit_code);
        }
    };

    if matches!(invocation.command, cli::CliCommand::Help) {
        if invocation.global.json {
            println!("{}", serde_json::json!({"ok":true,"command":"help"}));
        }
        println!(
            "NeurolingsCE-cli --list | --summon mascot --name NAME | --close LABEL | --version"
        );
        return;
    }
    if matches!(invocation.command, cli::CliCommand::Version) {
        if invocation.global.json {
            println!(
                "{}",
                serde_json::json!({"ok":true,"version":env!("CARGO_PKG_VERSION")})
            );
            return;
        }
        println!("NeurolingsCE-cli {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if matches!(invocation.command, cli::CliCommand::Mascot(_)) {
        let output =
            serde_json::json!({"ok": true, "command": format!("{:?}", invocation.command)});
        println!(
            "{}",
            serde_json::to_string(&output).expect("JSON 输出可序列化")
        );
        return;
    }
    let Some(request) = cli::request_for_command(&invocation.command) else {
        return;
    };
    match send_request(
        &request,
        invocation.global.connect_timeout_ms,
        invocation.global.read_timeout_ms,
    ) {
        Ok(response) => {
            if invocation.global.json {
                println!("{}", response);
            } else {
                match serde_json::from_str::<serde_json::Value>(&response) {
                    Ok(value) => println!(
                        "{}",
                        serde_json::to_string_pretty(&value).unwrap_or(response)
                    ),
                    Err(_) => println!("{}", response),
                }
            }
        }
        Err(error) => {
            let output =
                serde_json::json!({"code":"ipc_unavailable", "error":error, "exit_code":1});
            if invocation.global.json {
                println!("{}", output);
            } else {
                eprintln!("{}", error);
            }
            std::process::exit(1);
        }
    }
}

fn send_request(
    request: &api::ApiRequest,
    connect_timeout_ms: u64,
    read_timeout_ms: u64,
) -> Result<String, String> {
    let address =
        std::env::var("NEUROLINGSCE_IPC_ADDR").unwrap_or_else(|_| "127.0.0.1:32457".into());
    let address = address
        .to_socket_addrs()
        .map_err(|error| error.to_string())?
        .next()
        .ok_or_else(|| "IPC address is empty".to_owned())?;
    let mut stream =
        TcpStream::connect_timeout(&address, Duration::from_millis(connect_timeout_ms))
            .map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_millis(read_timeout_ms)))
        .map_err(|error| error.to_string())?;
    stream
        .write_all(&ipc::encode(request).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())?;
    let mut line = Vec::new();
    BufReader::new(stream)
        .read_until(b'\n', &mut line)
        .map_err(|error| error.to_string())?;
    ipc::decode::<serde_json::Value>(&line)
        .map(|value| serde_json::to_string(&value).unwrap_or_else(|_| "{}".into()))
        .map_err(|error| error.to_string())
}
