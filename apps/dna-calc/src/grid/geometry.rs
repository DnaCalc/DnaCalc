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
//! Every row is [`GridMetrics::row_height`] tall and every column [`GridMetrics::col_width`] wide.
//! Excel's own row heights and column widths are not read from the workbook yet; they would
//! replace the multiplication here with a prefix-sum lookup behind the same signatures.
//!
//! All coordinates are `f64` CSS pixels. [`hit_test`] takes pointer coordinates, so it refuses
//! negative and non-finite inputs and never panics.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnacalc-stage-sheet/src/geometry.rs`, whole, with
//! its tests.

/// Uniform grid metrics in CSS pixels.
///
/// The [`Default`] is Excel-like at 100%: a 22 px row, an 80 px column, a 22 px
/// column-header strip along the top, and a 48 px row-header strip down the
/// left. All four are cell/header *extents*, never positions — the grid origin
/// (top-left of cell `R1C1`) sits at `(header_w, header_h)` in the unscrolled
/// viewport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridMetrics {
    /// Height of every data row, in pixels.
    pub row_height: f64,
    /// Width of every data column, in pixels.
    pub col_width: f64,
    /// Height of the top column-header strip, in pixels.
    pub header_h: f64,
    /// Width of the left row-header strip, in pixels.
    pub header_w: f64,
}

impl Default for GridMetrics {
    fn default() -> Self {
        Self {
            row_height: 22.0,
            col_width: 80.0,
            header_h: 22.0,
            header_w: 48.0,
        }
    }
}

impl GridMetrics {
    /// The metrics scaled proportionally by a zoom `factor` (the grid skin's zoom).
    ///
    /// A pure, total multiply of all four cell/header extents — so `scaled(1.0)`
    /// is exactly the input (`22.0 * 1.0 == 22.0`), the property the default-zoom
    /// geometry relies on to reproduce today's unscaled grid bit-for-bit. A
    /// non-finite or non-positive `factor` is a guard case that returns the
    /// metrics unchanged, so a bad zoom value never yields `NaN`/zero pixels that
    /// would break [`hit_test`] or [`cell_rect`]. The caller clamps `factor` to a
    /// legible range ([`super::viewport::clamp_zoom`]); this method itself
    /// applies no floor, so the scaling is faithfully proportional.
    #[must_use]
    pub fn scaled(&self, factor: f64) -> Self {
        if !factor.is_finite() || factor <= 0.0 {
            return *self;
        }
        Self {
            row_height: self.row_height * factor,
            col_width: self.col_width * factor,
            header_h: self.header_h * factor,
            header_w: self.header_w * factor,
        }
    }
}

/// The scrolled viewport over the grid, in CSS pixels.
///
/// `scroll_x`/`scroll_y` are the distance the grid has been scrolled *away* from
/// its origin (how far the content has moved up/left under the viewport), so a
/// larger scroll shifts every [`cell_rect`] toward the origin by exactly that
/// amount. `width`/`height` are the visible extent of the viewport itself, used
/// by the RenderPlan to decide which cells and headers are on screen (they do
/// not affect [`hit_test`]'s classification — a caller only ever hands it a
/// point already inside the viewport).
#[derive(Debug, Clone, Copy, PartialEq)]
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
///
/// Derives `PartialEq` so geometry is directly assertable (`cell_rect` is stable
/// — equal inputs yield an equal rect — and a scroll shift is exactly a
/// translation), rather than comparing four floats by hand.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// What a viewport-local point lands on, as classified by [`hit_test`].
///
/// The four non-`Cell` variants partition the chrome around the data area: the
/// top-left [`Corner`](HitTarget::Corner), the top [`ColumnHeader`](HitTarget::ColumnHeader)
/// strip, the left [`RowHeader`](HitTarget::RowHeader) strip, and everything that
/// is neither a valid in-extent cell nor chrome, which is
/// [`Outside`](HitTarget::Outside). Rows/cols are 1-based, matching the projection.
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

/// The viewport-local rectangle of the 1-based cell at `(row, col)`.
///
/// `x = header_w + (col - 1) * col_width - scroll_x`, and symmetrically for `y`;
/// `w`/`h` are the column width / row height. The `(col - 1)` term is computed in
/// `f64` (via [`f64::from`]), so `col == 0` — an invalid 1-based address a caller
/// might pass — yields a rect one column to the left of the origin rather than a
/// `u32` underflow panic. This is the exact forward inverse of [`hit_test`].
#[must_use]
pub fn cell_rect(m: &GridMetrics, v: &Viewport, row: u32, col: u32) -> CellRect {
    CellRect {
        x: m.header_w + (f64::from(col) - 1.0) * m.col_width - v.scroll_x,
        y: m.header_h + (f64::from(row) - 1.0) * m.row_height - v.scroll_y,
        w: m.col_width,
        h: m.row_height,
    }
}

/// Classify a viewport-local point into a [`HitTarget`] — the exact inverse of
/// [`cell_rect`].
///
/// Regions (each half-open, so a cell owns its left/top edge but not its
/// right/bottom, matching [`cell_rect`]'s banding):
/// - a negative or non-finite `x`/`y` → [`HitTarget::Outside`] (guarded first, so
///   the function never panics on pointer noise);
/// - `x < header_w && y < header_h` → [`HitTarget::Corner`];
/// - `y < header_h` (and past the corner) → [`HitTarget::ColumnHeader`], the
///   column being the one under `x + scroll_x`;
/// - `x < header_w` (and past the corner) → [`HitTarget::RowHeader`], symmetrically;
/// - otherwise the data cell under the point — or [`HitTarget::Outside`] when the
///   resolved row/col is `< 1` or exceeds `extent_rows`/`extent_cols`.
#[must_use]
pub fn hit_test(
    m: &GridMetrics,
    v: &Viewport,
    extent_rows: u32,
    extent_cols: u32,
    x: f64,
    y: f64,
) -> HitTarget {
    // Guard pointer noise up front: non-finite or negative coordinates are
    // never a cell or a header — they are above/left of the whole surface.
    if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 {
        return HitTarget::Outside;
    }

    let in_row_header = x < m.header_w; // left strip
    let in_col_header = y < m.header_h; // top strip

    match (in_row_header, in_col_header) {
        (true, true) => HitTarget::Corner,
        (false, true) => match band_index(m.header_w, m.col_width, v.scroll_x, x, extent_cols) {
            Some(col) => HitTarget::ColumnHeader { col },
            None => HitTarget::Outside,
        },
        (true, false) => match band_index(m.header_h, m.row_height, v.scroll_y, y, extent_rows) {
            Some(row) => HitTarget::RowHeader { row },
            None => HitTarget::Outside,
        },
        (false, false) => {
            let col = band_index(m.header_w, m.col_width, v.scroll_x, x, extent_cols);
            let row = band_index(m.header_h, m.row_height, v.scroll_y, y, extent_rows);
            match (row, col) {
                (Some(row), Some(col)) => HitTarget::Cell { row, col },
                _ => HitTarget::Outside,
            }
        }
    }
}

/// The 1-based band index (column or row) a viewport-local coordinate falls in,
/// or `None` when it resolves before the first band, past the last band (i.e.
/// outside `[1, extent]`), or when the metric is degenerate. (Out-of-range
/// resolves to `None`, not a clamped boundary index — the caller treats that as
/// `Outside`.)
///
/// `content = coord - header + scroll` is the coordinate in grid-content space
/// (measured from the origin of the first data band); `floor(content / size)` is
/// its 0-based band, `+ 1` makes it 1-based. This is the exact inverse of the
/// `header + (index - 1) * size - scroll` term in [`cell_rect`].
fn band_index(header: f64, size: f64, scroll: f64, coord: f64, extent: u32) -> Option<u32> {
    if size <= 0.0 || !size.is_finite() {
        return None;
    }
    let content = coord - header + scroll;
    if !content.is_finite() || content < 0.0 {
        return None;
    }
    // `content >= 0` and `size > 0`, so `zero_based >= 0` and is integer-valued.
    let one_based = (content / size).floor() + 1.0;
    if one_based < 1.0 || one_based > f64::from(extent) {
        return None;
    }
    // Bounded to `[1, extent]` and integer-valued: the cast is exact.
    Some(one_based as u32)
}

/// The inclusive 1-based range of columns whose bands intersect the viewport,
/// clamped to `[1, extent_cols]`; `None` when the viewport has no data area or
/// the extent is empty. Used by the RenderPlan to lay down exactly the visible
/// column headers (and, symmetrically, [`visible_row_range`] the row headers).
#[must_use]
pub fn visible_col_range(m: &GridMetrics, v: &Viewport, extent_cols: u32) -> Option<(u32, u32)> {
    band_range(m.header_w, m.col_width, v.scroll_x, v.width, extent_cols)
}

/// The inclusive 1-based range of rows whose bands intersect the viewport,
/// clamped to `[1, extent_rows]`; `None` when the viewport has no data area or
/// the extent is empty. The vertical twin of [`visible_col_range`].
#[must_use]
pub fn visible_row_range(m: &GridMetrics, v: &Viewport, extent_rows: u32) -> Option<(u32, u32)> {
    band_range(m.header_h, m.row_height, v.scroll_y, v.height, extent_rows)
}

/// The inclusive 1-based `[first, last]` band range whose cells intersect the
/// viewport's data area (`viewport_len - header` pixels past the header strip),
/// clamped to `[1, extent]`. `None` for a degenerate metric, an empty extent, a
/// header wider than the viewport (no data area), or a range that clamps empty.
fn band_range(
    header: f64,
    size: f64,
    scroll: f64,
    viewport_len: f64,
    extent: u32,
) -> Option<(u32, u32)> {
    if extent == 0 || size <= 0.0 || !size.is_finite() {
        return None;
    }
    if !header.is_finite() || !scroll.is_finite() || !viewport_len.is_finite() {
        return None;
    }
    let data_len = viewport_len - header;
    if data_len <= 0.0 {
        return None;
    }
    // First band: the one containing the left content edge (`scroll`), clamped
    // to the origin so a negative scroll starts at band 1.
    let first_zero = (scroll / size).floor().max(0.0);
    // Last band: `ceil` of the right content edge yields the 1-based index of
    // the last band the viewport touches (a trailing partial band included).
    let last_ceil = ((scroll + data_len) / size).ceil();

    let first = (first_zero + 1.0).max(1.0);
    let last = last_ceil.min(f64::from(extent));
    if last < first {
        return None;
    }
    // Both are integer-valued and within `[1, extent]`: the casts are exact.
    Some((first as u32, last as u32))
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
                row_height: 44.0,
                col_width: 160.0,
                header_h: 44.0,
                header_w: 96.0,
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
}
