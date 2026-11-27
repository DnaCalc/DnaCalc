//! The grid skin's pure geometry: grid metrics, cell rectangles and the hit test.
//!
//! Functions that map between a grid's 1-based `(row, col)` addresses and viewport-local pixel
//! rectangles, and back:
//!
//! - [`cell_rect`] is the forward map: address to viewport-local [`CellRect`].
//! - [`hit_test`] is its exact inverse: a viewport-local point to the [`HitTarget`] (a cell, a
//!   header strip, the corner, or outside).
//!
//! The round trip is the invariant the tests hold: every point strictly inside
//! `cell_rect(m, v, r, c)` hit-tests back to `HitTarget::Cell { row: r, col: c }`, at any scroll
//! offset, because both share one half-open banding of the plane (each cell owns
//! `[left, right) x [top, bottom)`).
//!
//! A row is [`GridMetrics::row_height`] tall and a column [`GridMetrics::col_width`] wide unless
//! the workbook sizes it otherwise ([`Sizes`]: Excel's own row heights and column widths, and a
//! hidden row or column as size 0, dc-09v). A band's position is the default size times its
//! index plus the differences of the sized bands before it, found by binary search, so the
//! forward map and the hit test stay exact inverses over any sizes.
//!
//! All coordinates are `f64` CSS pixels. [`hit_test`] takes pointer coordinates, so it refuses
//! negative and non-finite inputs and never panics.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnacalc-stage-sheet/src/geometry.rs`, whole, with
//! its tests; the sized bands are dc-09v's.

pub use calc_skin::metrics::{GridMetrics, Sizes};

/// The scrolled viewport over the grid, in CSS pixels.
///
/// `scroll_x`/`scroll_y` are the distance the grid has been scrolled *away* from
/// its origin (how far the content has moved up/left under the viewport), so a
/// larger scroll shifts every [`cell_rect`] toward the origin by exactly that
/// amount. `width`/`height` are the visible extent of the viewport itself, used
/// by the RenderPlan to decide which cells and headers are on screen (they do
/// not affect [`hit_test`]'s classification — a caller only ever hands it a
/// point already inside the viewport).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Viewport {
    /// Horizontal scroll: content pixels hidden to the left of the viewport.
    pub scroll_x: f64,
    /// Vertical scroll: content pixels hidden above the viewport.
    pub scroll_y: f64,
    /// Visible viewport width, in pixels.
    pub width: f64,
    /// Visible viewport height, in pixels.
    pub height: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            scroll_y: 0.0,
            width: 0.0,
            height: 0.0,
        }
    }
}

/// A viewport-local pixel rectangle: `(x, y)` is the top-left corner and `w`/`h`
/// the size, all in the viewport's own coordinate space (so `(0, 0)` is the
/// viewport's top-left, and a cell scrolled off the top has a negative `y`).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct CellRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// What a viewport-local point lands on, as classified by [`hit_test`]. Rows and columns are
/// 1-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    /// A data cell at 1-based `(row, col)`, inside the grid extent.
    Cell { row: u32, col: u32 },
    /// The column-header strip above data column `col` (1-based).
    ColumnHeader { col: u32 },
    /// The row-header strip beside data row `row` (1-based).
    RowHeader { row: u32 },
    /// The top-left corner box where the two header strips meet.
    Corner,
    /// Outside the grid: beyond the extent, or a negative/non-finite coordinate.
    Outside,
}

/// The viewport-local rectangle of the 1-based cell at `(row, col)`: its bands' starts past the
/// header strips, less the scroll, sized by its row's height and its column's width. A `col` or
/// `row` of 0 (no cell) gives the rectangle of one default band before the first.
#[must_use]
pub fn cell_rect(m: &GridMetrics, v: &Viewport, row: u32, col: u32) -> CellRect {
    let (x, w) = if col == 0 {
        (-m.col_width, m.col_width)
    } else {
        (m.col_start(col), m.width(col))
    };
    let (y, h) = if row == 0 {
        (-m.row_height, m.row_height)
    } else {
        (m.row_start(row), m.height(row))
    };
    CellRect {
        x: m.header_w + x - v.scroll_x,
        y: m.header_h + y - v.scroll_y,
        w,
        h,
    }
}

/// Classify a viewport-local point into a [`HitTarget`], the exact inverse of [`cell_rect`].
///
/// Regions (each half-open, so a cell owns its left/top edge but not its right/bottom, matching
/// [`cell_rect`]'s banding; a hidden band, of size 0, owns no point):
/// - a negative or non-finite `x`/`y` → [`HitTarget::Outside`];
/// - `x < header_w && y < header_h` → [`HitTarget::Corner`];
/// - `y < header_h` → [`HitTarget::ColumnHeader`] of the column under `x + scroll_x`;
/// - `x < header_w` → [`HitTarget::RowHeader`], symmetrically;
/// - otherwise the data cell under the point, or [`HitTarget::Outside`] past the extent.
#[must_use]
pub fn hit_test(
    m: &GridMetrics,
    v: &Viewport,
    extent_rows: u32,
    extent_cols: u32,
    x: f64,
    y: f64,
) -> HitTarget {
    if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 {
        return HitTarget::Outside;
    }
    let in_row_header = x < m.header_w;
    let in_col_header = y < m.header_h;
    let col = || {
        m.cols
            .band_at(m.col_width, x - m.header_w + v.scroll_x, extent_cols)
    };
    let row = || {
        m.rows
            .band_at(m.row_height, y - m.header_h + v.scroll_y, extent_rows)
    };
    match (in_row_header, in_col_header) {
        (true, true) => HitTarget::Corner,
        (false, true) => col().map_or(HitTarget::Outside, |col| HitTarget::ColumnHeader { col }),
        (true, false) => row().map_or(HitTarget::Outside, |row| HitTarget::RowHeader { row }),
        (false, false) => match (row(), col()) {
            (Some(row), Some(col)) => HitTarget::Cell { row, col },
            _ => HitTarget::Outside,
        },
    }
}

/// The inclusive 1-based range of columns whose bands meet the viewport, clamped to
/// `[1, extent_cols]`; `None` when the viewport has no data area or the extent is empty.
#[must_use]
pub fn visible_col_range(m: &GridMetrics, v: &Viewport, extent_cols: u32) -> Option<(u32, u32)> {
    band_range(
        &m.cols,
        m.col_width,
        m.header_w,
        v.scroll_x,
        v.width,
        extent_cols,
    )
}

/// The inclusive 1-based range of rows whose bands meet the viewport, clamped to
/// `[1, extent_rows]`; the vertical twin of [`visible_col_range`].
#[must_use]
pub fn visible_row_range(m: &GridMetrics, v: &Viewport, extent_rows: u32) -> Option<(u32, u32)> {
    band_range(
        &m.rows,
        m.row_height,
        m.header_h,
        v.scroll_y,
        v.height,
        extent_rows,
    )
}

/// The bands `[first, last]` whose cells meet the data area (`viewport_len - header` pixels past
/// the header strip), clamped to `[1, extent]`; `None` for a degenerate size, an empty extent, a
/// header as wide as the viewport, or a range that clamps empty.
fn band_range(
    sizes: &Sizes,
    default: f64,
    header: f64,
    scroll: f64,
    viewport_len: f64,
    extent: u32,
) -> Option<(u32, u32)> {
    if extent == 0 || default <= 0.0 || !default.is_finite() {
        return None;
    }
    if !header.is_finite() || !scroll.is_finite() || !viewport_len.is_finite() {
        return None;
    }
    let data_len = viewport_len - header;
    if data_len <= 0.0 {
        return None;
    }
    let scroll = scroll.max(0.0);
    let content_end = sizes.start(default, extent) + sizes.size(default, extent);
    if scroll >= content_end {
        return None;
    }
    let first = sizes.band_at(default, scroll, extent).unwrap_or(1);
    // The band holding the last pixel in view, or the extent's last when the view reaches past it.
    let end = scroll + data_len;
    let last = if end >= content_end {
        extent
    } else {
        sizes
            .band_at(default, end - f64::EPSILON * end.max(1.0), extent)
            .unwrap_or(extent)
    };
    (last >= first).then_some((first, last))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A viewport with no scroll and a generous visible extent — the common
    /// fixture for forward-geometry and round-trip assertions.
    fn unscrolled(width: f64, height: f64) -> Viewport {
        Viewport {
            scroll_x: 0.0,
            scroll_y: 0.0,
            width,
            height,
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn default_metrics_are_excel_like() {
        let m = GridMetrics::default();
        assert_eq!(
            m,
            GridMetrics {
                row_height: 22.0,
                col_width: 80.0,
                header_h: 22.0,
                header_w: 48.0,
                ..GridMetrics::default()
            }
        );
    }

    /// `scaled` is faithfully proportional, `scaled(1.0)` is the identity (the
    /// property the default-zoom grid relies on to reproduce today's geometry
    /// exactly), and a degenerate factor returns the metrics unchanged rather
    /// than fabricating `NaN`/zero pixels.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn scaled_metrics_are_proportional_with_identity_at_one() {
        let m = GridMetrics::default();

        // Identity at 1.0 — bit-for-bit equal (so zoom=1.0 draws today's grid).
        assert_eq!(m.scaled(1.0), m);

        // Proportional at 2.0 and 0.6 (the legibility floor the caller clamps to).
        assert_eq!(
            m.scaled(2.0),
            GridMetrics {
                scale: 2.0,
                row_height: 44.0,
                col_width: 160.0,
                header_h: 44.0,
                header_w: 96.0,
                ..GridMetrics::default()
            }
        );
        let floor = m.scaled(0.6);
        assert!((floor.row_height - 13.2).abs() < 1e-9);
        assert!((floor.col_width - 48.0).abs() < 1e-9);

        // Degenerate factors are guarded to identity — never NaN/zero metrics.
        assert_eq!(m.scaled(f64::NAN), m);
        assert_eq!(m.scaled(0.0), m);
        assert_eq!(m.scaled(-1.5), m);
    }

    /// The first cell sits exactly at the grid origin `(header_w, header_h)` and
    /// is one column wide / one row tall.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn first_cell_sits_at_the_grid_origin() {
        let m = GridMetrics::default();
        let v = unscrolled(800.0, 600.0);
        assert_eq!(
            cell_rect(&m, &v, 1, 1),
            CellRect {
                x: m.header_w,
                y: m.header_h,
                w: m.col_width,
                h: m.row_height,
            }
        );
    }

    /// Band adjacency (columns): `cell_rect(_, col + 1)` is exactly
    /// `cell_rect(_, col)` shifted right by one `col_width` — contiguous, equal
    /// width, non-overlapping — for a fixed row across a sample of columns.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn column_band_is_contiguous_and_non_overlapping() {
        let m = GridMetrics::default();
        let v = unscrolled(2000.0, 600.0);
        let row = 3;
        for col in 1..=25u32 {
            let here = cell_rect(&m, &v, row, col);
            let next = cell_rect(&m, &v, row, col + 1);
            assert_eq!(
                next,
                CellRect {
                    x: here.x + m.col_width,
                    y: here.y,
                    w: m.col_width,
                    h: m.row_height,
                },
                "column {col} → {} must be contiguous and non-overlapping",
                col + 1
            );
        }
    }

    /// Band adjacency (rows): the vertical twin of the column test.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn row_band_is_contiguous_and_non_overlapping() {
        let m = GridMetrics::default();
        let v = unscrolled(800.0, 2000.0);
        let col = 4;
        for row in 1..=40u32 {
            let here = cell_rect(&m, &v, row, col);
            let next = cell_rect(&m, &v, row + 1, col);
            assert_eq!(
                next,
                CellRect {
                    x: here.x,
                    y: here.y + m.row_height,
                    w: m.col_width,
                    h: m.row_height,
                },
                "row {row} → {} must be contiguous and non-overlapping",
                row + 1
            );
        }
    }

    /// `cell_rect` is stable: equal inputs yield an equal rect.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cell_rect_is_stable_for_equal_inputs() {
        let m = GridMetrics::default();
        let v = Viewport {
            scroll_x: 37.5,
            scroll_y: 12.25,
            width: 640.0,
            height: 480.0,
        };
        assert_eq!(cell_rect(&m, &v, 7, 9), cell_rect(&m, &v, 7, 9));
    }

    /// THE load-bearing invariant, unscrolled: the center of every cell in a
    /// sample extent hit-tests back to its own `(row, col)`. Exhaustive over
    /// rows 1..=30 × cols 1..=20 (600 cells).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn hit_test_round_trips_every_cell_center_unscrolled() {
        let m = GridMetrics::default();
        let v = unscrolled(4000.0, 4000.0);
        let (extent_rows, extent_cols) = (100, 100);
        for row in 1..=30u32 {
            for col in 1..=20u32 {
                let r = cell_rect(&m, &v, row, col);
                let cx = r.x + r.w / 2.0;
                let cy = r.y + r.h / 2.0;
                assert_eq!(
                    hit_test(&m, &v, extent_rows, extent_cols, cx, cy),
                    HitTarget::Cell { row, col },
                    "center of R{row}C{col} must hit-test back to its own cell"
                );
            }
        }
    }

    /// All four corners *just inside* a cell rect (nudged inward so they clear
    /// the half-open right/bottom edges) hit-test to that cell.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn hit_test_round_trips_cell_corners_just_inside() {
        let m = GridMetrics::default();
        let v = unscrolled(2000.0, 2000.0);
        let (extent_rows, extent_cols) = (100, 100);
        let d = 0.5; // inward nudge, safely below one cell's extent
        for row in 1..=15u32 {
            for col in 1..=15u32 {
                let r = cell_rect(&m, &v, row, col);
                let corners = [
                    (r.x + d, r.y + d),
                    (r.x + r.w - d, r.y + d),
                    (r.x + d, r.y + r.h - d),
                    (r.x + r.w - d, r.y + r.h - d),
                ];
                for (px, py) in corners {
                    assert_eq!(
                        hit_test(&m, &v, extent_rows, extent_cols, px, py),
                        HitTarget::Cell { row, col },
                        "corner ({px}, {py}) of R{row}C{col} must map to that cell"
                    );
                }
            }
        }
    }

    /// A point in the top strip (below the corner) is the column header of the
    /// column under it; a point in the left strip (right of the corner) is the
    /// row header; the top-left box is the corner.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn hit_test_identifies_headers_and_corner() {
        let m = GridMetrics::default();
        let v = unscrolled(800.0, 600.0);
        let (rows, cols) = (100, 100);

        // Top-left corner box.
        assert_eq!(
            hit_test(&m, &v, rows, cols, m.header_w / 2.0, m.header_h / 2.0),
            HitTarget::Corner
        );

        // Column-header strip: x over column 3's band, y inside the top strip.
        let c3 = cell_rect(&m, &v, 1, 3);
        assert_eq!(
            hit_test(&m, &v, rows, cols, c3.x + c3.w / 2.0, m.header_h / 2.0),
            HitTarget::ColumnHeader { col: 3 }
        );

        // Row-header strip: y over row 5's band, x inside the left strip.
        let r5 = cell_rect(&m, &v, 5, 1);
        assert_eq!(
            hit_test(&m, &v, rows, cols, m.header_w / 2.0, r5.y + r5.h / 2.0),
            HitTarget::RowHeader { row: 5 }
        );
    }

    /// Beyond the extent is `Outside` — both a data point past the last
    /// row/column and a header point past the last column.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn hit_test_beyond_extent_is_outside() {
        let m = GridMetrics::default();
        let v = unscrolled(4000.0, 4000.0);
        let (rows, cols) = (5, 4);

        // Data point in column 6 / row 1 — past the 4-column extent.
        let c6 = cell_rect(&m, &v, 1, 6);
        assert_eq!(
            hit_test(&m, &v, rows, cols, c6.x + 1.0, c6.y + 1.0),
            HitTarget::Outside
        );

        // Data point in row 9 / column 1 — past the 5-row extent.
        let r9 = cell_rect(&m, &v, 9, 1);
        assert_eq!(
            hit_test(&m, &v, rows, cols, r9.x + 1.0, r9.y + 1.0),
            HitTarget::Outside
        );

        // Column header past the extent is `Outside`, not a phantom header.
        assert_eq!(
            hit_test(&m, &v, rows, cols, c6.x + 1.0, m.header_h / 2.0),
            HitTarget::Outside
        );
    }

    /// Negative and non-finite coordinates are `Outside` — never a panic, an
    /// underflow, or a fabricated cell.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn hit_test_negative_and_non_finite_are_outside() {
        let m = GridMetrics::default();
        let v = unscrolled(800.0, 600.0);
        let (rows, cols) = (100, 100);
        for (x, y) in [
            (-1.0, 10.0),
            (10.0, -1.0),
            (-5.0, -5.0),
            (f64::NAN, 10.0),
            (10.0, f64::NAN),
            (f64::INFINITY, 10.0),
            (10.0, f64::NEG_INFINITY),
        ] {
            assert_eq!(
                hit_test(&m, &v, rows, cols, x, y),
                HitTarget::Outside,
                "({x}, {y}) must be Outside"
            );
        }
    }

    /// A scrolled `cell_rect` is exactly the unscrolled rect translated by
    /// `-scroll` — geometry is pure translation under scroll, including a
    /// fractional offset.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cell_rect_under_scroll_is_unscrolled_shifted_by_negative_scroll() {
        let m = GridMetrics::default();
        let base = unscrolled(800.0, 600.0);
        let scrolled = Viewport {
            scroll_x: 130.5,
            scroll_y: 44.25,
            ..base
        };
        for row in 1..=10u32 {
            for col in 1..=10u32 {
                let un = cell_rect(&m, &base, row, col);
                assert_eq!(
                    cell_rect(&m, &scrolled, row, col),
                    CellRect {
                        x: un.x - scrolled.scroll_x,
                        y: un.y - scrolled.scroll_y,
                        w: un.w,
                        h: un.h,
                    },
                    "R{row}C{col} under scroll must be the unscrolled rect shifted by -scroll"
                );
            }
        }
    }

    /// The round-trip still holds under a fractional scroll offset — for every
    /// cell whose center lands in the data region.
    ///
    /// A cell scrolled *past* the origin has negative viewport coordinates, so
    /// its center is legitimately off-surface (→ `Outside`), and a cell whose
    /// center falls in a header strip is legitimately a header hit — neither is a
    /// round-trip failure. The invariant is therefore scoped to centers at
    /// `>= (header_w, header_h)`; a counter asserts the scope is non-vacuous.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn hit_test_round_trips_under_fractional_scroll() {
        let m = GridMetrics::default();
        let v = Viewport {
            scroll_x: 217.5,
            scroll_y: 88.25,
            width: 900.0,
            height: 700.0,
        };
        let (rows, cols) = (200, 200);
        let mut checked = 0u32;
        for row in 1..=40u32 {
            for col in 1..=30u32 {
                let r = cell_rect(&m, &v, row, col);
                let cx = r.x + r.w / 2.0;
                let cy = r.y + r.h / 2.0;
                // Only centers genuinely in the data region round-trip to a cell.
                if cx < m.header_w || cy < m.header_h {
                    continue;
                }
                checked += 1;
                assert_eq!(
                    hit_test(&m, &v, rows, cols, cx, cy),
                    HitTarget::Cell { row, col },
                    "R{row}C{col} center must round-trip under scroll"
                );
            }
        }
        assert!(checked > 100, "the scoped round-trip must be non-vacuous");
    }

    /// The exact right/bottom edge of a cell belongs to the *next* band (the
    /// half-open banding [`cell_rect`]/[`hit_test`] share), so a click on the
    /// shared boundary is unambiguous.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cell_edges_are_half_open() {
        let m = GridMetrics::default();
        let v = unscrolled(800.0, 600.0);
        let (rows, cols) = (100, 100);
        let r = cell_rect(&m, &v, 4, 4);
        // Exact right edge → column 5, same row.
        assert_eq!(
            hit_test(&m, &v, rows, cols, r.x + r.w, r.y + r.h / 2.0),
            HitTarget::Cell { row: 4, col: 5 }
        );
        // Exact bottom edge → row 5, same column.
        assert_eq!(
            hit_test(&m, &v, rows, cols, r.x + r.w / 2.0, r.y + r.h),
            HitTarget::Cell { row: 5, col: 4 }
        );
    }

    /// Visible ranges clamp to the extent and track the scroll: unscrolled they
    /// start at band 1; scrolled they advance; and they never exceed the extent.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn visible_ranges_track_scroll_and_clamp_to_extent() {
        let m = GridMetrics::default();
        // Exactly 3 columns of data area (48 + 3*80 = 288) and 5 rows
        // (22 + 5*22 = 132), unscrolled.
        let v = unscrolled(288.0, 132.0);
        assert_eq!(visible_col_range(&m, &v, 100), Some((1, 3)));
        assert_eq!(visible_row_range(&m, &v, 100), Some((1, 5)));

        // Half a column of horizontal scroll pulls in a trailing partial column.
        let scrolled = Viewport {
            scroll_x: 40.0,
            ..v
        };
        assert_eq!(visible_col_range(&m, &scrolled, 100), Some((1, 4)));

        // A one-column extent clamps the range to that single column.
        assert_eq!(visible_col_range(&m, &v, 1), Some((1, 1)));

        // A viewport narrower than the header has no data area → None.
        let tiny = unscrolled(m.header_w - 1.0, 132.0);
        assert_eq!(visible_col_range(&m, &tiny, 100), None);

        // An empty extent → None.
        assert_eq!(visible_col_range(&m, &v, 0), None);
    }

    /// Every column reported by [`visible_col_range`] is genuinely on screen and
    /// every one just outside it is not — the range is neither over- nor
    /// under-inclusive at its boundaries (checked against the cell rect itself).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn visible_col_range_matches_on_screen_cells() {
        let m = GridMetrics::default();
        let v = Viewport {
            scroll_x: 200.0,
            scroll_y: 0.0,
            width: 500.0,
            height: 400.0,
        };
        let (first, last) = visible_col_range(&m, &v, 100).expect("a visible range");
        let on_screen = |col: u32| {
            let r = cell_rect(&m, &v, 1, col);
            r.x < v.width && r.x + r.w > m.header_w
        };
        assert!(on_screen(first), "first visible column is on screen");
        assert!(on_screen(last), "last visible column is on screen");
        assert!(
            first == 1 || !on_screen(first - 1),
            "the column before the first is off screen"
        );
        assert!(
            !on_screen(last + 1),
            "the column after the last is off screen"
        );
    }

    /// dc-09v: sized and hidden bands. Column B is 200 px, C hidden, the rest 80; row 3 is 40 px.
    fn sized() -> GridMetrics {
        GridMetrics {
            cols: Sizes::new(80.0, [(2, 200.0), (3, 0.0)]),
            rows: Sizes::new(22.0, [(3, 40.0)]),
            ..GridMetrics::default()
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sized_bands_place_and_hit_their_cells() {
        let m = sized();
        let v = unscrolled(2000.0, 1000.0);
        // A at 48, B at 128 (200 wide), C hidden at 328 (0 wide), D at 328.
        assert_eq!(
            cell_rect(&m, &v, 1, 2),
            CellRect {
                x: 128.0,
                y: 22.0,
                w: 200.0,
                h: 22.0
            }
        );
        assert_eq!(cell_rect(&m, &v, 1, 3).w, 0.0);
        assert_eq!(cell_rect(&m, &v, 1, 4).x, 328.0);
        assert_eq!(cell_rect(&m, &v, 4, 1).y, 22.0 + 22.0 * 2.0 + 40.0);
        // Every point inside a visible cell hits it; the hidden column owns no point.
        for row in 1..=6u32 {
            for col in 1..=6u32 {
                let r = cell_rect(&m, &v, row, col);
                if r.w == 0.0 {
                    continue;
                }
                for (dx, dy) in [(0.0, 0.0), (r.w - 0.01, r.h - 0.01), (r.w / 2.0, r.h / 2.0)] {
                    assert_eq!(
                        hit_test(&m, &v, 100, 100, r.x + dx, r.y + dy),
                        HitTarget::Cell { row, col },
                        "row {row} col {col}"
                    );
                }
            }
        }
        assert_eq!(
            hit_test(&m, &v, 100, 100, 328.0, 30.0),
            HitTarget::Cell { row: 1, col: 4 }
        );
        // Scrolled 250 px right, the first visible column is B (128-48=80..280 in content).
        let scrolled = Viewport {
            scroll_x: 250.0,
            ..v
        };
        assert_eq!(visible_col_range(&m, &scrolled, 100).map(|r| r.0), Some(2));
        // Scaled, the sized bands scale too.
        assert_eq!(m.scaled(0.5).width(2), 100.0);
        assert_eq!(m.scaled(0.5).col_start(4), 140.0);
    }
}

#[cfg(test)]
mod paint_metric_tests {
    use super::*;
    /// Observed in the authored paint inputs in Excel at 96 DPI with Calibri 11.
    /// The capture command retains original screenshots and measured grid boundaries.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn excel_column_units_and_row_points_use_whole_base_pixels() {
        let widths = [0.5, 1., 2., 8.43, 8.7109375, 12., 20.];
        let geometry = crate::projection::Geometry {
            columns: widths
                .iter()
                .enumerate()
                .map(|(i, &w)| ((i + 1) as u32, (i + 1) as u32, Some(w), false))
                .collect(),
            rows: vec![
                (1, Some(23.), false),
                (2, Some(32.), false),
                (3, Some(20.), false),
            ],
            ..Default::default()
        };
        let metrics = sheet_metrics(Some(&geometry));
        assert_eq!(
            (1..=7).map(|c| metrics.width(c)).collect::<Vec<_>>(),
            [3., 7., 14., 59., 61., 84., 140.]
        );
        assert_eq!(
            (1..=3).map(|r| metrics.height(r)).collect::<Vec<_>>(),
            [30., 42., 26.]
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn display_zoom_retains_pixel_bands_and_cell_hit_targets() {
        let mut base = GridMetrics {
            row_height: 30.,
            col_width: 77.,
            ..Default::default()
        };
        base.rows = Sizes::new(30., [(2, 26.), (3, 0.)]);
        for (zoom, row, col) in [(0.75, 23., 58.), (1., 30., 77.), (1.5, 45., 116.)] {
            let m = base.scaled_for_display(zoom, 1.);
            assert_eq!((m.height(1), m.width(1), m.height(3)), (row, col, 0.));
            assert_eq!(m.row_start(18), row * 15. + m.height(2));
            assert_eq!(m.col_start(10), col * 9.);
        }
        for zoom in [0.1, 0.75, 1., 1.5] {
            for dpr in [1., 1.25, 2.] {
                let m = base.scaled_for_display(zoom, dpr);
                let v = Viewport {
                    width: 2000.,
                    height: 2000.,
                    ..Default::default()
                };
                for row in [1, 2, 4, 18] {
                    let rect = cell_rect(&m, &v, row, 10);
                    for edge in [rect.x, rect.y, rect.w, rect.h] {
                        assert!((edge * dpr - (edge * dpr).round()).abs() < 1e-9);
                    }
                    assert_eq!(
                        hit_test(&m, &v, 100, 100, rect.x + rect.w / 2., rect.y + rect.h / 2.),
                        HitTarget::Cell { row, col: 10 }
                    );
                }
            }
        }
    }
}

pub fn sheet_bands(geometry: &crate::projection::Geometry) -> calc_skin::metrics::SheetBands<'_> {
    let level = |rows| geometry.outline.iter().filter(|g| g.rows == rows).map(|g| g.level).max().unwrap_or(0);
    calc_skin::metrics::SheetBands {
        default_digit_px: geometry.default_digit_px,
        default_row_height: geometry.default_row_height,
        default_column_width: geometry.default_column_width,
        base_column_width: geometry.base_column_width,
        column_outline_levels: level(false),
        row_outline_levels: level(true),
        columns: &geometry.columns,
        rows: &geometry.rows,
    }
}

pub fn sheet_metrics(geometry: Option<&crate::projection::Geometry>) -> GridMetrics {
    calc_skin::metrics::sheet_metrics(geometry.map(sheet_bands))
}

#[cfg(test)]
mod outline_tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn outline_gutters_leave_cell_sizes_text_metrics_and_zoom_unchanged() {
        let (plain, grouped) = outline_metrics();
        for scale in [0.5, 1., 1.75, 4.] {
            let a = plain.scaled(scale);
            let b = grouped.scaled(scale);
            assert_eq!(a.scale, b.scale);
            assert_eq!(a.width(2), b.width(2));
            assert_eq!(a.height(2), b.height(2));
            assert_eq!(
                crate::render_plan::text_width("outline typography", &a),
                crate::render_plan::text_width("outline typography", &b)
            );
            assert_eq!(b.header_h - a.header_h, 112. * scale);
            assert_eq!(b.header_w - a.header_w, 48. * scale);
        }
    }
    fn outline_metrics() -> (GridMetrics, GridMetrics) {
        use crate::projection::{Geometry, OutlineGroup};
        let geometry = Geometry {
            outline: vec![
                OutlineGroup {
                    rows: false,
                    first: 2,
                    last: 4,
                    summary: 5,
                    level: 7,
                    collapsed: false,
                },
                OutlineGroup {
                    rows: true,
                    first: 2,
                    last: 4,
                    summary: 5,
                    level: 3,
                    collapsed: false,
                },
            ],
            ..Geometry::default()
        };
        (sheet_metrics(None), sheet_metrics(Some(&geometry)))
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn low_zoom_outline_gutters_preserve_sizes_and_cell_hit_targets() {
        let (plain, grouped) = outline_metrics();
        let viewport = Viewport::default();
        for scale in [0.1, 0.25] {
            let a = plain.scaled(scale);
            let b = grouped.scaled(scale);
            assert_eq!(a.scale, b.scale);
            assert_eq!(
                crate::render_plan::text_width("outline typography", &a),
                crate::render_plan::text_width("outline typography", &b)
            );
            for row in [1, 2, 10] {
                for column in [1, 2, 10] {
                    let first = cell_rect(&a, &viewport, row, column);
                    let second = cell_rect(&b, &viewport, row, column);
                    assert_eq!(first.w, second.w);
                    assert_eq!(first.h, second.h);
                    for (metrics, rect) in [(&a, first), (&b, second)] {
                        assert_eq!(
                            hit_test(
                                metrics,
                                &viewport,
                                100,
                                100,
                                rect.x + rect.w / 2.,
                                rect.y + rect.h / 2.
                            ),
                            HitTarget::Cell { row, col: column }
                        );
                    }
                }
            }
            // Adding scaled headers and then subtracting them can round one ULP
            // differently from scaling the gutter alone. Cell sizes above stay exact.
            for (actual, expected) in [
                (b.header_h - a.header_h, 112. * scale),
                (b.header_w - a.header_w, 48. * scale),
            ] {
                assert!(
                    (actual - expected).abs() <= expected.abs() * f64::EPSILON * 4.,
                    "{actual} != {expected}"
                );
            }
        }
    }
}
