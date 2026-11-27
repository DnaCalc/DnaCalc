//! Workbook-independent formula assistance, inspection and display.

pub mod display;
pub mod entry;
pub mod parts;
pub mod result;

pub use calc_fml::{assist, parse_formula, syntax::TextSpan};
