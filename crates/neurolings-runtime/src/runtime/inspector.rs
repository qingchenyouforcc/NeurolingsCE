//! 检查器：把单只桌宠的运行时状态格式化为可读文本。

use crate::runtime::session::Session;

/// 生成检查器文本（行为、动作、帧、环境快照）。
pub fn inspect_text(session: &Session) -> String {
    let state = session.manager.state.borrow();
    let behavior = state
        .behavior
        .as_ref()
        .map(|b| b.dereferenced().name.clone())
        .unwrap_or_else(|| "-".to_string());
    let env_lines = match &state.env {
        Some(env) => {
            let e = env.borrow();
            format!(
                "Floor: y={:.0} [{:.0}, {:.0}]\n\
                 Work area: ({:.0}, {:.0}) - ({:.0}, {:.0})\n\
                 Active window: ({:.0}, {:.0}) - ({:.0}, {:.0})\n\
                 Cursor: ({:.0}, {:.0}) d=({:.0}, {:.0})\n\
                 Mascots in environment: {}",
                e.floor.y,
                e.floor.xstart,
                e.floor.xend,
                e.work_area.left,
                e.work_area.top,
                e.work_area.right,
                e.work_area.bottom,
                e.active_ie.area.left,
                e.active_ie.area.top,
                e.active_ie.area.right,
                e.active_ie.area.bottom,
                e.cursor.x,
                e.cursor.y,
                e.cursor.dx,
                e.cursor.dy,
                e.mascot_count,
            )
        }
        None => "Environment: -".to_string(),
    };
    format!(
        "Name: {}\n\
         Runtime ID: {}\n\
         Template ID: {}\n\
         Behavior: {}\n\
         Anchor: ({:.1}, {:.1})\n\
         Frame: {}\n\
         Sound: {}\n\
         Looking right: {}\n\
         Dragging: {}\n\
         Time: {}\n\
         ---\n\
         {}",
        session.name,
        session.id,
        session.data_id,
        behavior,
        state.anchor.x,
        state.anchor.y,
        state.active_frame.name,
        if state.active_sound.is_empty() {
            "-".to_string()
        } else {
            state.active_sound.clone()
        },
        state.looking_right,
        state.dragging,
        state.time,
        env_lines,
    )
}
