//! 商店与服务端点配置：编译期默认值，可经环境变量运行时覆盖。

const DEFAULT_INDEX_URL: &str = match option_env!("NEUROLINGSCE_MASCOT_INDEX_URL") {
    Some(v) => v,
    None => "https://blog.qingchenyou.asia/NeurolingsCE-Mascots-Staging/index-v1.json",
};
const DEFAULT_SUBMISSION_URL: &str = match option_env!("NEUROLINGSCE_SUBMISSION_SERVICE_URL") {
    Some(v) => v,
    None => "",
};
const DEFAULT_GITHUB_CLIENT_ID: &str = match option_env!("NEUROLINGSCE_GITHUB_LOGIN_CLIENT_ID") {
    Some(v) => v,
    None => "",
};

fn env_or(key: &str, fallback: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| fallback.to_string())
}

pub fn index_url() -> String {
    env_or("NEUROLINGSCE_MASCOT_INDEX_URL", DEFAULT_INDEX_URL)
}

pub fn submission_service_url() -> String {
    env_or(
        "NEUROLINGSCE_SUBMISSION_SERVICE_URL",
        DEFAULT_SUBMISSION_URL,
    )
}

pub fn github_login_client_id() -> String {
    env_or(
        "NEUROLINGSCE_GITHUB_LOGIN_CLIENT_ID",
        DEFAULT_GITHUB_CLIENT_ID,
    )
}

pub fn is_index_configured() -> bool {
    let url = index_url();
    (url.starts_with("https://") || url.starts_with("http://")) && url.len() > 8
}

pub fn is_login_configured() -> bool {
    !github_login_client_id().trim().is_empty()
}

pub fn is_configured() -> bool {
    is_index_configured()
}
