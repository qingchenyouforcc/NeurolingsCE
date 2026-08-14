//! 模板发现与加载：从存储目录（.mascot 包/解压目录）或散开目录加载，
//! 并维护运行期模板注册表（导入/移除/重载）。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use neurolings_engine::mascot::Template;
use neurolings_pack::metadata::MascotMetadata;

/// 内置默认桌宠（首次运行时安装到存储目录）。
pub static DEFAULT_MASCOT: include_dir::Dir =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../assets/DefaultMascot");

/// 一个已加载模板的完整信息。
pub struct LoadedTemplate {
    pub name: String,
    pub dir: PathBuf,
    pub actions_xml: String,
    pub behaviors_xml: String,
    pub metadata: MascotMetadata,
}

impl LoadedTemplate {
    /// 转换为引擎模板（XML 文本克隆，目录信息保留在本结构）。
    pub fn engine_template(&self) -> Template {
        Template {
            name: self.name.clone(),
            actions_xml: self.actions_xml.clone(),
            behaviors_xml: self.behaviors_xml.clone(),
            path: self.dir.to_string_lossy().into_owned(),
        }
    }
}

/// 运行期模板注册表：名称、元数据与包目录的查询入口。
#[derive(Default)]
pub struct TemplateStore {
    names: Vec<String>,
    metadata: HashMap<String, MascotMetadata>,
    pack_dirs: HashMap<String, PathBuf>,
}

impl TemplateStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个已加载模板。
    pub fn register(&mut self, template: &LoadedTemplate) {
        if !self.names.iter().any(|n| n == &template.name) {
            self.names.push(template.name.clone());
        }
        self.metadata
            .insert(template.name.clone(), template.metadata.clone());
        self.pack_dirs
            .insert(template.name.clone(), template.dir.clone());
    }

    /// 注销模板。
    pub fn deregister(&mut self, name: &str) {
        self.names.retain(|n| n != name);
        self.metadata.remove(name);
        self.pack_dirs.remove(name);
    }

    /// 按名称升序排列的模板名列表。
    pub fn names_sorted(&self) -> Vec<String> {
        let mut names = self.names.clone();
        names.sort();
        names
    }

    pub fn contains(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }

    pub fn metadata(&self, name: &str) -> Option<&MascotMetadata> {
        self.metadata.get(name)
    }

    pub fn pack_dir(&self, name: &str) -> Option<PathBuf> {
        self.pack_dirs.get(name).cloned()
    }
}

fn load_dir(dir: &Path) -> Option<LoadedTemplate> {
    let actions_path = dir.join("actions.xml");
    let behaviors_path = dir.join("behaviors.xml");
    if !actions_path.is_file() || !behaviors_path.is_file() {
        return None;
    }
    let actions_xml = fs::read_to_string(&actions_path).ok()?;
    let behaviors_xml = fs::read_to_string(&behaviors_path).ok()?;
    let metadata: MascotMetadata = fs::read(dir.join("info.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let name = if metadata.name.trim().is_empty() {
        dir.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        metadata.name.clone()
    };
    Some(LoadedTemplate {
        name,
        dir: dir.to_path_buf(),
        actions_xml,
        behaviors_xml,
        metadata,
    })
}

/// 从子目录即桌宠包的目录结构加载（mascot_pack/ 布局）。
pub fn load_from_dir(root: &Path) -> Vec<LoadedTemplate> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir()
            && let Some(template) = load_dir(&path)
        {
            out.push(template);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 从运行时存储目录加载：解压的桌宠子目录与 .mascot 包文件
/// （包文件解压到缓存目录）。
pub fn load_from_storage(storage: &Path, cache: &Path) -> Vec<LoadedTemplate> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(storage) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(template) = load_dir(&path) {
                out.push(template);
            }
        } else if path.extension().is_some_and(|e| e == "mascot") {
            let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
            let Some(stem) = stem else { continue };
            let target = cache.join(&stem);
            if !target.join("actions.xml").is_file() {
                let _ = fs::create_dir_all(&target);
                if neurolings_pack::package::extract_package(&path, &target).is_err() {
                    continue;
                }
            }
            if let Some(template) = load_dir(&target) {
                out.push(template);
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 首次运行时把内置默认桌宠安装到存储目录。
pub fn install_default_if_missing(storage: &Path) {
    let target = storage.join("Default");
    if target.join("actions.xml").is_file() {
        return;
    }
    let _ = fs::create_dir_all(&target);
    extract_embedded_dir(&DEFAULT_MASCOT, &target);
}

fn extract_embedded_dir(dir: &include_dir::Dir, target: &Path) {
    for file in dir.files() {
        let Some(name) = file.path().file_name() else {
            continue;
        };
        let _ = fs::write(target.join(name), file.contents());
    }
    for subdir in dir.dirs() {
        let Some(name) = subdir.path().file_name() else {
            continue;
        };
        let sub_target = target.join(name);
        let _ = fs::create_dir_all(&sub_target);
        extract_embedded_dir(subdir, &sub_target);
    }
}
