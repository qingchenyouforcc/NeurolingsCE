//! 本地 HTTP API（127.0.0.1:32456/shijima/api/v1）的最小阻塞式服务。
//!
//! 路由契约与原版一致：未知路由一律 400（而非 404）；写请求要求
//! application/json；POST /mascots 同时给出 name 与 data_id 时拒绝。

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use crate::services::{self, PendingCommand};

const API_BASE: &str = "/shijima/api/v1";

/// 单连接读写超时为 5 秒，采用 cpp-httplib 的默认值。
const IO_TIMEOUT: Duration = Duration::from_secs(5);
/// 同时处理的连接数上限，超出直接丢弃，防止慢速客户端耗尽线程。
const MAX_CONNECTIONS: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ApiSurface {
    Public,
    Internal,
}

impl ApiSurface {
    fn allows_management_commands(self) -> bool {
        matches!(self, Self::Internal)
    }
}

/// 在指定端口启动公开 HTTP API，仅暴露文档化的桌宠路由。
pub fn serve_public(tx: Sender<PendingCommand>, port: u16) {
    serve(tx, port, ApiSurface::Public, None);
}

/// 在指定端口启动 Manager 内部 HTTP API，并开放管理命令路由。
pub fn serve_internal(tx: Sender<PendingCommand>, port: u16, token: String) {
    if token.is_empty() {
        crate::log::error(
            "http",
            "internal control API was not started without a token",
        );
        return;
    }
    serve(tx, port, ApiSurface::Internal, Some(Arc::new(token)));
}

fn serve(
    tx: Sender<PendingCommand>,
    port: u16,
    surface: ApiSurface,
    internal_token: Option<Arc<String>>,
) {
    let Ok(listener) = TcpListener::bind((neurolings_common::api::HTTP_HOST, port)) else {
        crate::log::warn("http", &format!("failed to bind 127.0.0.1:{port}"));
        return;
    };
    crate::log::info("http", &format!("listening on 127.0.0.1:{port}"));
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming().flatten() {
        let Some(slot) = ConnectionSlot::acquire(&active) else {
            crate::log::warn("http", "connection limit reached, dropping connection");
            continue;
        };
        let tx = tx.clone();
        let internal_token = internal_token.clone();
        std::thread::spawn(move || {
            let _slot = slot;
            handle_connection(stream, tx, surface, internal_token);
        });
    }
}

struct ConnectionSlot {
    active: Arc<AtomicUsize>,
}

impl ConnectionSlot {
    fn acquire(active: &Arc<AtomicUsize>) -> Option<Self> {
        if active.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
            active.fetch_sub(1, Ordering::Release);
            return None;
        }
        Some(Self {
            active: Arc::clone(active),
        })
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::Release);
    }
}

struct Request {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    body: Vec<u8>,
    content_type: String,
    authorization: Option<String>,
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
    let mut authorization = None;
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
        if name.trim().eq_ignore_ascii_case("authorization") {
            authorization = Some(value.trim().to_string());
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
        authorization,
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
        // 无效的百分号序列保留百分号，其后字节按普通内容继续处理。
        if let [b'%', high, low, ..] = &bytes[i..]
            && let (Some(high), Some(low)) = (hex_value(*high), hex_value(*low))
        {
            out.push((high << 4) | low);
            i += 3;
            continue;
        }
        match bytes[i] {
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
    // 百分号编码可表示任意字节，非 UTF-8 结果以 U+FFFD 替换，避免畸形输入中断请求解析。
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn respond(stream: &mut TcpStream, status: i32, body: &str, content_type: &str) {
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "OK",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

/// 内部控制面使用固定长度比较，避免错误令牌前缀泄露匹配位置。
fn internal_request_authorized(request: &Request, expected: Option<&str>) -> bool {
    let Some(expected) = expected else {
        return false;
    };
    let provided = request
        .authorization
        .as_deref()
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("");
    let mut difference = expected.len() ^ provided.len();
    for (index, expected_byte) in expected.bytes().enumerate() {
        difference |=
            (provided.as_bytes().get(index).copied().unwrap_or(0) ^ expected_byte) as usize;
    }
    difference == 0
}

fn unauthorized(stream: &mut TcpStream) {
    let body = neurolings_common::json::to_compact_string(&json!({
        "error": "Unauthorized",
        "code": "unauthorized",
        "status": 401,
    }));
    respond(stream, 401, &body, "application/json");
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

/// name/data_id 冲突判定遵循 spawnMascotRequestFromJson 的既定契约：
/// 仅当值分别是字符串/数字时视为已指定（显式 null 视为缺省），两者同存才冲突。
fn spawn_name_id_conflict(body: &Value) -> bool {
    body.get("name").is_some_and(Value::is_string)
        && body.get("data_id").is_some_and(Value::is_number)
}

fn handle_connection(
    mut stream: TcpStream,
    tx: Sender<PendingCommand>,
    surface: ApiSurface,
    internal_token: Option<Arc<String>>,
) {
    // 读写超时兜底：read 超时返回 WouldBlock/TimedOut 时按客户端断开处理
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let Some(request) = read_request(&mut stream) else {
        bad_request(&mut stream);
        return;
    };
    if surface == ApiSurface::Internal
        && !internal_request_authorized(&request, internal_token.as_deref().map(String::as_str))
    {
        unauthorized(&mut stream);
        return;
    }

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
            if spawn_name_id_conflict(&body) {
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
        ("POST", ["command"]) if surface.allows_management_commands() => {
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

/// 使用 RFC 4648 标准字母表编码二进制响应。
pub fn base64_encode(data: &[u8]) -> String {
    STANDARD.encode(data)
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let compact: String = s
        .chars()
        .filter(|character| !matches!(character, '\r' | '\n'))
        .collect();
    STANDARD.decode(compact).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::sync::mpsc;

    /// 用一次本地连接执行 /command，覆盖真实路由、鉴权和响应状态码。
    fn command_response(
        surface: ApiSurface,
        expected_token: Option<&str>,
        authorization: Option<&str>,
        reply_to_command: bool,
    ) -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        let token = expected_token.map(|value| Arc::new(value.to_string()));
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_connection(stream, tx, surface, token);
        });
        let responder = if reply_to_command {
            Some(std::thread::spawn(move || {
                let pending = rx.recv_timeout(Duration::from_secs(1)).unwrap();
                assert_eq!(pending.request["command"], "ping");
                pending.reply.send(json!({ "ok": true })).unwrap();
            }))
        } else {
            None
        };

        let body = r#"{"command":"ping"}"#;
        let authorization_line = authorization
            .map(|value| format!("Authorization: {value}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "POST {API_BASE}/command HTTP/1.1\r\nHost: 127.0.0.1\r\n{authorization_line}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len(),
        );
        let mut client = TcpStream::connect(address).unwrap();
        client.write_all(request.as_bytes()).unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        server.join().unwrap();
        if let Some(responder) = responder {
            responder.join().unwrap();
        }
        response
    }

    /// #3a：显式 null 视为缺省，不触发 name/data_id 冲突（遵循
    /// spawnMascotRequestFromJson 的 isString/isDouble 语义）。
    #[test]
    fn spawn_conflict_treats_null_as_absent() {
        assert!(!spawn_name_id_conflict(&json!({})));
        assert!(!spawn_name_id_conflict(
            &json!({"name": null, "data_id": null})
        ));
        assert!(!spawn_name_id_conflict(
            &json!({"name": "A", "data_id": null})
        ));
        assert!(!spawn_name_id_conflict(
            &json!({"name": null, "data_id": 2})
        ));
    }

    #[test]
    fn spawn_conflict_only_when_both_typed() {
        assert!(spawn_name_id_conflict(&json!({"name": "A", "data_id": 2})));
        // 类型不符（name 非字符串 / data_id 非数字）同样视为缺省。
        assert!(!spawn_name_id_conflict(&json!({"name": 5, "data_id": 2})));
        assert!(!spawn_name_id_conflict(
            &json!({"name": "A", "data_id": "2"})
        ));
    }

    #[test]
    fn base64_round_trip_handles_padding() {
        for value in [b"".as_slice(), b"a", b"ab", b"abc", b"abcd"] {
            assert_eq!(base64_decode(&base64_encode(value)), Some(value.to_vec()));
        }
        assert_eq!(base64_encode(b"a"), "YQ==");
        assert_eq!(base64_encode(b"ab"), "YWI=");
    }

    #[test]
    fn base64_decode_accepts_line_breaks_and_rejects_invalid_padding() {
        assert_eq!(base64_decode("YWJj\r\nZA=="), Some(b"abcd".to_vec()));
        assert_eq!(base64_decode("YQ="), None);
        assert_eq!(base64_decode("Y=Q="), None);
        assert_eq!(base64_decode("%%%%"), None);
    }

    #[test]
    fn url_decode_decodes_valid_percent_encoded_utf8() {
        assert_eq!(
            url_decode("selector=%E4%BD%A0%E5%A5%BD+World%21"),
            "selector=你好 World!"
        );
    }

    #[test]
    fn url_decode_preserves_malformed_percent_sequences() {
        assert_eq!(url_decode("%中"), "%中");
        assert_eq!(url_decode("truncated=%"), "truncated=%");
        assert_eq!(url_decode("truncated=%2"), "truncated=%2");
        assert_eq!(url_decode("invalid=%G0"), "invalid=%G0");
        assert_eq!(url_decode("invalid_utf8=%FF"), "invalid_utf8=\u{FFFD}");
    }

    #[test]
    fn connection_slot_releases_after_unwind() {
        let active = Arc::new(AtomicUsize::new(0));
        let slot = ConnectionSlot::acquire(&active).unwrap();
        let result = std::panic::catch_unwind(move || {
            let _slot = slot;
            panic!("simulated HTTP handler panic");
        });

        assert!(result.is_err());
        assert_eq!(active.load(Ordering::Acquire), 0);
        assert!(ConnectionSlot::acquire(&active).is_some());
    }

    #[test]
    fn internal_command_requires_missing_wrong_or_correct_bearer_token() {
        assert!(!ApiSurface::Public.allows_management_commands());
        assert!(ApiSurface::Internal.allows_management_commands());

        let expected = "internal-test-credential";
        let missing = command_response(ApiSurface::Internal, Some(expected), None, false);
        assert!(missing.starts_with("HTTP/1.1 401 Unauthorized"));

        let wrong = command_response(
            ApiSurface::Internal,
            Some(expected),
            Some("Bearer incorrect"),
            false,
        );
        assert!(wrong.starts_with("HTTP/1.1 401 Unauthorized"));

        let correct = command_response(
            ApiSurface::Internal,
            Some(expected),
            Some("Bearer internal-test-credential"),
            true,
        );
        assert!(correct.starts_with("HTTP/1.1 200 OK"));
    }

    #[test]
    fn public_command_route_is_rejected_even_with_authorization_header() {
        let response = command_response(ApiSurface::Public, None, Some("Bearer unrelated"), false);
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
    }
}
