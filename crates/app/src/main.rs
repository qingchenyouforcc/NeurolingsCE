//! `NeurolingsCE` 应用入口。
//!
//! 当前入口提供无窗口 JSONL 控制模式：它复用 GUI 线程会使用的同一个命令服务，
//! 方便桌面前端和自动化客户端先验证完整协议链路。

use std::io::{self, BufRead, BufReader, Read, Write};
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
    let service = Arc::new(Mutex::new(services::CommandService::new()));
    let http_service = Arc::clone(&service);
    std::thread::spawn(move || {
        if let Err(error) = serve_http("127.0.0.1:32456", http_service) {
            eprintln!("NeurolingsCE HTTP server failed: {error}");
        }
    });
    if let Err(error) = serve_tcp(&address, service) {
        eprintln!("NeurolingsCE IPC server failed: {error}");
        std::process::exit(1);
    }
}

type SharedService = Arc<Mutex<services::CommandService>>;

fn serve_tcp(address: &str, service: SharedService) -> io::Result<()> {
    let listener = TcpListener::bind(address)?;
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

fn handle_tcp_client(mut stream: TcpStream, service: SharedService) -> io::Result<()> {
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

fn serve_http(address: &str, service: SharedService) -> io::Result<()> {
    let listener = TcpListener::bind(address)?;
    for stream in listener.incoming() {
        let stream = stream?;
        let service = Arc::clone(&service);
        std::thread::spawn(move || {
            if let Err(error) = handle_http_client(stream, service) {
                eprintln!("NeurolingsCE HTTP client failed: {error}");
            }
        });
    }
    Ok(())
}

fn handle_http_client(mut stream: TcpStream, service: SharedService) -> io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .unwrap_or_default()
        .parse::<http::HttpMethod>()
        .map_err(|_| io::Error::other("unsupported HTTP method"))?;
    let target = parts.next().unwrap_or_default().to_owned();
    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        if header == "\r\n" || header == "\n" || header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;
    let request = http::HttpRequest::new(method, target).with_body(body);
    let response = match http::HttpRouter.route(&request) {
        Ok(route) => {
            let result = match route {
                http::HttpRoute::Api(request) => service
                    .lock()
                    .map_err(|_| io::Error::other("service lock poisoned"))?
                    .execute(request),
                http::HttpRoute::GetMascot { id } => service
                    .lock()
                    .map_err(|_| io::Error::other("service lock poisoned"))?
                    .get_mascot(id),
                http::HttpRoute::AlterMascot { id, patch } => {
                    let mut request = api::ApiRequest::new(api::Command::AlterMascot)
                        .with_field("id", serde_json::json!(id));
                    for (key, value) in patch {
                        request = request.with_field(key, value);
                    }
                    service
                        .lock()
                        .map_err(|_| io::Error::other("service lock poisoned"))?
                        .execute(request)
                }
                http::HttpRoute::GetLoadedMascot { id } => service
                    .lock()
                    .map_err(|_| io::Error::other("service lock poisoned"))?
                    .get_loaded_mascot(id),
                http::HttpRoute::Preview { .. } => Err(api::ApiError::failure(
                    404,
                    "preview_not_found",
                    "Preview is not available",
                )),
            };
            match result {
                Ok(value) => http::HttpResponse::json(200, &value),
                Err(error) => http::HttpResponse::api_error(&error),
            }
        }
        Err(error) => http::HttpRouter::error_response(&error),
    };
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        reason,
        response.content_type,
        response.body.len()
    )?;
    stream.write_all(&response.body)?;
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
