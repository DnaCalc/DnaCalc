//! Shared portable viewport; retained module path for the Grid adapter.
pub use dna_calc_model::viewport::*;
/// A validation popup stays in the pane viewport and opens above a cell when that has more room.
pub fn validation_popup(
    rect: super::geometry::CellRect,
    v: super::geometry::Viewport,
    header_w: f64,
    header_h: f64,
    height: f64,
) -> (f64, f64, f64, f64, bool) {
    let left = rect.x.max(header_w).min((v.width - 80.).max(header_w));
    let below = (v.height - rect.y - rect.h).max(0.);
    let above = (rect.y - header_h).max(0.);
    let down = below >= height || below >= above;
    let height = height.min(if down { below } else { above });
    (
        left,
        if down { rect.y + rect.h } else { rect.y },
        (v.width - left).max(0.),
        height,
        !down,
    )
}
