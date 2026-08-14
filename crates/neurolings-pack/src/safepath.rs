//! 路径安全工具。
//!
//! 拒绝绝对路径、盘符/`:` 分隔符、空值、`.` 与 `..` 组件，
//! 以及借符号链接逃出允许根目录的路径。

use std::path::{Component, Path, PathBuf};

/// 检查 path 是否位于 root 之内。
///
/// root 到 path 的相对路径不得为 `..`、不得以 `../` 开头、不得是绝对路径；
/// 两个入参都应已是清理过的绝对路径。
pub fn is_contained(root: &Path, path: &Path) -> bool {
    if let Ok(relative) = path.strip_prefix(root) {
        // 去掉前后分隔符的相对路径不可能逃逸；空串表示两者相同。
        let _ = relative;
        true
    } else {
        false
    }
}

/// 词法清理路径：合并冗余分隔符并解析 `.` / `..` 组件，不访问文件系统。
pub fn clean_path(path: &Path) -> PathBuf {
    let mut cleaned = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // 相对路径保留开头的 `..`；永不移除根前缀或路径起点之前的部分。
                if cleaned.has_root()
                    || matches!(cleaned.components().next_back(), Some(Component::Normal(_)))
                {
                    cleaned.pop();
                } else {
                    cleaned.push("..");
                }
            }
            other => cleaned.push(other.as_os_str()),
        }
    }
    if cleaned.as_os_str().is_empty() {
        cleaned.push(".");
    }
    cleaned
}

/// 在 root 之下解析不可信的相对名称 name；逃逸则拒绝。
///
/// 规则：
/// - 空 root/name、绝对 name、含 `:` 的 name 一律拒绝；
/// - 反斜杠按分隔符处理；
/// - 空组件与 `.` / `..` 组件拒绝；
/// - 清理后的候选路径必须保持在绝对 root 之内；
/// - 候选路径最深的已存在祖先经规范化后必须仍在规范化 root 之内
///   （封堵符号链接逃逸）。
pub fn safe_child_path(root: &Path, name: &str) -> Option<PathBuf> {
    if root.as_os_str().is_empty() || name.is_empty() || Path::new(name).is_absolute() {
        return None;
    }

    let normalized_name: String = name
        .chars()
        .map(|c| if c == '\\' { '/' } else { c })
        .collect();
    if normalized_name.starts_with('/') || normalized_name.contains(':') {
        return None;
    }
    for part in normalized_name.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return None;
        }
    }

    // `@` 开头的 root 是内置默认桌宠的资源路径，不涉及文件系统。
    if root.to_str().is_some_and(|root| root.starts_with('@')) {
        return Some(clean_path(&root.join(&normalized_name)));
    }

    let absolute_root = clean_path(&absolute_path(root));
    let canonical_root = root
        .canonicalize()
        .unwrap_or_else(|_| absolute_root.clone());

    let candidate = clean_path(&absolute_root.join(&normalized_name));
    if !is_contained(&absolute_root, &candidate) {
        return None;
    }

    // 从候选路径向上找到最深的已存在祖先，验证其规范形式仍在规范 root 之内
    // （封堵符号链接逃逸）。
    let mut existing = candidate.clone();
    while !existing.exists() && existing != absolute_root {
        if !existing.pop() {
            break;
        }
    }
    if let Ok(canonical_existing) = existing.canonicalize()
        && canonical_existing != canonical_root
        && !is_contained(&canonical_root, &canonical_existing)
    {
        return None;
    }

    Some(candidate)
}

/// 尽力解析的绝对路径，不解析符号链接。
pub fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_absolute_names() {
        let root = Path::new("/tmp/root");
        assert_eq!(safe_child_path(Path::new(""), "a"), None);
        assert_eq!(safe_child_path(root, ""), None);
        assert_eq!(safe_child_path(root, "/etc/passwd"), None);
        assert_eq!(safe_child_path(root, "C:/Windows"), None);
        assert_eq!(safe_child_path(root, "a:b"), None);
    }

    #[test]
    fn rejects_dot_and_dotdot_components() {
        let root = Path::new("/tmp/root");
        assert_eq!(safe_child_path(root, "."), None);
        assert_eq!(safe_child_path(root, ".."), None);
        assert_eq!(safe_child_path(root, "a/.."), None);
        assert_eq!(safe_child_path(root, "a//b"), None);
        assert_eq!(safe_child_path(root, "./a"), None);
    }

    #[test]
    fn accepts_plain_relative_names() {
        let root = std::env::temp_dir();
        let safe = safe_child_path(&root, "img/shime1.png").expect("should accept");
        assert!(safe.starts_with(clean_path(&absolute_path(&root))));
        assert!(safe.ends_with(Path::new("img/shime1.png")));
    }

    #[test]
    fn treats_backslashes_as_separators() {
        let root = std::env::temp_dir();
        let safe = safe_child_path(&root, "img\\shime1.png").expect("should accept");
        assert!(safe.ends_with(Path::new("img/shime1.png")));
    }

    #[test]
    fn containment_check() {
        let root = Path::new("/a/b");
        assert!(is_contained(root, Path::new("/a/b/c")));
        assert!(is_contained(root, Path::new("/a/b")));
        assert!(!is_contained(root, Path::new("/a/bc")));
        assert!(!is_contained(root, Path::new("/a")));
    }

    #[test]
    fn clean_path_collapses_components() {
        assert_eq!(clean_path(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
        assert_eq!(clean_path(Path::new("a//b/")), PathBuf::from("a/b"));
    }

    #[test]
    fn rejects_symlink_escape() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let outside = base.path().join("outside");
        std::fs::create_dir(&outside).unwrap();

        let link = root.join("link");
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&outside, &link).is_ok();
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(&outside, &link).is_ok();
        #[cfg(not(any(unix, windows)))]
        let made = false;
        if !made {
            return; // symlink privileges unavailable; skip
        }
        assert_eq!(safe_child_path(&root, "link/evil.txt"), None);
        assert!(safe_child_path(&root, "plain.txt").is_some());
    }
}
