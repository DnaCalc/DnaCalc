//! Integration suites for this package.

#[path = "default_font_columns.rs"]
mod default_font_columns;

#[path = "opened_vba.rs"]
mod opened_vba;

#[path = "support/projection_wire.rs"]
mod projection_wire;

#[cfg(not(target_arch = "wasm32"))]
#[path = "js.rs"]
mod js;
