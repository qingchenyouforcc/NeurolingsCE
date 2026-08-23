//! 独立命令执行：模板的 list/add/remove/validate 不依赖运行时；
//! 运行时命令转交 runtime 模块经 IPC 执行。

use std::fs;
use std::path::{Path, PathBuf};

use neurolings_pack::MascotMetadata;
use neurolings_pack::{
    MascotPackageReport, default_storage_path, import_archive, inspect_package, metadata_from_json,
    migrate_legacy_directories, package_path_for_name, sanitized_package_base_name,
    validate_package,
};

use crate::parser::{CliCommand, CliCommandKind, CliError};

/// 已加载模板（契约 loadedMascotInfo）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoadedMascotInfo {
    pub id: i64,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
}

/// 运行中的桌宠（契约 mascotInfo）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MascotInfo {
    pub id: i64,
    pub data_id: i64,
    pub name: String,
    pub cli_label: Option<i64>,
    pub anchor_x: f64,
    pub anchor_y: f64,
    pub active_behavior: Option<String>,
}

/// 命令执行结果。
#[derive(Debug, Default)]
pub struct CliExecutionResult {
    pub error: Option<CliError>,
    pub loaded_mascots: Vec<LoadedMascotInfo>,
    pub mascots: Vec<MascotInfo>,
    pub mascot: Option<MascotInfo>,
    pub mascot_validation: Option<MascotPackageReport>,
    pub removed_template_name: String,
    pub assigned_label: Option<i64>,
    pub closed_label: Option<i64>,
    pub stopped: bool,
    pub codex_handled: bool,
    pub codex_event_type: Option<String>,
    pub codex_state: Option<String>,
}

const STORAGE_README: &str = "\
Manually importing shimeji by copying its contents into this folder may\n\
cause problems. Prefer NeurolingsCE-cli --mascot add <zip> or the GUI import\n\
dialog unless you have a good reason not to.\n";

/// 解析并创建桌宠存储目录。
pub fn ensure_mascot_storage(storage: Option<&Path>) -> Result<PathBuf, CliError> {
    let Some(path) = storage.map(Path::to_path_buf).or_else(default_storage_path) else {
        return Err(CliError::new(
            "storage_unavailable",
            "Could not determine mascot storage directory",
            1,
        ));
    };
    if fs::create_dir_all(&path).is_err() {
        let mut error = CliError::new(
            "storage_unavailable",
            "Could not create mascot storage directory",
            1,
        );
        error.details = path.display().to_string();
        return Err(error);
    }
    let readme = path.join("README.txt");
    if !readme.exists() {
        let _ = fs::write(&readme, STORAGE_README);
    }
    Ok(path)
}

/// 列出存储中已安装的模板：内置默认模板（id 0）在前，
/// 其余按名称升序（不区分大小写）编号 1..n。
pub fn list_standalone_loaded_mascots(storage: &Path) -> Vec<LoadedMascotInfo> {
    migrate_legacy_directories(storage);

    // 内置默认模板始终以 id 0 列在首位。
    let default_meta = default_metadata();
    let mut templates: Vec<LoadedMascotInfo> = vec![LoadedMascotInfo {
        id: 0,
        name: default_meta.name,
        version: default_meta.version,
        description: default_meta.description,
        author: default_meta.author,
    }];
    let Ok(read) = fs::read_dir(storage) else {
        return templates;
    };
    for entry in read.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let entry_name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        if file_type.is_file() && entry_name.to_ascii_lowercase().ends_with(".mascot") {
            if let Ok(metadata) = inspect_package(&path) {
                templates.push(LoadedMascotInfo {
                    id: -1,
                    name: metadata.name,
                    version: metadata.version,
                    description: metadata.description,
                    author: metadata.author,
                });
            }
        } else if file_type.is_dir() {
            let info_path = path.join("info.json");
            if !info_path.is_file() {
                continue;
            }
            let Ok(bytes) = fs::read(&info_path) else {
                continue;
            };
            if let Ok(metadata) = metadata_from_json(&bytes) {
                templates.push(LoadedMascotInfo {
                    id: -1,
                    name: metadata.name,
                    version: metadata.version,
                    description: metadata.description,
                    author: metadata.author,
                });
            }
        }
    }

    // 包模板排序后从 1 开始编号（默认模板固定为 0）。
    let mut packages: Vec<LoadedMascotInfo> = templates.drain(1..).collect();
    packages.sort_by(|lhs, rhs| {
        lhs.name
            .to_ascii_lowercase()
            .cmp(&rhs.name.to_ascii_lowercase())
    });
    for (index, template) in packages.iter_mut().enumerate() {
        template.id = (index + 1) as i64;
    }
    templates.extend(packages);
    templates
}

/// 内置默认模板的元数据（虚拟模板 @，与运行时内嵌一致）。
fn default_metadata() -> MascotMetadata {
    MascotMetadata {
        name: "@".to_string(),
        version: "1.0".to_string(),
        description: "Default mascot for the application.".to_string(),
        author: "pixelomer[https://github.com/pixelomer]".to_string(),
    }
}

/// 从压缩包导入模板到存储，返回导入的模板信息。
pub fn import_standalone_mascot_template(
    archive_path: &str,
    storage: Option<&Path>,
) -> Result<Vec<LoadedMascotInfo>, CliError> {
    let path = Path::new(archive_path);
    if !path.exists() || !path.is_file() {
        let mut error = CliError::new("invalid_arguments", "Mascot archive does not exist", 2);
        error.details = archive_path.to_string();
        return Err(error);
    }

    let storage_path = ensure_mascot_storage(storage)?;
    migrate_legacy_directories(&storage_path);

    let changed = match import_archive(path, &storage_path) {
        Ok(changed) => changed,
        Err(error) => {
            return Err(CliError::new("import_failed", &error.to_string(), 1));
        }
    };
    if changed.is_empty() {
        return Err(CliError::new(
            "import_failed",
            "Could not import any mascots from the specified archive",
            1,
        ));
    }

    let all = list_standalone_loaded_mascots(&storage_path);
    let mut imported = Vec::new();
    for name in changed {
        match all.iter().find(|template| template.name == name) {
            Some(template) => imported.push(template.clone()),
            None => imported.push(LoadedMascotInfo {
                id: -1,
                name,
                ..Default::default()
            }),
        }
    }
    Ok(imported)
}

fn normalize_mascot_template_name(name: &str) -> &str {
    let trimmed = name.trim();
    if trimmed.len() >= ".mascot".len()
        && trimmed[trimmed.len() - ".mascot".len()..].eq_ignore_ascii_case(".mascot")
    {
        &trimmed[..trimmed.len() - ".mascot".len()]
    } else {
        trimmed
    }
}

fn has_path_separator(value: &str) -> bool {
    value.contains('/') || value.contains('\\') || value == "." || value == ".."
}

/// 拒绝删除存储目录之外的路径。
fn is_inside_storage(storage: &Path, target: &Path) -> bool {
    let (Ok(storage_canonical), Ok(target_canonical)) =
        (storage.canonicalize(), target.canonicalize())
    else {
        return false;
    };
    target_canonical.starts_with(storage_canonical)
}

/// 按名称从存储中移除模板。
pub fn remove_standalone_mascot_template(
    requested_name: &str,
    storage: Option<&Path>,
) -> Result<String, CliError> {
    let name = normalize_mascot_template_name(requested_name);
    if name.is_empty() || has_path_separator(name) {
        return Err(CliError::new(
            "invalid_arguments",
            "Mascot template name must be a plain template name",
            2,
        ));
    }
    if name == "@" || name == "Default" || name == "Default Mascot" {
        return Err(CliError::new(
            "mascot_template_not_deletable",
            "Mascot template cannot be deleted",
            1,
        ));
    }

    let storage_path = ensure_mascot_storage(storage)?;
    migrate_legacy_directories(&storage_path);

    let package_target = package_path_for_name(&storage_path, name);
    let directory_target = storage_path.join(sanitized_package_base_name(name));
    let target = if package_target.is_file() {
        package_target
    } else if directory_target.is_dir() {
        directory_target
    } else {
        return Err(CliError::new(
            "mascot_template_not_found",
            "No such mascot template",
            1,
        ));
    };

    if !is_inside_storage(&storage_path, &target) {
        return Err(CliError::new(
            "invalid_template_path",
            "Refusing to delete a mascot outside the storage directory",
            1,
        ));
    }

    let removed = if target.is_dir() {
        fs::remove_dir_all(&target)
    } else {
        fs::remove_file(&target)
    };
    if removed.is_err() {
        let mut error = CliError::new("remove_failed", "Could not remove mascot template", 1);
        error.details = target.display().to_string();
        return Err(error);
    }

    Ok(name.to_string())
}

fn runtime_not_implemented() -> CliError {
    CliError::new("not_implemented", "Unsupported mascot action", 2)
}

/// 执行一条已解析的命令（独立命令就地执行，运行时命令转交 IPC）。
pub fn execute(command: &CliCommand, storage: Option<&Path>) -> CliExecutionResult {
    let mut result = CliExecutionResult::default();
    match command.kind {
        CliCommandKind::Help | CliCommandKind::Version => {
            // 输出由格式化层生成。
        }
        CliCommandKind::DocumentMascot => match command.mascot_action.as_str() {
            "list" => match ensure_mascot_storage(storage) {
                Ok(storage_path) => {
                    result.loaded_mascots = list_standalone_loaded_mascots(&storage_path);
                }
                Err(error) => result.error = Some(error),
            },
            "add" => match import_standalone_mascot_template(&command.mascot_archive_path, storage)
            {
                Ok(imported) => result.loaded_mascots = imported,
                Err(error) => result.error = Some(error),
            },
            "remove" => {
                match remove_standalone_mascot_template(&command.mascot_template_name, storage) {
                    Ok(removed) => result.removed_template_name = removed,
                    Err(error) => result.error = Some(error),
                }
            }
            "validate" => {
                let path = Path::new(&command.mascot_archive_path);
                if !path.exists() || !path.is_file() {
                    let mut error =
                        CliError::new("invalid_arguments", "Mascot package does not exist", 2);
                    error.details = path
                        .canonicalize()
                        .unwrap_or_else(|_| path.to_path_buf())
                        .display()
                        .to_string();
                    result.error = Some(error);
                } else {
                    result.mascot_validation = Some(validate_package(path));
                }
            }
            _ => result.error = Some(runtime_not_implemented()),
        },
        _ => return crate::runtime::execute_runtime(command),
    }
    result
}
