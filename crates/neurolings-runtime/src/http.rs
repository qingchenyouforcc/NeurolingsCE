//! 本地 HTTP API（127.0.0.1:32456/shijima/api/v1）的最小阻塞式服务。
//!
//! 路由契约与原版一致：未知路由一律 400（而非 404）；写请求要求
//! application/json；POST /mascots 同时给出 name 与 data_id 时拒绝。

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Sender;

use serde_json::{Value, json};

use crate::services::{self, PendingCommand};

const API_BASE: &str = "/shijima/api/v1";

/// 在指定端口启动 HTTP 服务（127.0.0.1）。
/// 公开端口受 http/enabled 控制；内部管理端口常开供 Manager 使用。
pub fn serve(tx: Sender<PendingCommand>, port: u16) {
    let Ok(listener) = TcpListener::bind((neurolings_common::api::HTTP_HOST, port)) else {
        crate::log::warn("http", &format!("failed to bind 127.0.0.1:{port}"));
        return;
    };
    crate::log::info("http", &format!("listening on 127.0.0.1:{port}"));
    for stream in listener.incoming().flatten() {
        let tx = tx.clone();
        std::thread::spawn(move || handle_connection(stream, tx));
    }
}

struct Request {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    body: Vec<u8>,
    content_type: String,
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut head = Vec::new();
    let mut buf = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut buf).ok()?;
        if n == 0 {
            return None;
        }
        head.extend_from_slice(&buf[..n]);
        if head.len() > 64 * 1024 {
            return None;
        }
    }
    let split = head.windows(4).position(|w| w == b"\r\n\r\n")?;
    let header_text = String::from_utf8_lossy(&head[..split]).into_owned();
    let mut lines = header_text.lines();
    let request_line = lines.next()?.to_string();
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();

    let mut content_length = 0usize;
    let mut content_type = String::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            content_length = value.trim().parse().unwrap_or(0);
        }
        if name.trim().eq_ignore_ascii_case("content-type") {
            content_type = value.trim().to_string();
        }
    }

    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), parse_query(q)),
        None => (target.clone(), Vec::new()),
    };

    let mut body = head[split + 4..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buf[..n]);
        if body.len() > services::MESSAGE_MAX_BYTES {
            return None;
        }
    }
    body.truncate(content_length);

    Some(Request {
        method,
        path,
        query,
        body,
        content_type,
    })
}

fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            Some((url_decode(k), url_decode(v)))
        })
        .collect()
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = &s[i + 1..i + 3];
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    out.push(b);
                    i += 3;
                    continue;
                }
                out.push(bytes[i]);
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn respond(stream: &mut TcpStream, status: i32, body: &str, content_type: &str) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

fn respond_json(stream: &mut TcpStream, value: &Value) {
    let status = value.get("status").and_then(Value::as_i64).unwrap_or(200) as i32;
    let body = neurolings_common::json::to_compact_string(value);
    respond(stream, status, &body, "application/json");
}

/// 未知路由/坏请求的标准响应（与原版一致：400 + 固定错误体）。
fn bad_request(stream: &mut TcpStream) {
    let body = neurolings_common::json::to_compact_string(&json!({
        "error": "400 Bad Request",
        "code": "bad_request",
    }));
    respond(stream, 400, &body, "application/json");
}

fn call(tx: &Sender<PendingCommand>, request: Value) -> Value {
    services::call(tx, request)
}

/// Content-Type 必须声明 application/json，且请求体为 JSON 对象。
fn parse_json_body(request: &Request) -> Option<Value> {
    let mut content_type = request.content_type.to_lowercase();
    if let Some(pos) = content_type.find(';') {
        content_type.truncate(pos);
    }
    if content_type.trim() != "application/json" {
        return None;
    }
    let value: Value = serde_json::from_slice(&request.body).ok()?;
    value.is_object().then_some(value)
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn handle_connection(mut stream: TcpStream, tx: Sender<PendingCommand>) {
    let Some(request) = read_request(&mut stream) else {
        bad_request(&mut stream);
        return;
    };

    let Some(path) = request.path.strip_prefix(API_BASE) else {
        bad_request(&mut stream);
        return;
    };

    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    match (request.method.as_str(), segments.as_slice()) {
        ("GET", ["ping"]) => {
            respond_json(&mut stream, &call(&tx, json!({ "command": "ping" })));
        }
        ("GET", ["mascots"]) => {
            let mut req = json!({ "command": "list_mascots" });
            if let Some((_, selector)) = request.query.iter().find(|(k, _)| k == "selector") {
                req["selector"] = json!(selector);
            }
            respond_json(&mut stream, &call(&tx, req));
        }
        ("POST", ["mascots"]) => {
            let Some(body) = parse_json_body(&request) else {
                bad_request(&mut stream);
                return;
            };
            if body.get("name").is_some() && body.get("data_id").is_some() {
                bad_request(&mut stream);
                return;
            }
            respond_json(
                &mut stream,
                &call(&tx, json!({ "command": "spawn_mascot", "request": body })),
            );
        }
        ("DELETE", ["mascots"]) => {
            let mut req = json!({ "command": "dismiss_all_mascots" });
            if let Some(body) = parse_json_body(&request)
                && let Some(selector) = body.get("selector")
            {
                req["selector"] = selector.clone();
            }
            respond_json(&mut stream, &call(&tx, req));
        }
        ("GET", ["mascots", id]) if is_digits(id) => {
            let id_num: i64 = id.parse().unwrap_or(-1);
            respond_json(
                &mut stream,
                &call(&tx, json!({ "command": "get_mascot", "mascot_id": id_num })),
            );
        }
        ("PUT", ["mascots", id]) if is_digits(id) => {
            let Some(body) = parse_json_body(&request) else {
                bad_request(&mut stream);
                return;
            };
            let id_num: i64 = id.parse().unwrap_or(-1);
            respond_json(
                &mut stream,
                &call(
                    &tx,
                    json!({
                        "command": "alter_mascot",
                        "mascot_id": id_num,
                        "patch": body,
                    }),
                ),
            );
        }
        ("DELETE", ["mascots", id]) if is_digits(id) => {
            let id_num: i64 = id.parse().unwrap_or(-1);
            respond_json(
                &mut stream,
                &call(
                    &tx,
                    json!({ "command": "dismiss_mascot", "mascot_id": id_num }),
                ),
            );
        }
        ("GET", ["loadedMascots"]) => {
            respond_json(
                &mut stream,
                &call(&tx, json!({ "command": "list_loaded_mascots" })),
            );
        }
        ("GET", ["loadedMascots", id]) if is_digits(id) => {
            let listed = call(&tx, json!({ "command": "list_loaded_mascots" }));
            let id_num: i64 = id.parse().unwrap_or(-1);
            let found = listed
                .get("loaded_mascots")
                .and_then(Value::as_array)
                .and_then(|arr| {
                    arr.iter()
                        .find(|m| m.get("id").and_then(Value::as_i64) == Some(id_num))
                });
            match found {
                Some(m) => respond_json(&mut stream, &json!({ "loaded_mascot": m })),
                None => {
                    let mut err = services::error_json(
                        "No such loaded mascot",
                        "loaded_mascot_not_found",
                        404,
                    );
                    err["loaded_mascot"] = Value::Null;
                    respond_json(&mut stream, &err);
                }
            }
        }
        ("GET", ["loadedMascots", id, "preview.png"]) if is_digits(id) => {
            let id_num: i64 = id.parse().unwrap_or(-1);
            let response = call(&tx, json!({ "command": "preview_png", "id": id_num }));
            if let Some(b64) = response.get("preview_base64").and_then(Value::as_str) {
                match base64_decode(b64) {
                    Some(bytes) => {
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            bytes.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&bytes);
                        return;
                    }
                    None => {
                        respond_json(
                            &mut stream,
                            &services::error_json("invalid preview", "internal_error", 500),
                        );
                        return;
                    }
                }
            }
            respond_json(&mut stream, &response);
        }
        ("POST", ["cli", "labels"]) => {
            let Some(body) = parse_json_body(&request) else {
                bad_request(&mut stream);
                return;
            };
            let mascot_id = body.get("mascot_id").and_then(Value::as_i64).unwrap_or(-1);
            if mascot_id < 0 {
                bad_request(&mut stream);
                return;
            }
            let mut req = json!({ "command": "register_cli_label", "mascot_id": mascot_id });
            if let Some(label) = body.get("label") {
                req["label"] = label.clone();
            }
            respond_json(&mut stream, &call(&tx, req));
        }
        ("GET", ["cli", "labels", label]) if is_digits(label) => {
            let label_num: i64 = label.parse().unwrap_or(-1);
            respond_json(
                &mut stream,
                &call(
                    &tx,
                    json!({ "command": "get_cli_label", "label": label_num }),
                ),
            );
        }
        // 运行时扩展命令（管理器使用）。
        ("POST", ["command"]) => {
            let Some(body) = parse_json_body(&request) else {
                bad_request(&mut stream);
                return;
            };
            if body.get("command").and_then(Value::as_str).is_none() {
                bad_request(&mut stream);
            } else {
                respond_json(&mut stream, &call(&tx, body));
            }
        }
        _ => {
            bad_request(&mut stream);
        }
    }
}

const B64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_TABLE[(n >> 18 & 63) as usize] as char);
        out.push(B64_TABLE[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            B64_TABLE[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let vals: Vec<u32> = s
        .bytes()
        .filter(|b| *b != b'\n' && *b != b'\r')
        .map(|b| match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => u32::MAX,
        })
        .collect();
    if vals.contains(&u32::MAX) || !vals.len().is_multiple_of(4) {
        return None;
    }
    for chunk in vals.chunks(4) {
        let n = (chunk[0] << 18) | (chunk[1] << 12) | (chunk[2] << 6) | chunk[3];
        out.push((n >> 16 & 0xFF) as u8);
        out.push((n >> 8 & 0xFF) as u8);
        out.push((n & 0xFF) as u8);
    }
    let padding = s.chars().rev().take(2).filter(|c| *c == '=').count();
    out.truncate(out.len().saturating_sub(padding));
    Some(out)
}
