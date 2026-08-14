//! 动画数据：姿态序列、hotspot 列表与播放时长的解析结果。

use crate::hotspot::Hotspot;
use crate::math::Vec2;
use crate::pose::Pose;
use crate::scripting::condition::Condition;

/// 一段可播放的动画：按顺序循环播放姿态，duration 为总帧数。
#[derive(Debug, Clone)]
pub struct Animation {
    pub poses: Vec<Pose>,
    pub hotspots: Vec<Hotspot>,
    pub duration: i32,
    pub condition: Condition,
}

impl Animation {
    pub fn new(poses: Vec<Pose>, hotspots: Vec<Hotspot>) -> Self {
        let duration = poses.iter().map(|p| p.duration).sum();
        Self {
            poses,
            hotspots,
            duration,
            condition: Condition::from(true),
        }
    }
    /// 取第 time 帧对应的姿态（0 起计，超出总长则循环）。
    pub fn get_pose(&self, time: i32) -> &Pose {
        let mut time = time % self.duration;
        for pose in &self.poses {
            time -= pose.duration;
            if time < 0 {
                return pose;
            }
        }
        unreachable!("impossible condition");
    }
    pub fn hotspot_at(&self, offset: Vec2) -> Option<&Hotspot> {
        self.hotspots.iter().find(|h| h.point_inside(offset))
    }
}
