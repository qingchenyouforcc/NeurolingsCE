//! 姿态与帧：单帧图像（含右朝向变体、音效、锚点）与带速度/时长的姿态。

use crate::math::Vec2;

#[derive(Debug, Clone, Default)]
pub struct Frame {
    pub visible: bool,
    pub name: String,
    pub right_name: String,
    pub sound: String,
    pub anchor: Vec2,
}

impl Frame {
    pub fn new(name: String, right_name: String, sound: String, anchor: Vec2) -> Self {
        Self {
            visible: true,
            name,
            right_name,
            sound,
            anchor,
        }
    }
    pub fn get_name(&self, right: bool) -> &str {
        if right && !self.right_name.is_empty() {
            &self.right_name
        } else {
            &self.name
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Pose {
    pub frame: Frame,
    pub velocity: Vec2,
    pub duration: i32,
}
