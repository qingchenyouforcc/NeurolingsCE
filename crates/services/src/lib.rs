//! API 命令到运行时的线程内业务适配层。

use api::{
    Anchor, ApiError, ApiRequest, CliLabel, Command, LoadedMascotInfo, MascotInfo, Selector,
};
use engine::{EvalContext, Expression, Value as EngineValue};
use runtime::{Runtime, RuntimeError, SpawnRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
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
                let selector = selector_from_request(&request)?;
                let matcher = SelectorMatcher::new(&selector);
                let mascots = self
                    .runtime
                    .list()
                    .into_iter()
                    .filter(|mascot| matcher.matches(mascot))
                    .map(to_info)
                    .collect::<Vec<_>>();
                Ok(json!({ "mascots": mascots }))
            }
            Command::ListLoadedMascots => Ok(json!({ "loaded_mascots": self.loaded })),
            Command::SpawnMascot => self.spawn(request),
            Command::AlterMascot => self.alter(request),
            Command::DismissMascot => self.dismiss(request),
            Command::DismissAllMascots => {
                let selector = selector_from_request(&request)?;
                let matcher = SelectorMatcher::new(&selector);
                let ids = self
                    .runtime
                    .list()
                    .into_iter()
                    .filter(|mascot| matcher.matches(mascot))
                    .map(|mascot| mascot.id())
                    .collect::<Vec<_>>();
                let count = ids
                    .into_iter()
                    .filter(|id| self.runtime.dismiss(*id).is_ok())
                    .count();
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

fn selector_from_request(request: &ApiRequest) -> Result<String, ApiError> {
    let selector = request
        .field("selector")
        .and_then(Value::as_str)
        .unwrap_or("");
    Selector::try_from(selector).map(Selector::into_inner)
}

struct SelectorMatcher {
    raw: String,
    expression: Option<Expression>,
}

impl SelectorMatcher {
    fn new(raw: &str) -> Self {
        Self {
            raw: raw.to_owned(),
            expression: (!raw.trim().is_empty())
                .then(|| Expression::parse(raw).ok())
                .flatten(),
        }
    }

    fn matches(&self, mascot: &runtime::MascotSession) -> bool {
        if self.raw.trim().is_empty() || self.raw == mascot.name() {
            return true;
        }
        let Some(expression) = &self.expression else {
            return false;
        };
        let mut context = EvalContext::default();
        let mut object = BTreeMap::new();
        object.insert("id".to_owned(), EngineValue::Number(f64::from(mascot.id())));
        object.insert(
            "dataId".to_owned(),
            EngineValue::Number(f64::from(mascot.data_id())),
        );
        object.insert(
            "name".to_owned(),
            EngineValue::String(mascot.name().to_owned()),
        );
        object.insert(
            "label".to_owned(),
            mascot.label().map_or(EngineValue::Null, |label| {
                EngineValue::Number(label.value() as f64)
            }),
        );
        object.insert(
            "activeBehavior".to_owned(),
            mascot.active_behavior().map_or(EngineValue::Null, |value| {
                EngineValue::String(value.to_owned())
            }),
        );
        context.set_variable("mascot", EngineValue::Object(object.clone()));
        for (name, value) in object {
            context.set_variable(name, value);
        }
        match expression.evaluate(&mut context) {
            Ok(EngineValue::Bool(value)) => value,
            Ok(EngineValue::Number(value)) => value != 0.0 && !value.is_nan(),
            Ok(EngineValue::String(value)) => !value.is_empty(),
            Ok(EngineValue::Object(_)) => true,
            Ok(EngineValue::Null) | Err(_) => false,
        }
    }
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
    use std::io::Cursor;

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

    #[test]
    fn selector_expressions_filter_by_public_mascot_fields() {
        let mut service = CommandService::new();
        service
            .execute(
                ApiRequest::new(Command::SpawnMascot)
                    .with_field("name", json!("Alpha"))
                    .with_field("label", json!(3))
                    .with_field("behavior", json!("Walk")),
            )
            .unwrap();
        service
            .execute(
                ApiRequest::new(Command::SpawnMascot)
                    .with_field("name", json!("Beta"))
                    .with_field("label", json!(4))
                    .with_field("behavior", json!("Idle")),
            )
            .unwrap();

        let by_name = service
            .execute(
                ApiRequest::new(Command::ListMascots)
                    .with_field("selector", json!("name == 'Alpha'")),
            )
            .unwrap();
        assert_eq!(by_name["mascots"].as_array().unwrap().len(), 1);
        assert_eq!(by_name["mascots"][0]["name"], "Alpha");

        let by_label = service
            .execute(
                ApiRequest::new(Command::ListMascots)
                    .with_field("selector", json!("mascot.label == 4")),
            )
            .unwrap();
        assert_eq!(by_label["mascots"].as_array().unwrap().len(), 1);
        assert_eq!(by_label["mascots"][0]["name"], "Beta");

        let dismissed = service
            .execute(
                ApiRequest::new(Command::DismissAllMascots)
                    .with_field("selector", json!("activeBehavior == 'Walk'")),
            )
            .unwrap();
        assert_eq!(dismissed["dismissed"], 1);
    }

    #[test]
    fn import_and_remove_template_updates_loaded_catalog() {
        let root =
            std::env::temp_dir().join(format!("neurolingsce-services-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let package_path = root.join("fox.zip");
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file("info.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(
            &mut archive,
            br#"{"name":"Fox","version":"1.0","description":"A fox","author":"Test"}"#,
        )
        .unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        std::fs::write(&package_path, bytes).unwrap();

        let mut service = CommandService::with_storage(root.join("mascots"), root.join("cache"));
        let imported = service
            .execute(
                ApiRequest::new(Command::ImportMascotTemplate)
                    .with_field("path", json!(package_path.to_string_lossy())),
            )
            .unwrap();
        assert_eq!(imported["loaded_mascot"]["name"], "Fox");
        assert_eq!(
            service
                .execute(ApiRequest::new(Command::ListLoadedMascots))
                .unwrap()["loaded_mascots"]
                .as_array()
                .unwrap()
                .len(),
            2
        );

        let removed = service
            .execute(
                ApiRequest::new(Command::RemoveMascotTemplate).with_field("name", json!("Fox")),
            )
            .unwrap();
        assert_eq!(removed["removed"], true);
        assert_eq!(
            service
                .execute(ApiRequest::new(Command::ListLoadedMascots))
                .unwrap()["loaded_mascots"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
