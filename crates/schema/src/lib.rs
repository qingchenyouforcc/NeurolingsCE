//! 共享数据模型与 JSON 契约。
//!
//! 该 crate 只描述「数据是什么」，不包含任何业务逻辑，
//! 以便引擎、包管理、网络服务与界面层共用同一套结构定义。

/// 桌宠元数据，对应资源包内的 `info.json`。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MascotInfo {
    /// 桌宠显示名称。
    pub name: String,
    /// 资源包版本字符串。
    pub version: String,
    /// 简介文本。
    pub description: String,
    /// 作者署名。
    pub author: String,
}
