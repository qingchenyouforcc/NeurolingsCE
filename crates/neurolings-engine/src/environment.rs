//! 桌宠所处的屏幕环境：边界、工作区、活动窗口区域与全局物理参数。

use std::cell::RefCell;
use std::rc::Rc;

use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::broadcast::BroadcastManager;
use crate::math::{Rec, Vec2};

/// 附带帧间位移增量的二维坐标，用于追踪光标或窗口的移动速度。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DVec2 {
    pub x: f64,
    pub y: f64,
    pub dx: f64,
    pub dy: f64,
}

impl DVec2 {
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            dx: 0.0,
            dy: 0.0,
        }
    }
    pub fn with_delta(x: f64, y: f64, dx: f64, dy: f64) -> Self {
        Self { x, y, dx, dy }
    }
    pub fn move_to(&mut self, new_pos: Vec2) {
        self.dx += new_pos.x - self.x;
        self.dy += new_pos.y - self.y;
        self.x = new_pos.x;
        self.y = new_pos.y;
    }
    pub fn as_vec2(&self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }
}

impl MulScale for DVec2 {
    fn mul_scale(&self, rhs: f64) -> Self {
        Self::with_delta(self.x * rhs, self.y * rhs, self.dx * rhs, self.dy * rhs)
    }
}

/// 支持按缩放系数等比缩放。
pub trait MulScale {
    fn mul_scale(&self, rhs: f64) -> Self;
}

/// 边界判定：锚点是否贴附于边界、是否位于边界的覆盖范围内。
pub trait Border: Copy {
    fn is_on(&self, p: Vec2) -> bool;
    fn faces(&self, p: Vec2) -> bool;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HBorder {
    pub y: f64,
    pub xstart: f64,
    pub xend: f64,
}

impl HBorder {
    pub fn new(y: f64, xstart: f64, xend: f64) -> Self {
        Self { y, xstart, xend }
    }
}

impl Border for HBorder {
    fn faces(&self, p: Vec2) -> bool {
        p.x >= self.xstart && p.x <= self.xend
    }
    fn is_on(&self, p: Vec2) -> bool {
        (p.y - self.y).abs() < 1.0 && self.faces(p)
    }
}

impl MulScale for HBorder {
    fn mul_scale(&self, rhs: f64) -> Self {
        Self::new(self.y * rhs, self.xstart * rhs, self.xend * rhs)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct VBorder {
    pub x: f64,
    pub ystart: f64,
    pub yend: f64,
}

impl VBorder {
    pub fn new(x: f64, ystart: f64, yend: f64) -> Self {
        Self { x, ystart, yend }
    }
}

impl Border for VBorder {
    fn faces(&self, p: Vec2) -> bool {
        p.y >= self.ystart && p.y <= self.yend
    }
    fn is_on(&self, p: Vec2) -> bool {
        (p.x - self.x).abs() < 1.0 && self.faces(p)
    }
}

impl MulScale for VBorder {
    fn mul_scale(&self, rhs: f64) -> Self {
        Self::new(self.x * rhs, self.ystart * rhs, self.yend * rhs)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Area {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Area {
    pub fn new(top: f64, right: f64, bottom: f64, left: f64) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
    pub fn from_rec(rec: Rec) -> Self {
        Self::new(rec.y, rec.x + rec.width, rec.y + rec.height, rec.x)
    }
    pub fn from_vec2(v: Vec2) -> Self {
        Self::from_rec(Rec::new(0.0, 0.0, v.x, v.y))
    }
    pub fn visible(&self) -> bool {
        self.left != self.right && self.top != self.bottom
    }
    pub fn bottom_border(&self) -> HBorder {
        HBorder::new(self.bottom, self.left, self.right)
    }
    pub fn top_border(&self) -> HBorder {
        HBorder::new(self.top, self.left, self.right)
    }
    pub fn left_border(&self) -> VBorder {
        VBorder::new(self.left, self.top, self.bottom)
    }
    pub fn right_border(&self) -> VBorder {
        VBorder::new(self.right, self.top, self.bottom)
    }
    pub fn width(&self) -> f64 {
        self.right - self.left
    }
    pub fn height(&self) -> f64 {
        self.bottom - self.top
    }
    pub fn is_on(&self, anchor: Vec2) -> bool {
        self.left_border().is_on(anchor)
            || self.right_border().is_on(anchor)
            || self.bottom_border().is_on(anchor)
            || self.top_border().is_on(anchor)
    }
}

impl MulScale for Area {
    fn mul_scale(&self, rhs: f64) -> Self {
        Self::new(
            self.top * rhs,
            self.right * rhs,
            self.bottom * rhs,
            self.left * rhs,
        )
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DArea {
    pub area: Area,
    pub dx: f64,
    pub dy: f64,
    pub left_dx: f64,
    pub right_dx: f64,
    pub top_dy: f64,
    pub bottom_dy: f64,
}

impl DArea {
    pub fn new(top: f64, right: f64, bottom: f64, left: f64, dx: f64, dy: f64) -> Self {
        Self {
            area: Area::new(top, right, bottom, left),
            dx,
            dy,
            left_dx: dx,
            right_dx: dx,
            top_dy: dy,
            bottom_dy: dy,
        }
    }
    pub fn set_edge_offsets(&mut self, left_dx: f64, right_dx: f64, top_dy: f64, bottom_dy: f64) {
        self.left_dx = left_dx;
        self.right_dx = right_dx;
        self.top_dy = top_dy;
        self.bottom_dy = bottom_dy;
        self.dx = if right_dx.abs() > left_dx.abs() {
            right_dx
        } else {
            left_dx
        };
        self.dy = if bottom_dy.abs() > top_dy.abs() {
            bottom_dy
        } else {
            top_dy
        };
    }
    pub fn visible(&self) -> bool {
        self.area.visible()
    }
    pub fn is_on(&self, anchor: Vec2) -> bool {
        self.area.is_on(anchor)
    }
    pub fn top_border(&self) -> HBorder {
        self.area.top_border()
    }
    pub fn bottom_border(&self) -> HBorder {
        self.area.bottom_border()
    }
    pub fn left_border(&self) -> VBorder {
        self.area.left_border()
    }
    pub fn right_border(&self) -> VBorder {
        self.area.right_border()
    }
}

impl From<Area> for DArea {
    fn from(area: Area) -> Self {
        Self {
            area,
            ..Default::default()
        }
    }
}

impl MulScale for DArea {
    fn mul_scale(&self, rhs: f64) -> Self {
        Self {
            area: self.area.mul_scale(rhs),
            dx: self.dx * rhs,
            dy: self.dy * rhs,
            left_dx: self.left_dx * rhs,
            right_dx: self.right_dx * rhs,
            top_dy: self.top_dy * rhs,
            bottom_dy: self.bottom_dy * rhs,
        }
    }
}

pub type WindowPushCallback = Box<dyn FnMut(f64, f64) -> bool>;

pub struct Environment {
    pub ceiling: HBorder,
    pub floor: HBorder,
    pub screen: Area,
    pub work_area: Area,
    pub active_ie: DArea,
    pub cursor: DVec2,
    pub allows_breeding: bool,
    pub allows_hotspots: bool,
    pub allows_window_pushing: bool,
    pub window_push_callback: Option<WindowPushCallback>,
    pub mascot_count: i64,
    pub sticky_ie: bool,
    pub subtick_count: i32,
    active_scale: f64,
    rng: StdRng,
    pub broadcasts: Rc<RefCell<BroadcastManager>>,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            ceiling: HBorder::default(),
            floor: HBorder::default(),
            screen: Area::default(),
            work_area: Area::default(),
            active_ie: DArea::default(),
            cursor: DVec2::default(),
            allows_breeding: true,
            allows_hotspots: true,
            allows_window_pushing: false,
            window_push_callback: None,
            mascot_count: 0,
            sticky_ie: true,
            subtick_count: 1,
            active_scale: 1.0,
            rng: StdRng::from_os_rng(),
            broadcasts: Rc::new(RefCell::new(BroadcastManager::default())),
        }
    }
}

impl Environment {
    pub fn request_window_push(&mut self, dx: f64, dy: f64) -> bool {
        if !self.allows_window_pushing || !self.active_ie.visible() {
            return false;
        }
        match &mut self.window_push_callback {
            None => false,
            Some(cb) => cb(dx, dy),
        }
    }

    /// 返回 [0.0, 1.0) 区间内的随机浮点数。
    pub fn random(&mut self) -> f64 {
        self.rng.random::<f64>()
    }

    /// 返回 [0, upper_range) 区间内的随机整数。
    pub fn random_int(&mut self, upper_range: i32) -> i32 {
        self.rng.random_range(0..upper_range)
    }

    pub fn seed(&mut self, seed: u64) {
        self.rng = StdRng::seed_from_u64(seed);
    }

    pub fn get_scale(&self) -> f64 {
        self.active_scale
    }

    pub fn sanitized_subtick_count(&mut self) -> i32 {
        const FALLBACK: i32 = 4;
        const MAX: i32 = 120;
        if self.subtick_count < 1 || self.subtick_count > MAX {
            self.subtick_count = FALLBACK;
        }
        self.subtick_count
    }

    pub fn reset_scale(&mut self) {
        if self.active_scale == 1.0 {
            return;
        }
        if !self.active_scale.is_finite() || self.active_scale <= 0.0 {
            self.active_scale = 1.0;
            return;
        }
        let s = self.active_scale;
        self.ceiling = self.ceiling.mul_scale(1.0 / s);
        self.floor = self.floor.mul_scale(1.0 / s);
        self.screen = self.screen.mul_scale(1.0 / s);
        self.work_area = self.work_area.mul_scale(1.0 / s);
        self.active_ie = self.active_ie.mul_scale(1.0 / s);
        self.cursor = self.cursor.mul_scale(1.0 / s);
        self.active_scale = 1.0;
    }

    pub fn set_scale(&mut self, scale: f64) {
        let scale = if !scale.is_finite() || scale <= 0.0 {
            1.0
        } else {
            scale
        };
        if self.active_scale != 1.0 {
            self.reset_scale();
        }
        self.ceiling = self.ceiling.mul_scale(scale);
        self.floor = self.floor.mul_scale(scale);
        self.screen = self.screen.mul_scale(scale);
        self.work_area = self.work_area.mul_scale(scale);
        self.active_ie = self.active_ie.mul_scale(scale);
        self.cursor = self.cursor.mul_scale(scale);
        self.active_scale = scale;
    }
}
