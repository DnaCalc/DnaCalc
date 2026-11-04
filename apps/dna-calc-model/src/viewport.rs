//! The grid skin's pure scroll and zoom arithmetic: the zoom factor's bounds, and the minimal
//! scroll that brings a row or column into view when the keyboard moves the active cell.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnacalc-stage-sheet/src/viewport.rs`:
//! `ZOOM_STEP`, `ZOOM_MIN`, `ZOOM_MAX`, `clamp_zoom` and `reveal_scroll`, with their tests. Left
//! behind: the wheel coalescer (the grid scrolls natively, and the browser delivers at most one
//! scroll event per frame), the semantic zoom tiers that were never built, and the window-edge
//! jump.

/// The zoom step each zoom-in or zoom-out multiplies or divides the factor by.
pub const ZOOM_STEP: f64 = 1.25;
/// The smallest worksheet zoom factor (10%), matching the documented Excel range.
pub const ZOOM_MIN: f64 = 0.1;
/// The largest zoom factor (400%).
pub const ZOOM_MAX: f64 = 4.0;

/// Session view state, separate from authored worksheet moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridView {
    pub viewport: super::geometry::Viewport,
    pub zoom: f64,
    pub active: (u32, u32),
}

pub fn parse_zoom_percent(text: &str) -> Result<f64, &'static str> {
    let value = text
        .trim()
        .trim_end_matches('%')
        .trim()
        .parse::<f64>()
        .map_err(|_| "Enter a zoom percentage from 10 to 400")?;
    if value.is_finite() && (ZOOM_MIN * 100.0..=ZOOM_MAX * 100.0).contains(&value) {
        Ok(value / 100.0)
    } else {
        Err("Enter a zoom percentage from 10 to 400")
    }
}

/// Keep physical scroll surfaces below browser layout limits while retaining Excel coordinates.
pub const SCROLL_SURFACE_LIMIT: f64 = 8_000_000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollScale {
    pub surface: f64,
    logical_max: f64,
    physical_max: f64,
}

impl ScrollScale {
    pub fn new(content: f64, viewport: f64) -> Self {
        let content = content.max(viewport).max(0.0);
        let surface = content.min(SCROLL_SURFACE_LIMIT).max(viewport);
        Self {
            surface,
            logical_max: (content - viewport).max(0.0),
            physical_max: (surface - viewport).max(0.0),
        }
    }
    pub fn logical(self, physical: f64) -> f64 {
        if self.physical_max <= 0.0 {
            0.0
        } else {
            physical.clamp(0.0, self.physical_max) * self.logical_max / self.physical_max
        }
    }
    pub fn physical(self, logical: f64) -> f64 {
        if self.logical_max <= 0.0 {
            0.0
        } else {
            logical.clamp(0.0, self.logical_max) * self.physical_max / self.logical_max
        }
    }
}

/// Clamp a zoom factor to the control's `[ZOOM_MIN, ZOOM_MAX]` range; a
/// non-finite value resets to `1.0` (100%). Pure and total, so a `+`/`−`/reset
/// press can never drive the zoom out of range or to `NaN`.
#[must_use]
pub fn clamp_zoom(factor: f64) -> f64 {
    if !factor.is_finite() {
        return 1.0;
    }
    factor.clamp(ZOOM_MIN, ZOOM_MAX)
}

/// The scroll offset along one axis that brings the content span `[near, far)` (a row or column of
/// any size) fully into the viewport's data area, moving as little as possible: back to `near`
/// when it starts before the view, forward so `far` meets the view's far edge when it ends after
/// it, and not at all when it is in view (dc-09v: [`reveal_scroll`] for sized bands).
#[must_use]
pub fn reveal_span(near: f64, far: f64, header: f64, viewport_len: f64, scroll: f64) -> f64 {
    if !scroll.is_finite() {
        return 0.0;
    }
    let scroll = scroll.max(0.0);
    let data_len = if viewport_len.is_finite() && header.is_finite() {
        (viewport_len - header).max(0.0)
    } else {
        0.0
    };
    if data_len <= 0.0 || !near.is_finite() || !far.is_finite() {
        return scroll;
    }
    if near < scroll {
        near.max(0.0)
    } else if far > scroll + data_len {
        (far - data_len).max(0.0)
    } else {
        scroll
    }
}

/// The scroll offset along one axis that brings the 1-based `band` (a row or a
/// column) fully into the viewport's data area, moving the viewport **as little
/// as possible** (Excel's minimal-scroll reveal) — the pure core of the
/// keyboard-nav auto-scroll.
///
/// The band's content-space span is `[(band − 1)·band_size, band·band_size)`
/// (measured from the grid origin, the exact inverse of the
/// `header + (index − 1)·size − scroll` term in [`super::geometry::cell_rect`]);
/// the visible data window in that space is `[scroll, scroll + (viewport_len −
/// header)]`. Three cases:
/// - band **before** the window (`near < scroll`) → scroll back to the band's
///   near edge, so it appears flush against the header;
/// - band **after** the window (`far > scroll + data_len`) → scroll forward so the
///   band's far edge aligns with the viewport's far edge;
/// - band **already fully visible** → the scroll is returned unchanged (no jump).
///
/// Pure and total: a non-finite `scroll` resets to `0`; a degenerate `band_size`
/// or a non-positive data area (pre-first-paint, or a header wider than the
/// viewport) leaves the scroll untouched rather than fabricating a bogus offset;
/// the result is never negative. The upper bound is implicit — callers only ever
/// reveal an in-extent band, whose far edge is within the grid content, so the
/// forward target `far − data_len` is always ≤ [`clamp_scroll`]'s maximum.
#[must_use]
pub fn reveal_scroll(
    band: u32,
    band_size: f64,
    header: f64,
    viewport_len: f64,
    scroll: f64,
) -> f64 {
    if !scroll.is_finite() {
        return 0.0;
    }
    let scroll = scroll.max(0.0);
    if !band_size.is_finite() || band_size <= 0.0 {
        return scroll;
    }
    let data_len = if viewport_len.is_finite() && header.is_finite() {
        (viewport_len - header).max(0.0)
    } else {
        0.0
    };
    if data_len <= 0.0 {
        // No measurable data area (pre-first-paint size 0, or a header wider than
        // the viewport): nothing to reveal into — leave the scroll as-is.
        return scroll;
    }
    let near = ((f64::from(band) - 1.0) * band_size).max(0.0);
    let far = f64::from(band) * band_size;
    if near < scroll {
        near
    } else if far > scroll + data_len {
        (far - data_len).max(0.0)
    } else {
        scroll
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn physical_scroll_surfaces_reach_both_excel_edges_at_every_zoom() {
        for zoom in [0.1, 0.25, 0.5, 1.0, 4.0] {
            for viewport in [320.0, 844.0, 1000.0] {
                let content = 22.0 + 1_048_576.0 * 20.0 * zoom;
                let scale = ScrollScale::new(content, viewport);
                assert!(scale.surface <= SCROLL_SURFACE_LIMIT);
                assert_eq!(scale.logical(0.0), 0.0);
                assert_eq!(scale.logical(scale.surface - viewport), content - viewport);
                for logical in [0.0, 1000.0, (content - viewport) / 2.0, content - viewport] {
                    assert!((scale.logical(scale.physical(logical)) - logical).abs() < 0.000001);
                }
            }
        }
        let small = ScrollScale::new(1000.0, 500.0);
        assert_eq!(small.logical(123.0), 123.0);
        assert_eq!(small.physical(123.0), 123.0);
    }

    /// `clamp_zoom` holds the factor to `[ZOOM_MIN, ZOOM_MAX]` and resets a
    /// non-finite value to 1.0.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn clamp_zoom_bounds_the_factor() {
        assert!((clamp_zoom(1.0) - 1.0).abs() < 1e-9);
        assert!(
            (clamp_zoom(0.05) - ZOOM_MIN).abs() < 1e-9,
            "below min clamps up"
        );
        assert!(
            (clamp_zoom(10.0) - ZOOM_MAX).abs() < 1e-9,
            "above max clamps down"
        );
        assert!(
            (clamp_zoom(f64::NAN) - 1.0).abs() < 1e-9,
            "NaN resets to 1.0"
        );
    }

    /// `reveal_scroll` leaves an already-fully-visible band's scroll UNCHANGED —
    /// the no-jump case (nav within the visible window never scrolls).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn reveal_scroll_keeps_a_visible_band_put() {
        // 22px bands, 22px header, 200px viewport → 178px data area (~8 rows).
        let (band_size, header, vp) = (22.0, 22.0, 200.0);
        // Unscrolled, band 1 (near=0, far=22) and band 8 (near=154, far=176) both
        // sit inside [0, 178] → no scroll change.
        assert_eq!(reveal_scroll(1, band_size, header, vp, 0.0), 0.0);
        assert_eq!(reveal_scroll(8, band_size, header, vp, 0.0), 0.0);
    }

    /// A band BEFORE the window (its near edge above/left of the scroll) reveals
    /// to the band's near edge, flush against the header.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn reveal_scroll_pulls_back_to_a_band_above_the_window() {
        let (band_size, header, vp) = (22.0, 22.0, 200.0);
        // Scrolled to 500px, band 3's near edge is at (3−1)·22 = 44px < 500 → the
        // window jumps back so band 3 sits flush at the top.
        assert_eq!(reveal_scroll(3, band_size, header, vp, 500.0), 44.0);
    }

    /// A band AFTER the window (its far edge past `scroll + data_len`) reveals by
    /// aligning that far edge to the viewport's far edge — minimal forward scroll.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn reveal_scroll_pushes_forward_to_a_band_below_the_window() {
        let (band_size, header, vp) = (22.0, 22.0, 200.0);
        let data_len = vp - header; // 178
        // Unscrolled, band 9's far edge is 9·22 = 198 > 0 + 178 → scroll forward to
        // 198 − 178 = 20, so band 9's far edge lands exactly on the data-area edge.
        let s = reveal_scroll(9, band_size, header, vp, 0.0);
        assert!(
            (s - 20.0).abs() < 1e-9,
            "band 9 far edge flush to the viewport far edge"
        );
        // And after that scroll the band is genuinely fully inside the data window.
        let near = (9.0 - 1.0) * band_size;
        let far = 9.0 * band_size;
        assert!(
            near >= s - 1e-9 && far <= s + data_len + 1e-9,
            "band 9 now fully visible"
        );
    }

    /// After one reveal, EVERY navigated band lands fully inside the data window
    /// and the scroll is never negative — the load-bearing property, swept over a
    /// range of bands and starting scrolls (data area comfortably wider than one
    /// band, so every band fits).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn reveal_scroll_makes_the_band_fully_visible() {
        let (band_size, header, vp) = (22.0, 22.0, 200.0);
        let data_len = vp - header;
        for &start in &[0.0, 60.0, 500.0, 5000.0] {
            for band in 1..=60u32 {
                let s = reveal_scroll(band, band_size, header, vp, start);
                assert!(
                    s >= 0.0,
                    "scroll never negative (band {band}, start {start})"
                );
                let near = (f64::from(band) - 1.0) * band_size;
                let far = f64::from(band) * band_size;
                assert!(
                    near >= s - 1e-9 && far <= s + data_len + 1e-9,
                    "band {band} must be fully visible after reveal (start {start}, scroll {s})"
                );
            }
        }
    }

    /// `reveal_scroll` is idempotent: revealing a band that a first reveal already
    /// brought on screen returns the same scroll (no oscillation on repeat nav).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn reveal_scroll_is_idempotent() {
        let (band_size, header, vp) = (22.0, 22.0, 200.0);
        for band in 1..=60u32 {
            let once = reveal_scroll(band, band_size, header, vp, 3000.0);
            let twice = reveal_scroll(band, band_size, header, vp, once);
            assert!(
                (once - twice).abs() < 1e-9,
                "band {band} reveal must be stable"
            );
        }
    }

    /// Totality guards: a non-finite scroll resets to 0; a degenerate band size or
    /// a non-positive data area (size-0 pre-paint viewport, or header ≥ viewport)
    /// leaves the scroll untouched rather than fabricating a bogus offset.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn reveal_scroll_guards_degenerate_inputs() {
        // NaN scroll → the early-return 0 (independent of the band).
        assert_eq!(
            reveal_scroll(9, 22.0, 22.0, 200.0, f64::NAN),
            0.0,
            "NaN scroll → 0"
        );
        // A negative scroll is clamped to 0; band 1 is visible there, so the reveal
        // is a no-op and the result stays 0 (isolating the negative→0 guard from the
        // reveal itself — a band that needs revealing would legitimately move off 0).
        assert_eq!(
            reveal_scroll(1, 22.0, 22.0, 200.0, -50.0),
            0.0,
            "negative scroll → 0"
        );
        // Degenerate band size: scroll passes through (clamped ≥ 0).
        assert_eq!(reveal_scroll(9, 0.0, 22.0, 200.0, 130.0), 130.0);
        assert_eq!(reveal_scroll(9, f64::NAN, 22.0, 200.0, 130.0), 130.0);
        // No data area (viewport not yet measured, or header wider than viewport):
        // leave the scroll as-is, never scroll a band into a zero-height window.
        assert_eq!(
            reveal_scroll(9, 22.0, 22.0, 0.0, 130.0),
            130.0,
            "size-0 viewport → no-op"
        );
        assert_eq!(
            reveal_scroll(9, 22.0, 22.0, 10.0, 130.0),
            130.0,
            "header ≥ viewport → no-op"
        );
    }
}

#[cfg(test)]
mod zoom_entry_tests {
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn zoom_entry_is_a_bounded_percentage() {
        assert_eq!(super::parse_zoom_percent(" 125 % "), Ok(1.25));
        assert_eq!(super::parse_zoom_percent("50"), Ok(0.5));
        assert_eq!(super::parse_zoom_percent("400"), Ok(4.0));
        assert_eq!(super::parse_zoom_percent("10"), Ok(0.1));
        assert_eq!(super::parse_zoom_percent("25"), Ok(0.25));
        assert!(matches!(super::parse_zoom_percent("49.9"),Ok(z) if (z-0.499).abs()<1e-12));
        for text in ["", "NaN", "inf", "0", "9.9", "400.1", "one"] {
            assert!(super::parse_zoom_percent(text).is_err(), "{text}");
        }
    }
}
