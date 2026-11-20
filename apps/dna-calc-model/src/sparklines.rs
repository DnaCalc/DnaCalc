//! Sparklines as the grid draws them (dc-zjsg.7): the wire form the workbook server sends (a cell, its values and
//! its group's look with colours resolved) and the SVG of one sparkline in its cell. Line sparklines are a
//! polyline with optional markers; column sparklines are bars from the axis; win/loss (`stacked`) sparklines
//! are equal bars up for a gain and down for a loss. As Excel 16 draws them (dw1 Windows run, 2026-11-20, 64 x 20 px
//! cells at 100% and 200%): 4 px above and below the plot; columns 2 px in from the cell's sides and 72% of their
//! slot; a line's first and last points about 7 px in; markers diamonds; the axis a solid line over the bars. Still
//! assumed: the marker's size (1.5 line widths, at least 2 px, to its corners) and a column sparkline whose values
//! are all of one sign standing on zero.

use crate::projection::Key;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawnSparkline {
    pub cell: Key,
    /// `line`, `column` or `stacked`.
    pub kind: String,
    pub values: Vec<Option<f64>>,
    /// The vertical axis's ends: the group's or the sparkline's own minimum and maximum, or the stated ones.
    pub min: f64,
    pub max: f64,
    /// `gap`, `zero` or `span`.
    pub empty: String,
    /// Line weight in pixels at 100%.
    pub weight: f64,
    pub series: String,
    pub negative: Option<String>,
    pub axis: Option<String>,
    pub markers: Option<String>,
    pub first: Option<String>,
    pub last: Option<String>,
    pub high: Option<String>,
    pub low: Option<String>,
    pub right_to_left: bool,
    /// On a date axis, where each value stands across the cell (0 the earliest date, 1 the latest); empty for evenly
    /// spaced points.
    #[serde(default)]
    pub positions: Vec<Option<f64>>,
}

/// Above and below every sparkline; beside columns; beside a line's end points (px at 100%).
const PAD_Y: f64 = 4.0;
const PAD_COLUMNS: f64 = 2.0;
const PAD_LINE: f64 = 7.0;

fn number(value: f64) -> String {
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0".to_owned() } else { text.to_owned() }
}

/// The SVG of `s` in a cell `w` by `h` pixels at `zoom`; empty when it has no values to draw.
pub fn sparkline_svg(s: &DrawnSparkline, w: f64, h: f64, zoom: f64) -> String {
    let mut values: Vec<Option<f64>> = s.values.clone();
    // On a date axis a point stands at its date; one without a date is not drawn.
    let mut positions: Vec<Option<f64>> = if s.positions.is_empty() { Vec::new() } else { s.positions.clone() };
    if !positions.is_empty() {
        positions.resize(values.len(), None);
        for (v, p) in values.iter_mut().zip(&positions) {
            if p.is_none() {
                *v = None;
            }
        }
    }
    if s.right_to_left {
        values.reverse();
        positions.reverse();
        positions.iter_mut().flatten().for_each(|p| *p = 1.0 - *p);
    }
    if s.empty == "zero" {
        values.iter_mut().filter(|v| v.is_none()).for_each(|v| *v = Some(0.0));
    }
    let n = values.len();
    if n == 0 || values.iter().all(Option::is_none) {
        return String::new();
    }
    let pad = if s.kind == "line" || !matches!(s.kind.as_str(), "column" | "stacked") { PAD_LINE } else { PAD_COLUMNS } * zoom;
    let pad_y = PAD_Y * zoom;
    let (x0, x1, y0, y1) = (pad.min(w / 2.0), (w - pad).max(w / 2.0), pad_y.min(h / 2.0), (h - pad_y).max(h / 2.0));
    let (lo, hi) = (s.min, s.max);
    let y_of = |v: f64| if hi > lo { y1 - (v - lo) / (hi - lo) * (y1 - y0) } else { (y0 + y1) / 2.0 };
    let present: Vec<(usize, f64)> = values.iter().enumerate().filter_map(|(i, v)| v.map(|v| (i, v))).collect();
    let high = present.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let low = present.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let (first, last) = (present.first().map(|p| p.0), present.last().map(|p| p.0));
    // The colour of point `i` with value `v`: high and low first, then first and last, then negative.
    let special = |i: usize, v: f64| -> Option<&String> {
        (v == high).then_some(s.high.as_ref()).flatten()
            .or_else(|| (v == low).then_some(s.low.as_ref()).flatten())
            .or_else(|| (Some(i) == first).then_some(s.first.as_ref()).flatten())
            .or_else(|| (Some(i) == last).then_some(s.last.as_ref()).flatten())
            .or_else(|| (v < 0.0).then_some(s.negative.as_ref()).flatten())
    };
    let mut out = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\">", number(w), number(h));
    // The axis, drawn last over the marks as a crisp line of whole pixels, from the cell's padding to its padding.
    let mut axis = String::new();
    let axis_at = |out: &mut String, y: f64| {
        if let Some(colour) = &s.axis {
            let width = zoom.round().max(1.0);
            let y = if width as i64 % 2 == 1 { y.floor() + 0.5 } else { y.round() };
            let _ = write!(out, "<path d=\"M{} {}H{}\" stroke=\"{colour}\" stroke-width=\"{}\"/>", number(x0.min(PAD_COLUMNS * zoom)), number(y), number(x1.max(w - PAD_COLUMNS * zoom)), number(width));
        }
    };
    match s.kind.as_str() {
        "column" | "stacked" => {
            let slot = (x1 - x0) / n as f64;
            let bar = slot * 0.72;
            let (base, top, bottom) = if s.kind == "stacked" {
                let mid = (y0 + y1) / 2.0;
                (mid, y0, y1)
            } else {
                (y_of(if lo <= 0.0 && hi >= 0.0 { 0.0 } else if lo > 0.0 { lo.min(0.0) } else { hi }), y0, y1)
            };
            let base = base.clamp(top, bottom);
            if s.kind == "stacked" || lo < 0.0 && hi > 0.0 {
                axis_at(&mut axis, base);
            }
            for &(i, v) in &present {
                let x = match positions.get(i).copied().flatten() {
                    // By date: the slot's middle across the cell less a slot.
                    Some(p) => x0 + slot / 2.0 + (x1 - x0 - slot) * p - bar / 2.0,
                    None => x0 + slot * i as f64 + (slot - bar) / 2.0,
                };
                let y = if s.kind == "stacked" {
                    if v > 0.0 { top } else if v < 0.0 { bottom } else { continue }
                } else {
                    y_of(v).clamp(top, bottom)
                };
                let colour = special(i, v).unwrap_or(&s.series);
                let (from, to) = if y < base { (y, base) } else { (base, y) };
                let _ = write!(
                    out,
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{colour}\"/>",
                    number(x),
                    number(from),
                    number(bar),
                    number((to - from).max(zoom.min(1.0)))
                );
            }
        }
        _ => {
            let x_of = |i: usize| match positions.get(i).copied().flatten() {
                Some(p) => x0 + (x1 - x0) * p,
                None if n > 1 => x0 + (x1 - x0) * i as f64 / (n - 1) as f64,
                None => (x0 + x1) / 2.0,
            };
            if s.axis.is_some() && lo < 0.0 && hi > 0.0 {
                axis_at(&mut axis, y_of(0.0));
            }
            let mut d = String::new();
            let mut pen = false;
            for (i, v) in values.iter().enumerate() {
                match v {
                    Some(v) => {
                        let _ = write!(d, "{}{} {}", if pen { "L" } else { "M" }, number(x_of(i)), number(y_of(*v)));
                        pen = true;
                    }
                    // A gap lifts the pen; `span` joins the points either side.
                    None if s.empty != "span" => pen = false,
                    None => {}
                }
            }
            // Excel (dw3 Windows item 3): snaps line weights >= 0.75 pt to whole pixels (round-half-up, min 1 px);
            // thinner weights (< 1 px at 100%) draw as a 1 px hairline scaling with zoom.
            let stroke_width = if s.weight < 1.0 {
                zoom.max(1.0)
            } else {
                (s.weight * zoom).round().max(1.0)
            };
            let _ = write!(
                out,
                "<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\" stroke-linecap=\"round\"/>",
                s.series,
                number(stroke_width)
            );
            let radius = (s.weight * 1.5).max(2.0) * zoom;
            for &(i, v) in &present {
                let colour = special(i, v).or(s.markers.as_ref());
                if let Some(colour) = colour {
                    // A diamond, as Excel draws a sparkline's marker.
                    let (x, y) = (x_of(i), y_of(v));
                    let _ = write!(
                        out,
                        "<path data-marker=\"{i}\" d=\"M{} {}L{} {}L{} {}L{} {}Z\" fill=\"{colour}\"/>",
                        number(x), number(y - radius), number(x + radius), number(y), number(x), number(y + radius), number(x - radius), number(y)
                    );
                }
            }
        }
    }
    out.push_str(&axis);
    out.push_str("</svg>");
    out
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    fn line(values: &[Option<f64>]) -> DrawnSparkline {
        DrawnSparkline {
            cell: Key::of(calc_value::CellKey::new(calc_value::SheetId::from_parts(0, 1), 1, 1).unwrap()),
            kind: "line".into(),
            values: values.to_vec(),
            min: 0.0,
            max: 4.0,
            empty: "gap".into(),
            weight: 1.0,
            series: "#376092".into(),
            negative: None,
            axis: None,
            markers: None,
            first: None,
            last: None,
            high: None,
            low: None,
            right_to_left: false,
            positions: Vec::new(),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_7__a_line_spans_the_cell_and_a_gap_lifts_the_pen() {
        // A 64 by 20 cell: x from 7 to 57, y from 16 (the minimum) to 4 (the maximum), as Excel insets a line.
        let svg = sparkline_svg(&line(&[Some(0.0), Some(4.0), None, Some(2.0)]), 64.0, 20.0, 1.0);
        assert!(svg.contains("d=\"M7 16L23.67 4M57 10\""), "{svg}");
        let mut span = line(&[Some(0.0), Some(4.0), None, Some(2.0)]);
        span.empty = "span".into();
        assert!(sparkline_svg(&span, 64.0, 20.0, 1.0).contains("d=\"M7 16L23.67 4L57 10\""));
        let mut zero = span.clone();
        zero.empty = "zero".into();
        assert!(sparkline_svg(&zero, 64.0, 20.0, 1.0).contains("L40.33 16L57 10"));
        assert_eq!(sparkline_svg(&line(&[None, None]), 64.0, 20.0, 1.0), "");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_7__markers_columns_and_win_loss() {
        let mut s = line(&[Some(1.0), Some(4.0), Some(0.0)]);
        s.markers = Some("#000000".into());
        s.high = Some("#00B050".into());
        let svg = sparkline_svg(&s, 64.0, 20.0, 1.0);
        assert_eq!(svg.matches("data-marker").count(), 3);
        // The high point a diamond of 2 px to its corners at x 32, y 4.
        assert!(svg.contains("d=\"M32 2L34 4L32 6L30 4Z\" fill=\"#00B050\""), "{svg}");
        let mut col = line(&[Some(-2.0), Some(4.0)]);
        col.kind = "column".into();
        col.min = -2.0;
        col.negative = Some("#C00000".into());
        col.axis = Some("#000000".into());
        let svg = sparkline_svg(&col, 64.0, 20.0, 1.0);
        assert_eq!(svg.matches("<rect").count(), 2);
        assert!(svg.contains("fill=\"#C00000\"") && svg.contains("<path d=\"M2 "), "{svg}");
        let mut wl = line(&[Some(3.0), Some(-1.0), Some(0.0)]);
        wl.kind = "stacked".into();
        let svg = sparkline_svg(&wl, 64.0, 20.0, 1.0);
        // A gain fills the upper half, a loss the lower, nothing for zero: 6 px each, 72% of a 20 px slot wide.
        assert!(svg.contains("y=\"4\" width=\"14.4\" height=\"6\"") && svg.contains("y=\"10\" width=\"14.4\" height=\"6\""), "{svg}");
        assert_eq!(svg.matches("<rect").count(), 2);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_7__on_a_date_axis_points_stand_at_their_dates() {
        // Dates 0, 1 and 4 of 0..4: x at 7, 19.5 and 57 across a 64 px cell (7 px each side).
        let mut s = line(&[Some(0.0), Some(4.0), Some(2.0)]);
        s.positions = vec![Some(0.0), Some(0.25), Some(1.0)];
        assert!(sparkline_svg(&s, 64.0, 20.0, 1.0).contains("d=\"M7 16L19.5 4L57 10\""));
        // A point without a date is not drawn.
        s.positions = vec![Some(0.0), None, Some(1.0)];
        s.empty = "gap".into();
        assert!(sparkline_svg(&s, 64.0, 20.0, 1.0).contains("d=\"M7 16M57 10\""));
    }

    /// dc-zjsg.7 (dw4): Excel-measured sparkline line weights across zooms (dw3 Windows item 3):
    /// round-half-up to whole pixels (min 1 px), with <0.75 pt as a 1 px hairline scaling with zoom.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dw4_dc_zjsg_7__line_weight_snaps_to_excel_pixels_at_each_zoom() {
        for (pt, expected) in [
            (0.5, [(1.0, "1"), (1.5, "1.5"), (2.0, "2")]),
            (0.75, [(1.0, "1"), (1.5, "2"), (2.0, "2")]),
            (1.0, [(1.0, "1"), (1.5, "2"), (2.0, "3")]),
            (2.25, [(1.0, "3"), (1.5, "5"), (2.0, "6")]),
            (4.5, [(1.0, "6"), (1.5, "9"), (2.0, "12")]),
        ] {
            let mut s = line(&[Some(1.0), Some(3.0)]);
            s.weight = pt * 96.0 / 72.0;
            for (zoom, want) in expected {
                let svg = sparkline_svg(&s, 64.0 * zoom, 20.0 * zoom, zoom);
                let needle = format!("stroke-width=\"{want}\"");
                assert!(svg.contains(&needle), "pt={pt} zoom={zoom}: expected {needle} in {svg}");
            }
        }
    }
}
