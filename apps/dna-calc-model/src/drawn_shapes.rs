//! Shapes and text boxes of a sheet's drawing as the grid draws them (dc-zjsg.2): the wire form the
//! workbook server sends, where an anchor lands on the grid (hidden rows and columns as Excel does),
//! the preset geometries as paths, and the markup of one object. The first slice draws `rect`,
//! `roundRect`, `ellipse`, `line` and `straightConnector1` with a solid fill, a solid outline of a
//! weight and a dash, and text with insets, a vertical anchor and paragraphs. Gradients,
//! arrowheads, shadows, rotation, groups, pictures and the other presets are not drawn.

use crate::projection::ChartPlace;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// One object, with its colours resolved and its lengths in pixels at 100%.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawnShape {
    pub name: String,
    /// `shape`, `textbox` or `connector`.
    pub kind: String,
    pub place: ChartPlace,
    /// The preset geometry's name.
    pub geometry: String,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    /// `#RRGGBB`, `None` for no fill.
    pub fill: Option<String>,
    /// `None` for no outline.
    pub line: Option<ShapeStroke>,
    pub text: Option<ShapeText>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeStroke {
    pub width: f64,
    pub colour: String,
    /// `prstDash val`: `solid`, `dash`, `sysDot`, ...
    pub dash: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeParagraph {
    pub text: String,
    /// `l`, `ctr`, `r` or `just`.
    pub align: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeText {
    pub paragraphs: Vec<ShapeParagraph>,
    /// Left, top, right and bottom insets, in pixels at 100%.
    pub inset: [f64; 4],
    /// `t`, `ctr` or `b`.
    pub anchor: String,
    pub wrap: bool,
    /// Font size in pixels at 100%.
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub colour: String,
}

/// Where a frame lands, in the pixels of the view: the point of an anchor corner is its cell's
/// top-left plus its offset, the offset held inside the cell. A hidden row or column has no size, so
/// an offset into it is nothing: the object closes up over it as Excel's does.
fn corner(
    cell_rect: &impl Fn(u32, u32) -> Option<(f64, f64, f64, f64)>,
    corner: &(crate::projection::Key, f64, f64),
    zoom: f64,
) -> Option<(f64, f64)> {
    let at = corner.0.cell()?;
    let (x, y, w, h) = cell_rect(at.row(), at.column())?;
    Some((
        x + (corner.1 * zoom).clamp(0.0, w.max(0.0)),
        y + (corner.2 * zoom).clamp(0.0, h.max(0.0)),
    ))
}

/// The frame of an object `(left, top, right, bottom)` in view pixels. `cell_rect` is the rectangle
/// `(x, y, width, height)` of the cell (row, column) in the view; `origin` is that of cell A1 for an
/// absolute anchor; `zoom` scales lengths given at 100%.
pub fn frame(
    place: &ChartPlace,
    cell_rect: impl Fn(u32, u32) -> Option<(f64, f64, f64, f64)>,
    origin: (f64, f64),
    zoom: f64,
) -> Option<(f64, f64, f64, f64)> {
    Some(match place {
        ChartPlace::Cells { from, to } => {
            let (a, b) = corner(&cell_rect, from, zoom)?;
            let (c, d) = corner(&cell_rect, to, zoom)?;
            (a, b, c.max(a), d.max(b))
        }
        ChartPlace::Sized {
            from,
            width,
            height,
        } => {
            let (a, b) = corner(&cell_rect, from, zoom)?;
            (a, b, a + width * zoom, b + height * zoom)
        }
        ChartPlace::Absolute {
            x,
            y,
            width,
            height,
        } => {
            let (a, b) = (origin.0 + x * zoom, origin.1 + y * zoom);
            (a, b, a + width * zoom, b + height * zoom)
        }
    })
}

/// One drawing command of a path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Move(f64, f64),
    Line(f64, f64),
    Cubic((f64, f64), (f64, f64), (f64, f64)),
    Close,
}

/// A circular quarter's control distance: 4/3 * (sqrt 2 - 1).
const KAPPA: f64 = 0.552_284_749_830_793_4;

/// Whether the geometry is a preset this slice draws.
pub fn drawn_geometry(prst: &str) -> bool {
    matches!(
        prst,
        "rect" | "roundRect" | "ellipse" | "line" | "straightConnector1"
    )
}

/// The path of preset `prst` in a `w` by `h` box, `None` for a preset not drawn. A `roundRect`'s
/// corner radius is its default adjustment, 16667/100000 of the shorter side.
pub fn geometry_path(prst: &str, w: f64, h: f64) -> Option<Vec<Seg>> {
    Some(match prst {
        "rect" => vec![
            Seg::Move(0.0, 0.0),
            Seg::Line(w, 0.0),
            Seg::Line(w, h),
            Seg::Line(0.0, h),
            Seg::Close,
        ],
        "roundRect" => {
            let r = w.min(h) * 16667.0 / 100000.0;
            let k = r * KAPPA;
            vec![
                Seg::Move(r, 0.0),
                Seg::Line(w - r, 0.0),
                Seg::Cubic((w - r + k, 0.0), (w, r - k), (w, r)),
                Seg::Line(w, h - r),
                Seg::Cubic((w, h - r + k), (w - r + k, h), (w - r, h)),
                Seg::Line(r, h),
                Seg::Cubic((r - k, h), (0.0, h - r + k), (0.0, h - r)),
                Seg::Line(0.0, r),
                Seg::Cubic((0.0, r - k), (r - k, 0.0), (r, 0.0)),
                Seg::Close,
            ]
        }
        "ellipse" => {
            let (rx, ry) = (w / 2.0, h / 2.0);
            let (kx, ky) = (rx * KAPPA, ry * KAPPA);
            vec![
                Seg::Move(0.0, ry),
                Seg::Cubic((0.0, ry - ky), (rx - kx, 0.0), (rx, 0.0)),
                Seg::Cubic((rx + kx, 0.0), (w, ry - ky), (w, ry)),
                Seg::Cubic((w, ry + ky), (rx + kx, h), (rx, h)),
                Seg::Cubic((rx - kx, h), (0.0, ry + ky), (0.0, ry)),
                Seg::Close,
            ]
        }
        "line" | "straightConnector1" => vec![Seg::Move(0.0, 0.0), Seg::Line(w, h)],
        _ => return None,
    })
}

fn number(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// SVG path data of `segments`, mirrored across the box `w` by `h` when flipped.
pub fn path_data(segments: &[Seg], w: f64, h: f64, flip_h: bool, flip_v: bool) -> String {
    let x = |v: f64| number(if flip_h { w - v } else { v });
    let y = |v: f64| number(if flip_v { h - v } else { v });
    let mut out = String::new();
    for segment in segments {
        match *segment {
            Seg::Move(a, b) => {
                let _ = write!(out, "M{} {}", x(a), y(b));
            }
            Seg::Line(a, b) => {
                let _ = write!(out, "L{} {}", x(a), y(b));
            }
            Seg::Cubic(c1, c2, p) => {
                let _ = write!(
                    out,
                    "C{} {} {} {} {} {}",
                    x(c1.0),
                    y(c1.1),
                    x(c2.0),
                    y(c2.1),
                    x(p.0),
                    y(p.1)
                );
            }
            Seg::Close => out.push('Z'),
        }
    }
    out
}

/// A preset dash as the lengths of its dashes and gaps, in line widths.
pub fn dash_pattern(preset: &str) -> &'static [f64] {
    match preset {
        "dot" | "sysDot" => &[1.0, 1.0],
        "dash" => &[4.0, 3.0],
        "lgDash" => &[8.0, 3.0],
        "dashDot" => &[4.0, 3.0, 1.0, 3.0],
        "lgDashDot" => &[8.0, 3.0, 1.0, 3.0],
        "lgDashDotDot" => &[8.0, 3.0, 1.0, 3.0, 1.0, 3.0],
        "sysDash" => &[3.0, 1.0],
        "sysDashDot" => &[3.0, 1.0, 1.0, 1.0],
        "sysDashDotDot" => &[3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        _ => &[],
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\u{b}' => out.push_str("<br>"),
            c => out.push(c),
        }
    }
    out
}

/// The markup of `shape` for a frame `w` by `h` pixels at zoom `zoom`: an SVG of the geometry with
/// the text over it as HTML, padded by the insets and placed by the vertical anchor. `None` for a
/// geometry not drawn.
pub fn shape_markup(shape: &DrawnShape, w: f64, h: f64, zoom: f64) -> Option<String> {
    let path = geometry_path(&shape.geometry, w, h)?;
    let d = path_data(&path, w, h, shape.flip_horizontal, shape.flip_vertical);
    let open = !matches!(
        shape.geometry.as_str(),
        "rect" | "roundRect" | "ellipse"
    );
    let fill = match (&shape.fill, open) {
        (Some(colour), false) => colour.as_str(),
        _ => "none",
    };
    let mut stroke = String::from("stroke=\"none\"");
    if let Some(line) = &shape.line {
        let width = line.width * zoom;
        stroke = format!("stroke=\"{}\" stroke-width=\"{}\"", line.colour, number(width));
        let dashes = dash_pattern(&line.dash);
        if !dashes.is_empty() {
            let list: Vec<String> = dashes.iter().map(|d| number(d * width)).collect();
            let _ = write!(stroke, " stroke-dasharray=\"{}\"", list.join(" "));
        }
    }
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" overflow=\"visible\"><path d=\"{d}\" fill=\"{fill}\" {stroke}/></svg>",
        number(w),
        number(h)
    );
    if let Some(text) = &shape.text {
        let [l, t, r, b] = text.inset.map(|v| v * zoom);
        let justify = match text.anchor.as_str() {
            "ctr" => "center",
            "b" => "flex-end",
            _ => "flex-start",
        };
        let _ = write!(
            out,
            "<div class=\"dc-shape-text\" style=\"position:absolute;left:0;top:0;width:{}px;height:{}px;box-sizing:border-box;padding:{}px {}px {}px {}px;display:flex;flex-direction:column;justify-content:{justify};font-size:{}px;line-height:1.2;color:{};font-weight:{};font-style:{};white-space:{};overflow:visible\">",
            number(w),
            number(h),
            number(t),
            number(r),
            number(b),
            number(l),
            number(text.size * zoom),
            text.colour,
            if text.bold { "bold" } else { "normal" },
            if text.italic { "italic" } else { "normal" },
            if text.wrap { "pre-wrap" } else { "pre" },
        );
        for paragraph in &text.paragraphs {
            let align = match paragraph.align.as_str() {
                "ctr" => "center",
                "r" => "right",
                "just" => "justify",
                _ => "left",
            };
            let _ = write!(
                out,
                "<p style=\"margin:0;min-height:1.2em;text-align:{align}\">{}</p>",
                escape(&paragraph.text)
            );
        }
        out.push_str("</div>");
    }
    Some(out)
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use crate::projection::Key;

    fn key(row: u32, col: u32) -> Key {
        Key::of(calc_value::CellKey::new(calc_value::SheetId::from_parts(0, 1), row, col).unwrap())
    }

    /// Rows of 20 px and columns of 64 px, except row 3 and column 2 hidden.
    fn cells(row: u32, col: u32) -> Option<(f64, f64, f64, f64)> {
        let h = |r: u32| if r == 3 { 0.0 } else { 20.0 };
        let w = |c: u32| if c == 2 { 0.0 } else { 64.0 };
        let y: f64 = (1..row).map(h).sum();
        let x: f64 = (1..col).map(w).sum();
        Some((x, y, w(col), h(row)))
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__a_two_cell_anchor_follows_row_heights_and_column_widths() {
        let place = ChartPlace::Cells {
            from: (key(2, 1), 9.5, 4.0),
            to: (key(5, 4), 0.0, 10.0),
        };
        // Column 4 starts at 64 + 0 + 64 = 128 (column 2 is hidden); row 5 at 20 + 20 + 0 + 20 = 60 (row 3 is hidden).
        assert_eq!(
            frame(&place, cells, (0.0, 0.0), 1.0),
            Some((9.5, 24.0, 128.0, 70.0))
        );
        // Zoomed, the offsets scale and the cell sizes (already scaled by the caller) are given scaled.
        let zoomed = |row: u32, col: u32| cells(row, col).map(|(x, y, w, h)| (x * 2.0, y * 2.0, w * 2.0, h * 2.0));
        assert_eq!(
            frame(&place, zoomed, (0.0, 0.0), 2.0),
            Some((19.0, 48.0, 256.0, 140.0))
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__an_anchor_into_a_hidden_row_or_column_closes_up_over_it() {
        // From the hidden column 2 (offset ignored) to column 3: the object is the width of the part of column 3 it spans.
        let place = ChartPlace::Cells {
            from: (key(1, 2), 30.0, 0.0),
            to: (key(1, 3), 40.0, 0.0),
        };
        let (a, _, c, _) = frame(&place, cells, (0.0, 0.0), 1.0).unwrap();
        assert_eq!((a, c), (64.0, 104.0));
        // From and to both in the hidden row 3: no height.
        let flat = ChartPlace::Cells {
            from: (key(3, 1), 0.0, 5.0),
            to: (key(3, 1), 0.0, 15.0),
        };
        let (_, b, _, d) = frame(&flat, cells, (0.0, 0.0), 1.0).unwrap();
        assert_eq!(b, d);
        // An offset past its cell is held to the cell.
        let long = ChartPlace::Cells {
            from: (key(1, 1), 500.0, 0.0),
            to: (key(1, 1), 500.0, 0.0),
        };
        assert_eq!(frame(&long, cells, (0.0, 0.0), 1.0).unwrap().0, 64.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__one_cell_and_absolute_anchors_take_their_size_scaled() {
        let sized = ChartPlace::Sized {
            from: (key(2, 3), 1.0, 2.0),
            width: 100.0,
            height: 40.0,
        };
        assert_eq!(
            frame(&sized, cells, (0.0, 0.0), 1.0),
            Some((65.0, 22.0, 165.0, 62.0))
        );
        let absolute = ChartPlace::Absolute {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
        };
        assert_eq!(
            frame(&absolute, cells, (5.0, 7.0), 1.5),
            Some((20.0, 37.0, 65.0, 97.0))
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__preset_paths() {
        let rect = geometry_path("rect", 40.0, 20.0).unwrap();
        assert_eq!(path_data(&rect, 40.0, 20.0, false, false), "M0 0L40 0L40 20L0 20Z");
        // A rounded rectangle's radius is a sixth of the shorter side, 20 * 0.16667 = 3.333.
        let round = path_data(&geometry_path("roundRect", 40.0, 20.0).unwrap(), 40.0, 20.0, false, false);
        assert!(round.starts_with("M3.333 0L36.667 0C"), "{round}");
        assert!(round.ends_with("Z"));
        // An ellipse touches the middle of each side.
        let oval = path_data(&geometry_path("ellipse", 40.0, 20.0).unwrap(), 40.0, 20.0, false, false);
        assert!(oval.starts_with("M0 10C0 4.477 "), "{oval}");
        assert!(oval.contains(" 20 0C") && oval.contains(" 40 10C") && oval.contains(" 20 20C"), "{oval}");
        // A line flipped vertically runs from the bottom-left corner to the top-right.
        let line = geometry_path("straightConnector1", 40.0, 20.0).unwrap();
        assert_eq!(path_data(&line, 40.0, 20.0, false, true), "M0 20L40 0");
        assert_eq!(path_data(&line, 40.0, 20.0, true, false), "M40 0L0 20");
        assert!(geometry_path("star5", 40.0, 20.0).is_none());
    }

    fn sample() -> DrawnShape {
        DrawnShape {
            name: "Rectangle 1".into(),
            kind: "shape".into(),
            place: ChartPlace::Absolute { x: 0.0, y: 0.0, width: 80.0, height: 40.0 },
            geometry: "rect".into(),
            flip_horizontal: false,
            flip_vertical: false,
            fill: Some("#FF8000".into()),
            line: Some(ShapeStroke { width: 2.0, colour: "#ED7D31".into(), dash: "dash".into() }),
            text: Some(ShapeText {
                paragraphs: vec![
                    ShapeParagraph { text: "a < b".into(), align: "ctr".into() },
                    ShapeParagraph { text: String::new(), align: "l".into() },
                ],
                inset: [9.6, 4.8, 9.6, 4.8],
                anchor: "ctr".into(),
                wrap: true,
                size: 14.666,
                bold: true,
                italic: false,
                colour: "#000000".into(),
            }),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__markup_has_fill_outline_dash_and_padded_anchored_paragraphs() {
        let markup = shape_markup(&sample(), 80.0, 40.0, 1.0).unwrap();
        assert!(markup.contains("<path d=\"M0 0L80 0L80 40L0 40Z\" fill=\"#FF8000\" stroke=\"#ED7D31\" stroke-width=\"2\" stroke-dasharray=\"8 6\"/>"), "{markup}");
        assert!(markup.contains("padding:4.8px 9.6px 4.8px 9.6px"), "{markup}");
        assert!(markup.contains("justify-content:center"), "{markup}");
        assert!(markup.contains("<p style=\"margin:0;min-height:1.2em;text-align:center\">a &lt; b</p>"), "{markup}");
        assert!(markup.contains("text-align:left\"></p>"), "{markup}");
        assert!(markup.contains("font-weight:bold"), "{markup}");
        // Zoom scales the stroke and the insets with the frame.
        let zoomed = shape_markup(&sample(), 160.0, 80.0, 2.0).unwrap();
        assert!(zoomed.contains("stroke-width=\"4\" stroke-dasharray=\"16 12\""), "{zoomed}");
        assert!(zoomed.contains("padding:9.6px 19.2px 9.6px 19.2px"), "{zoomed}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__a_line_has_no_fill_and_an_undrawn_geometry_has_no_markup() {
        let mut line = sample();
        line.geometry = "line".into();
        line.text = None;
        let markup = shape_markup(&line, 40.0, 0.0, 1.0).unwrap();
        assert!(markup.contains("fill=\"none\""), "{markup}");
        assert!(markup.contains("d=\"M0 0L40 0\""), "{markup}");
        let mut star = sample();
        star.geometry = "star5".into();
        assert!(shape_markup(&star, 40.0, 40.0, 1.0).is_none());
        // No outline: none is drawn.
        let mut bare = sample();
        bare.line = None;
        bare.text = None;
        assert!(shape_markup(&bare, 80.0, 40.0, 1.0).unwrap().contains("stroke=\"none\""));
    }
}
