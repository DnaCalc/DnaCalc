//! Fixture application holding the callers for check (e).

pub fn red() -> lib_pkg::Color {
    lib_pkg::Color::Red
}

pub struct Brush;

impl lib_pkg::Paint for Brush {
    fn paint(&self) -> lib_pkg::Color {
        lib_pkg::Color::Red
    }
}

pub fn use_paint(brush: &dyn lib_pkg::Paint) -> lib_pkg::Color {
    brush.paint()
}

pub fn only_in_tests() {}

#[cfg(test)]
mod tests {
    fn only_in_tests() -> lib_pkg::Color {
        lib_pkg::Color::Green
    }
}
