//! 可点击区域：椭圆或矩形，命中后可触发指定行为。

use crate::math::Vec2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shape {
    #[default]
    Invalid = 0,
    Ellipse = 1,
    Rectangle = 2,
}

impl Shape {
    pub fn from_name(name: &str) -> Shape {
        match name {
            "Ellipse" => Shape::Ellipse,
            "Rectangle" => Shape::Rectangle,
            _ => Shape::Invalid,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Hotspot {
    pub shape: Shape,
    pub origin: Vec2,
    pub size: Vec2,
    pub behavior: String,
}

impl Hotspot {
    pub fn new(shape: Shape, origin: Vec2, size: Vec2, behavior: String) -> Self {
        Self {
            shape,
            origin,
            size,
            behavior,
        }
    }
    pub fn valid(&self) -> bool {
        self.shape != Shape::Invalid
    }
    pub fn point_inside(&self, point: Vec2) -> bool {
        match self.shape {
            Shape::Ellipse => {
                ((point.x - (self.origin.x + self.size.x / 2.0)) / self.size.x).powi(2)
                    + ((point.y - (self.origin.y + self.size.y / 2.0)) / self.size.y).powi(2)
                    < 1.0
            }
            Shape::Rectangle => {
                point.x >= self.origin.x
                    && point.x <= self.origin.x + self.size.x
                    && point.y >= self.origin.y
                    && point.y <= self.origin.y + self.size.y
            }
            Shape::Invalid => false,
        }
    }
}
