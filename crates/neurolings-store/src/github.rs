//! GitHub OAuth 设备流登录与凭据存储。

use std::time::Duration;

use serde::Deserialize;

pub const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
pub const ACCESS_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
pub const API_BASE_URL: &str = "https://api.github.com";

const SERVICE: &str = "NeurolingsCE";
const ACCOUNT: &str = "github";

#[derive(Debug, Clone, PartialEq)]
pub enum AuthState {
    SignedOut,
    WaitingForDeviceCode,
    AwaitingAuthorization,
    SignedIn,
    Error,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UserInfo {
    pub login: String,
    pub user_id: String,
    pub display_name: String,
    pub avatar_url: String,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceCodeInfo {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval_seconds: u32,
    pub expires_in_seconds: u32,
}

/// 凭据存储抽象：支持平台走系统密钥库，否则退回内存。
pub trait CredentialStore {
    fn save(&self, service: &str, account: &str, secret: &str) -> Result<(), String>;
    fn load(&self, service: &str, account: &str) -> Result<Option<String>, String>;
    fn remove(&self, service: &str, account: &str) -> Result<(), String>;
    fn is_available(&self) -> bool;
}

pub struct InMemoryCredentialStore {
    secrets: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

impl InMemoryCredentialStore {
    pub fn new() -> Self {
        Self {
            secrets: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for InMemoryCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for InMemoryCredentialStore {
    fn save(&self, service: &str, account: &str, secret: &str) -> Result<(), String> {
        self.secrets
            .lock()
            .unwrap()
            .insert(format!("{service}/{account}"), secret.to_string());
        Ok(())
    }
    fn load(&self, service: &str, account: &str) -> Result<Option<String>, String> {
        Ok(self
            .secrets
            .lock()
            .unwrap()
            .get(&format!("{service}/{account}"))
            .cloned())
    }
    fn remove(&self, service: &str, account: &str) -> Result<(), String> {
        self.secrets
            .lock()
            .unwrap()
            .remove(&format!("{service}/{account}"));
        Ok(())
    }
    fn is_available(&self) -> bool {
        true
    }
}

#[cfg(feature = "keyring")]
pub struct KeyringCredentialStore;

#[cfg(feature = "keyring")]
impl CredentialStore for KeyringCredentialStore {
    fn save(&self, service: &str, account: &str, secret: &str) -> Result<(), String> {
        keyring::Entry::new(service, account)
            .map_err(|e| e.to_string())?
            .set_password(secret)
            .map_err(|e| e.to_string())
    }
    fn load(&self, service: &str, account: &str) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(service, account).map_err(|e| e.to_string())?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn remove(&self, service: &str, account: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(service, account).map_err(|e| e.to_string())?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
    fn is_available(&self) -> bool {
        true
    }
}

pub fn create_platform_credential_store() -> Box<dyn CredentialStore> {
    #[cfg(feature = "keyring")]
    {
        Box::new(KeyringCredentialStore)
    }
    #[cfg(not(feature = "keyring"))]
    {
        Box::new(InMemoryCredentialStore::new())
    }
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    #[serde(default)]
    device_code: String,
    #[serde(default)]
    user_code: String,
    #[serde(default)]
    verification_uri: String,
    #[serde(default = "default_interval")]
    interval: u32,
    #[serde(default = "default_expiry")]
    expires_in: u32,
    #[serde(default)]
    error: Option<String>,
}

fn default_interval() -> u32 {
    5
}
fn default_expiry() -> u32 {
    900
}

#[derive(Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Deserialize)]
struct UserResponse {
    #[serde(default)]
    login: String,
    #[serde(default)]
    id: serde_json::Value,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    avatar_url: Option<String>,
}

/// 设备流第一步：请求设备码。
pub fn request_device_code(client_id: &str) -> Result<DeviceCodeInfo, String> {
    let response = ureq::post(DEVICE_CODE_URL)
        .timeout(Duration::from_secs(15))
        .set("Accept", "application/json")
        .send_string(&format!("client_id={client_id}"))
        .map_err(|e| format!("device code request failed: {e}"))?;
    let parsed: DeviceCodeResponse = response
        .into_json()
        .map_err(|e| format!("device code parse failed: {e}"))?;
    if let Some(err) = parsed.error {
        return Err(format!("device code error: {err}"));
    }
    if parsed.device_code.is_empty() || parsed.user_code.is_empty() {
        return Err("device code response missing fields".into());
    }
    Ok(DeviceCodeInfo {
        device_code: parsed.device_code,
        user_code: parsed.user_code,
        verification_uri: parsed.verification_uri,
        interval_seconds: parsed.interval.max(1),
        expires_in_seconds: parsed.expires_in,
    })
}

/// 对访问令牌端点一次轮询的结果。
pub enum PollOutcome {
    Authorized {
        access_token: String,
        refresh_token: Option<String>,
    },
    Pending {
        slow_down: bool,
    },
    Failed(String),
}

pub fn poll_access_token(client_id: &str, device_code: &str) -> PollOutcome {
    let body = format!(
        "client_id={client_id}&device_code={device_code}&grant_type=urn:ietf:params:oauth:grant-type:device_code"
    );
    let response = match ureq::post(ACCESS_TOKEN_URL)
        .timeout(Duration::from_secs(15))
        .set("Accept", "application/json")
        .send_string(&body)
    {
        Ok(r) => r,
        Err(e) => return PollOutcome::Failed(format!("token request failed: {e}")),
    };
    let parsed: TokenResponse = match response.into_json() {
        Ok(t) => t,
        Err(e) => return PollOutcome::Failed(format!("token parse failed: {e}")),
    };
    if let Some(token) = parsed.access_token {
        return PollOutcome::Authorized {
            access_token: token,
            refresh_token: parsed.refresh_token,
        };
    }
    match parsed.error.as_deref() {
        Some("authorization_pending") => PollOutcome::Pending { slow_down: false },
        Some("slow_down") => PollOutcome::Pending { slow_down: true },
        Some(other) => PollOutcome::Failed(other.to_string()),
        None => PollOutcome::Failed("no access token returned".into()),
    }
}

pub fn fetch_user(access_token: &str) -> Result<UserInfo, String> {
    let response = ureq::get(&format!("{API_BASE_URL}/user"))
        .timeout(Duration::from_secs(15))
        .set("Accept", "application/vnd.github+json")
        .set("Authorization", &format!("Bearer {access_token}"))
        .call()
        .map_err(|e| format!("user request failed: {e}"))?;
    let parsed: UserResponse = response
        .into_json()
        .map_err(|e| format!("user parse failed: {e}"))?;
    let user_id = match parsed.id {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s,
        _ => String::new(),
    };
    Ok(UserInfo {
        login: parsed.login,
        user_id,
        display_name: parsed.name.unwrap_or_default(),
        avatar_url: parsed.avatar_url.unwrap_or_default(),
    })
}

/// 会话管理器：封装设备流与凭据存储。
pub struct GitHubAuth {
    pub client_id: String,
    pub state: AuthState,
    pub user: UserInfo,
    pub device: Option<DeviceCodeInfo>,
    pub access_token: String,
    pub last_error: String,
    credentials: Box<dyn CredentialStore>,
}

impl GitHubAuth {
    pub fn new(client_id: String, credentials: Box<dyn CredentialStore>) -> Self {
        let mut auth = Self {
            client_id,
            state: AuthState::SignedOut,
            user: UserInfo::default(),
            device: None,
            access_token: String::new(),
            last_error: String::new(),
            credentials,
        };
        auth.try_restore_session();
        auth
    }

    pub fn is_signed_in(&self) -> bool {
        self.state == AuthState::SignedIn
    }

    fn try_restore_session(&mut self) {
        if let Ok(Some(secret)) = self.credentials.load(SERVICE, ACCOUNT)
            && !secret.is_empty()
        {
            self.access_token = secret;
            if let Ok(user) = fetch_user(&self.access_token) {
                self.user = user;
                self.state = AuthState::SignedIn;
            } else {
                self.access_token.clear();
            }
        }
    }

    /// 启动设备流，返回用户码与验证 URL。
    pub fn start_device_flow(&mut self) -> Result<DeviceCodeInfo, String> {
        self.state = AuthState::WaitingForDeviceCode;
        let info = request_device_code(&self.client_id).inspect_err(|e| {
            self.state = AuthState::Error;
            self.last_error = e.clone();
        })?;
        self.device = Some(info.clone());
        self.state = AuthState::AwaitingAuthorization;
        Ok(info)
    }

    /// 阻塞式便捷方法：轮询直至授权成功、过期或出错。
    pub fn poll_until_done(
        &mut self,
        on_pending: &mut dyn FnMut(&DeviceCodeInfo),
    ) -> Result<UserInfo, String> {
        let device = self.device.clone().ok_or("device flow not started")?;
        let interval = Duration::from_secs(device.interval_seconds.max(1) as u64);
        let deadline = std::time::Instant::now()
            + Duration::from_secs(device.expires_in_seconds.max(60) as u64);
        loop {
            if std::time::Instant::now() >= deadline {
                self.state = AuthState::Error;
                self.last_error = "device flow expired".into();
                return Err(self.last_error.clone());
            }
            on_pending(&device);
            std::thread::sleep(interval);
            match poll_access_token(&self.client_id, &device.device_code) {
                PollOutcome::Authorized { access_token, .. } => {
                    self.access_token = access_token.clone();
                    let user = fetch_user(&access_token)?;
                    self.user = user.clone();
                    self.state = AuthState::SignedIn;
                    let _ = self.credentials.save(SERVICE, ACCOUNT, &access_token);
                    return Ok(user);
                }
                PollOutcome::Pending { .. } => continue,
                PollOutcome::Failed(e) => {
                    self.state = AuthState::Error;
                    self.last_error = e.clone();
                    return Err(e);
                }
            }
        }
    }

    pub fn sign_out(&mut self) {
        let _ = self.credentials.remove(SERVICE, ACCOUNT);
        self.access_token.clear();
        self.user = UserInfo::default();
        self.state = AuthState::SignedOut;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_credential_store_roundtrip() {
        let store = InMemoryCredentialStore::new();
        store.save("svc", "acct", "secret").unwrap();
        assert_eq!(
            store.load("svc", "acct").unwrap().as_deref(),
            Some("secret")
        );
        store.remove("svc", "acct").unwrap();
        assert_eq!(store.load("svc", "acct").unwrap(), None);
    }
}
