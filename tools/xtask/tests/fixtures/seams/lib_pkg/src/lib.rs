//! Fixture library for check (e).

pub mod shapes;

pub enum Color {
    Red,
    Green,
    Blue,
}

pub trait Paint {
    fn paint(&self) -> Color;
}

pub(crate) enum Hidden {
    Kept,
}

#[cfg(test)]
pub enum OnlyInTests {
    Probe,
}
