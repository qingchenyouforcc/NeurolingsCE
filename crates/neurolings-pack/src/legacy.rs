//! 旧版 Shimeji-ee 压缩包的分析与导入。
//!
//! 支持 ZIP、7z、RAR、TAR 与 TGZ；明确列入拒绝清单的格式返回
//! [`crate::error::PackError::Unsupported`]。

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use crate::error::{PackError, Result};
use crate::limits;
use crate::metadata::{MascotMetadata, metadata_from_json, metadata_to_json};
use crate::package::{
    ensure_legacy_metadata, is_valid_package_name, max_size_for_package_path,
    normalized_archive_path, sanitized_package_base_name, validate_image_file,
};
use crate::safepath::safe_child_path;
use crate::zipio;

/// 内置默认 actions.xml：给只通过 shime1.png..shime46.png 帧发现的
/// 桌宠使用。
pub const DEFAULT_ACTIONS_XML: &str = include_str!("../assets/default_actions.xml");
/// 内置默认 behaviors.xml。
pub const DEFAULT_BEHAVIORS_XML: &str = include_str!("../assets/default_behaviors.xml");

const BEHAVIOR_NAMES: &[&str] = &[
    "\u{884c}\u{52d5}.xml",
    "behaviors.xml",
    "behavior.xml",
    "two.xml",
    "2.xml",
];
const ACTION_NAMES: &[&str] = &[
    "\u{52d5}\u{4f5c}.xml",
    "actions.xml",
    "action.xml",
    "one.xml",
    "1.xml",
];
const NAME_BLACKLIST: &[&str] = &[
    "img",
    "conf",
    "shimeji",
    "unused",
    "shimeji-ee",
    "shimejiee",
    "src",
    "/",
    ".",
    "..",
    "",
];
const UNSUPPORTED_EXTENSIONS: &[&str] = &["gz", "bz2", "xz", "cab", "iso", "apk", "war", "ear"];
const PREVIEW_FILE_NAMES: &[&str] = &["a.png", "cover.png"];

/// 旧版压缩包中发现的一只候选桌宠。
#[derive(Debug, Clone, Default)]
pub struct LegacyMascotCandidate {
    pub name: String,
    pub metadata: MascotMetadata,
    pub convertible: bool,
    pub generated_metadata: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub source_name: String,
    pub info_json: Vec<u8>,
    pub info_json_error: String,
    pub info_json_valid: bool,
}

/// 旧版压缩包的分析结果。
#[derive(Debug, Clone, Default)]
pub struct LegacyArchiveAnalysis {
    pub ok: bool,
    pub error_message: String,
    pub candidates: Vec<LegacyMascotCandidate>,
}

/// 单个选中候选的转换结果。
#[derive(Debug, Clone, Default)]
pub struct LegacyMascotConversionResult {
    pub name: String,
    pub package_path: String,
    pub ok: bool,
    pub error_message: String,
}

// ---------------------------------------------------------------------------
// 字符串小工具
// ---------------------------------------------------------------------------

fn ascii_lower(data: &str) -> String {
    data.to_ascii_lowercase()
}

fn file_extension(lower_name: &str) -> String {
    match lower_name.rfind('.') {
        Some(pos) => lower_name[pos + 1..].to_string(),
        None => String::new(),
    }
}

fn last_component(path: &str) -> &str {
    match path.rfind('/') {
        Some(pos) => &path[pos + 1..],
        None => path,
    }
}

/// 把路径拍平成小写的单段文件名，`/` 替换为 `_`。
fn normalize_filename(name: &str) -> String {
    let trimmed = name.trim_start_matches('/');
    ascii_lower(&trimmed.replace('/', "_"))
}

fn strip_mascot_ext(name: &str) -> &str {
    if name.len() >= ".mascot".len() && &name[name.len() - ".mascot".len()..] == ".mascot" {
        &name[..name.len() - ".mascot".len()]
    } else {
        name
    }
}

fn normalized_legacy_candidate_name(name: &str) -> String {
    strip_mascot_ext(name).to_string()
}

/// conf 目录路径改写：`.../conf/Name/{actions,behaviors}.xml`
/// 重写为 `.../img/Name/...`。
fn apply_conf_hack(path: &str) -> String {
    let Some(slash3) = path.rfind('/') else {
        return path.to_string();
    };
    if slash3 == 0 {
        return path.to_string();
    }
    let Some(slash2) = path[..slash3].rfind('/') else {
        return path.to_string();
    };
    if slash2 == 0 {
        return path.to_string();
    }
    let slash1 = match path[..slash2].rfind('/') {
        Some(pos) => pos + 1,
        None => 0,
    };
    let file_name = &path[slash3 + 1..];
    if &path[slash1..slash2] == "conf"
        && slash3 - slash2 > 1
        && (file_name == "actions.xml" || file_name == "behaviors.xml")
    {
        return format!("{}img{}", &path[..slash1], &path[slash2..]);
    }
    path.to_string()
}

// ---------------------------------------------------------------------------
// 压缩包目录树与分析器
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetKind {
    Image,
    Sound,
    Xml,
}

#[derive(Debug, Clone)]
struct ExtractTarget {
    shimeji_name: String,
    extract_name: String,
    kind: TargetKind,
}

#[derive(Debug)]
struct EntryNode {
    path: String,
    data: Vec<u8>,
    extension: String,
    targets: Vec<ExtractTarget>,
}

#[derive(Debug)]
struct FolderNode {
    name: String,
    parent: Option<usize>,
    folders: BTreeMap<String, usize>,
    files: BTreeMap<String, usize>,
}

/// 旧版压缩包的内存表示，可直接用于解压。
pub struct LegacyArchive {
    fallback_name: String,
    entries: Vec<EntryNode>,
    folders: Vec<FolderNode>,
    shimejis: BTreeSet<String>,
    default_xml_targets: Vec<String>,
}

impl LegacyArchive {
    /// 打开并分析旧版压缩包。
    ///
    /// 支持 ZIP、7z、RAR、TAR 与 TGZ；明确列入拒绝清单的格式返回
    /// [`PackError::Unsupported`]。
    pub fn open(archive_path: &Path, fallback_name: &str) -> Result<Self> {
        if let Some(extension) = archive_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
        {
            if UNSUPPORTED_EXTENSIONS.contains(&extension.as_str()) {
                return Err(PackError::Unsupported(format!(".{extension}")));
            }
            // rar/7z/tar/tgz：解压到受控临时目录后按目录分析。
            if matches!(extension.as_str(), "rar" | "7z" | "tar" | "tgz") {
                let tempdir = tempfile::tempdir()?;
                extract_to_tempdir(archive_path, tempdir.path(), &extension)?;
                return Self::open_from_directory(tempdir.path(), fallback_name);
            }
        }

        let mut zip = zipio::open_zip(archive_path)?;
        let raw_entries = zipio::read_raw_archive_entries(&mut zip)?;

        let mut archive = LegacyArchive {
            fallback_name: fallback_name.to_string(),
            entries: Vec::new(),
            folders: vec![FolderNode {
                name: "/".to_string(),
                parent: None,
                folders: BTreeMap::new(),
                files: BTreeMap::new(),
            }],
            shimejis: BTreeSet::new(),
            default_xml_targets: Vec::new(),
        };

        let mut total_bytes: u64 = 0;
        for (index, raw) in raw_entries.iter().enumerate() {
            if raw.is_directory {
                continue;
            }
            let path = apply_conf_hack(&raw.name);
            let lower_name = ascii_lower(last_component(&path));
            let extension = file_extension(&lower_name);
            if !zipio::ALLOWED_LEGACY_EXTENSIONS.contains(&extension.as_str()) {
                continue;
            }
            if raw.uncompressed_size > limits::MASCOT_SINGLE_FILE_MAX_BYTES {
                return Err(PackError::msg("Mascot package entry is too large"));
            }
            if raw.uncompressed_size > limits::MASCOT_EXTRACTED_MAX_BYTES - total_bytes {
                return Err(PackError::msg("Mascot package extracted data is too large"));
            }
            total_bytes += raw.uncompressed_size;
            let data = zipio::read_zip_entry_bytes(&mut zip, index, raw.uncompressed_size)?;
            let node_index = archive.entries.len();
            archive.entries.push(EntryNode {
                path: path.clone(),
                data,
                extension,
                targets: Vec::new(),
            });
            archive.insert_into_tree(&path, node_index);
        }

        archive.analyze();
        Ok(archive)
    }

    /// 从解压目录构建分析树（rar/7z/tar/tgz 的入口；规则与 zip 一致）。
    pub fn open_from_directory(dir: &Path, fallback_name: &str) -> Result<Self> {
        let mut archive = LegacyArchive {
            fallback_name: fallback_name.to_string(),
            entries: Vec::new(),
            folders: vec![FolderNode {
                name: "/".to_string(),
                parent: None,
                folders: BTreeMap::new(),
                files: BTreeMap::new(),
            }],
            shimejis: BTreeSet::new(),
            default_xml_targets: Vec::new(),
        };

        let mut total_bytes: u64 = 0;
        for entry in walkdir::WalkDir::new(dir).follow_links(false) {
            let entry = entry.map_err(|e| PackError::msg(format!("walk error: {e}")))?;
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = entry
                .path()
                .strip_prefix(dir)
                .map_err(|_| PackError::msg("invalid archive layout"))?;
            let rel_text = rel.to_string_lossy().replace('\\', "/");
            let path = apply_conf_hack(&rel_text);
            let lower_name = ascii_lower(last_component(&path));
            let extension = file_extension(&lower_name);
            if !zipio::ALLOWED_LEGACY_EXTENSIONS.contains(&extension.as_str()) {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if size > limits::MASCOT_SINGLE_FILE_MAX_BYTES {
                return Err(PackError::msg("Mascot package entry is too large"));
            }
            if size > limits::MASCOT_EXTRACTED_MAX_BYTES - total_bytes {
                return Err(PackError::msg("Mascot package extracted data is too large"));
            }
            total_bytes += size;
            let data = std::fs::read(entry.path())?;
            let node_index = archive.entries.len();
            archive.entries.push(EntryNode {
                path: path.clone(),
                data,
                extension,
                targets: Vec::new(),
            });
            archive.insert_into_tree(&path, node_index);
        }

        archive.analyze();
        Ok(archive)
    }

    /// 全部已发现桌宠的名称（已排序）。
    pub fn shimejis(&self) -> &BTreeSet<String> {
        &self.shimejis
    }

    fn insert_into_tree(&mut self, path: &str, node_index: usize) {
        let mut relative = path;
        if let Some(stripped) = relative.strip_prefix('/') {
            relative = stripped;
        }
        if relative.is_empty() {
            return;
        }
        let components: Vec<&str> = relative.split('/').collect();
        let mut folder = 0usize;
        for (position, component) in components.iter().enumerate() {
            if position + 1 < components.len() {
                let key = ascii_lower(component);
                let next = match self.folders[folder].folders.get(&key).copied() {
                    Some(existing) => existing,
                    None => {
                        let index = self.folders.len();
                        self.folders.push(FolderNode {
                            name: (*component).to_string(),
                            parent: Some(folder),
                            folders: BTreeMap::new(),
                            files: BTreeMap::new(),
                        });
                        self.folders[folder].folders.insert(key, index);
                        index
                    }
                };
                folder = next;
            } else if !component.is_empty() {
                self.folders[folder]
                    .files
                    .insert(ascii_lower(component), node_index);
            }
        }
    }

    fn parent_of(&self, folder: usize) -> usize {
        self.folders[folder].parent.unwrap_or(folder)
    }

    fn folder_named(&self, folder: usize, name: &str) -> Option<usize> {
        self.folders[folder].folders.get(name).copied()
    }

    fn entry_named(&self, folder: usize, name: &str) -> Option<usize> {
        self.folders[folder].files.get(name).copied()
    }

    fn find_file(&self, folder: usize, names: &[&str]) -> Option<usize> {
        for name in names {
            if let Some(entry) = self.entry_named(folder, name) {
                return Some(entry);
            }
        }
        None
    }

    /// 解析目录下的相对路径：处理 `..`，跳过 `.` 与空组件。
    fn relative_file(&self, start_folder: usize, path: &str) -> Option<usize> {
        let mut cwd = start_folder;
        let mut start = 0usize;
        loop {
            let end = path[start..].find('/').map(|offset| start + offset);
            match end {
                Some(end) => {
                    if start != end {
                        let substr = &path[start..end];
                        if substr == "." {
                            // 保持当前目录不变
                        } else if substr == ".." {
                            cwd = self.parent_of(cwd);
                        } else {
                            cwd = self.folders[cwd].folders.get(substr).copied()?;
                        }
                    }
                    start = end + 1;
                }
                None => {
                    let substr = &path[start..];
                    if substr.is_empty() || substr == "." || substr == ".." {
                        return None;
                    }
                    return self.folders[cwd].files.get(substr).copied();
                }
            }
        }
    }

    /// 为目录推导桌宠名：逐级向上，直到名称不在黑名单内。
    fn shimeji_name(&self, base: usize) -> String {
        let mut cwd = base;
        let mut lower = strip_mascot_ext(&ascii_lower(&self.folders[cwd].name)).to_string();
        while NAME_BLACKLIST.contains(&lower.as_str()) {
            let parent = self.parent_of(cwd);
            if parent == cwd {
                return self.fallback_name.clone();
            }
            cwd = parent;
            lower = strip_mascot_ext(&ascii_lower(&self.folders[cwd].name)).to_string();
        }
        self.folders[cwd].name[..lower.len()].to_string()
    }

    fn add_search_paths(&self, base: usize) -> Vec<usize> {
        let mut paths = Vec::with_capacity(12);
        let mut level = base;
        for _ in 0..4 {
            paths.push(level);
            if let Some(img) = self.folder_named(level, "img") {
                paths.push(img);
            }
            if let Some(sound) = self.folder_named(level, "sound") {
                paths.push(sound);
            }
            level = self.parent_of(level);
        }
        paths
    }

    fn register_shimeji(
        &mut self,
        base: usize,
        actions: usize,
        behaviors: usize,
        paths: &BTreeSet<String>,
        alternative_base: Option<usize>,
    ) -> bool {
        let name = self.shimeji_name(base);
        let mut search_paths = self.add_search_paths(base);
        if let Some(alternative) = alternative_base {
            search_paths.extend(self.add_search_paths(alternative));
        }
        // 去重并保持顺序。
        let mut seen = HashSet::new();
        search_paths.retain(|folder| seen.insert(*folder));

        let path_list: Vec<String> = paths.iter().cloned().collect();
        let mut targets: Vec<(Option<usize>, String)> =
            path_list.iter().map(|path| (None, path.clone())).collect();
        let mut has_images = false;
        for search in &search_paths {
            for (index, path) in path_list.iter().enumerate() {
                if targets[index].0.is_some() {
                    continue;
                }
                let found = self
                    .relative_file(*search, path)
                    .or_else(|| self.relative_file(*search, &normalize_filename(path)));
                if let Some(entry_index) = found {
                    targets[index].0 = Some(entry_index);
                    targets[index].1 = normalize_filename(path);
                    if self.entries[entry_index].extension == "png" {
                        has_images = true;
                    }
                }
            }
        }
        if !has_images {
            return false;
        }
        for (entry_index, normalized) in targets {
            let Some(entry_index) = entry_index else {
                continue;
            };
            let kind = if self.entries[entry_index].extension == "png" {
                TargetKind::Image
            } else {
                TargetKind::Sound
            };
            self.entries[entry_index].targets.push(ExtractTarget {
                shimeji_name: name.clone(),
                extract_name: normalized,
                kind,
            });
        }
        self.entries[actions].targets.push(ExtractTarget {
            shimeji_name: name.clone(),
            extract_name: "actions.xml".to_string(),
            kind: TargetKind::Xml,
        });
        self.entries[behaviors].targets.push(ExtractTarget {
            shimeji_name: name.clone(),
            extract_name: "behaviors.xml".to_string(),
            kind: TargetKind::Xml,
        });
        self.shimejis.insert(name);
        true
    }

    fn discover_shimejiee(
        &mut self,
        img: usize,
        actions: usize,
        behaviors: usize,
        paths: &BTreeSet<String>,
    ) -> usize {
        let subfolders: Vec<usize> = self.folders[img].folders.values().copied().collect();
        let mut associated = 0;
        for folder in subfolders {
            if ascii_lower(&self.folders[folder].name) == "unused" {
                continue;
            }
            if self.find_file(folder, ACTION_NAMES).is_some()
                || self.find_file(folder, BEHAVIOR_NAMES).is_some()
            {
                continue;
            }
            if self.register_shimeji(folder, actions, behaviors, paths, Some(img)) {
                associated += 1;
            }
        }
        associated
    }

    /// 运行桌宠发现算法。
    fn analyze(&mut self) {
        // 广度优先扫描目录：寻找 actions/behaviors 配对与 shime1.png 根。
        let mut all_folders: Vec<usize> = Vec::new();
        let mut queue: std::collections::VecDeque<usize> =
            std::collections::VecDeque::from([0usize]);
        while let Some(folder) = queue.pop_front() {
            all_folders.push(folder);
            for child in self.folders[folder].folders.values() {
                queue.push_back(*child);
            }
        }

        let mut shime1_roots: Vec<usize> = Vec::new();
        let mut unparsed: Vec<(usize, usize, usize)> = Vec::new();
        for folder in &all_folders {
            if self.entry_named(*folder, "shime1.png").is_some() {
                shime1_roots.push(*folder);
            }
            let Some(behaviors) = self.find_file(*folder, BEHAVIOR_NAMES) else {
                continue;
            };
            let Some(actions) = self.find_file(*folder, ACTION_NAMES) else {
                continue;
            };
            unparsed.push((actions, behaviors, *folder));
        }

        // 预先读取每个候选的 actions XML。
        let actions_xmls: Vec<Vec<u8>> = unparsed
            .iter()
            .map(|(actions, _, _)| self.entries[*actions].data.clone())
            .collect();

        for (index, (actions, behaviors, root)) in unparsed.iter().enumerate() {
            let (actions, behaviors, root) = (*actions, *behaviors, *root);
            let paths = find_paths(&actions_xmls[index]);
            if paths.is_empty() {
                continue;
            }

            let mut associated = 0;
            if ascii_lower(&self.folders[root].name) == "conf" {
                let parent = self.parent_of(root);
                if let Some(img) = self.folder_named(parent, "img") {
                    associated = self.discover_shimejiee(img, actions, behaviors, &paths);
                }
            }
            if associated == 0 {
                self.register_shimeji(root, actions, behaviors, &paths, None);
            }
        }

        // 对没有配置文件的桌宠，按 shime1.png 进行发现。
        for root in shime1_roots {
            if self.entry_named(root, "shime47.png").is_some() {
                continue;
            }
            let mut shimes: Vec<usize> = Vec::new();
            let mut complete = true;
            for i in 0..46usize {
                let Some(entry) = self.entry_named(root, &format!("shime{}.png", i + 1)) else {
                    complete = false;
                    break;
                };
                if !self.entries[entry].targets.is_empty() {
                    complete = false;
                    break;
                }
                shimes.push(entry);
            }
            if !complete {
                continue;
            }
            let name = self.shimeji_name(root);
            for (i, entry) in shimes.iter().enumerate() {
                self.entries[*entry].targets.push(ExtractTarget {
                    shimeji_name: name.clone(),
                    extract_name: format!("shime{}.png", i + 1),
                    kind: TargetKind::Image,
                });
            }
            self.default_xml_targets.push(name.clone());
            self.shimejis.insert(name);
        }
    }

    /// 把全部发现的桌宠解压到 output：目标条目重排为 `<name>.mascot/`
    /// 目录结构，强制执行限额与路径安全规则，并对结果做校验。
    pub fn extract_safely(&self, output: &Path) -> Result<()> {
        if output.exists() {
            fs::remove_dir_all(output)?;
        }
        fs::create_dir_all(output)?;

        let mut total_bytes: u64 = 0;
        for entry in &self.entries {
            if entry.targets.is_empty() {
                continue;
            }
            let chunk = entry.data.len() as u64;
            if chunk > limits::MASCOT_SINGLE_FILE_MAX_BYTES {
                return Err(PackError::msg("Mascot package entry is too large"));
            }
            if total_bytes > limits::MASCOT_EXTRACTED_MAX_BYTES
                || chunk > limits::MASCOT_EXTRACTED_MAX_BYTES - total_bytes
            {
                return Err(PackError::msg("Mascot package extracted data is too large"));
            }
            total_bytes += chunk;
            for target in &entry.targets {
                let subdirectory = match target.kind {
                    TargetKind::Image => "img/",
                    TargetKind::Sound => "sound/",
                    TargetKind::Xml => "",
                };
                let relative = format!(
                    "{}.mascot/{}{}",
                    target.shimeji_name, subdirectory, target.extract_name
                );
                let Some(path) = safe_child_path(output, &relative) else {
                    return Err(PackError::msg("Unsafe package extraction path"));
                };
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&path, &entry.data)?;
            }
        }

        // 为按 shime1.png 发现的桌宠写入内置默认 XML。
        for name in &self.default_xml_targets {
            for (file_name, data) in [
                ("actions.xml", DEFAULT_ACTIONS_XML.as_bytes()),
                ("behaviors.xml", DEFAULT_BEHAVIORS_XML.as_bytes()),
            ] {
                let chunk = data.len() as u64;
                if total_bytes > limits::MASCOT_EXTRACTED_MAX_BYTES
                    || chunk > limits::MASCOT_EXTRACTED_MAX_BYTES - total_bytes
                {
                    return Err(PackError::msg("Mascot package extracted data is too large"));
                }
                total_bytes += chunk;
                let relative = format!("{name}.mascot/{file_name}");
                let Some(path) = safe_child_path(output, &relative) else {
                    return Err(PackError::msg("Unsafe package extraction path"));
                };
                fs::write(&path, data)?;
            }
        }

        validate_extracted_directory(output)
    }
}

/// 扫描 actions XML 中 Pose 元素引用的图像/音效路径（返回小写）。
fn find_paths(actions_xml: &[u8]) -> BTreeSet<String> {
    let owned;
    let text: &str = match std::str::from_utf8(actions_xml) {
        Ok(text) => text,
        Err(_) => {
            owned = String::from_utf8_lossy(actions_xml).into_owned();
            &owned
        }
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Ok(document) = roxmltree::Document::parse(text) else {
        return BTreeSet::new();
    };
    let root = document.root_element();
    if !matches!(
        root.tag_name().name(),
        "Mascot" | "\u{30de}\u{30b9}\u{30b3}\u{30c3}\u{30c8}"
    ) {
        return BTreeSet::new();
    }

    let mut paths = BTreeSet::new();
    let mut queue = vec![root];
    while let Some(node) = queue.pop() {
        if matches!(node.tag_name().name(), "Pose" | "\u{30dd}\u{30fc}\u{30ba}") {
            for attribute in ["\u{753b}\u{50cf}", "Image", "ImageRight", "Sound"] {
                if let Some(value) = node.attribute(attribute) {
                    paths.insert(ascii_lower(value));
                }
            }
        } else {
            for child in node.children() {
                if child.is_element() {
                    queue.push(child);
                }
            }
        }
    }
    paths
}

// ---------------------------------------------------------------------------
// 解压目录校验
// ---------------------------------------------------------------------------

/// 校验压缩包解压出的目录树：强制执行包含关系、符号链接、数量、
/// 尺寸与 PNG 规则。
pub fn validate_extracted_directory(root_path: &Path) -> Result<()> {
    if !root_path.is_dir() {
        return Err(PackError::msg("Extracted archive directory is missing"));
    }
    let root_absolute = crate::safepath::clean_path(&crate::safepath::absolute_path(root_path));

    let mut total_bytes: u64 = 0;
    let mut file_count: usize = 0;
    for dir_entry in walkdir::WalkDir::new(&root_absolute) {
        let dir_entry = dir_entry?;
        let absolute =
            crate::safepath::clean_path(&crate::safepath::absolute_path(dir_entry.path()));
        let Ok(stripped) = absolute.strip_prefix(&root_absolute) else {
            return Err(PackError::msg("Archive extracted an unsafe path"));
        };
        let relative = stripped.to_string_lossy().replace('\\', "/");
        if relative.is_empty() {
            continue; // 跳过根目录自身
        }
        let safe = safe_child_path(&root_absolute, &relative);
        if safe.as_ref() != Some(&absolute) {
            return Err(PackError::msg("Archive extracted an unsafe path"));
        }
        if dir_entry.file_type().is_symlink() {
            return Err(PackError::msg("Archive contains symbolic links"));
        }
        if !dir_entry.file_type().is_file() {
            continue;
        }

        file_count += 1;
        if file_count > limits::MASCOT_ZIP_ENTRY_MAX_COUNT {
            return Err(PackError::msg("Archive contains too many extracted files"));
        }

        let size = dir_entry.metadata()?.len();
        if size > max_size_for_package_path(&relative) {
            return Err(PackError::msg(format!(
                "Extracted file {relative} exceeds size limits"
            )));
        }
        total_bytes += size;
        if total_bytes > limits::MASCOT_EXTRACTED_MAX_BYTES {
            return Err(PackError::msg("Archive extracted data is too large"));
        }

        let lower = relative.to_ascii_lowercase();
        if lower.contains("/img/") && lower.ends_with(".png") {
            validate_image_file(dir_entry.path())?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 旧版导入辅助函数
// ---------------------------------------------------------------------------

/// 定位某个候选桌宠解压后的目录。
fn extracted_legacy_mascot_path(extraction_root: &Path, name: &str) -> PathBuf {
    for relative in [format!("{name}.mascot"), name.to_string()] {
        if let Some(path) = safe_child_path(extraction_root, &relative)
            && path.is_dir()
        {
            return path;
        }
    }
    safe_child_path(extraction_root, &format!("{name}.mascot")).unwrap_or_default()
}

/// 原压缩包中的 info.json 若未包含在解压目标里，则补拷贝到解压出的
/// 桌宠目录旁。
fn try_extract_info_json(archive: &LegacyArchive, mascot_name: &str, target_path: &Path) {
    let target_file = target_path.join("info.json");
    if target_file.exists() {
        return;
    }
    let mascot = ascii_lower(mascot_name);
    let mascot_directory = format!("{mascot}.mascot");
    let mut root_match: Option<usize> = None;
    let mut mascot_match: Option<usize> = None;
    for (index, entry) in archive.entries.iter().enumerate() {
        let Some(path) = normalized_archive_path(&entry.path) else {
            continue;
        };
        let lower_path = ascii_lower(&path);
        let parts: Vec<&str> = lower_path.split('/').filter(|p| !p.is_empty()).collect();
        if parts.is_empty() || *parts.last().unwrap() != "info.json" {
            continue;
        }
        if parts.len() == 1 {
            root_match = Some(index);
        } else if parts[parts.len() - 2] == mascot || parts[parts.len() - 2] == mascot_directory {
            mascot_match = Some(index);
            break;
        }
    }
    let Some(index) = mascot_match.or(root_match) else {
        return;
    };
    let _ = fs::write(&target_file, &archive.entries[index].data);
}

/// 从原压缩包补拷贝匹配的 bubble_context.txt。
fn try_extract_bubble_context(archive: &LegacyArchive, mascot_name: &str, target_path: &Path) {
    let target_file = target_path.join("bubble_context.txt");
    if target_file.exists() {
        return;
    }
    let mascot = ascii_lower(mascot_name);
    for entry in &archive.entries {
        let Some(path) = normalized_archive_path(&entry.path) else {
            continue;
        };
        let lower = ascii_lower(&path);
        if !lower.ends_with("bubble_context.txt") {
            continue;
        }
        if lower == "bubble_context.txt"
            || lower.contains(&format!("/{mascot}/"))
            || lower.starts_with(&format!("{mascot}/"))
            || lower.starts_with(&format!("{mascot}.mascot/"))
        {
            let _ = fs::write(&target_file, &entry.data);
            return;
        }
    }
}

/// 判断预览图路径是否属于指定桌宠。
fn legacy_preview_path_matches_mascot(path: &str, mascot_name: &str) -> bool {
    let lower_path = ascii_lower(path);
    let parts: Vec<&str> = lower_path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() < 2 {
        return false;
    }
    let Some(img_index) = parts.iter().position(|part| *part == "img") else {
        return false;
    };
    if img_index >= parts.len() - 1 {
        return false;
    }
    let mascot = ascii_lower(mascot_name);
    let mascot_directory = format!("{mascot}.mascot");
    if img_index == 0 && parts.len() == 2 {
        return true;
    }
    if img_index > 0 && (parts[img_index - 1] == mascot || parts[img_index - 1] == mascot_directory)
    {
        return true;
    }
    img_index + 2 < parts.len()
        && (parts[img_index + 1] == mascot || parts[img_index + 1] == mascot_directory)
}

/// 从原压缩包补拷贝 a.png / cover.png 预览图。
fn try_extract_preview_images(archive: &LegacyArchive, mascot_name: &str, target_path: &Path) {
    let img_dir = target_path.join("img");
    for file_name in PREVIEW_FILE_NAMES {
        if img_dir.join(file_name).exists() {
            continue;
        }
        let source = archive.entries.iter().find(|entry| {
            last_component(&entry.path).eq_ignore_ascii_case(file_name)
                && legacy_preview_path_matches_mascot(&entry.path, mascot_name)
        });
        let Some(source) = source else {
            continue;
        };
        if fs::create_dir_all(&img_dir).is_err() {
            continue;
        }
        let _ = fs::write(img_dir.join(file_name), &source.data);
    }
}

/// 检查一个解压后的旧版桌宠目录。
pub fn inspect_legacy_directory(source_path: &Path, fallback_name: &str) -> LegacyMascotCandidate {
    let mut candidate = LegacyMascotCandidate {
        source_name: fallback_name.to_string(),
        name: fallback_name.to_string(),
        metadata: MascotMetadata {
            name: fallback_name.to_string(),
            ..Default::default()
        },
        ..Default::default()
    };

    if !source_path.join("actions.xml").is_file() {
        candidate.errors.push("Missing actions.xml".to_string());
    }
    if !source_path.join("behaviors.xml").is_file() {
        candidate.errors.push("Missing behaviors.xml".to_string());
    }
    let has_images = fs::read_dir(source_path.join("img"))
        .map(|read| {
            read.flatten()
                .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
                .any(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .ends_with(".png")
                })
        })
        .unwrap_or(false);
    if !has_images {
        candidate.errors.push("Missing img/*.png".to_string());
    }

    let info_path = source_path.join("info.json");
    let loaded_info_json = info_path.is_file()
        && match fs::read(&info_path) {
            Ok(bytes) => {
                candidate.info_json = bytes.clone();
                match metadata_from_json(&bytes) {
                    Ok(metadata) => {
                        candidate.metadata = metadata.clone();
                        candidate.name = metadata.name.clone();
                        candidate.info_json_valid = is_valid_package_name(&metadata.name);
                        if !candidate.info_json_valid {
                            candidate.info_json_error =
                                "Edited info.json has an invalid package name".to_string();
                        }
                        true
                    }
                    Err(error) => {
                        candidate.generated_metadata = true;
                        candidate.info_json_error = error.to_string();
                        candidate.warnings.push(format!(
                            "info.json is invalid; fallback metadata will be generated ({error})"
                        ));
                        true
                    }
                }
            }
            Err(_) => false,
        };
    if !loaded_info_json {
        candidate.generated_metadata = true;
        candidate
            .warnings
            .push("Missing info.json; fallback metadata will be generated".to_string());
    }

    if candidate.metadata.name.trim().is_empty() {
        candidate.metadata.name = fallback_name.to_string();
        candidate.name = fallback_name.to_string();
    }
    if candidate.generated_metadata && !loaded_info_json {
        candidate.info_json = metadata_to_json(&candidate.metadata).into_bytes();
        candidate.info_json_valid = is_valid_package_name(&candidate.metadata.name);
        if !candidate.info_json_valid {
            candidate.info_json_error = "Edited info.json has an invalid package name".to_string();
        }
    }
    candidate.convertible = candidate.errors.is_empty();
    candidate
}

/// 分析旧版 Shimeji 压缩包并报告可转换的候选。
pub fn analyze_legacy_archive(archive_path: &Path) -> LegacyArchiveAnalysis {
    let mut analysis = LegacyArchiveAnalysis::default();

    let Ok(metadata) = fs::metadata(archive_path) else {
        analysis.error_message = "Archive does not exist".to_string();
        return analysis;
    };
    if !metadata.is_file() {
        analysis.error_message = "Archive does not exist".to_string();
        return analysis;
    }
    if metadata.len() > limits::MASCOT_PACKAGE_MAX_BYTES {
        analysis.error_message = format!(
            "Archive exceeds the maximum size of {} bytes",
            limits::MASCOT_PACKAGE_MAX_BYTES
        );
        return analysis;
    }

    let fallback_name = archive_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_default();
    let archive = match LegacyArchive::open(archive_path, &fallback_name) {
        Ok(archive) => archive,
        Err(error) => {
            analysis.error_message = match error {
                PackError::Unsupported(_) => error.to_string(),
                _ => "Could not analyze archive".to_string(),
            };
            return analysis;
        }
    };

    let raw_candidates = raw_legacy_candidates(&archive, &fallback_name);

    let temp_dir = match tempfile::tempdir() {
        Ok(temp_dir) => temp_dir,
        Err(_) => {
            analysis.error_message = "Could not create temporary directory".to_string();
            return analysis;
        }
    };
    if let Err(error) = archive.extract_safely(temp_dir.path()) {
        analysis.error_message = error.to_string();
        return analysis;
    }

    let mut seen_names: HashSet<String> = HashSet::new();
    for name in archive.shimejis() {
        seen_names.insert(name.clone());
        let source_path = extracted_legacy_mascot_path(temp_dir.path(), name);
        try_extract_info_json(&archive, name, &source_path);
        try_extract_bubble_context(&archive, name, &source_path);
        analysis
            .candidates
            .push(inspect_legacy_directory(&source_path, name));
    }
    for (name, flags) in raw_candidates {
        if seen_names.contains(&name) {
            continue;
        }
        let mut candidate = LegacyMascotCandidate {
            source_name: name.clone(),
            name: name.clone(),
            metadata: MascotMetadata {
                name: name.clone(),
                ..Default::default()
            },
            ..Default::default()
        };
        if !flags.has_actions {
            candidate.errors.push("Missing actions.xml".to_string());
        }
        if !flags.has_behaviors {
            candidate.errors.push("Missing behaviors.xml".to_string());
        }
        if !flags.has_image {
            candidate.errors.push("Missing img/*.png".to_string());
        }
        if candidate.errors.is_empty() {
            candidate
                .errors
                .push("Could not recognize this mascot in the archive".to_string());
        }
        analysis.candidates.push(candidate);
    }
    if analysis.candidates.is_empty() {
        analysis.error_message = "No Shimeji mascots were found in the archive".to_string();
        return analysis;
    }

    analysis.ok = analysis.candidates.iter().any(|c| c.convertible);
    if !analysis.ok {
        analysis.error_message = "No convertible mascots were found".to_string();
    }
    analysis
}

#[derive(Debug, Default)]
struct RawLegacyCandidateFlags {
    has_actions: bool,
    has_behaviors: bool,
    has_image: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LegacyPathKind {
    Unknown,
    Actions,
    Behaviors,
    Image,
}

fn classify_legacy_path(lower_path: &str) -> LegacyPathKind {
    const ACTION_NAMES: &[&str] = &[
        "actions.xml",
        "action.xml",
        "one.xml",
        "\u{52d5}\u{4f5c}.xml",
    ];
    const BEHAVIOR_NAMES: &[&str] = &[
        "behaviors.xml",
        "behavior.xml",
        "two.xml",
        "\u{884c}\u{52d5}.xml",
    ];
    let file_name = last_component(lower_path);
    if ACTION_NAMES.contains(&file_name) {
        return LegacyPathKind::Actions;
    }
    if BEHAVIOR_NAMES.contains(&file_name) {
        return LegacyPathKind::Behaviors;
    }
    if lower_path.ends_with(".png")
        && (lower_path.starts_with("img/") || lower_path.contains("/img/"))
    {
        return LegacyPathKind::Image;
    }
    LegacyPathKind::Unknown
}

fn legacy_root_for_path(path: &str, archive_base_name: &str) -> String {
    let lower = ascii_lower(path);
    if lower.starts_with("img/") {
        return archive_base_name.to_string();
    }
    if !lower.contains('/') {
        return archive_base_name.to_string();
    }
    if let Some(img_index) = lower.find("/img/") {
        return normalized_legacy_candidate_name(&path[..img_index]);
    }
    let last_slash = path.rfind('/').unwrap_or(path.len());
    normalized_legacy_candidate_name(&path[..last_slash])
}

/// 按原始路径扫描发现的类桌宠目录的标记。
fn raw_legacy_candidates(
    archive: &LegacyArchive,
    archive_base_name: &str,
) -> BTreeMap<String, RawLegacyCandidateFlags> {
    let mut candidates: BTreeMap<String, RawLegacyCandidateFlags> = BTreeMap::new();
    for entry in &archive.entries {
        let Some(path) = normalized_archive_path(&entry.path) else {
            continue;
        };
        let kind = classify_legacy_path(&ascii_lower(&path));
        if kind == LegacyPathKind::Unknown {
            continue;
        }
        let root = legacy_root_for_path(&path, archive_base_name);
        let flags = candidates.entry(root).or_default();
        match kind {
            LegacyPathKind::Actions => flags.has_actions = true,
            LegacyPathKind::Behaviors => flags.has_behaviors = true,
            LegacyPathKind::Image => flags.has_image = true,
            LegacyPathKind::Unknown => {}
        }
    }
    candidates
}

fn unique_package_path(output_path: &Path, name: &str, reserved: &mut HashSet<PathBuf>) -> PathBuf {
    let base = sanitized_package_base_name(name);
    let mut candidate = output_path.join(format!("{base}.mascot"));
    let mut suffix = 2;
    while candidate.exists() || reserved.contains(&candidate) {
        candidate = output_path.join(format!("{base}-{suffix}.mascot"));
        suffix += 1;
    }
    reserved.insert(candidate.clone());
    candidate
}

fn write_fallback_metadata(source_path: &Path, fallback_name: &str) {
    let metadata = MascotMetadata {
        name: fallback_name.to_string(),
        ..Default::default()
    };
    let _ = fs::write(source_path.join("info.json"), metadata_to_json(&metadata));
}

/// 把旧版压缩包中选中的候选转换为 .mascot 包。
pub fn write_legacy_archive_selection_as_packages(
    archive_path: &Path,
    output_path: &Path,
    selected_names: &[String],
    info_json_overrides: &BTreeMap<String, Vec<u8>>,
) -> Vec<LegacyMascotConversionResult> {
    let mut results: Vec<LegacyMascotConversionResult> = Vec::new();
    let fail_all = |results: &mut Vec<_>, message: String| {
        results.push(LegacyMascotConversionResult {
            error_message: message,
            ..Default::default()
        });
    };

    let Ok(archive_info) = fs::metadata(archive_path) else {
        fail_all(&mut results, "Archive does not exist".to_string());
        return results;
    };
    if !archive_info.is_file() {
        fail_all(&mut results, "Archive does not exist".to_string());
        return results;
    }
    if archive_info.len() > limits::MASCOT_PACKAGE_MAX_BYTES {
        fail_all(
            &mut results,
            format!(
                "Archive exceeds the maximum size of {} bytes",
                limits::MASCOT_PACKAGE_MAX_BYTES
            ),
        );
        return results;
    }
    if fs::create_dir_all(output_path).is_err() {
        fail_all(
            &mut results,
            "Could not create output directory".to_string(),
        );
        return results;
    }

    let selected: HashSet<&str> = selected_names.iter().map(String::as_str).collect();
    let mut processed: HashSet<String> = HashSet::new();

    let fallback_name = archive_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_default();
    let temp_dir = match tempfile::tempdir() {
        Ok(temp_dir) => temp_dir,
        Err(_) => {
            fail_all(
                &mut results,
                "Could not create temporary directory".to_string(),
            );
            return results;
        }
    };
    let archive = match LegacyArchive::open(archive_path, &fallback_name) {
        Ok(archive) => archive,
        Err(error) => {
            fail_all(
                &mut results,
                match error {
                    PackError::Unsupported(_) => error.to_string(),
                    _ => "Could not analyze archive".to_string(),
                },
            );
            return results;
        }
    };
    if let Err(error) = archive.extract_safely(temp_dir.path()) {
        fail_all(&mut results, error.to_string());
        return results;
    }

    let mut reserved_output_paths: HashSet<PathBuf> = HashSet::new();
    for name in archive.shimejis() {
        if !selected.contains(name.as_str()) {
            continue;
        }
        processed.insert(name.clone());

        let mut result = LegacyMascotConversionResult {
            name: name.clone(),
            ..Default::default()
        };
        let source_path = extracted_legacy_mascot_path(temp_dir.path(), name);
        try_extract_info_json(&archive, name, &source_path);
        try_extract_bubble_context(&archive, name, &source_path);
        try_extract_preview_images(&archive, name, &source_path);

        let mut candidate = inspect_legacy_directory(&source_path, name);
        if !candidate.convertible {
            result.error_message = candidate.errors.join("; ");
            results.push(result);
            continue;
        }

        if let Some(override_json) = info_json_overrides.get(name) {
            if override_json.len() as u64 > limits::MASCOT_SINGLE_FILE_MAX_BYTES {
                result.error_message = "Edited info.json is too large".to_string();
                results.push(result);
                continue;
            }
            let metadata = match metadata_from_json(override_json) {
                Ok(metadata) => metadata,
                Err(_) => {
                    result.error_message = "Edited info.json is invalid".to_string();
                    results.push(result);
                    continue;
                }
            };
            if !is_valid_package_name(&metadata.name) {
                result.error_message = "Edited info.json has an invalid package name".to_string();
                results.push(result);
                continue;
            }
            candidate.metadata = metadata;
            if fs::write(source_path.join("info.json"), override_json).is_err() {
                result.error_message = "Could not write edited info.json".to_string();
                results.push(result);
                continue;
            }
        } else {
            ensure_legacy_metadata(&source_path, name);
            if candidate.generated_metadata {
                write_fallback_metadata(&source_path, name);
            }
        }
        result.name = candidate.metadata.name.clone();
        let target_path = unique_package_path(
            output_path,
            &candidate.metadata.name,
            &mut reserved_output_paths,
        );
        match crate::package::write_package_from_directory(&source_path, &target_path) {
            Ok(()) => {
                result.ok = true;
                result.package_path = target_path.to_string_lossy().to_string();
            }
            Err(error) => result.error_message = error.to_string(),
        }
        results.push(result);
    }

    for selected_name in &selected {
        if !processed.contains(*selected_name) {
            results.push(LegacyMascotConversionResult {
                name: (*selected_name).to_string(),
                error_message: "Selected mascot was not found".to_string(),
                ..Default::default()
            });
        }
    }
    results
}

/// 把压缩包端到端导入桌宠存储：
/// - 目录按旧版目录方式打包；
/// - .mascot 文件直接安装；
/// - 其他压缩包先分析，每个可转换候选都打包安装。
///
/// 返回导入的模板名集合。不支持的压缩格式返回
/// [`PackError::Unsupported`]。
pub fn import_archive(archive_path: &Path, storage_path: &Path) -> Result<BTreeSet<String>> {
    let mut imported: BTreeSet<String> = BTreeSet::new();
    let Ok(info) = fs::metadata(archive_path) else {
        return Ok(imported);
    };

    if info.is_dir() {
        let fallback_name = archive_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        if let Ok(installed_name) =
            crate::package::package_legacy_directory(archive_path, storage_path, &fallback_name)
        {
            imported.insert(installed_name);
        }
        return Ok(imported);
    }

    if !info.is_file() || info.len() > limits::MASCOT_PACKAGE_MAX_BYTES {
        return Ok(imported);
    }

    let extension = archive_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();
    if extension == "mascot"
        && let Ok(installed_name) = crate::package::install_package(archive_path, storage_path)
    {
        imported.insert(installed_name);
        return Ok(imported);
    }
    // 直接安装失败：回退到旧版分析流程。

    let fallback_name = archive_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_default();
    let archive = LegacyArchive::open(archive_path, &fallback_name)?;
    let temp_dir = tempfile::tempdir()?;
    archive.extract_safely(temp_dir.path())?;
    for name in archive.shimejis().clone() {
        let source_path = extracted_legacy_mascot_path(temp_dir.path(), &name);
        try_extract_info_json(&archive, &name, &source_path);
        try_extract_bubble_context(&archive, &name, &source_path);
        try_extract_preview_images(&archive, &name, &source_path);
        if let Ok(installed_name) =
            crate::package::package_legacy_directory(&source_path, storage_path, &name)
        {
            imported.insert(installed_name);
        }
    }
    Ok(imported)
}

/// 把 rar/7z/tar/tgz 解压到受控临时目录（文件名防目录逃逸）。
pub(crate) fn extract_to_tempdir(archive_path: &Path, dest: &Path, extension: &str) -> Result<()> {
    match extension {
        "rar" => extract_rar_to(archive_path, dest),
        "7z" => extract_7z_to(archive_path, dest),
        "tar" => extract_tar_to(archive_path, dest, false),
        "tgz" => extract_tar_to(archive_path, dest, true),
        _ => Err(PackError::Unsupported(format!(".{extension}"))),
    }
}

/// 解压单条到目标目录时防目录逃逸（拒绝绝对路径与 .. 组件）。
fn safe_join(root: &Path, name: &Path) -> Option<std::path::PathBuf> {
    let mut out = root.to_path_buf();
    for component in name.components() {
        match component {
            std::path::Component::Normal(part) => out.push(part),
            std::path::Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

/// 累计解压总量校验：超出 MASCOT_EXTRACTED_MAX_BYTES 立即报错（防解压炸弹）。
fn check_extracted_total(total_bytes: u64, chunk: u64) -> Result<()> {
    if total_bytes > limits::MASCOT_EXTRACTED_MAX_BYTES
        || chunk > limits::MASCOT_EXTRACTED_MAX_BYTES - total_bytes
    {
        return Err(PackError::msg("Mascot package extracted data is too large"));
    }
    Ok(())
}

/// 校验单条及累计解压预算，避免在读取超限载荷后才拒绝。
fn check_extracted_entry_size(total_bytes: u64, entry_bytes: u64) -> Result<()> {
    if entry_bytes > limits::MASCOT_SINGLE_FILE_MAX_BYTES {
        return Err(PackError::msg("Mascot package entry is too large"));
    }
    check_extracted_total(total_bytes, entry_bytes)
}

/// 校验已处理的文件数，拒绝超过归档条目上限的下一条文件。
fn check_extracted_entry_count(file_count: usize) -> Result<()> {
    if file_count >= limits::MASCOT_ZIP_ENTRY_MAX_COUNT {
        return Err(PackError::msg("Archive contains too many extracted files"));
    }
    Ok(())
}

/// 校验实际读取长度与归档头声明一致，拒绝截断或伪造的条目数据。
fn check_extracted_entry_data_size(declared_size: u64, actual_size: u64) -> Result<()> {
    if declared_size != actual_size {
        return Err(PackError::msg(
            "Archive entry size does not match its header",
        ));
    }
    Ok(())
}

/// 读取归档条目时保留一个探测字节，防止声明尺寸与实际流不一致时越过预算。
fn read_entry_limited<R: Read>(reader: R, declared_size: u64, total_bytes: u64) -> Result<Vec<u8>> {
    check_extracted_entry_size(total_bytes, declared_size)?;
    let remaining_total = limits::MASCOT_EXTRACTED_MAX_BYTES - total_bytes;
    let byte_limit = remaining_total.min(limits::MASCOT_SINGLE_FILE_MAX_BYTES);
    let mut data = Vec::new();
    reader
        .take(byte_limit + 1)
        .read_to_end(&mut data)
        .map_err(|error| PackError::msg(format!("archive extract error: {error}")))?;
    check_extracted_entry_size(total_bytes, data.len() as u64)?;
    Ok(data)
}

/// 把条目内容写入目标路径（父目录自动创建），并累计解压总量。
fn write_entry(dest: &Path, name: &Path, data: &[u8], total_bytes: &mut u64) -> Result<()> {
    let entry_bytes = data.len() as u64;
    check_extracted_entry_size(*total_bytes, entry_bytes)?;
    let Some(target) = safe_join(dest, name) else {
        *total_bytes += entry_bytes;
        return Ok(());
    };
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&target, data)?;
    *total_bytes += entry_bytes;
    Ok(())
}

/// 用自定义抽取回调解压 7z，逐条强制单文件和累计解压预算。
fn extract_7z_to(archive_path: &Path, dest: &Path) -> Result<()> {
    let mut total_bytes: u64 = 0;
    let mut file_count: usize = 0;
    sevenz_rust2::decompress_file_with_extract_fn(
        archive_path,
        dest,
        |entry, reader, entry_dest| {
            if entry.is_directory() {
                return sevenz_rust2::default_entry_extract_fn(entry, reader, entry_dest);
            }
            check_extracted_entry_count(file_count)
                .map_err(|error| sevenz_rust2::Error::Other(error.to_string().into()))?;
            let data = read_entry_limited(reader, entry.size(), total_bytes)
                .map_err(|error| sevenz_rust2::Error::Other(error.to_string().into()))?;
            check_extracted_entry_data_size(entry.size(), data.len() as u64)
                .map_err(|error| sevenz_rust2::Error::Other(error.to_string().into()))?;
            let mut data_reader = Cursor::new(data);
            let result =
                sevenz_rust2::default_entry_extract_fn(entry, &mut data_reader, entry_dest)?;
            total_bytes += entry.size();
            file_count += 1;
            Ok(result)
        },
    )
    .map_err(|e| PackError::msg(format!("Failed to extract 7z archive: {e}")))
}

fn extract_rar_to(archive_path: &Path, dest: &Path) -> Result<()> {
    let mut archive = unrar::Archive::new(archive_path)
        .open_for_processing()
        .map_err(|e| PackError::msg(format!("Failed to open rar archive: {e}")))?;
    let mut total_bytes: u64 = 0;
    let mut file_count: usize = 0;
    while let Some(cursor) = archive
        .read_header()
        .map_err(|e| PackError::msg(format!("rar read error: {e}")))?
    {
        let name = cursor.entry().filename.clone();
        if !cursor.entry().is_file() {
            archive = cursor
                .skip()
                .map_err(|e| PackError::msg(format!("rar skip error: {e}")))?;
            continue;
        }
        check_extracted_entry_count(file_count)?;
        let declared_size = cursor.entry().unpacked_size;
        check_extracted_entry_size(total_bytes, declared_size)?;
        // 即使路径随后被拒绝，该文件也必须占用条目数和声明尺寸预算。
        file_count += 1;
        total_bytes += declared_size;
        let Some(target) = safe_join(dest, &name) else {
            archive = cursor
                .skip()
                .map_err(|e| PackError::msg(format!("rar skip error: {e}")))?;
            continue;
        };
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }

        // unrar 直接写入受控路径；头部预算在落盘前检查，落盘后再核对实际长度。
        let rest = match cursor.extract_to(&target) {
            Ok(rest) => rest,
            Err(error) => {
                let _ = fs::remove_file(&target);
                return Err(PackError::msg(format!("rar extract error: {error}")));
            }
        };
        let actual_size = match fs::metadata(&target) {
            Ok(metadata) => metadata.len(),
            Err(error) => {
                let _ = fs::remove_file(&target);
                return Err(error.into());
            }
        };
        if let Err(error) = check_extracted_entry_data_size(declared_size, actual_size) {
            let _ = fs::remove_file(&target);
            return Err(error);
        }
        archive = rest;
    }
    Ok(())
}

fn extract_tar_to(archive_path: &Path, dest: &Path, gzipped: bool) -> Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let reader: Box<dyn std::io::Read> = if gzipped {
        Box::new(flate2::read::GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut archive = tar::Archive::new(reader);
    let mut total_bytes: u64 = 0;
    let mut file_count: usize = 0;
    for entry in archive
        .entries()
        .map_err(|e| PackError::msg(format!("tar read error: {e}")))?
    {
        let mut entry = entry.map_err(|e| PackError::msg(format!("tar entry error: {e}")))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        check_extracted_entry_count(file_count)?;
        let name = entry
            .path()
            .map_err(|e| PackError::msg(format!("tar path error: {e}")))?
            .to_path_buf();
        let declared_size = entry.size();
        let data = read_entry_limited(&mut entry, declared_size, total_bytes)?;
        check_extracted_entry_data_size(declared_size, data.len() as u64)?;
        write_entry(dest, &name, &data, &mut total_bytes)?;
        file_count += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_copy_7z<R: Read>(archive_path: &Path, entry_name: &str, reader: R) {
        let mut archive =
            sevenz_rust2::ArchiveWriter::new(std::fs::File::create(archive_path).unwrap()).unwrap();
        archive.set_content_methods(vec![sevenz_rust2::EncoderMethod::COPY.into()]);
        archive
            .push_archive_entry(
                sevenz_rust2::ArchiveEntry::new_file(entry_name),
                Some(reader),
            )
            .unwrap();
        archive.finish().unwrap();
    }

    #[test]
    fn check_extracted_total_rejects_over_limit() {
        // 未超限时放行；累计达到上限后任何非零条目都拒绝。
        assert!(check_extracted_total(0, 1).is_ok());
        assert!(check_extracted_total(0, limits::MASCOT_EXTRACTED_MAX_BYTES).is_ok());
        assert!(check_extracted_total(limits::MASCOT_EXTRACTED_MAX_BYTES, 1).is_err());
        assert!(check_extracted_total(limits::MASCOT_EXTRACTED_MAX_BYTES - 1, 2).is_err());
    }

    #[test]
    fn check_extracted_entry_size_rejects_per_file_limit() {
        assert!(check_extracted_entry_size(0, limits::MASCOT_SINGLE_FILE_MAX_BYTES).is_ok());
        assert!(check_extracted_entry_size(0, limits::MASCOT_SINGLE_FILE_MAX_BYTES + 1).is_err());
    }

    #[test]
    fn check_extracted_entry_count_rejects_next_file_after_limit() {
        assert!(check_extracted_entry_count(limits::MASCOT_ZIP_ENTRY_MAX_COUNT - 1).is_ok());
        assert!(check_extracted_entry_count(limits::MASCOT_ZIP_ENTRY_MAX_COUNT).is_err());
    }

    #[test]
    fn check_extracted_entry_data_size_rejects_mismatch() {
        assert!(check_extracted_entry_data_size(3, 3).is_ok());
        assert!(check_extracted_entry_data_size(3, 2).is_err());
    }

    #[test]
    fn read_entry_limited_stops_after_hard_budget() {
        let mut reader = Cursor::new(vec![
            0u8;
            (limits::MASCOT_SINGLE_FILE_MAX_BYTES + 2) as usize
        ]);
        let error = read_entry_limited(&mut reader, 0, 0).unwrap_err();

        assert_eq!(error.to_string(), "Mascot package entry is too large");
        assert_eq!(reader.position(), limits::MASCOT_SINGLE_FILE_MAX_BYTES + 1);
    }

    #[test]
    fn read_entry_limited_respects_remaining_total_budget() {
        let mut reader = Cursor::new(vec![0u8; 2]);
        let error =
            read_entry_limited(&mut reader, 0, limits::MASCOT_EXTRACTED_MAX_BYTES - 1).unwrap_err();

        assert_eq!(
            error.to_string(),
            "Mascot package extracted data is too large"
        );
        assert_eq!(reader.position(), 2);
    }

    #[test]
    fn tar_rejects_oversized_declared_entry_before_writing() {
        let tempdir = tempfile::tempdir().unwrap();
        let archive_path = tempdir.path().join("oversized.tar");
        let mut builder = tar::Builder::new(std::fs::File::create(&archive_path).unwrap());
        let mut header = tar::Header::new_gnu();
        let entry_size = limits::MASCOT_SINGLE_FILE_MAX_BYTES + 1;
        header.set_size(entry_size);
        header.set_mode(0o644);
        header.set_cksum();
        let mut contents = std::io::repeat(0).take(entry_size);
        builder
            .append_data(&mut header, "oversized.bin", &mut contents)
            .unwrap();
        builder.finish().unwrap();

        let destination = tempdir.path().join("extracted");
        let error = extract_tar_to(&archive_path, &destination, false).unwrap_err();
        assert_eq!(error.to_string(), "Mascot package entry is too large");
        assert!(!destination.join("oversized.bin").exists());
    }

    #[test]
    fn sevenz_rejects_oversized_declared_entry_before_writing() {
        let tempdir = tempfile::tempdir().unwrap();
        let entry_size = limits::MASCOT_SINGLE_FILE_MAX_BYTES + 1;
        let archive_path = tempdir.path().join("oversized.7z");
        write_copy_7z(
            &archive_path,
            "oversized.bin",
            std::io::repeat(0).take(entry_size),
        );

        let destination = tempdir.path().join("extracted");
        let error = extract_7z_to(&archive_path, &destination).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Mascot package entry is too large")
        );
        assert!(!destination.join("oversized.bin").exists());
    }

    #[test]
    fn sevenz_extracts_small_entry_with_bounded_reader() {
        let tempdir = tempfile::tempdir().unwrap();
        let archive_path = tempdir.path().join("small.7z");
        write_copy_7z(&archive_path, "small.txt", Cursor::new(b"bounded"));

        let destination = tempdir.path().join("extracted");
        extract_7z_to(&archive_path, &destination).unwrap();
        assert_eq!(
            std::fs::read(destination.join("small.txt")).unwrap(),
            b"bounded"
        );
    }

    #[test]
    fn write_entry_accumulates_and_enforces_total_limit() {
        let tempdir = tempfile::tempdir().unwrap();
        let mut total_bytes: u64 = 0;
        write_entry(tempdir.path(), Path::new("a.txt"), b"abc", &mut total_bytes).unwrap();
        assert_eq!(total_bytes, 3);
        // 不安全路径跳过写入，但已读取的数据必须占用预算。
        write_entry(
            tempdir.path(),
            Path::new("../evil.txt"),
            b"x",
            &mut total_bytes,
        )
        .unwrap();
        assert_eq!(total_bytes, 4);
        // 总量已达上限时写入报错，且不落盘。
        total_bytes = limits::MASCOT_EXTRACTED_MAX_BYTES;
        assert!(write_entry(tempdir.path(), Path::new("b.txt"), b"x", &mut total_bytes).is_err());
        assert!(!tempdir.path().join("b.txt").exists());
    }
}
