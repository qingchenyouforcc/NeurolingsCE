//! API 命令到运行时的线程内业务适配层。

use api::{Anchor, ApiError, ApiRequest, CliLabel, Command, LoadedMascotInfo, MascotInfo};
use runtime::{Runtime, RuntimeError, SpawnRequest};
use serde_json::{Value, json};
use std::path::PathBuf;
use store::StoreInstaller;

/// 应用名，保持 HTTP 与 CLI 的稳定输出。
pub const APP_NAME: &str = "NeurolingsCE";

/// 当前进程内的命令服务。
#[derive(Debug)]
pub struct CommandService {
    runtime: Runtime,
    loaded: Vec<LoadedMascotInfo>,
    stopped: bool,
    installer: Option<StoreInstaller>,
}

impl Default for CommandService {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandService {
    /// 创建带默认模板的服务。
    pub fn new() -> Self {
        let root = std::env::var_os("NEUROLINGSCE_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("NeurolingsCE"));
        Self::with_storage(root.join("mascots"), root.join("cache"))
    }

    /// 使用指定目录创建服务，便于桌面配置和隔离测试。
    pub fn with_storage(storage: impl Into<PathBuf>, cache: impl Into<PathBuf>) -> Self {
        let installer = StoreInstaller::new(storage, cache).ok();
        Self {
            runtime: Runtime::new(),
            loaded: vec![LoadedMascotInfo {
                id: 0,
                name: "Default Mascot".into(),
                version: "1.0".into(),
                description: "Default mascot for the application.".into(),
                author: "NeurolingsCE".into(),
            }],
            stopped: false,
            installer,
        }
    }

    /// 访问运行时，用于 GUI tick 和测试。
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// 访问可变运行时。
    pub fn runtime_mut(&mut self) -> &mut Runtime {
        &mut self.runtime
    }

    /// 返回单个 mascot 的 HTTP/IPC 响应对象。
    pub fn get_mascot(&self, id: i32) -> Result<Value, ApiError> {
        let mascot = self.runtime.get(id).map_err(map_runtime_error)?;
        Ok(json!({ "mascot": to_info(mascot) }))
    }

    /// 返回单个已加载模板的响应对象。
    pub fn get_loaded_mascot(&self, id: i32) -> Result<Value, ApiError> {
        let mascot = self
            .loaded
            .iter()
            .find(|mascot| mascot.id == id)
            .ok_or_else(|| {
                ApiError::failure(404, "loaded_mascot_not_found", "No such loaded mascot")
            })?;
        Ok(json!({ "loaded_mascot": mascot }))
    }

    /// 执行一个已解析的 API 请求并生成稳定 JSON 对象。
    pub fn execute(&mut self, request: ApiRequest) -> Result<Value, ApiError> {
        if self.stopped && !matches!(request.command, Command::Ping | Command::StopRuntime) {
            return Err(ApiError::failure(
                409,
                "runtime_stopped",
                "Runtime is stopped",
            ));
        }
        match request.command {
            Command::Ping => {
                Ok(json!({ "ok": true, "app": APP_NAME, "api_version": api::API_VERSION }))
            }
            Command::ListMascots => {
                let selector = request
                    .field("selector")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let mascots = self
                    .runtime
                    .list()
                    .into_iter()
                    .filter(|mascot| selector.is_empty() || mascot.name() == selector)
                    .map(to_info)
                    .collect::<Vec<_>>();
                Ok(json!({ "mascots": mascots }))
            }
            Command::ListLoadedMascots => Ok(json!({ "loaded_mascots": self.loaded })),
            Command::SpawnMascot => self.spawn(request),
            Command::AlterMascot => self.alter(request),
            Command::DismissMascot => self.dismiss(request),
            Command::DismissAllMascots => {
                let selector = request
                    .field("selector")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let count = if selector.is_empty() {
                    self.runtime.dismiss_all()
                } else {
                    self.runtime.dismiss_all_named(selector)
                };
                Ok(json!({ "dismissed": count }))
            }
            Command::RegisterCliLabel => self.register_label(request),
            Command::ImportMascotTemplate => self.import_template(request),
            Command::RemoveMascotTemplate => self.remove_template(request),
            Command::GetCliLabel => {
                let id = field_i32(&request, "mascot_id")?;
                let label = self
                    .runtime
                    .get(id)
                    .map_err(map_runtime_error)?
                    .label()
                    .ok_or_else(|| {
                        ApiError::failure(404, "label_not_found", "No label for mascot")
                    })?;
                Ok(json!({ "label": label, "mascot_id": id }))
            }
            Command::StopRuntime => {
                self.runtime.dismiss_all();
                self.stopped = true;
                Ok(json!({ "stopped": true }))
            }
            Command::ShowManager | Command::ShowCodexNotification | Command::Unknown(_) => {
                Err(ApiError::failure(
                    400,
                    "unsupported_command",
                    "Command is not available in this runtime",
                ))
            }
        }
    }

    fn spawn(&mut self, request: ApiRequest) -> Result<Value, ApiError> {
        let name = request
            .field("name")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                request
                    .field("data_id")
                    .and_then(Value::as_i64)
                    .and_then(|id| {
                        self.loaded
                            .iter()
                            .find(|template| template.id == id as i32)
                            .map(|template| template.name.clone())
                    })
            })
            .ok_or_else(|| ApiError::bad_request("name or data_id is required"))?;
        let data_id = request
            .field("data_id")
            .and_then(Value::as_i64)
            .map(|value| value as i32)
            .unwrap_or(0);
        let mut spawn = SpawnRequest::new(name, data_id);
        if let Some(anchor) = request.field("anchor") {
            spawn.anchor =
                serde_json::from_value(anchor.clone()).map_err(|_| ApiError::invalid_anchor())?;
        }
        if let Some(behavior) = request.field("behavior").and_then(Value::as_str) {
            spawn.active_behavior = Some(behavior.to_owned());
        }
        if let Some(label) = request.field("label").and_then(Value::as_u64) {
            spawn.label = Some(CliLabel::try_from(label)?);
        }
        let mascot = self.runtime.spawn(spawn).map_err(map_runtime_error)?;
        Ok(json!({ "mascot": to_info(mascot) }))
    }

    fn alter(&mut self, request: ApiRequest) -> Result<Value, ApiError> {
        let id = field_i32(&request, "id")?;
        if let Some(anchor) = request.field("anchor") {
            let anchor: Anchor =
                serde_json::from_value(anchor.clone()).map_err(|_| ApiError::invalid_anchor())?;
            self.runtime
                .set_anchor(id, anchor)
                .map_err(map_runtime_error)?;
        }
        if let Some(behavior) = request.field("behavior").and_then(Value::as_str) {
            self.runtime
                .set_active_behavior(id, Some(behavior.to_owned()))
                .map_err(map_runtime_error)?;
        }
        Ok(json!({ "mascot": to_info(self.runtime.get(id).map_err(map_runtime_error)?) }))
    }

    fn dismiss(&mut self, request: ApiRequest) -> Result<Value, ApiError> {
        let id = request
            .field("id")
            .and_then(Value::as_i64)
            .map(|value| value as i32)
            .or_else(|| {
                request
                    .field("mascot_id")
                    .and_then(Value::as_i64)
                    .map(|value| value as i32)
            })
            .ok_or_else(|| ApiError::bad_request("id is required"))?;
        self.runtime.dismiss(id).map_err(map_runtime_error)?;
        Ok(json!({ "dismissed": true, "id": id }))
    }

    fn register_label(&mut self, request: ApiRequest) -> Result<Value, ApiError> {
        let id = field_i32(&request, "mascot_id")?;
        let preferred = request
            .field("label")
            .and_then(Value::as_u64)
            .map(CliLabel::try_from)
            .transpose()?;
        let label = self
            .runtime
            .register_label(id, preferred)
            .map_err(map_runtime_error)?;
        Ok(json!({ "label": label, "mascot_id": id }))
    }

    fn import_template(&mut self, request: ApiRequest) -> Result<Value, ApiError> {
        let path = request
            .field("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError::bad_request("path is required"))?;
        let bytes = std::fs::read(path)
            .map_err(|error| ApiError::failure(400, "package_read_failed", error.to_string()))?;
        let installer = self.installer.as_ref().ok_or_else(|| {
            ApiError::failure(503, "storage_unavailable", "Mascot storage is unavailable")
        })?;
        let (metadata, _) = installer
            .install_bytes(&bytes)
            .map_err(|error| ApiError::failure(400, "package_invalid", error.to_string()))?;
        let id = self.loaded.iter().map(|item| item.id).max().unwrap_or(-1) + 1;
        let info = LoadedMascotInfo {
            id,
            name: metadata.name,
            version: metadata.version,
            description: metadata.description,
            author: metadata.author,
        };
        self.loaded.push(info.clone());
        Ok(json!({ "loaded_mascot": info }))
    }

    fn remove_template(&mut self, request: ApiRequest) -> Result<Value, ApiError> {
        let name = request
            .field("name")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError::bad_request("name is required"))?;
        if name == "Default Mascot" {
            return Err(ApiError::failure(
                409,
                "protected_template",
                "The default mascot cannot be removed",
            ));
        }
        let Some(index) = self.loaded.iter().position(|item| item.name == name) else {
            return Err(ApiError::failure(
                404,
                "loaded_mascot_not_found",
                "No such loaded mascot",
            ));
        };
        let info = self.loaded.remove(index);
        if let Some(installer) = &self.installer {
            let path = assets::package_path_for_name(installer.storage_path(), &info.name);
            let _ = std::fs::remove_file(path);
        }
        Ok(json!({ "removed": true, "id": info.id }))
    }
}

fn to_info(mascot: runtime::MascotSession) -> MascotInfo {
    MascotInfo {
        id: mascot.id(),
        data_id: mascot.data_id(),
        name: mascot.name().to_owned(),
        active_behavior: mascot.active_behavior().map(str::to_owned),
        label: mascot.label(),
        anchor: mascot.anchor(),
    }
}

fn field_i32(request: &ApiRequest, name: &str) -> Result<i32, ApiError> {
    request
        .field(name)
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| ApiError::bad_request(format!("{name} must be an integer")))
}

fn map_runtime_error(error: RuntimeError) -> ApiError {
    match error {
        RuntimeError::MascotNotFound(_) => {
            ApiError::failure(404, "mascot_not_found", error.to_string())
        }
        RuntimeError::LabelInUse(_) | RuntimeError::LabelConflict { .. } => {
            ApiError::failure(409, "label_in_use", error.to_string())
        }
        _ => ApiError::bad_request(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::CommandService;
    use api::{ApiRequest, Command};
    use serde_json::json;

    #[test]
    fn command_service_spawns_lists_alters_and_dismisses() {
        let mut service = CommandService::new();
        assert_eq!(
            service.execute(ApiRequest::new(Command::Ping)).unwrap()["ok"],
            true
        );
        let spawned = service
            .execute(
                ApiRequest::new(Command::SpawnMascot)
                    .with_field("name", json!("Default Mascot"))
                    .with_field("anchor", json!({"x": 4.0, "y": 8.0})),
            )
            .unwrap();
        let id = spawned["mascot"]["id"].as_i64().unwrap() as i32;
        let listed = service
            .execute(ApiRequest::new(Command::ListMascots))
            .unwrap();
        assert_eq!(listed["mascots"].as_array().unwrap().len(), 1);
        service
            .execute(
                ApiRequest::new(Command::AlterMascot)
                    .with_field("id", json!(id))
                    .with_field("behavior", json!("Fall")),
            )
            .unwrap();
        assert_eq!(
            service
                .execute(ApiRequest::new(Command::DismissMascot).with_field("id", json!(id)))
                .unwrap()["dismissed"],
            true
        );
    }

    #[test]
    fn stop_is_idempotent_and_blocks_mutations() {
        let mut service = CommandService::new();
        service
            .execute(ApiRequest::new(Command::StopRuntime))
            .unwrap();
        service
            .execute(ApiRequest::new(Command::StopRuntime))
            .unwrap();
        assert!(
            service
                .execute(ApiRequest::new(Command::SpawnMascot).with_field("name", json!("A")))
                .is_err()
        );
    }
}
