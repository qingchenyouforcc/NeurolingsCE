//! 模板发现与加载：从存储目录（.mascot 包/解压目录）或散开目录加载，
//! 并维护运行期模板注册表（导入/移除/重载）。
//! 默认桌宠是从内嵌资源构建的虚拟模板（名为 @，不落盘、不可删除）。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use neurolings_engine::mascot::{Factory, Template};
use neurolings_pack::metadata::MascotMetadata;

/// 内置默认桌宠资源（虚拟模板 @ 的内容来源）。
pub static DEFAULT_MASCOT: include_dir::Dir =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../assets/DefaultMascot");

/// 默认模板的对外名称（info.json / CLI / 列表统一显示为 Default）。
pub const DEFAULT_TEMPLATE_NAME: &str = "Default";
/// 内嵌资源路径别名；召唤与 Codex 仍接受 `@`。
pub const DEFAULT_TEMPLATE_ALIAS: &str = "@";

/// 是否为内置默认模板（显示名 Default 或路径别名 @）。
pub fn is_default_template(name: &str) -> bool {
    name == DEFAULT_TEMPLATE_NAME || name == DEFAULT_TEMPLATE_ALIAS || name == "Default Mascot"
}

/// 一个已加载模板的完整信息。
pub struct LoadedTemplate {
    pub name: String,
    pub dir: PathBuf,
    pub actions_xml: String,
    pub behaviors_xml: String,
    pub metadata: MascotMetadata,
    /// 内嵌虚拟模板：无磁盘目录，不可删除、无包级音效/气泡文件。
    pub virtual_: bool,
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

    /// 注册一个已加载模板（虚拟模板不记录包目录）。
    pub fn register(&mut self, template: &LoadedTemplate) {
        if !self.names.iter().any(|n| n == &template.name) {
            self.names.push(template.name.clone());
        }
        self.metadata
            .insert(template.name.clone(), template.metadata.clone());
        if !template.virtual_ {
            self.pack_dirs
                .insert(template.name.clone(), template.dir.clone());
        }
    }

    /// 把 `@` / `Default` / `Default Mascot` 解析成库里实际登记的名字。
    pub fn resolve<'a>(&'a self, name: &str) -> Option<&'a str> {
        if let Some(found) = self.names.iter().find(|n| n.as_str() == name) {
            return Some(found.as_str());
        }
        if is_default_template(name) {
            return self
                .names
                .iter()
                .find(|n| is_default_template(n))
                .map(|n| n.as_str());
        }
        None
    }

    /// 注销模板（虚拟模板不可注销）。
    pub fn deregister(&mut self, name: &str) -> bool {
        if is_default_template(name) {
            return false;
        }
        self.names.retain(|n| n != name);
        self.metadata.remove(name);
        self.pack_dirs.remove(name);
        true
    }

    /// 按名称升序排列的模板名列表（大小写不敏感）。
    pub fn names_sorted(&self) -> Vec<String> {
        let mut names = self.names.clone();
        names.sort_by_key(|n| n.to_lowercase());
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
    // 对齐 C++ MascotData：img 目录存在且至少含一张 PNG（大小写不敏感），
    // 否则模板无效、不进库。
    let has_png = fs::read_dir(dir.join("img"))
        .map(|entries| {
            entries.flatten().any(|entry| {
                let path = entry.path();
                path.is_file()
                    && path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
            })
        })
        .unwrap_or(false);
    if !has_png {
        crate::log::warn(
            "mascot",
            &format!(
                "skipping template {}: no PNG images in img directory",
                dir.display()
            ),
        );
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
        virtual_: false,
    })
}

/// 从内嵌资源构建默认虚拟模板（显示名 Default，不落盘、不可删除）。
pub fn load_default_virtual() -> Option<LoadedTemplate> {
    let read = |path: &str| -> Option<String> {
        DEFAULT_MASCOT
            .get_file(path)
            .map(|f| String::from_utf8_lossy(f.contents()).into_owned())
    };
    let actions_xml = read("actions.xml")?;
    let behaviors_xml = read("behaviors.xml")?;
    let metadata: MascotMetadata = read("info.json")
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let name = if metadata.name.trim().is_empty() {
        DEFAULT_TEMPLATE_NAME.to_string()
    } else {
        metadata.name.clone()
    };
    Some(LoadedTemplate {
        name,
        dir: PathBuf::new(),
        actions_xml,
        behaviors_xml,
        metadata,
        virtual_: true,
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

fn is_mascot_package_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mascot"))
}

fn cache_is_stale(package: &Path, cache_dir: &Path) -> bool {
    let xml = cache_dir.join("actions.xml");
    if !xml.is_file() {
        return true;
    }
    let package_time = fs::metadata(package).and_then(|m| m.modified()).ok();
    let cache_time = fs::metadata(xml).and_then(|m| m.modified()).ok();
    match (package_time, cache_time) {
        (Some(package_time), Some(cache_time)) => package_time > cache_time,
        _ => false,
    }
}

/// 从运行时存储目录加载：解压的桌宠子目录与 .mascot 包文件
/// （包文件解压到缓存目录；包比缓存新时重新解压）。
pub fn load_from_storage(storage: &Path, cache: &Path) -> Vec<LoadedTemplate> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(storage) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "log") {
                continue;
            }
            if let Some(template) = load_dir(&path) {
                out.push(template);
            }
        } else if is_mascot_package_file(&path) {
            let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
            let Some(stem) = stem else { continue };
            let target = cache.join(&stem);
            if cache_is_stale(&path, &target) {
                let _ = fs::remove_dir_all(&target);
                let _ = fs::create_dir_all(&target);
                if neurolings_pack::package::extract_package(&path, &target).is_err() {
                    let _ = fs::remove_dir_all(&target);
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

/// 把已在后台读取的模板快照同步进注册表与引擎工厂。
///
/// 此函数只更新内存状态，不执行磁盘 I/O，适合由运行时主循环提交后台结果。
pub fn apply_loaded_templates(
    store: &mut TemplateStore,
    factory: &mut Factory,
    on_disk: &[LoadedTemplate],
) -> Vec<String> {
    let disk_names: HashSet<String> = on_disk
        .iter()
        .map(|template| template.name.clone())
        .filter(|name| !is_default_template(name))
        .collect();

    let stale: Vec<String> = store
        .names_sorted()
        .into_iter()
        .filter(|name| !is_default_template(name) && !disk_names.contains(name))
        .collect();
    for name in &stale {
        store.deregister(name);
        let _ = factory.deregister_template(name);
    }

    for template in on_disk {
        if is_default_template(&template.name) || template.virtual_ {
            continue;
        }
        store.deregister(&template.name);
        let _ = factory.deregister_template(&template.name);
        store.register(template);
        if let Err(err) = factory.register_template(template.engine_template()) {
            crate::log::warn(
                "mascot",
                &format!("failed to register template {}: {err}", template.name),
            );
        }
    }
    store.names_sorted()
}

/// 初始化存储目录：写入警示 README（不覆盖已有文件），
/// 并清理早期版本落盘的默认模板目录（内容与内嵌一致才删除）。
pub fn prepare_storage(storage: &Path) {
    let _ = fs::create_dir_all(storage);
    // README 内容对齐原版 ManagerWindowSetup（NewOnly 语义：已存在则跳过）。
    let readme = storage.join("README.txt");
    if !readme.is_file() {
        let _ = fs::write(
            &readme,
            "Manually importing shimeji by copying its contents into this folder may\n\
             cause problems. You should use the import dialog in Shijima-Qt unless you\n\
             have a good reason not to.\n",
        );
    }
    cleanup_legacy_default(storage);
}

/// 早期版本把默认桌宠落盘到 <storage>/Default；默认模板已改为内嵌虚拟
/// 模板 @，目录内容与内嵌一致时删除，被用户改动过则保留为普通模板。
fn cleanup_legacy_default(storage: &Path) {
    let legacy = storage.join("Default");
    if !legacy.join("actions.xml").is_file() {
        return;
    }
    let matches_embedded = ["actions.xml", "behaviors.xml", "info.json"]
        .iter()
        .all(|name| {
            legacy.join(name).is_file()
                && DEFAULT_MASCOT.get_file(name).is_some_and(|f| {
                    fs::read(legacy.join(name)).is_ok_and(|bytes| bytes == f.contents())
                })
        });
    if matches_embedded {
        let _ = fs::remove_dir_all(&legacy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 在临时目录构造一个散开包：actions/behaviors 必有，img 按需。
    fn write_pack(root: &Path, name: &str, img_files: Option<&[&str]>) -> PathBuf {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("actions.xml"), "<actions/>").unwrap();
        fs::write(dir.join("behaviors.xml"), "<behaviors/>").unwrap();
        if let Some(files) = img_files {
            let img = dir.join("img");
            fs::create_dir_all(&img).unwrap();
            for file in files {
                fs::write(img.join(file), b"png").unwrap();
            }
        }
        dir
    }

    /// 无 img 目录的包不是有效模板（对齐 C++ MascotData）。
    #[test]
    fn load_from_dir_skips_pack_without_img() {
        let dir = tempfile::tempdir().unwrap();
        write_pack(dir.path(), "NoImg", None);
        assert!(load_from_dir(dir.path()).is_empty());
    }

    /// img 目录存在但没有任何 PNG：同样无效。
    #[test]
    fn load_from_dir_skips_pack_without_png() {
        let dir = tempfile::tempdir().unwrap();
        write_pack(dir.path(), "NoPng", Some(&["a.gif"]));
        assert!(load_from_dir(dir.path()).is_empty());
    }

    /// img 至少一张 PNG（后缀大小写不敏感）：正常加载。
    #[test]
    fn load_from_dir_accepts_pack_with_png_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        write_pack(dir.path(), "WithPng", Some(&["A.PNG"]));
        let loaded = load_from_dir(dir.path());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "WithPng");
    }

    #[test]
    fn load_from_storage_reads_mascot_package() {
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mascot_pack/Cerber.mascot");
        assert!(fixture.is_file(), "fixture missing: {}", fixture.display());
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().join("mascots");
        let cache = dir.path().join("mascot-cache");
        fs::create_dir_all(&storage).unwrap();
        fs::copy(&fixture, storage.join("Cerber.mascot")).unwrap();
        let loaded = load_from_storage(&storage, &cache);
        assert!(
            loaded.iter().any(|t| t.name == "Cerber"),
            "loaded names: {:?}",
            loaded.iter().map(|t| t.name.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn sync_picks_up_newly_copied_package() {
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mascot_pack/Cerber.mascot");
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().join("mascots");
        let cache = dir.path().join("mascot-cache");
        fs::create_dir_all(&storage).unwrap();
        let mut store = TemplateStore::new();
        let mut factory = Factory::new(None);
        let empty_snapshot = load_from_storage(&storage, &cache);
        let empty = apply_loaded_templates(&mut store, &mut factory, &empty_snapshot);
        assert!(!empty.iter().any(|n| n == "Cerber"));
        fs::copy(&fixture, storage.join("Cerber.mascot")).unwrap();
        let snapshot = load_from_storage(&storage, &cache);
        let names = apply_loaded_templates(&mut store, &mut factory, &snapshot);
        assert!(names.iter().any(|n| n == "Cerber"), "names={names:?}");
        assert!(factory.get_template("Cerber").is_some());
    }
}
