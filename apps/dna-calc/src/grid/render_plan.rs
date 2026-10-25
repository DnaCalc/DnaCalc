//! The grid skin's display list: what the grid draws for one [`Viewport`], as data. The on-screen
//! cells with their shown text and provenance, the visible column and row header bands, and the
//! extent the grid scrolls over. No Leptos and no DOM, so its two rules are asserted directly:
//!
//! - **Determinism**: equal inputs give an equal plan; cells are sorted by `(row, col)`, so the
//!   plan never depends on the order the cells arrived in.
//! - **Faithfulness**: the plan holds exactly the given cells that lie in the extent and meet the
//!   viewport, never a cell calc-host did not project, and never drops one that is on screen.
//!
//! Gridlines are not in the plan: they follow from the metrics and the scroll, which the view
//! draws as a background.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnacalc-stage-sheet/src/render_plan.rs`
//! (`build_render_plan`, `col_label`, the header rectangles and the viewport test, with the tests
//! that still apply), planning calc-host's projected cells ([`CellShown`]) instead of the legacy
//! grid projection. The column labels are A1 letters.

use super::geometry::{
    CellRect, GridMetrics, Viewport, cell_rect, visible_col_range, visible_row_range,
};
use crate::cells::{CellShown, column_letters};

/// One cell of a [`RenderPlan`]: its 1-based address, its viewport-local rectangle and what it
/// shows.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedCell {
    pub row: u32,
    pub col: u32,
    pub rect: CellRect,
    pub shown: CellShown,
}

/// One visible column-header band: the 1-based column, its rectangle in the top strip, and its
/// letters.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedColHeader {
    pub col: u32,
    pub rect: CellRect,
    pub label: String,
}

/// One visible row-header band: the 1-based row, its rectangle in the left strip, and its number.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedRowHeader {
    pub row: u32,
    pub rect: CellRect,
    pub label: String,
}

/// The display list for one [`Viewport`].
#[derive(Debug, Clone, PartialEq)]
pub struct RenderPlan {
    /// The on-screen cells, sorted by `(row, col)`.
    pub cells: Vec<PlannedCell>,
    pub col_headers: Vec<PlannedColHeader>,
    pub row_headers: Vec<PlannedRowHeader>,
    /// The rows and columns the grid scrolls over.
    pub extent_rows: u32,
    pub extent_cols: u32,
}

/// The plan of `cells` under `m` and `v` over an extent of `extent_rows` by `extent_cols`.
///
/// A cell is planned when it lies in the extent and its rectangle meets the viewport. A planned
/// cell is always hittable: [`super::geometry::hit_test`] with this plan's extent maps every point
/// inside its rectangle back to it.
#[must_use]
pub fn build_render_plan<'a>(
    cells: impl IntoIterator<Item = &'a CellShown>,
    m: &GridMetrics,
    v: &Viewport,
    extent_rows: u32,
    extent_cols: u32,
) -> RenderPlan {
    let mut planned: Vec<PlannedCell> = cells
        .into_iter()
        .filter_map(|shown| {
            let (row, col) = (shown.at.row(), shown.at.column());
            if row > extent_rows || col > extent_cols {
                return None;
            }
            let rect = cell_rect(m, v, row, col);
            rect_intersects_viewport(&rect, v).then(|| PlannedCell {
                row,
                col,
                rect,
                shown: shown.clone(),
            })
        })
        .collect();
    planned.sort_by_key(|cell| (cell.row, cell.col));

    let col_headers = visible_col_range(m, v, extent_cols)
        .map(|(first, last)| {
            (first..=last)
                .map(|col| PlannedColHeader {
                    col,
                    rect: col_header_rect(m, v, col),
                    label: col_label(col),
                })
                .collect()
        })
        .unwrap_or_default();
    let row_headers = visible_row_range(m, v, extent_rows)
        .map(|(first, last)| {
            (first..=last)
                .map(|row| PlannedRowHeader {
                    row,
                    rect: row_header_rect(m, v, row),
                    label: row.to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    RenderPlan {
        cells: planned,
        col_headers,
        row_headers,
        extent_rows,
        extent_cols,
    }
}

/// The A1 letters of a 1-based column: 1 is A, 26 Z, 27 AA, 703 AAA; 0 gives nothing.
#[must_use]
pub fn col_label(col: u32) -> String {
    column_letters(col)
}

/// The header rectangle of `col`: its data band, spanning the top header strip.
fn col_header_rect(m: &GridMetrics, v: &Viewport, col: u32) -> CellRect {
    let data = cell_rect(m, v, 1, col);
    CellRect {
        x: data.x,
        y: 0.0,
        w: m.col_width,
        h: m.header_h,
    }
}

/// The header rectangle of `row`: its data band, spanning the left header strip.
fn row_header_rect(m: &GridMetrics, v: &Viewport, row: u32) -> CellRect {
    let data = cell_rect(m, v, row, 1);
    CellRect {
        x: 0.0,
        y: data.y,
        w: m.header_w,
        h: m.row_height,
    }
}

/// Whether a rectangle meets the viewport `[0, width) x [0, height)`. A cell partly under a header
/// strip counts as on screen; the header draws over it.
fn rect_intersects_viewport(rect: &CellRect, v: &Viewport) -> bool {
    rect.x < v.width && rect.x + rect.w > 0.0 && rect.y < v.height && rect.y + rect.h > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cells::Provenance;
    use crate::grid::geometry::{HitTarget, hit_test};
    use calc_host::value::{CellKey, IdAllocator, SheetId};

    fn sheet() -> SheetId {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        ids.issue().unwrap()
    }

    fn cell(sheet: SheetId, row: u32, col: u32, text: &str) -> CellShown {
        CellShown {
            at: CellKey::new(sheet, row, col).unwrap(),
            text: text.to_owned(),
            kind: "number",
            provenance: Provenance::Calculated,
            formula: true,
            format: crate::cells::FormatShown::General,
        }
    }

    fn viewport(width: f64, height: f64) -> Viewport {
        Viewport {
            scroll_x: 0.0,
            scroll_y: 0.0,
            width,
            height,
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn build_render_plan_is_deterministic_and_order_independent() {
        let s = sheet();
        let m = GridMetrics::default();
        let v = viewport(800.0, 400.0);
        let forward = vec![cell(s, 1, 1, "1"), cell(s, 2, 1, "2"), cell(s, 1, 2, "3")];
        let backward: Vec<CellShown> = forward.iter().rev().cloned().collect();
        let a = build_render_plan(&forward, &m, &v, 100, 10);
        let b = build_render_plan(&backward, &m, &v, 100, 10);
        assert_eq!(a, b);
        let order: Vec<(u32, u32)> = a.cells.iter().map(|c| (c.row, c.col)).collect();
        assert_eq!(order, [(1, 1), (1, 2), (2, 1)]);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn build_render_plan_plans_exactly_the_on_screen_cells() {
        let s = sheet();
        let m = GridMetrics::default();
        // 48 + 80 * 4 = 368: columns 1 to 4 are on screen in a 360-pixel viewport (the fourth in
        // part), column 5 is not; rows 1 to 8 meet a 190-pixel one (22 + 22 * 8 = 198).
        let v = viewport(360.0, 190.0);
        let cells = vec![
            cell(s, 1, 1, "a"),
            cell(s, 8, 4, "b"),
            cell(s, 1, 5, "off right"),
            cell(s, 9, 1, "off bottom"),
        ];
        let plan = build_render_plan(&cells, &m, &v, 100, 100);
        let texts: Vec<&str> = plan.cells.iter().map(|c| c.shown.text.as_str()).collect();
        assert_eq!(texts, ["a", "b"]);
        // Scrolled one column right and one row down, the off-screen cells come in.
        let scrolled = Viewport {
            scroll_x: 80.0,
            scroll_y: 22.0,
            ..v
        };
        let plan = build_render_plan(&cells, &m, &scrolled, 100, 100);
        let texts: Vec<&str> = plan.cells.iter().map(|c| c.shown.text.as_str()).collect();
        assert_eq!(texts, ["a", "off right", "b", "off bottom"]);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn planned_cell_rects_round_trip_through_hit_test() {
        let s = sheet();
        let m = GridMetrics::default();
        let v = Viewport {
            scroll_x: 37.5,
            scroll_y: 11.0,
            width: 900.0,
            height: 500.0,
        };
        let cells: Vec<CellShown> = (1..=12)
            .flat_map(|row| (1..=8).map(move |col| (row, col)))
            .map(|(row, col)| cell(s, row, col, "x"))
            .collect();
        let plan = build_render_plan(&cells, &m, &v, 12, 8);
        for planned in &plan.cells {
            let (cx, cy) = (
                planned.rect.x + planned.rect.w / 2.0,
                planned.rect.y + planned.rect.h / 2.0,
            );
            if cx < m.header_w || cy < m.header_h || cx < 0.0 || cy < 0.0 {
                continue;
            }
            assert_eq!(
                hit_test(&m, &v, plan.extent_rows, plan.extent_cols, cx, cy),
                HitTarget::Cell {
                    row: planned.row,
                    col: planned.col
                }
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn header_bands_cover_the_visible_range_clamped_to_extent() {
        let m = GridMetrics::default();
        let v = viewport(1000.0, 300.0);
        let plan = build_render_plan(&[], &m, &v, 5, 3);
        let cols: Vec<&str> = plan.col_headers.iter().map(|h| h.label.as_str()).collect();
        assert_eq!(cols, ["A", "B", "C"]);
        let rows: Vec<&str> = plan.row_headers.iter().map(|h| h.label.as_str()).collect();
        assert_eq!(rows, ["1", "2", "3", "4", "5"]);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cells_outside_the_extent_are_not_planned() {
        let s = sheet();
        let m = GridMetrics::default();
        let v = viewport(2000.0, 2000.0);
        let plan = build_render_plan(&[cell(s, 3, 1, "in"), cell(s, 4, 1, "out")], &m, &v, 3, 1);
        assert_eq!(plan.cells.len(), 1);
        assert_eq!(plan.cells[0].shown.text, "in");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn col_label_matches_excel_a1_columns() {
        for (col, label) in [
            (1, "A"),
            (26, "Z"),
            (27, "AA"),
            (52, "AZ"),
            (702, "ZZ"),
            (703, "AAA"),
            (16_384, "XFD"),
        ] {
            assert_eq!(col_label(col), label);
        }
        assert_eq!(col_label(0), "");
    }
}
