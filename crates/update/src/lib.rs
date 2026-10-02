//! GitHub release 更新检查与安装准备入口。

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 简化 SemVer，支持主版本、次版本、补丁和预发布标识。
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Version {
    /// 主版本。
    pub major: u64,
    /// 次版本。
    pub minor: u64,
    /// 补丁版本。
    pub patch: u64,
    /// 预发布标识。
    pub prerelease: Option<String>,
}

impl Version {
    /// 解析 `v1.2.3` 或 `1.2.3`。
    pub fn parse(value: &str) -> Result<Self, UpdateError> {
        let value = value.strip_prefix('v').unwrap_or(value);
        let (core, prerelease) = value
            .split_once('-')
            .map_or((value, None), |(core, pre)| (core, Some(pre.to_owned())));
        let mut parts = core.split('.');
        let major = parts
            .next()
            .ok_or(UpdateError::InvalidVersion)?
            .parse()
            .map_err(|_| UpdateError::InvalidVersion)?;
        let minor = parts
            .next()
            .ok_or(UpdateError::InvalidVersion)?
            .parse()
            .map_err(|_| UpdateError::InvalidVersion)?;
        let patch = parts
            .next()
            .ok_or(UpdateError::InvalidVersion)?
            .parse()
            .map_err(|_| UpdateError::InvalidVersion)?;
        if parts.next().is_some() {
            return Err(UpdateError::InvalidVersion);
        }
        Ok(Self {
            major,
            minor,
            patch,
            prerelease,
        })
    }
}

/// GitHub release 的必要字段。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseInfo {
    /// release 标签。
    pub tag_name: String,
    /// 是否草稿。
    #[serde(default)]
    pub draft: bool,
    /// 是否预发布。
    #[serde(default)]
    pub prerelease: bool,
    /// 下载地址。
    pub html_url: String,
}

/// 可用更新。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateCandidate {
    /// 当前版本。
    pub current: Version,
    /// 目标版本。
    pub target: Version,
    /// Release 元数据。
    pub release: ReleaseInfo,
}

/// 更新错误。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum UpdateError {
    /// 版本字符串无效。
    #[error("invalid semantic version")]
    InvalidVersion,
    /// URL 不符合下载安全边界。
    #[error("update URL must use https")]
    InsecureUrl,
}

/// 从 release 列表挑选最高稳定更新。
pub fn select_update(
    current: &Version,
    releases: &[ReleaseInfo],
) -> Result<Option<UpdateCandidate>, UpdateError> {
    let mut candidates = releases
        .iter()
        .filter(|release| !release.draft && !release.prerelease)
        .filter_map(|release| Version::parse(&release.tag_name).ok())
        .filter(|version| version > current)
        .collect::<Vec<_>>();
    candidates.sort();
    let Some(target) = candidates.pop() else {
        return Ok(None);
    };
    let release = releases
        .iter()
        .find(|release| Version::parse(&release.tag_name).ok().as_ref() == Some(&target))
        .expect("candidate release exists")
        .clone();
    Ok(Some(UpdateCandidate {
        current: current.clone(),
        target,
        release,
    }))
}

/// 校验更新下载地址，只允许 HTTPS。
pub fn validate_download_url(url: &str) -> Result<(), UpdateError> {
    if url.strip_prefix("https://").is_some() && !url[8..].is_empty() {
        Ok(())
    } else {
        Err(UpdateError::InsecureUrl)
    }
}

#[cfg(test)]
mod tests {
    use super::{ReleaseInfo, UpdateError, Version, select_update, validate_download_url};

    #[test]
    fn selects_highest_stable_release() {
        let current = Version::parse("1.0.0").unwrap();
        let releases = vec![
            ReleaseInfo {
                tag_name: "v1.1.0".into(),
                draft: false,
                prerelease: false,
                html_url: "https://example/1.1".into(),
            },
            ReleaseInfo {
                tag_name: "v2.0.0-beta".into(),
                draft: false,
                prerelease: true,
                html_url: "https://example/2".into(),
            },
            ReleaseInfo {
                tag_name: "v1.2.0".into(),
                draft: false,
                prerelease: false,
                html_url: "https://example/1.2".into(),
            },
        ];
        assert_eq!(
            select_update(&current, &releases).unwrap().unwrap().target,
            Version::parse("1.2.0").unwrap()
        );
    }

    #[test]
    fn rejects_insecure_download_url() {
        assert_eq!(
            validate_download_url("http://example/update"),
            Err(UpdateError::InsecureUrl)
        );
        assert!(validate_download_url("https://example/update").is_ok());
    }
}
