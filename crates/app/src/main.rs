//! `NeurolingsCE` 应用入口。
//!
//! 当前入口提供无窗口 JSONL 控制模式：它复用 GUI 线程会使用的同一个命令服务，
//! 方便桌面前端和自动化客户端先验证完整协议链路。

use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use api::ApiRequest;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--version" || arg == "-v") {
        println!("NeurolingsCE {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.iter().any(|arg| arg == "--gui") {
        if let Err(error) = ui::run() {
            eprintln!("NeurolingsCE GUI failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|arg| arg == "--stdio") {
        serve_jsonl(io::stdin().lock(), io::stdout().lock());
        return;
    }
    let address =
        std::env::var("NEUROLINGSCE_IPC_ADDR").unwrap_or_else(|_| "127.0.0.1:32457".into());
    if let Err(error) = serve_tcp(&address) {
        eprintln!("NeurolingsCE IPC server failed: {error}");
        std::process::exit(1);
    }
}

fn serve_tcp(address: &str) -> io::Result<()> {
    let listener = TcpListener::bind(address)?;
    let service = Arc::new(Mutex::new(services::CommandService::new()));
    for stream in listener.incoming() {
        let stream = stream?;
        let service = Arc::clone(&service);
        std::thread::spawn(move || {
            if let Err(error) = handle_tcp_client(stream, service) {
                eprintln!("NeurolingsCE IPC client failed: {error}");
            }
        });
    }
    Ok(())
}

fn handle_tcp_client(
    mut stream: TcpStream,
    service: Arc<Mutex<services::CommandService>>,
) -> io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = Vec::new();
    reader.read_until(b'\n', &mut line)?;
    let response = match ipc::decode::<ApiRequest>(&line) {
        Ok(request) => match service
            .lock()
            .map_err(|_| io::Error::other("service lock poisoned"))?
            .execute(request)
        {
            Ok(value) => value,
            Err(error) => serde_json::to_value(error).unwrap_or_else(
                |_| serde_json::json!({"status": 500, "error": "serialization_failed"}),
            ),
        },
        Err(error) => {
            serde_json::json!({"status": 400, "code": "invalid_frame", "error": error.to_string()})
        }
    };
    let frame = ipc::encode(&response).map_err(|error| io::Error::other(error.to_string()))?;
    stream.write_all(&frame)?;
    stream.flush()
}

fn serve_jsonl<R: BufRead, W: Write>(reader: R, mut writer: W) {
    let mut service = services::CommandService::new();
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let response = match ipc::decode::<ApiRequest>(line.as_bytes()) {
            Ok(request) => match service.execute(request) {
                Ok(value) => value,
                Err(error) => serde_json::to_value(error).unwrap_or_else(
                    |_| serde_json::json!({"status": 500, "error": "serialization_failed"}),
                ),
            },
            Err(error) => {
                serde_json::json!({"status": 400, "code": "invalid_frame", "error": error.to_string()})
            }
        };
        let Ok(frame) = ipc::encode(&response) else {
            break;
        };
        if writer.write_all(&frame).is_err() || writer.flush().is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::serve_jsonl;

    #[test]
    fn jsonl_server_dispatches_ping_and_returns_one_frame() {
        let mut output = Vec::new();
        serve_jsonl(
            Cursor::new(
                br#"{"command":"ping"}
"#,
            ),
            &mut output,
        );
        let body = String::from_utf8(output).unwrap();
        assert!(body.contains(r#""ok":true"#));
        assert!(body.ends_with('\n'));
    }
}
