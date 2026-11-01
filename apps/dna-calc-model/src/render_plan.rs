//! The grid skin's display list: what the grid draws for one [`Viewport`], as data. The on-screen
//! cells with their shown text and provenance, the visible column and row header bands, and the
//! extent the grid scrolls over. No Leptos and no DOM, so its two rules are asserted directly:
//!
//! - **Determinism**: equal inputs give an equal plan; cells are sorted by `(row, col)`, so the
//!   plan never depends on the order the cells arrived in.
//! - **Faithfulness**: the plan holds exactly the given cells that lie in the extent and meet the
//!   viewport (including text spilling from an adjacent cell), never a cell calc-host did not project.
//!
//! Gridlines are not in the plan: they follow from the metrics and the scroll, which the view
//! draws as a background.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnacalc-stage-sheet/src/render_plan.rs`
//! (`build_render_plan`, `col_label`, the header rectangles and the viewport test, with the tests
//! that still apply), planning calc-host's projected cells ([`CellShown`]) instead of the legacy
//! grid projection. The column labels are A1 letters.

use crate::address::column_letters;
use crate::geometry::{
    CellRect, GridMetrics, Viewport, cell_rect, visible_col_range, visible_row_range,
};
use crate::projection::CellShown;
use std::collections::BTreeSet;

/// One cell of a [`RenderPlan`]: its 1-based address, its viewport-local rectangle and what it
/// shows.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PlannedCell {
    pub address: String,
    pub row: u32,
    pub col: u32,
    pub rect: CellRect,
    /// Physical cell before text extends over blank neighbours.
    pub anchor: CellRect,
    #[serde(serialize_with = "serialize_shown")]
    pub shown: CellShown,
}

/// One visible column-header band: the 1-based column, its rectangle in the top strip, and its
/// letters.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PlannedColHeader {
    pub col: u32,
    pub rect: CellRect,
    pub label: String,
}

/// One visible row-header band: the 1-based row, its rectangle in the left strip, and its number.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PlannedRowHeader {
    pub row: u32,
    pub rect: CellRect,
    pub label: String,
}

/// The display list for one [`Viewport`].
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RenderPlan {
    pub viewport: Viewport,
    pub scale: f64,
    pub publication: Option<crate::tiles::TileStamp>,
    pub presentation: crate::presentation::PresentationStamp,
    pub semantic_rows: Vec<SemanticRow>,
    pub interrupted: bool,
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
/// A cell is planned when it lies in the extent and its displayed rectangle meets the viewport.
/// Its physical `anchor` remains the worksheet cell; hit testing uses physical geometry, so a
/// click on text spilling into a blank neighbour selects that neighbour.
#[must_use]
pub fn build_render_plan<'a>(
    cells: impl IntoIterator<Item = &'a CellShown>,
    m: &GridMetrics,
    v: &Viewport,
    extent_rows: u32,
    extent_cols: u32,
) -> RenderPlan {
    let cells: Vec<&CellShown> = cells.into_iter().collect();
    let occupied: std::collections::BTreeSet<(u32, u32)> = cells
        .iter()
        .filter(|shown| {
            !shown.text.is_empty()
                || shown.formula
                || shown.provenance != crate::projection::Provenance::Empty
        })
        .map(|shown| (shown.at.row(), shown.at.column()))
        .collect();
    let mut planned: Vec<PlannedCell> = cells
        .into_iter()
        .filter_map(|shown| {
            let (row, col) = (shown.at.row(), shown.at.column());
            if row > extent_rows || col > extent_cols {
                return None;
            }
            let rect = cell_rect(m, v, row, col);
            let mut cell = PlannedCell {
                address: format!("{}{}", column_letters(col), row),
                row,
                col,
                rect,
                anchor: rect,
                shown: shown.clone(),
            };
            fit(&mut cell, m, &occupied, extent_cols);
            rect_intersects_viewport(&cell.rect, v).then_some(cell)
        })
        .collect();
    planned.sort_by_key(|cell| (cell.row, cell.col));

    let col_headers = visible_col_range(m, v, extent_cols)
        .map(|(first, last)| {
            (first..=last)
                .filter(|&col| m.width(col) > 0.0)
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
                .filter(|&row| m.height(row) > 0.0)
                .map(|row| PlannedRowHeader {
                    row,
                    rect: row_header_rect(m, v, row),
                    label: row.to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    RenderPlan {
        viewport: *v,
        scale: m.header_h / 22.,
        publication: None,
        presentation: Default::default(),
        semantic_rows: Vec::new(),
        interrupted: false,
        cells: planned,
        col_headers,
        row_headers,
        extent_rows,
        extent_cols,
    }
}

/// The width a cell's text takes, estimated from Excel's default font: each character the maximum
/// digit width (7 pixels at 100%, scaled with the grid) and the cell's padding (dc-09v).
pub fn text_width(text: &str, m: &GridMetrics) -> f64 {
    let scale = m.header_h / 22.0;
    (text.chars().count() as f64 * 7.0 + 5.0) * scale
}

/// Fits a planned cell's text to its column, as Excel's grid does: a number shown under a
/// format that is wider than its column shows as a row of `#` filling it, and a text wider than
/// its column overflows into the empty cells in its alignment direction (`occupied`: the cells that show
/// something). A number in General, which Excel narrows to fewer digits, is clipped here.
fn fit(cell: &mut PlannedCell, m: &GridMetrics, occupied: &BTreeSet<(u32, u32)>, extent_cols: u32) {
    if matches!(
        cell.shown.style.horizontal.as_deref(),
        Some("centerContinuous" | "fill")
    ) {
        for col in cell.col + 1
            ..=cell
                .col
                .saturating_add(cell.shown.style.alignment_columns - 1)
                .min(extent_cols)
        {
            if occupied.contains(&(cell.row, col)) {
                break;
            }
            cell.rect.w += m.width(col);
        }
        cell.anchor = cell.rect;
        return;
    }
    // Wrapped and rotated spans belong to their physical cell, including beside blank cells.
    if cell.shown.style.wrap
        || cell.shown.style.rotation != 0
        || matches!(
            cell.shown.style.horizontal.as_deref(),
            Some("fill" | "justify" | "distributed")
        )
    {
        return;
    }
    let needed = text_width(&cell.shown.text, m);
    if needed <= cell.rect.w || cell.rect.w <= 0.0 {
        return;
    }
    match cell.shown.kind {
        "number" if cell.shown.format == crate::projection::FormatShown::Applied => {
            let scale = m.header_h / 22.0;
            let count = ((cell.rect.w - 5.0 * scale) / (7.0 * scale))
                .floor()
                .max(1.0) as usize;
            cell.shown.text = "#".repeat(count);
        }
        "text" => {
            let horizontal = cell.shown.style.horizontal.as_deref().unwrap_or("general");
            let (left, right) = match horizontal {
                "right" => (needed - cell.rect.w, 0.0),
                "center" => ((needed - cell.rect.w) / 2.0, (needed - cell.rect.w) / 2.0),
                _ => (0.0, needed - cell.rect.w),
            };
            let mut col = cell.col;
            let mut grown = 0.0;
            while grown < left && col > 1 && !occupied.contains(&(cell.row, col - 1)) {
                col -= 1;
                let width = m.width(col);
                grown += width;
                cell.rect.x -= width;
                cell.rect.w += width;
            }
            col = cell.col;
            grown = 0.0;
            while grown < right && col < extent_cols && !occupied.contains(&(cell.row, col + 1)) {
                col += 1;
                let width = m.width(col);
                grown += width;
                cell.rect.w += width;
            }
        }
        _ => {}
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
        w: m.width(col),
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
        h: m.height(row),
    }
}

/// Whether a rectangle meets the viewport `[0, width) x [0, height)`. A cell partly under a header
/// strip counts as on screen; the header draws over it.
fn rect_intersects_viewport(rect: &CellRect, v: &Viewport) -> bool {
    rect.x < v.width && rect.x + rect.w > 0.0 && rect.y < v.height && rect.y + rect.h > 0.0
}

#[cfg(test)]
mod tests {
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn hidden_bands_have_no_rendered_header_or_border() {
        use super::*;
        use crate::geometry::Sizes;
        let mut m = GridMetrics::default();
        m.cols = Sizes::new(m.col_width, [(2, 0.0)]);
        m.rows = Sizes::new(m.row_height, [(2, 0.0)]);
        let v = Viewport {
            scroll_x: 0.0,
            scroll_y: 0.0,
            width: 500.0,
            height: 300.0,
        };
        let plan = build_render_plan(&[], &m, &v, 20, 20);
        assert!(!plan.col_headers.iter().any(|h| h.col == 2));
        assert!(!plan.row_headers.iter().any(|h| h.row == 2));
        assert_eq!(plan.col_headers[1].col, 3);
        assert_eq!(plan.col_headers[1].rect.x, m.header_w + m.width(1));
    }

    use super::*;
    use crate::geometry::{HitTarget, hit_test};
    use crate::projection::Provenance;
    use calc_value::{CellKey, IdAllocator, SheetId};

    fn sheet() -> SheetId {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        ids.issue().unwrap()
    }

    fn cell(sheet: SheetId, row: u32, col: u32, text: &str) -> CellShown {
        CellShown {
            style: Default::default(),
            at: CellKey::new(sheet, row, col).unwrap(),
            text: text.to_owned(),
            kind: "number",
            provenance: Provenance::Calculated,
            formula: true,
            format: crate::projection::FormatShown::General,
            color: None,
            conditional: None,
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

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn wide_formatted_numbers_show_hashes_and_text_overflows_into_empty_cells() {
        let s = sheet();
        let m = GridMetrics::default();
        let v = viewport(2000.0, 400.0);
        let mut number = cell(s, 1, 1, "123456.79");
        number.format = crate::projection::FormatShown::Applied;
        let narrow = GridMetrics {
            cols: crate::geometry::Sizes::new(80.0, [(1, 33.0)]),
            ..m.clone()
        };
        let plan = build_render_plan(&[number.clone()], &narrow, &v, 10, 10);
        // 33 pixels hold four digits: Excel shows #### in a column 4 characters wide.
        assert_eq!(plan.cells[0].shown.text, "####");
        // In General the number is not replaced (Excel narrows it instead; clipped here).
        number.format = crate::projection::FormatShown::General;
        let plan = build_render_plan(&[number], &narrow, &v, 10, 10);
        assert_eq!(plan.cells[0].shown.text, "123456.79");
        // A text overflows over B and C while they are empty, and stops at a cell that is not.
        let mut long = cell(s, 1, 1, "a text of thirty characters...");
        long.kind = "text";
        let plan = build_render_plan(&[long.clone()], &m, &v, 10, 10);
        assert_eq!(plan.cells[0].rect.w, 240.0);
        let plan = build_render_plan(&[long, cell(s, 1, 3, "x")], &m, &v, 10, 10);
        assert_eq!(plan.cells[0].rect.w, 160.0);
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn directional_overflow_retains_the_source_anchor_and_stops_at_empty_formula_results() {
        let s = sheet();
        let m = GridMetrics::default();
        let v = viewport(2000., 400.);
        let source = cell(
            s,
            1,
            4,
            "long text that extends into several adjacent columns",
        );
        for (alignment, first, last) in [("left", 4, 5), ("right", 3, 4), ("center", 3, 5)] {
            let mut text = source.clone();
            text.kind = "text";
            text.style.horizontal = Some(alignment.into());
            let mut blank_formula = cell(s, 1, 2, "");
            blank_formula.kind = "text";
            let plan = build_render_plan(
                &[text, blank_formula, cell(s, 1, 6, "stop")],
                &m,
                &v,
                10,
                10,
            );
            let shown = plan.cells.iter().find(|c| c.col == 4).unwrap();
            assert_eq!(shown.rect.x, cell_rect(&m, &v, 1, first).x);
            assert_eq!(
                shown.rect.x + shown.rect.w,
                cell_rect(&m, &v, 1, last).x + m.width(last)
            );
            assert_eq!(shown.anchor, cell_rect(&m, &v, 1, 4));
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn centered_clipping_does_not_move_the_anchor_when_only_one_side_can_extend() {
        let s = sheet();
        let m = GridMetrics::default();
        let v = viewport(2000., 400.);
        let mut text = cell(s, 1, 4, "long centred text wider than one column");
        text.kind = "text";
        text.style.horizontal = Some("center".into());
        let plan = build_render_plan(&[text, cell(s, 1, 5, "stop")], &m, &v, 10, 10);
        let shown = &plan.cells[0];
        assert!(shown.rect.x < shown.anchor.x);
        assert_eq!(shown.rect.x + shown.rect.w, shown.anchor.x + shown.anchor.w);
        let offset = shown.anchor.x + shown.anchor.w / 2. - shown.rect.x - shown.rect.w / 2.;
        assert!(offset > 0.);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn right_aligned_source_just_outside_the_view_can_spill_into_it() {
        let s = sheet();
        let m = GridMetrics::default();
        let v = viewport(240., 400.);
        let mut text = cell(s, 1, 4, "text overflowing from the offscreen right");
        text.kind = "text";
        text.style.horizontal = Some("right".into());
        let plan = build_render_plan(&[text], &m, &v, 10, 10);
        assert_eq!(plan.cells.len(), 1);
        assert!(plan.cells[0].anchor.x >= v.width);
        assert!(plan.cells[0].rect.x < v.width);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn wrapped_and_rotated_text_stays_in_its_own_cell_beside_empty_neighbours() {
        let s = sheet();
        let m = GridMetrics::default();
        let v = viewport(2000., 400.);
        for rotate in [false, true] {
            let mut shown = cell(
                s,
                1,
                1,
                "long text beside empty cells must not widen this cell",
            );
            shown.kind = "text";
            shown.style.wrap = !rotate;
            shown.style.rotation = if rotate { 45 } else { 0 };
            let plan = build_render_plan(&[shown], &m, &v, 10, 10);
            assert_eq!(plan.cells[0].rect.w, m.width(1));
        }
    }
}

/// The bounded accessibility window comes from the same committed root as painted cells.
/// Empty cells have an identity too; an empty cell is never fabricated as calculated.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct SemanticCell {
    pub at: crate::projection::Key,
    pub row: u32,
    pub col: u32,
    pub address: String,
    pub text: String,
    pub provenance: &'static str,
    pub selected: bool,
    pub active: bool,
    pub label: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct SemanticRow {
    pub row: u32,
    pub cells: Vec<SemanticCell>,
}
/// One read-only observation of the live plan consumed by the renderer and accessibility
/// adapter. Browser tests read this instead of reconstructing semantics from painted elements.
#[derive(serde::Serialize)]
pub struct Inspection<'a> {
    pub pane: u8,
    pub active: bool,
    pub commands: u64,
    pub pending: usize,
    pub plan: &'a RenderPlan,
}
impl RenderPlan {
    pub fn attach_semantics(
        &mut self,
        mirror: &crate::ViewMirror,
        sheet: calc_value::SheetId,
        selected: &[calc_value::GridArea],
        active: (u32, u32),
    ) {
        self.publication = mirror.stamp();
        self.interrupted = mirror.needs_reset();
        // At most 4096 semantic cells, centred on the active visible cell. This is independent
        // of paint count, so text spilling from an off-screen anchor adds no extra gridcell.
        let cols = semantic_window(&self.col_headers, active.1, 64, |h| h.col);
        let rows = semantic_window(&self.row_headers, active.0, 4096 / cols.len().max(1), |h| {
            h.row
        });
        self.semantic_rows = rows
            .iter()
            .map(|row| SemanticRow {
                row: row.row,
                cells: cols
                    .iter()
                    .map(|col| {
                        let at = calc_value::CellKey::new(sheet, row.row, col.col)
                            .expect("validated grid headers");
                        let cell = mirror.cell(at);
                        let address =
                            format!("{}{}", crate::address::column_letters(col.col), row.row);
                        let text = cell.map_or("", |c| c.text.as_str()).to_owned();
                        let provenance = cell.map_or("empty", |c| c.provenance.name());
                        let mut label = if cell.is_some() {
                            format!("{address}, {text}, {provenance}")
                        } else {
                            format!("{address}, empty")
                        };
                        if let Some(cell) =
                            cell.filter(|c| c.provenance == crate::projection::Provenance::Refused)
                        {
                            label.push_str(": ");
                            label.push_str(&cell.why);
                        }
                        SemanticCell {
                            at: crate::projection::Key::of(at),
                            row: row.row,
                            col: col.col,
                            address,
                            text,
                            provenance,
                            selected: selected.iter().any(|area| area.contains(at)),
                            active: active == (row.row, col.col),
                            label,
                        }
                    })
                    .collect(),
            })
            .collect();
    }
}
fn semantic_window<T>(headers: &[T], active: u32, limit: usize, index: impl Fn(&T) -> u32) -> &[T] {
    if headers.len() <= limit {
        return headers;
    }
    let position = headers.iter().position(|h| index(h) == active).unwrap_or(0);
    let first = position
        .saturating_sub(limit / 2)
        .min(headers.len() - limit);
    &headers[first..first + limit]
}
fn serialize_shown<S: serde::Serializer>(shown: &CellShown, s: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeStruct;
    let mut out = s.serialize_struct("CellShown", 9)?;
    out.serialize_field("at", &crate::projection::Key::of(shown.at))?;
    out.serialize_field("text", &shown.text)?;
    out.serialize_field("kind", shown.kind)?;
    out.serialize_field("provenance", shown.provenance.name())?;
    out.serialize_field("formula", &shown.formula)?;
    out.serialize_field("format", shown.format.name())?;
    out.serialize_field("color", &shown.color)?;
    out.serialize_field("conditional", &shown.conditional)?;
    out.serialize_field("style", &shown.style)?;
    out.end()
}

#[cfg(test)]
mod semantic_tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn semantic_window_is_bounded_keeps_active_selection_and_never_invents_a_value() {
        let sheet = calc_value::SheetId::from_parts(0, 0);
        let active = (900, 100);
        let at = calc_value::CellKey::new(sheet, active.0, active.1).unwrap();
        let viewport = Viewport {
            width: 10_000.,
            height: 30_000.,
            ..Default::default()
        };
        let mut plan =
            build_render_plan(&[], &GridMetrics::default(), &viewport, 1_048_576, 16_384);
        let mut mirror = crate::ViewMirror::default();
        plan.attach_semantics(&mirror, sheet, &[calc_value::GridArea::cell(at)], active);
        let cells: Vec<_> = plan.semantic_rows.iter().flat_map(|r| &r.cells).collect();
        assert_eq!(cells.len(), 4096);
        assert!(
            cells
                .iter()
                .all(|c| c.text.is_empty() && c.provenance == "empty")
        );
        let focused = cells.iter().find(|c| c.active).unwrap();
        assert!(focused.selected);
        assert_eq!(focused.address, "CV900");
        assert_eq!(focused.label, "CV900, empty");
        assert_eq!(cells.iter().filter(|c| c.active).count(), 1);
        let keys: std::collections::BTreeSet<_> = cells.iter().map(|c| c.at).collect();
        assert_eq!(keys.len(), cells.len());
        assert!(!plan.interrupted);
        mirror.invalidate();
        plan.attach_semantics(&mirror, sheet, &[calc_value::GridArea::cell(at)], active);
        assert!(plan.interrupted);
        assert!(plan.publication.is_none());
    }
}
