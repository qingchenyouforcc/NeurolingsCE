//! neurolings-store：桌宠商店客户端——索引模型、原子缓存、SHA-256 校验
//! 下载、GitHub 设备流登录与投稿上传。

pub mod cache;
pub mod config;
pub mod github;
pub mod index;
pub mod network;
pub mod submission;
pub mod updater;

pub use cache::{CachedIndex, StoreCache};
pub use github::{
    CredentialStore, DeviceCodeInfo, GitHubAuth, UserInfo, create_platform_credential_store,
};
pub use index::{StoreEntry, StoreIndex, StoreMedia};
pub use network::{IndexResponse, download, fetch_index, sha256_bytes, sha256_file};
pub use submission::{SubmissionClient, SubmissionResult};
pub use updater::{
    UpdateAsset, UpdateDecision, UpdateManifest, current_asset_key, decide, download_update,
    fetch_manifest, parse_manifest,
};
