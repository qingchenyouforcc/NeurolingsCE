//! NeurolingsCE 跨传输层 API 数据模型。
//!
//! 本 crate 只负责协议字段、边界校验和稳定 JSON 序列化，不访问运行时或窗口对象。

use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as DeError};
use serde_json::Value;

/// 当前协议版本。
pub const API_VERSION: &str = "v1";

/// selector 允许的最大字符数。
pub const MAX_SELECTOR_LENGTH: usize = 1024;

/// API 错误响应。
///
/// `error` 和 `status` 始终写入 JSON；没有错误代码时省略 `code` 字段，以保持
/// HTTP 与本地 IPC 的响应格式一致。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    /// HTTP 风格状态码。
    pub status: u16,
    /// 稳定的机器可读错误代码。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// 面向用户的错误描述。
    #[serde(rename = "error")]
    pub message: String,
}

impl ApiError {
    /// 创建一个 API 错误。
    pub fn new(status: u16, code: Option<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    /// 创建 selector 超长错误。
    pub fn selector_too_long() -> Self {
        Self::new(
            400,
            Some("selector_too_long".into()),
            "Selector must not exceed 1024 characters",
        )
    }

    /// 创建无效 anchor 错误。
    pub fn invalid_anchor() -> Self {
        Self::new(400, Some("invalid_anchor".into()), "Anchor coordinates must be finite")
    }

    /// 创建无效 label 错误。
    pub fn invalid_label() -> Self {
        Self::new(
            400,
            Some("invalid_cli_label".into()),
            "CLI label must be a non-negative integer",
        )
    }

    /// 创建通用 bad request 错误。
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(400, Some("bad_request".into()), message)
    }

    /// 判断状态码是否表示成功。
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.code {
            Some(code) => write!(formatter, "{} ({code})", self.message),
            None => formatter.write_str(&self.message),
        }
    }
}

impl std::error::Error for ApiError {}

/// 传输层请求的返回类型。
pub type Response<T> = Result<T, ApiError>;

/// 屏幕上的 mascot 锚点坐标。
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Anchor {
    /// 横坐标。
    pub x: f64,
    /// 纵坐标。
    pub y: f64,
}

impl Anchor {
    /// 创建并校验一个 anchor。
    pub fn new(x: f64, y: f64) -> Response<Self> {
        if !x.is_finite() || !y.is_finite() {
            return Err(ApiError::invalid_anchor());
        }
        Ok(Self { x, y })
    }
}

impl<'de> Deserialize<'de> for Anchor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireAnchor {
            x: f64,
            y: f64,
        }

        let value = WireAnchor::deserialize(deserializer)?;
        Self::new(value.x, value.y).map_err(D::Error::custom)
    }
}

/// 用于 mascot 筛选的脚本表达式。
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Selector(String);

impl Selector {
    /// 返回 selector 文本。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 消耗 selector 并返回其文本。
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl TryFrom<&str> for Selector {
    type Error = ApiError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.chars().count() > MAX_SELECTOR_LENGTH {
            return Err(ApiError::selector_too_long());
        }
        Ok(Self(value.to_owned()))
    }
}

impl TryFrom<String> for Selector {
    type Error = ApiError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str()).map(|_| Self(value))
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for Selector {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Selector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_from(value).map_err(D::Error::custom)
    }
}

/// CLI 使用的非负 label。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CliLabel(u32);

impl CliLabel {
    /// 创建 label。
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// 返回数值形式的 label。
    pub const fn value(self) -> u32 {
        self.0
    }
}

impl TryFrom<i64> for CliLabel {
    type Error = ApiError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        u32::try_from(value)
            .map(Self)
            .map_err(|_| ApiError::invalid_label())
    }
}

impl TryFrom<u64> for CliLabel {
    type Error = ApiError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        u32::try_from(value)
            .map(Self)
            .map_err(|_| ApiError::invalid_label())
    }
}

impl Serialize for CliLabel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(self.0)
    }
}

impl<'de> Deserialize<'de> for CliLabel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = i64::deserialize(deserializer)?;
        Self::try_from(value).map_err(D::Error::custom)
    }
}

/// `CliLabel` 的简短别名，便于调用方按协议字段命名。
pub type Label = CliLabel;

/// 支持的传输层命令。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    /// 返回 API 就绪信息。
    Ping,
    /// 展示 Codex 通知。
    ShowCodexNotification,
    /// 列出屏幕上的 mascot。
    ListMascots,
    /// 列出已加载模板。
    ListLoadedMascots,
    /// 导入 mascot 模板。
    ImportMascotTemplate,
    /// 删除 mascot 模板。
    RemoveMascotTemplate,
    /// 召唤 mascot。
    SpawnMascot,
    /// 注册 CLI label。
    RegisterCliLabel,
    /// 查询 CLI label。
    GetCliLabel,
    /// 修改 mascot。
    AlterMascot,
    /// 关闭一个 mascot。
    DismissMascot,
    /// 关闭匹配 selector 的 mascot。
    DismissAllMascots,
    /// 停止运行时。
    StopRuntime,
    /// 展示管理器窗口。
    ShowManager,
    /// 保留未知命令，让 dispatcher 统一返回 bad request。
    Unknown(String),
}

impl Command {
    /// 返回协议中的命令字符串。
    pub fn as_str(&self) -> &str {
        match self {
            Self::Ping => "ping",
            Self::ShowCodexNotification => "show_codex_notification",
            Self::ListMascots => "list_mascots",
            Self::ListLoadedMascots => "list_loaded_mascots",
            Self::ImportMascotTemplate => "import_mascot_template",
            Self::RemoveMascotTemplate => "remove_mascot_template",
            Self::SpawnMascot => "spawn_mascot",
            Self::RegisterCliLabel => "register_cli_label",
            Self::GetCliLabel => "get_cli_label",
            Self::AlterMascot => "alter_mascot",
            Self::DismissMascot => "dismiss_mascot",
            Self::DismissAllMascots => "dismiss_all_mascots",
            Self::StopRuntime => "stop_runtime",
            Self::ShowManager => "show_manager",
            Self::Unknown(command) => command,
        }
    }
}

impl fmt::Display for Command {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for Command {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Command {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "ping" => Self::Ping,
            "show_codex_notification" => Self::ShowCodexNotification,
            "list_mascots" => Self::ListMascots,
            "list_loaded_mascots" => Self::ListLoadedMascots,
            "import_mascot_template" => Self::ImportMascotTemplate,
            "remove_mascot_template" => Self::RemoveMascotTemplate,
            "spawn_mascot" => Self::SpawnMascot,
            "register_cli_label" => Self::RegisterCliLabel,
            "get_cli_label" => Self::GetCliLabel,
            "alter_mascot" => Self::AlterMascot,
            "dismiss_mascot" => Self::DismissMascot,
            "dismiss_all_mascots" => Self::DismissAllMascots,
            "stop_runtime" => Self::StopRuntime,
            "show_manager" => Self::ShowManager,
            _ => Self::Unknown(value),
        })
    }
}

/// 传输层的通用请求。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApiRequest {
    /// 要执行的命令。
    pub command: Command,
    /// 命令专属的额外字段。
    #[serde(flatten)]
    pub fields: BTreeMap<String, Value>,
}

impl ApiRequest {
    /// 创建不带额外字段的请求。
    pub fn new(command: Command) -> Self {
        Self {
            command,
            fields: BTreeMap::new(),
        }
    }

    /// 获取命令字段。
    pub fn field(&self, key: &str) -> Option<&Value> {
        self.fields.get(key)
    }

    /// 写入命令字段并返回自身，便于构造请求。
    pub fn with_field(mut self, key: impl Into<String>, value: Value) -> Self {
        self.fields.insert(key.into(), value);
        self
    }
}

/// 通用请求的协议名称别名。
pub type Request = ApiRequest;

/// 当前屏幕上的 mascot 信息。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MascotInfo {
    /// 运行时 mascot ID。
    pub id: i32,
    /// 模板数据 ID。
    pub data_id: i32,
    /// 模板名称。
    pub name: String,
    /// 当前行为；没有行为时为 null。
    pub active_behavior: Option<String>,
    /// CLI label；没有注册时为 null。
    pub label: Option<CliLabel>,
    /// 当前锚点。
    pub anchor: Anchor,
}

/// 已加载 mascot 模板信息。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadedMascotInfo {
    /// 模板数据 ID。
    pub id: i32,
    /// 模板名称。
    pub name: String,
    /// 模板版本。
    pub version: String,
    /// 模板描述。
    pub description: String,
    /// 模板作者。
    pub author: String,
}

/// 修改 mascot 状态时可提供的字段。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MascotPatch {
    /// 完整的锚点；协议不接受只有 x 或只有 y 的部分锚点。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Anchor>,
    /// 下一行为名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavior: Option<String>,
}

impl MascotPatch {
    /// 判断是否提供了锚点字段。
    pub fn has_anchor(&self) -> bool {
        self.anchor.is_some()
    }

    /// 判断是否提供了完整锚点。
    pub fn has_complete_anchor(&self) -> bool {
        self.anchor.is_some()
    }
}

/// 召唤 mascot 请求。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SpawnMascotRequest {
    /// 模板名称，与 `data_id` 至少提供一个。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 模板数据 ID，与 `name` 至少提供一个。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_id: Option<i32>,
    /// 初始状态修改字段。
    #[serde(flatten)]
    pub patch: MascotPatch,
}

/// 注册 CLI label 请求。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterCliLabelRequest {
    /// 目标 mascot ID。
    pub mascot_id: i32,
    /// 可选的首选 label。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<CliLabel>,
}

/// CLI label 与 mascot ID 的对应关系。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliLabelInfo {
    /// 已分配的 label。
    pub label: CliLabel,
    /// 对应的 mascot ID。
    pub mascot_id: i32,
}

/// 列表 selector 请求。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListMascotsRequest {
    /// 空 selector 匹配全部 mascot。
    #[serde(default)]
    pub selector: Selector,
}

/// 关闭 mascot selector 请求。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DismissAllMascotsRequest {
    /// 空 selector 关闭全部 mascot。
    #[serde(default)]
    pub selector: Selector,
}

/// ping 响应信息。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiPingInfo {
    /// 服务是否就绪。
    pub ok: bool,
    /// 应用名称。
    pub app: String,
    /// API 版本。
    pub api_version: String,
}

/// ping 响应的简短别名。
pub type PingInfo = ApiPingInfo;

/// 兼容旧命名的命令状态别名。
pub type MascotCommandStatus = ApiError;

