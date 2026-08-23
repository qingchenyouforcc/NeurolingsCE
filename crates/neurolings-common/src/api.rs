//! HTTP API 契约常量。路由定义见 docs/HTTP-API.md。

pub const HTTP_HOST: &str = "127.0.0.1";
pub const HTTP_PORT: u16 = 32456;
pub const API_BASE: &str = "/shijima/api/v1";

/// Manager 私有管理端口：双进程架构的内部通道，仅绑本机、常开。
/// 公开 HTTP API（HTTP_PORT）的启停仍由 http/enabled 设置控制，契约不变。
pub const INTERNAL_HTTP_PORT: u16 = 32457;
