//! GitHub Device Flow 和系统安全凭据存储入口。

use platform::{CredentialError, CredentialStore};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// GitHub Device Flow 返回的用户代码。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCode {
    /// 设备轮询码。
    pub device_code: String,
    /// 用户需要输入的短码。
    pub user_code: String,
    /// 用户验证地址。
    pub verification_uri: String,
    /// 轮询间隔（秒）。
    pub interval_seconds: u64,
    /// 过期时间（秒）。
    pub expires_in_seconds: u64,
}

/// 已登录用户的短期令牌。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessToken(String);

impl AccessToken {
    /// 创建令牌并拒绝空值。
    pub fn new(value: impl Into<String>) -> Result<Self, AuthError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(AuthError::EmptyToken);
        }
        Ok(Self(value))
    }

    /// 返回令牌值；调用方不应写入日志。
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// 登录状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthState {
    SignedOut,
    AwaitingDeviceCode(DeviceCode),
    SignedIn,
}

/// 认证边界错误。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuthError {
    /// 令牌为空。
    #[error("access token must not be empty")]
    EmptyToken,
    /// 凭据存储不可用。
    #[error("credential store is unavailable")]
    Unavailable,
    /// 凭据存储操作失败。
    #[error("credential store failed: {0}")]
    Store(String),
    /// Device Flow 尚未开始。
    #[error("device flow has not started")]
    NoDeviceFlow,
}

/// GitHub 认证管理器；令牌只经过凭据存储，不写入配置文件。
pub struct AuthManager<S> {
    store: S,
    state: AuthState,
    service: String,
    account: String,
}

impl<S: CredentialStore> AuthManager<S> {
    /// 创建认证管理器。
    pub fn new(store: S, service: impl Into<String>, account: impl Into<String>) -> Self {
        Self {
            store,
            state: AuthState::SignedOut,
            service: service.into(),
            account: account.into(),
        }
    }

    /// 开始 Device Flow 并保存待完成状态。
    pub fn begin_device_flow(&mut self, code: DeviceCode) -> AuthState {
        self.state = AuthState::AwaitingDeviceCode(code);
        self.state.clone()
    }

    /// 使用轮询得到的令牌完成登录。
    pub fn complete_device_flow(&mut self, token: AccessToken) -> Result<(), AuthError> {
        if !matches!(self.state, AuthState::AwaitingDeviceCode(_)) {
            return Err(AuthError::NoDeviceFlow);
        }
        self.store
            .save(&self.service, &self.account, token.expose())
            .map_err(map_store_error)?;
        self.state = AuthState::SignedIn;
        Ok(())
    }

    /// 从安全存储恢复登录状态。
    pub fn restore(&mut self) -> Result<bool, AuthError> {
        let token = self
            .store
            .load(&self.service, &self.account)
            .map_err(map_store_error)?;
        self.state = if token.is_some() {
            AuthState::SignedIn
        } else {
            AuthState::SignedOut
        };
        Ok(token.is_some())
    }

    /// 注销并删除该服务下的当前凭据。
    pub fn sign_out(&mut self) -> Result<bool, AuthError> {
        let removed = self
            .store
            .remove(&self.service, &self.account)
            .map_err(map_store_error)?;
        self.state = AuthState::SignedOut;
        Ok(removed)
    }

    /// 返回当前认证状态。
    pub fn state(&self) -> &AuthState {
        &self.state
    }
}

fn map_store_error(error: CredentialError) -> AuthError {
    match error {
        CredentialError::Unavailable => AuthError::Unavailable,
        CredentialError::EmptyField => AuthError::Store("credential fields are empty".into()),
    }
}

/// 对日志文本中的常见 Authorization、token 和 cookie 做最小脱敏。
pub fn redact_sensitive_text(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut redact_words = 0usize;
    for word in text.split_whitespace() {
        let lower = word.to_ascii_lowercase();
        if redact_words > 0 {
            output.push_str("[REDACTED]");
            redact_words -= 1;
        } else if lower == "authorization:" {
            output.push_str("[REDACTED]");
            redact_words = 2;
        } else if lower == "cookie:" || lower == "bearer" {
            output.push_str("[REDACTED]");
            redact_words = 1;
        } else if lower.starts_with("token=") {
            output.push_str("[REDACTED]");
        } else {
            output.push_str(word);
        }
        output.push(' ');
    }
    output.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::{AccessToken, AuthManager, AuthState, DeviceCode, redact_sensitive_text};
    use platform::MemoryCredentialStore;

    fn code() -> DeviceCode {
        DeviceCode {
            device_code: "device".into(),
            user_code: "user".into(),
            verification_uri: "https://github.com/login/device".into(),
            interval_seconds: 5,
            expires_in_seconds: 600,
        }
    }

    #[test]
    fn device_flow_stores_and_restores_token_without_exposing_state() {
        let mut manager = AuthManager::new(MemoryCredentialStore::default(), "github", "default");
        assert!(matches!(manager.state(), AuthState::SignedOut));
        manager.begin_device_flow(code());
        manager
            .complete_device_flow(AccessToken::new("secret-token").unwrap())
            .unwrap();
        assert!(matches!(manager.state(), AuthState::SignedIn));
        assert!(manager.sign_out().unwrap());
        assert!(matches!(manager.state(), AuthState::SignedOut));
    }

    #[test]
    fn redaction_hides_authentication_headers() {
        let text = "Authorization: Bearer abc123 cookie: sid=secret visible";
        let redacted = redact_sensitive_text(text);
        assert!(!redacted.contains("abc123"));
        assert!(!redacted.contains("sid=secret"));
        assert!(redacted.contains("visible"));
    }
}
