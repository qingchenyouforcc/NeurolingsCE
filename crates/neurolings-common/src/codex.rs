//! Codex 通知契约：CLI `--codex-notify` 与运行时气泡共用的活动解析。
//!
//! 解析器对桌面端各异的通知负载保持宽容：`type`/`method` 等价、
//! 新会话事件别名归一、允许把标题对象嵌在最终回复文本里。
//! 与通知无关的字段（input-messages 等）不参与气泡展示，也不落盘。

use serde_json::Value;

/// 单条通知 JSON 的最大字节数。
pub const NOTIFY_MAX_BYTES: usize = 256 * 1024;
/// 字符串字段的最大字节数。
const MAX_STRING_BYTES: usize = 128 * 1024;
/// input-messages 的最大条目数。
const MAX_MESSAGES: usize = 128;
/// 气泡文本压缩保留的最大字符数。
pub const MAX_RETAINED_CHARS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivityState {
    Running,
    NeedsInput,
    #[default]
    Ready,
    Blocked,
}

impl ActivityState {
    pub fn name(self) -> &'static str {
        match self {
            ActivityState::Running => "running",
            ActivityState::NeedsInput => "needs_input",
            ActivityState::Ready => "ready",
            ActivityState::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CodexActivity {
    pub event_type: String,
    pub state: ActivityState,
    pub thread_id: String,
    pub turn_id: String,
    pub cwd: String,
    pub last_assistant_message: String,
    pub session_title: String,
    pub session_description: String,
    pub is_new_session: bool,
    pub input_messages: Vec<String>,
}

/// 解析结果：recognized=false 表示事件类型不在处理范围内（应静默忽略）。
pub struct ParseOutcome {
    pub activity: CodexActivity,
    pub recognized: bool,
}

fn normalized_event_type(raw: &str) -> String {
    raw.trim().to_lowercase().replace('_', "-")
}

fn is_new_session_event(raw: &str) -> bool {
    matches!(
        normalized_event_type(raw).as_str(),
        "session-title-updated"
            | "session-title-generated"
            | "session-title-changed"
            | "session/title/updated"
            | "session/title/changed"
            | "session/titlechanged"
            | "session-name-updated"
            | "session-name-changed"
            | "session/name/updated"
            | "session/name/changed"
            | "thread-title-updated"
            | "thread-title-changed"
            | "thread/title/updated"
            | "thread/title/changed"
            | "thread-name-updated"
            | "thread-name-changed"
            | "thread-name/updated"
            | "thread/name/changed"
            | "thread/name/updated"
            | "new-session"
            | "session-started"
    )
}

/// 在对象自身及一层已知容器（params/session/thread/data）内查找字段。
fn find_value<'a>(object: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    for key in keys {
        if let Some(value) = object.get(*key)
            && !value.is_null()
        {
            return Some(value);
        }
    }
    for container_key in ["params", "session", "thread", "data"] {
        if let Some(nested) = object.get(container_key).filter(|v| v.is_object()) {
            for key in keys {
                if let Some(value) = nested.get(*key)
                    && !value.is_null()
                {
                    return Some(value);
                }
            }
        }
    }
    None
}

fn read_aliased_string(object: &Value, keys: &[&str]) -> Result<Option<String>, String> {
    for key in keys {
        let Some(value) = find_value(object, &[*key]) else {
            continue;
        };
        let Some(text) = value.as_str() else {
            return Err(format!("{key} must be a string"));
        };
        if text.len() > MAX_STRING_BYTES {
            return Err(format!("{key} is too large"));
        }
        return Ok(Some(text.to_string()));
    }
    Ok(None)
}

fn read_optional_string(object: &Value, key: &str) -> Result<Option<String>, String> {
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(text) = value.as_str() else {
        return Err(format!("{key} must be a string"));
    };
    if text.len() > MAX_STRING_BYTES {
        return Err(format!("{key} is too large"));
    }
    Ok(Some(text.to_string()))
}

const TITLE_KEYS: &[&str] = &[
    "session-title",
    "sessionTitle",
    "thread-title",
    "threadTitle",
    "session-name",
    "sessionName",
    "thread-name",
    "threadName",
    "title",
];

const DESCRIPTION_KEYS: &[&str] = &[
    "description",
    "summary",
    "preview",
    "message",
    "body",
    "content",
    "text",
];

/// 部分客户端把会话标题以 JSON 对象形式塞进最终回复文本；
/// 只解析边界完整的对象，且只提取白名单字段。
fn embedded_session_title(object: &Value, depth: u32) -> Option<(String, String)> {
    if let Some(title) = find_value(object, TITLE_KEYS)
        .and_then(Value::as_str)
        .filter(|t| !t.trim().is_empty())
    {
        let description = find_value(object, DESCRIPTION_KEYS)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        return Some((title.to_string(), description));
    }
    if depth >= 2 {
        return None;
    }
    for key in ["result", "response", "data", "message", "content"] {
        if let Some(nested) = object.get(key).filter(|v| v.is_object())
            && let Some(found) = embedded_session_title(nested, depth + 1)
        {
            return Some(found);
        }
    }
    None
}

fn parse_embedded_session_title(message: &str) -> Option<(String, String)> {
    let trimmed = message.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_STRING_BYTES
        || !trimmed.starts_with('{')
        || !trimmed.ends_with('}')
    {
        return None;
    }
    let value: Value = serde_json::from_str(trimmed).ok()?;
    if !value.is_object() {
        return None;
    }
    embedded_session_title(&value, 0)
}

/// 解析通知负载。Err 为输入校验失败（调用方应返回 400 级错误）。
pub fn parse_activity(object: &Value) -> Result<ParseOutcome, String> {
    if !object.is_object() {
        return Err("payload must be an object".to_string());
    }
    let compact = crate::json::to_compact_string(object);
    if compact.len() > NOTIFY_MAX_BYTES {
        return Err("Codex notification JSON is too large".to_string());
    }

    let mut activity = CodexActivity::default();
    let type_text = object
        .get("type")
        .and_then(Value::as_str)
        .or_else(|| object.get("method").and_then(Value::as_str))
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| "type must be a non-empty string".to_string())?;
    activity.event_type = type_text.to_string();

    // 未识别的事件类型直接吞掉：新版 Codex 新增事件不应击破旧版。
    let new_session = is_new_session_event(&activity.event_type);
    if activity.event_type != "agent-turn-complete" && !new_session {
        return Ok(ParseOutcome {
            activity,
            recognized: false,
        });
    }

    for (key, target) in [
        ("thread-id", &mut activity.thread_id),
        ("turn-id", &mut activity.turn_id),
        ("cwd", &mut activity.cwd),
        (
            "last-assistant-message",
            &mut activity.last_assistant_message,
        ),
    ] {
        *target = read_optional_string(object, key)?.unwrap_or_default();
    }

    if new_session {
        let title = read_aliased_string(
            object,
            &[
                "title",
                "session-title",
                "sessionTitle",
                "thread-title",
                "threadTitle",
                "thread-name",
                "threadName",
                "session-name",
                "sessionName",
                "name",
            ],
        )?;
        if title.as_deref().is_none_or(|t| t.trim().is_empty()) {
            return Err("new session notification requires a title".to_string());
        }
        activity.session_title = title.unwrap_or_default();
        activity.session_description = read_aliased_string(
            object,
            &[
                "description",
                "summary",
                "session-description",
                "sessionDescription",
                "preview",
                "message",
                "body",
                "content",
            ],
        )?
        .unwrap_or_default();
        activity.is_new_session = true;
        // 部分版本把描述放在完成字段里。
        if activity.session_description.is_empty() {
            activity.session_description = activity.last_assistant_message.clone();
        }
    } else if activity.event_type == "agent-turn-complete" {
        // 完成事件附带标题元数据时按新会话处理。
        let title = read_aliased_string(
            object,
            &[
                "title",
                "session-title",
                "sessionTitle",
                "thread-title",
                "threadTitle",
                "thread-name",
                "threadName",
            ],
        )?;
        if title.as_deref().is_some_and(|t| !t.trim().is_empty()) {
            activity.session_title = title.unwrap_or_default();
            activity.session_description = read_aliased_string(
                object,
                &[
                    "description",
                    "summary",
                    "preview",
                    "message",
                    "body",
                    "content",
                ],
            )?
            .unwrap_or_default();
            if activity.session_description.is_empty() {
                activity.session_description = activity.last_assistant_message.clone();
            }
            activity.is_new_session = true;
        }

        // 官方 notify 没有独立标题事件；回复文本里嵌的 JSON 标题对象
        // 转成标题/摘要展示，避免协议 JSON 直接上气泡。
        if !activity.is_new_session
            && let Some((title, description)) =
                parse_embedded_session_title(&activity.last_assistant_message)
        {
            activity.session_title = title.trim().to_string();
            activity.session_description = description.trim().to_string();
            activity.last_assistant_message = if activity.session_description.is_empty() {
                activity.session_title.clone()
            } else {
                activity.session_description.clone()
            };
            activity.is_new_session = true;
        }
    }

    if let Some(messages) = object.get("input-messages")
        && !messages.is_null()
    {
        let Some(items) = messages.as_array() else {
            return Err("input-messages must be an array".to_string());
        };
        if items.len() > MAX_MESSAGES {
            return Err("input-messages contains too many entries".to_string());
        }
        for item in items {
            if !item.is_string() && !item.is_object() {
                return Err("input-messages entries must be strings or objects".to_string());
            }
            if let Some(text) = item.as_str() {
                if text.len() > MAX_STRING_BYTES {
                    return Err("input-messages entry is too large".to_string());
                }
                activity.input_messages.push(text.to_string());
            }
        }
    }
    if activity.is_new_session
        && activity.session_description.is_empty()
        && !activity.input_messages.is_empty()
    {
        activity.session_description = activity.input_messages.join("\n");
    }
    activity.state = ActivityState::Ready;
    Ok(ParseOutcome {
        activity,
        recognized: true,
    })
}

/// 归一化气泡文本：统一换行、压缩三连换行、去首尾空白。
pub fn normalize_bubble_text(text: &str) -> String {
    let mut normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    while normalized.contains("\n\n\n") {
        normalized = normalized.replace("\n\n\n", "\n\n");
    }
    normalized.trim().to_string()
}

/// 压缩气泡源文本：超过上限时保留开头并追加省略号。
/// 保留开头是有意的：尾部可能携带工具协议片段，脱离上下文展示有风险。
pub fn compact_bubble_source(text: &str, max_retained: usize) -> String {
    let normalized = normalize_bubble_text(text);
    if max_retained == 0 || normalized.is_empty() {
        return String::new();
    }
    if normalized.chars().count() <= max_retained {
        return normalized;
    }
    let kept: String = normalized.chars().take(max_retained.max(1)).collect();
    format!("{}…", kept.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unrecognized_event_is_passed_through() {
        let outcome = parse_activity(&json!({"type": "some-future-event"})).unwrap();
        assert!(!outcome.recognized);
        assert_eq!(outcome.activity.event_type, "some-future-event");
    }

    #[test]
    fn turn_complete_is_recognized() {
        let outcome = parse_activity(&json!({
            "type": "agent-turn-complete",
            "thread-id": "t1",
            "turn-id": "u1",
            "last-assistant-message": "done",
        }))
        .unwrap();
        assert!(outcome.recognized);
        assert!(!outcome.activity.is_new_session);
        assert_eq!(outcome.activity.last_assistant_message, "done");
    }

    #[test]
    fn new_session_requires_title() {
        assert!(parse_activity(&json!({"type": "session-title-updated"})).is_err());
        let outcome = parse_activity(&json!({
            "type": "session-title-updated",
            "title": "My session",
        }))
        .unwrap();
        assert!(outcome.activity.is_new_session);
        assert_eq!(outcome.activity.session_title, "My session");
    }

    #[test]
    fn embedded_title_object_is_unwrapped() {
        let outcome = parse_activity(&json!({
            "type": "agent-turn-complete",
            "last-assistant-message": "{\"title\": \"T\", \"description\": \"D\"}",
        }))
        .unwrap();
        assert!(outcome.activity.is_new_session);
        assert_eq!(outcome.activity.session_title, "T");
        assert_eq!(outcome.activity.session_description, "D");
    }

    #[test]
    fn method_alias_and_underscore_normalization() {
        let outcome =
            parse_activity(&json!({"method": "Session_Title_Updated", "title": "x"})).unwrap();
        assert!(outcome.recognized);
        assert!(outcome.activity.is_new_session);
    }

    #[test]
    fn compact_truncates_with_ellipsis() {
        let long = "a".repeat(5000);
        let short = compact_bubble_source(&long, MAX_RETAINED_CHARS);
        assert!(short.ends_with('…'));
        assert!(short.chars().count() <= MAX_RETAINED_CHARS + 1);
    }
}
