use std::path::{Component, Path, PathBuf};

/// 路径安全校验错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SafePathError {
    #[error("path root or child name is empty")]
    EmptyPath,
    #[error("absolute paths are not allowed")]
    AbsolutePath,
    #[error("drive prefixes are not allowed")]
    DrivePrefix,
    #[error("path contains an empty, dot, or parent component")]
    InvalidComponent,
    #[error("path escapes its allowed root")]
    EscapesRoot,
    #[error("path component cannot be resolved safely")]
    Unresolvable,
}

/// 校验归档 entry 或其他不可信相对路径。
pub fn validate_relative_path(name: &str) -> Result<String, SafePathError> {
    if name.is_empty() {
        return Err(SafePathError::EmptyPath);
    }

    // 归档在 Windows 与 Unix 上都可能使用反斜杠，先统一后再逐段判断。
    let normalized = name.replace('\\', "/");
    if normalized.starts_with('/') {
        return Err(SafePathError::AbsolutePath);
    }
    if normalized.contains(':') {
        return Err(SafePathError::DrivePrefix);
    }

    let mut components = Vec::new();
    for component in normalized.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err(SafePathError::InvalidComponent);
        }
        components.push(component);
    }
    if components.is_empty() {
        return Err(SafePathError::InvalidComponent);
    }
    Ok(components.join("/"))
}

/// 返回 root 下的安全子路径，并拒绝符号链接逃逸。
///
/// `@` 开头的 root 代表内置资源虚拟根；该路径不会访问文件系统，但仍然
/// 使用同一套相对路径规则。
pub fn safe_child_path(root: impl AsRef<Path>, name: &str) -> Result<PathBuf, SafePathError> {
    let root = root.as_ref();
    if root.as_os_str().is_empty() {
        return Err(SafePathError::EmptyPath);
    }
    let normalized = validate_relative_path(name)?;

    if root.to_string_lossy().starts_with('@') {
        return Ok(root.join(normalized));
    }

    if !root.is_absolute()
        && root
            .components()
            .any(|component| matches!(component, Component::Prefix(_)))
    {
        return Err(SafePathError::DrivePrefix);
    }
    let absolute_root = std::fs::canonicalize(root).or_else(|_| {
        let absolute = if root.is_absolute() {
            root.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::NotFound))?
                .join(root)
        };
        Ok::<PathBuf, std::io::Error>(absolute)
    })?;
    let candidate = absolute_root.join(&normalized);

    // 仅 canonicalize 已存在的祖先，允许安全地创建新的叶子文件。
    let mut existing = candidate.as_path();
    let canonical_existing = loop {
        match std::fs::canonicalize(existing) {
            Ok(path) => break path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                existing = existing.parent().ok_or(SafePathError::Unresolvable)?;
            }
            Err(_) => return Err(SafePathError::Unresolvable),
        }
    };
    if !canonical_existing.starts_with(&absolute_root) {
        return Err(SafePathError::EscapesRoot);
    }
    Ok(candidate)
}

impl From<std::io::Error> for SafePathError {
    fn from(error: std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::NotFound => Self::Unresolvable,
            _ => Self::Unresolvable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_absolute_and_parent_paths() {
        for path in ["/etc/passwd", r"C:\\temp\\x", "../x", "a/../x", "a//x", "."] {
            assert!(validate_relative_path(path).is_err(), "{path}");
        }
    }

    #[test]
    fn normalizes_backslashes_without_allowing_escape() {
        assert_eq!(
            validate_relative_path(r"img\frame.png").unwrap(),
            "img/frame.png"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), temp.path().join("link")).unwrap();
        assert_eq!(
            safe_child_path(temp.path(), "link/file.txt"),
            Err(SafePathError::EscapesRoot)
        );
    }

    #[test]
    fn accepts_virtual_resource_root() {
        assert_eq!(
            safe_child_path(Path::new("@"), "img/frame.png").unwrap(),
            PathBuf::from("@/img/frame.png")
        );
    }
}
