//! A module of the fixture library.

pub enum Shape {
    Circle,
}

impl Shape {
    pub fn unit() -> Self {
        Self::Circle
    }
}
