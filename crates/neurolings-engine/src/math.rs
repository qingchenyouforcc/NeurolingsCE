//! 基础几何类型：向量与矩形，及宽松字符串解析。

use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rec {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rec {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

impl Mul<f64> for Rec {
    type Output = Rec;
    fn mul(self, rhs: f64) -> Rec {
        Rec::new(
            self.x * rhs,
            self.y * rhs,
            self.width * rhs,
            self.height * rhs,
        )
    }
}
impl MulAssign<f64> for Rec {
    fn mul_assign(&mut self, rhs: f64) {
        *self = *self * rhs;
    }
}
impl Div<f64> for Rec {
    type Output = Rec;
    fn div(self, rhs: f64) -> Rec {
        Rec::new(
            self.x / rhs,
            self.y / rhs,
            self.width / rhs,
            self.height / rhs,
        )
    }
}
impl DivAssign<f64> for Rec {
    fn div_assign(&mut self, rhs: f64) {
        *self = *self / rhs;
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    /// 解析 "x,y" 格式；任一部分无法解析时得到 (0, 0)。
    pub fn from_str_lenient(s: &str) -> Self {
        match s.split_once(',') {
            None => Vec2::ZERO,
            Some((a, b)) => match (parse_f64_prefix(a), parse_f64_prefix(b)) {
                (Some(x), Some(y)) => Vec2 { x, y },
                _ => Vec2::ZERO,
            },
        }
    }
    /// 校验字符串是否为合法的 "x,y" 坐标。
    pub fn validate_str(s: &str) -> bool {
        match s.split_once(',') {
            None => false,
            Some((a, b)) => parse_f64_prefix(a).is_some() && parse_f64_prefix(b).is_some(),
        }
    }
}

impl From<&str> for Vec2 {
    fn from(s: &str) -> Self {
        Vec2::from_str_lenient(s)
    }
}

impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, rhs: f64) -> Vec2 {
        Vec2::new(self.x * rhs, self.y * rhs)
    }
}
impl MulAssign<f64> for Vec2 {
    fn mul_assign(&mut self, rhs: f64) {
        *self = *self * rhs;
    }
}
impl Div<f64> for Vec2 {
    type Output = Vec2;
    fn div(self, rhs: f64) -> Vec2 {
        Vec2::new(self.x / rhs, self.y / rhs)
    }
}
impl DivAssign<f64> for Vec2 {
    fn div_assign(&mut self, rhs: f64) {
        *self = *self / rhs;
    }
}
impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x + rhs.x, self.y + rhs.y)
    }
}
impl AddAssign for Vec2 {
    fn add_assign(&mut self, rhs: Vec2) {
        *self = *self + rhs;
    }
}
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}
impl SubAssign for Vec2 {
    fn sub_assign(&mut self, rhs: Vec2) {
        *self = *self - rhs;
    }
}

/// 解析浮点数前缀：跳过前导空白，接受符号、小数与指数部分；
/// 完全无法解析时返回 None。
fn parse_f64_prefix(s: &str) -> Option<f64> {
    let s = s.trim_start();
    let bytes = s.as_bytes();
    let mut end = 0;
    if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
        end += 1;
    }
    let mut digits = 0;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
        digits += 1;
    }
    if end < bytes.len() && bytes[end] == b'.' {
        end += 1;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
            digits += 1;
        }
    }
    if digits > 0 && end < bytes.len() && (bytes[end] == b'e' || bytes[end] == b'E') {
        let mut exp_end = end + 1;
        if exp_end < bytes.len() && (bytes[exp_end] == b'+' || bytes[exp_end] == b'-') {
            exp_end += 1;
        }
        let exp_digits_start = exp_end;
        while exp_end < bytes.len() && bytes[exp_end].is_ascii_digit() {
            exp_end += 1;
        }
        if exp_end > exp_digits_start {
            end = exp_end;
        }
    }
    if digits == 0 {
        return None;
    }
    s[..end].parse::<f64>().ok()
}
