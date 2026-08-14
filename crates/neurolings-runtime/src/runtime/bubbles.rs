//! 语气泡与 Codex 通知气泡：文案目录、显示时长、定位与生命周期。
//!
//! 文案来源优先级：桌宠包 bubble_context.txt → 用户 bubbles.txt →
//! 内置默认列表 → 兜底短句。气泡窗口懒创建并复用。

use std::time::{Duration, Instant};

use neurolings_platform::{MascotBackend, Point, bubble};

use crate::runtime::session::Session;

/// 普通语气泡显示时长。
const BUBBLE_TTL: Duration = Duration::from_millis(3000);
/// Codex 气泡的基础/最大时长与按字数的增量。
const CODEX_BASE: Duration = Duration::from_millis(8000);
const CODEX_MAX: Duration = Duration::from_millis(12000);
const CODEX_FREE_CHARS: usize = 80;
const CODEX_MS_PER_CHAR: u64 = 25;
/// 气泡窗口的虚拟 id 起点（与真实桌宠 id 隔开）。
pub const BUBBLE_ID_BASE: u64 = 1_000_000;

/// 内置默认文案（随二进制分发）。
const EMBEDDED_TEXTS: &str = include_str!("../../../../assets/bubbles.txt");

/// 按 grapheme 数计算的 Codex 气泡显示时长。
pub fn codex_display_duration(retained_chars: usize) -> Duration {
    let extra_ms = retained_chars.saturating_sub(CODEX_FREE_CHARS) as u64 * CODEX_MS_PER_CHAR;
    let extra = Duration::from_millis(extra_ms).min(CODEX_MAX - CODEX_BASE);
    CODEX_BASE + extra
}

/// 加载桌宠的语气泡文案。
pub fn load_bubble_texts(
    pack_dir: &std::path::Path,
    app_data_dir: &std::path::Path,
) -> Vec<String> {
    let from_file = |path: &std::path::Path| -> Vec<String> {
        std::fs::read_to_string(path)
            .map(|text| {
                text.lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };

    let pack_texts = from_file(&pack_dir.join("bubble_context.txt"));
    if !pack_texts.is_empty() {
        return pack_texts;
    }
    let user_texts = from_file(&app_data_dir.join("bubbles.txt"));
    if !user_texts.is_empty() {
        return user_texts;
    }
    let embedded: Vec<String> = EMBEDDED_TEXTS
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    if !embedded.is_empty() {
        return embedded;
    }
    vec!["Hello!".into(), "Hi there~".into(), "(^_^)".into()]
}

/// 从文案列表随机取一条。
pub fn random_bubble_text(texts: &[String], roll: usize) -> String {
    if texts.is_empty() {
        return "Hello!".to_string();
    }
    texts[roll % texts.len()].clone()
}

/// 每帧处理所有会话的气泡：更新文本、跟随桌宠移动、回收过期窗口。
pub fn process_bubbles(
    sessions: &mut [Session],
    backend: &mut Option<&mut Box<dyn MascotBackend>>,
) {
    let now = Instant::now();

    for session in sessions.iter_mut() {
        let codex = session.pending_codex_bubble.take();
        let plain = session.pending_bubble.take();
        let incoming = match (codex, plain) {
            (Some((title, body, ttl)), _) => Some((title, body, ttl)),
            (None, Some(text)) => Some((String::new(), text, BUBBLE_TTL)),
            (None, None) => None,
        };

        if let Some((_title, body, ttl)) = incoming {
            if let Ok((bitmap, width, height)) = bubble::render_bubble(&body, 260) {
                session.bubble_bitmap = Some(bitmap.clone());
                if session.bubble_window.is_none() {
                    match backend {
                        Some(b) => match b.create_window(BUBBLE_ID_BASE + session.id) {
                            Ok(window) => session.bubble_window = Some(window),
                            Err(_) => continue,
                        },
                        None => continue,
                    }
                }
                session.bubble_size = (width, height);
                // 气泡底部对齐桌宠窗口顶部，水平居中。
                let center_x = session.window_top_left.x + session.frame_size.0 as i32 / 2;
                let top_left = Point::new(
                    center_x - width as i32 / 2,
                    session.window_top_left.y - height as i32,
                );
                if let Some(window) = &mut session.bubble_window {
                    let _ = window.update_frame(&bitmap, width, height, top_left);
                }
                session.bubble_until = now + ttl;
            }
        } else if session.bubble_window.is_some() && now < session.bubble_until {
            // 跟随桌宠移动（位图不变，只更新位置）。
            let (bw, bh) = session.bubble_size;
            let center_x = session.window_top_left.x + session.frame_size.0 as i32 / 2;
            let top_left = Point::new(
                center_x - bw as i32 / 2,
                session.window_top_left.y - bh as i32,
            );
            if let Some(window) = &mut session.bubble_window
                && let Some(bitmap) = session.bubble_bitmap.clone()
            {
                let _ = window.update_frame(&bitmap, bw, bh, top_left);
            }
        }

        // 过期回收。
        if session.bubble_window.is_some() && now >= session.bubble_until {
            session.bubble_window = None;
        }
    }
}
