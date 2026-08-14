//! 下落穿透追踪：连续下落超过 700px 仍未落地的桌宠会绕过任务栏地板，
//! 一直落到屏幕绝对底部。

use neurolings_engine::environment::Environment;

/// 触发穿透模式所需的下落距离（像素）。
pub const FALL_THROUGH_DISTANCE: f64 = 700.0;

/// 穿透帧覆盖地板期间保存的环境原值。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FallThroughOverride {
    pub floor_y: f64,
    pub work_area_bottom: f64,
}

/// 单只桌宠的下落穿透状态机。
#[derive(Debug, Default)]
pub struct FallThroughTracker {
    fall_tracking: bool,
    fall_start_y: f64,
    fall_through_mode: bool,
}

impl FallThroughTracker {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn fall_through_mode(&self) -> bool {
        self.fall_through_mode
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn fall_tracking(&self) -> bool {
        self.fall_tracking
    }

    /// 穿透模式激活时，本帧绕过任务栏地板；返回恢复环境所需的
    /// 保存值。
    pub fn apply_env_override(&self, env: &mut Environment) -> Option<FallThroughOverride> {
        if !self.fall_through_mode {
            return None;
        }
        let saved = FallThroughOverride {
            floor_y: env.floor.y,
            work_area_bottom: env.work_area.bottom,
        };
        env.floor.y = env.screen.bottom;
        env.work_area.bottom = env.screen.bottom;
        Some(saved)
    }

    /// 恢复被穿透帧覆盖的环境值。
    pub fn restore_env_override(env: &mut Environment, saved: FallThroughOverride) {
        env.floor.y = saved.floor_y;
        env.work_area.bottom = saved.work_area_bottom;
    }

    /// 被拖拽时重置穿透状态。
    pub fn reset_if_dragged(&mut self, dragging: bool) {
        if !dragging || !self.fall_through_mode {
            return;
        }
        self.fall_through_mode = false;
        self.fall_tracking = false;
    }

    /// 观察下落进度，累计距离并在达标时开启穿透。
    pub fn observe(&mut self, on_land: bool, dragging: bool, y_before: f64, y_after: f64) {
        if !on_land && !dragging && y_after > y_before {
            if !self.fall_tracking {
                self.fall_tracking = true;
                self.fall_start_y = y_before;
            }
            if y_after - self.fall_start_y >= FALL_THROUGH_DISTANCE {
                self.fall_through_mode = true;
            }
        } else {
            self.fall_tracking = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neurolings_engine::environment::{Area, HBorder};

    fn env_with_taskbar() -> Environment {
        let mut env = Environment::default();
        env.screen = Area::new(0.0, 1920.0, 1080.0, 0.0);
        env.work_area = Area::new(0.0, 1920.0, 1040.0, 0.0);
        env.floor = HBorder::new(1040.0, 0.0, 1080.0);
        env
    }

    #[test]
    fn short_falls_do_not_enable_fall_through() {
        let mut tracker = FallThroughTracker::new();
        tracker.observe(false, false, 0.0, 300.0);
        tracker.observe(false, false, 300.0, 699.9);
        assert!(!tracker.fall_through_mode());
        tracker.observe(false, false, 699.9, 700.0);
        assert!(tracker.fall_through_mode());
    }

    #[test]
    fn landing_resets_tracking() {
        let mut tracker = FallThroughTracker::new();
        tracker.observe(false, false, 0.0, 500.0);
        tracker.observe(true, false, 500.0, 500.0); // 已落地
        assert!(!tracker.fall_tracking());
        // 新一轮下落从落地点重新累计。
        tracker.observe(false, false, 500.0, 1100.0);
        assert!(!tracker.fall_through_mode());
        tracker.observe(false, false, 1100.0, 1201.0);
        assert!(tracker.fall_through_mode());
    }

    #[test]
    fn dragging_blocks_activation_but_reset_requires_mode() {
        let mut tracker = FallThroughTracker::new();
        tracker.observe(false, true, 0.0, 900.0);
        assert!(!tracker.fall_through_mode());
        assert!(!tracker.fall_tracking());
    }

    #[test]
    fn reset_if_dragged_clears_active_mode() {
        let mut tracker = FallThroughTracker::new();
        tracker.observe(false, false, 0.0, 800.0);
        assert!(tracker.fall_through_mode());
        tracker.reset_if_dragged(true);
        assert!(!tracker.fall_through_mode());
        assert!(!tracker.fall_tracking());
        // 拖拽清除后需要重新累积 700px 下落才会再次激活。
        tracker.observe(false, false, 800.0, 810.0);
        assert!(!tracker.fall_through_mode());
        tracker.observe(false, false, 810.0, 1520.0);
        assert!(tracker.fall_through_mode());
        // 未拖拽时 reset 不改变状态。
        tracker.reset_if_dragged(false);
        assert!(tracker.fall_through_mode());
    }

    #[test]
    fn env_override_bypasses_taskbar_floor_and_restores() {
        let mut tracker = FallThroughTracker::new();
        let mut env = env_with_taskbar();

        // 未激活：不覆盖。
        assert!(tracker.apply_env_override(&mut env).is_none());
        assert_eq!(env.floor.y, 1040.0);

        tracker.observe(false, false, 0.0, 900.0);
        let saved = tracker
            .apply_env_override(&mut env)
            .expect("override active");
        assert_eq!(env.floor.y, 1080.0);
        assert_eq!(env.work_area.bottom, 1080.0);
        assert_eq!(saved.floor_y, 1040.0);
        assert_eq!(saved.work_area_bottom, 1040.0);

        FallThroughTracker::restore_env_override(&mut env, saved);
        assert_eq!(env.floor.y, 1040.0);
        assert_eq!(env.work_area.bottom, 1040.0);
    }

    #[test]
    fn fall_distance_accumulates_across_ticks_from_first_observed_start() {
        let mut tracker = FallThroughTracker::new();
        // 小步增量：从首个下落样本开始累计。
        let mut y = 0.0;
        for _ in 0..70 {
            let next = y + 10.0;
            tracker.observe(false, false, y, next);
            y = next;
        }
        assert!(tracker.fall_through_mode());
    }
}
