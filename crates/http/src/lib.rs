//! 本地 HTTP API 的请求解析、路由与响应编码。
//!
//! 这里不直接访问运行时对象，只把 HTTP 语义转换成 `api` crate 的协议请求，
//! 由上层服务在正确的线程中执行。

use std::{collections::BTreeMap, fmt, str::FromStr};

use api::{ApiError, ApiRequest, Command};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use thiserror::Error;

/// HTTP API 的固定根路径。
pub const API_BASE_PATH: &str = "/shijima/api/v1";
/// 单个请求体的最大长度，与本地 IPC 共用协议限制。
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

/// 支持的 HTTP 方法。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    /// GET 请求。
    Get,
    /// POST 请求。
    Post,
    /// PUT 请求。
    Put,
    /// DELETE 请求。
    Delete,
}

impl FromStr for HttpMethod {
    type Err = HttpError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "GET" => Ok(Self::Get),
            "POST" => Ok(Self::Post),
            "PUT" => Ok(Self::Put),
            "DELETE" => Ok(Self::Delete),
            _ => Err(HttpError::MethodNotAllowed),
        }
    }
}

/// 供路由器消费的 HTTP 请求。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest {
    /// HTTP 方法。
    pub method: HttpMethod,
    /// 请求目标，包含路径和可选查询字符串。
    pub target: String,
    /// 请求体字节。
    pub body: Vec<u8>,
}

impl HttpRequest {
    /// 构造不带请求体的请求。
    pub fn new(method: HttpMethod, target: impl Into<String>) -> Self {
        Self {
            method,
            target: target.into(),
            body: Vec::new(),
        }
    }

    /// 设置请求体。
    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }
}

/// 路由后的 HTTP API 目标。
#[derive(Clone, Debug, PartialEq)]
pub enum HttpRoute {
    /// 可直接交给通用命令 dispatcher 的请求。
    Api(ApiRequest),
    /// 查询单个屏幕 mascot。
    GetMascot { id: i32 },
    /// 查询单个已加载模板。
    GetLoadedMascot { id: i32 },
    /// 返回已加载模板的预览图片。
    Preview { id: i32 },
    /// 修改单个 mascot。
    AlterMascot {
        id: i32,
        patch: BTreeMap<String, Value>,
    },
}

/// HTTP 路由与 JSON 校验错误。
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum HttpError {
    /// 请求体超过协议限制。
    #[error("request body exceeds {MAX_BODY_BYTES} bytes")]
    BodyTooLarge,
    /// 请求体不是 JSON object。
    #[error("request body must be a JSON object")]
    InvalidBody,
    /// JSON 语法或字段类型错误。
    #[error("invalid JSON body: {0}")]
    InvalidJson(String),
    /// 路径没有注册路由。
    #[error("route not found")]
    NotFound,
    /// 方法不适用于当前路径。
    #[error("method not allowed")]
    MethodNotAllowed,
    /// 路径中的 ID 无效。
    #[error("invalid resource id")]
    InvalidId,
}

impl HttpError {
    /// 返回对应的 HTTP 状态码。
    pub const fn status(&self) -> u16 {
        match self {
            Self::BodyTooLarge => 413,
            Self::InvalidBody | Self::InvalidJson(_) | Self::InvalidId => 400,
            Self::NotFound => 404,
            Self::MethodNotAllowed => 405,
        }
    }
}

/// 无状态 HTTP 路由器。
#[derive(Clone, Copy, Debug, Default)]
pub struct HttpRouter;

impl HttpRouter {
    /// 解析请求并返回协议目标。
    pub fn route(&self, request: &HttpRequest) -> Result<HttpRoute, HttpError> {
        if request.body.len() > MAX_BODY_BYTES {
            return Err(HttpError::BodyTooLarge);
        }

        let (path, query) = split_target(&request.target);
        let path = path.trim_end_matches('/');
        let segments = path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();
        let base = API_BASE_PATH
            .trim_matches('/')
            .split('/')
            .collect::<Vec<_>>();
        if segments.len() < base.len() || segments[..base.len()] != base {
            return Err(HttpError::NotFound);
        }
        let resource = &segments[base.len()..];

        match (request.method, resource) {
            (HttpMethod::Get, ["ping"]) => Ok(HttpRoute::Api(ApiRequest::new(Command::Ping))),
            (HttpMethod::Get, ["mascots"]) => {
                let mut api_request = ApiRequest::new(Command::ListMascots);
                if let Some(selector) = query.get("selector") {
                    api_request =
                        api_request.with_field("selector", Value::String(selector.clone()));
                }
                Ok(HttpRoute::Api(api_request))
            }
            (HttpMethod::Post, ["mascots"]) => Ok(HttpRoute::Api(request_with_body(
                request,
                Command::SpawnMascot,
            )?)),
            (HttpMethod::Delete, ["mascots"]) => Ok(HttpRoute::Api(request_with_optional_body(
                request,
                Command::DismissAllMascots,
            )?)),
            (HttpMethod::Get, ["mascots", id]) => Ok(HttpRoute::GetMascot { id: parse_id(id)? }),
            (HttpMethod::Put, ["mascots", id]) => Ok(HttpRoute::AlterMascot {
                id: parse_id(id)?,
                patch: body_object(request)?,
            }),
            (HttpMethod::Get, ["loadedMascots"]) => {
                Ok(HttpRoute::Api(ApiRequest::new(Command::ListLoadedMascots)))
            }
            (HttpMethod::Get, ["loadedMascots", id]) => {
                Ok(HttpRoute::GetLoadedMascot { id: parse_id(id)? })
            }
            (HttpMethod::Get, ["loadedMascots", id, "preview.png"]) => {
                Ok(HttpRoute::Preview { id: parse_id(id)? })
            }
            _ if resource
                .first()
                .is_some_and(|name| ["ping", "mascots", "loadedMascots"].contains(name)) =>
            {
                Err(HttpError::MethodNotAllowed)
            }
            _ => Err(HttpError::NotFound),
        }
    }

    /// 将路由错误编码成统一 API JSON 响应。
    pub fn error_response(error: &HttpError) -> HttpResponse {
        let api_error = ApiError::new(error.status(), Some(error_code(error)), error.to_string());
        HttpResponse::json(error.status(), &api_error)
    }
}

/// HTTP 响应的最小传输表示。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    /// HTTP 状态码。
    pub status: u16,
    /// 内容类型。
    pub content_type: &'static str,
    /// 响应体。
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// 编码 JSON 响应，并保证输出是紧凑 JSON。
    pub fn json<T: Serialize>(status: u16, value: &T) -> Self {
        let body = serde_json::to_vec(value)
            .unwrap_or_else(|_| b"{\"error\":\"serialization_failed\"}".to_vec());
        Self {
            status,
            content_type: "application/json; charset=utf-8",
            body,
        }
    }

    /// 将 API 错误编码为 HTTP 响应。
    pub fn api_error(error: &ApiError) -> Self {
        Self::json(error.status, error)
    }
}

fn request_with_body(request: &HttpRequest, command: Command) -> Result<ApiRequest, HttpError> {
    let fields = body_object(request)?;
    Ok(ApiRequest { command, fields })
}

fn request_with_optional_body(
    request: &HttpRequest,
    command: Command,
) -> Result<ApiRequest, HttpError> {
    if request.body.is_empty() {
        return Ok(ApiRequest::new(command));
    }
    request_with_body(request, command)
}

fn body_object(request: &HttpRequest) -> Result<BTreeMap<String, Value>, HttpError> {
    if request.body.is_empty() {
        return Ok(BTreeMap::new());
    }
    let value: Value = serde_json::from_slice(&request.body)
        .map_err(|error| HttpError::InvalidJson(error.to_string()))?;
    value
        .as_object()
        .map(|object| object.clone().into_iter().collect())
        .ok_or(HttpError::InvalidBody)
}

fn parse_id(value: &str) -> Result<i32, HttpError> {
    value.parse::<i32>().map_err(|_| HttpError::InvalidId)
}

fn split_target(target: &str) -> (&str, BTreeMap<String, String>) {
    let Some((path, query)) = target.split_once('?') else {
        return (target, BTreeMap::new());
    };
    let mut params = BTreeMap::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        params.insert(percent_decode(key), percent_decode(value));
    }
    (path, params)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            output.push(high * 16 + low);
            index += 3;
            continue;
        }
        output.push(if bytes[index] == b'+' {
            b' '
        } else {
            bytes[index]
        });
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn error_code(error: &HttpError) -> String {
    match error {
        HttpError::BodyTooLarge => "body_too_large",
        HttpError::InvalidBody => "invalid_body",
        HttpError::InvalidJson(_) => "invalid_json",
        HttpError::NotFound => "not_found",
        HttpError::MethodNotAllowed => "method_not_allowed",
        HttpError::InvalidId => "invalid_id",
    }
    .to_owned()
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        })
    }
}

/// 从 JSON body 解码协议结构，供具体 handler 复用。
pub fn decode_json<T: DeserializeOwned>(request: &HttpRequest) -> Result<T, HttpError> {
    if request.body.len() > MAX_BODY_BYTES {
        return Err(HttpError::BodyTooLarge);
    }
    serde_json::from_slice(&request.body).map_err(|error| HttpError::InvalidJson(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn routes_ping_and_selector_query() {
        let router = HttpRouter;
        let ping = router
            .route(&HttpRequest::new(HttpMethod::Get, "/shijima/api/v1/ping"))
            .unwrap();
        assert_eq!(ping, HttpRoute::Api(ApiRequest::new(Command::Ping)));

        let mascots = router
            .route(&HttpRequest::new(
                HttpMethod::Get,
                "/shijima/api/v1/mascots?selector=name%20%3D%3D%20%27Default%27",
            ))
            .unwrap();
        let HttpRoute::Api(request) = mascots else {
            panic!("expected API route");
        };
        assert_eq!(request.command, Command::ListMascots);
        assert_eq!(request.field("selector"), Some(&json!("name == 'Default'")));
    }

    #[test]
    fn routes_body_and_resource_ids() {
        let router = HttpRouter;
        let spawned = router
            .route(
                &HttpRequest::new(HttpMethod::Post, "/shijima/api/v1/mascots")
                    .with_body(br#"{"name":"Default Mascot","anchor":{"x":1,"y":2}}"#.to_vec()),
            )
            .unwrap();
        let HttpRoute::Api(request) = spawned else {
            panic!("expected API route");
        };
        assert_eq!(request.command, Command::SpawnMascot);
        assert_eq!(request.field("name"), Some(&json!("Default Mascot")));
        assert_eq!(
            router
                .route(&HttpRequest::new(
                    HttpMethod::Get,
                    "/shijima/api/v1/mascots/42"
                ))
                .unwrap(),
            HttpRoute::GetMascot { id: 42 }
        );
    }

    #[test]
    fn rejects_oversized_and_non_object_body() {
        let router = HttpRouter;
        let oversized = HttpRequest::new(HttpMethod::Post, "/shijima/api/v1/mascots")
            .with_body(vec![b'a'; MAX_BODY_BYTES + 1]);
        assert_eq!(router.route(&oversized), Err(HttpError::BodyTooLarge));
        let array = HttpRequest::new(HttpMethod::Post, "/shijima/api/v1/mascots")
            .with_body(br#"[]"#.to_vec());
        assert_eq!(router.route(&array), Err(HttpError::InvalidBody));
    }

    #[test]
    fn encodes_compact_error_response() {
        let response = HttpRouter::error_response(&HttpError::NotFound);
        assert_eq!(response.status, 404);
        assert_eq!(response.content_type, "application/json; charset=utf-8");
        let body = String::from_utf8(response.body).unwrap();
        assert!(body.contains("\"code\":\"not_found\""));
        assert!(!body.contains('\n'));
    }
}
