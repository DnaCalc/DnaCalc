//! Shapes and text boxes of a sheet's drawing as the grid draws them (dc-zjsg.2): the wire form the
//! workbook server sends, where an anchor lands on the grid (hidden rows and columns as Excel does),
//! the preset geometries as paths, and the markup of one object. The first slice drew `rect`,
//! `roundRect`, `ellipse`, `line` and `straightConnector1` with a solid fill, a solid outline of a
//! weight and a dash, and text with insets, a vertical anchor and paragraphs. The second adds
//! rotation and flips, arrowheads, linear and path gradients, group shapes and the common presets
//! (from ECMA-376 Part 1's presetShapeDefinitions, with their `avLst` adjust values). The third adds
//! outer shadows as a second pass, the text rectangle of the presets worked out in [`text_rect`], bent and
//! curved connectors, and flips and group membership of pictures. Custom geometry is not drawn.

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
    /// A gradient fill; it takes the place of `fill`.
    #[serde(default)]
    pub gradient: Option<ShapeGradient>,
    /// `None` for no outline.
    pub line: Option<ShapeStroke>,
    pub text: Option<ShapeText>,
    /// Degrees clockwise.
    #[serde(default)]
    pub rotation: f64,
    /// The unrotated shape's size as fractions of `place`'s frame: Excel stores the anchor of a rotated
    /// shape as the box around it, so the shape itself is smaller by the rotated box's ratio.
    #[serde(default = "whole")]
    pub extent: [f64; 2],
    /// The preset's adjust values by name (`adj1`, ...), in 100000ths.
    #[serde(default)]
    pub adjust: Vec<(String, i64)>,
    /// A group's children, each placed as fractions of the group's frame.
    #[serde(default)]
    pub children: Vec<DrawnChild>,
    /// The drawing object's identity (`cNvPr id`): what a picture's bytes are asked by.
    #[serde(default)]
    pub id: u32,
    /// An outer shadow, drawn beneath the shape as a second pass.
    #[serde(default)]
    pub shadow: Option<ShapeShadow>,
    /// A picture's crop and, when the page cannot show its image, what to show instead.
    #[serde(default)]
    pub picture: Option<DrawnPicture>,
}

/// An outer shadow (`a:outerShdw`) with its lengths in pixels at 100%.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeShadow {
    /// The shadow's offset from the shape: `dist` along `dir`.
    pub dx: f64,
    pub dy: f64,
    /// `blurRad`.
    pub blur: f64,
    /// `sx`, `sy` as fractions (1 is the shape's own size) about the `align` point.
    pub scale: [f64; 2],
    /// `algn`: `tl`, `t`, `tr`, `l`, `ctr`, `r`, `bl`, `b`, `br`.
    pub align: String,
    /// `rotWithShape`: the offset turns with the shape.
    pub rotate: bool,
    /// CSS colour, with the shadow colour's alpha.
    pub colour: String,
}

/// A picture beside its bytes (`Request::ShapePicture` gives those).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawnPicture {
    /// `a:srcRect` `[l, t, r, b]` as fractions of the image (negative extends it past its box).
    pub crop: [f64; 4],
    /// For an image the page cannot decode (EMF or WMF without a PNG inside, TIFF, a part too large
    /// to keep): the typed label to draw in its box, with the picture's size when the file says it.
    pub placeholder: Option<String>,
    pub description: Option<String>,
}

/// The CSS of an image `w` by `h` pixels inside a box cropped by `crop`: the visible part of the
/// image fills the box.
pub fn crop_style(crop: [f64; 4]) -> String {
    let (kept_w, kept_h) = ((1.0 - crop[0] - crop[2]).max(0.001), (1.0 - crop[1] - crop[3]).max(0.001));
    format!(
        "position:absolute;max-width:none;max-height:none;width:{}%;height:{}%;left:{}%;top:{}%",
        number(100.0 / kept_w),
        number(100.0 / kept_h),
        number(-crop[0] / kept_w * 100.0),
        number(-crop[1] / kept_h * 100.0)
    )
}

/// A picture inside a group, placed for the page: its identity (the page asks the server for its image), its
/// crop, its flips, and the boxes it sits in from the group's outermost child down to the picture itself, each
/// `[left, top, width, height, rotation]` in pixels of the parent box (rotation in degrees clockwise).
#[derive(Clone, Debug, PartialEq)]
pub struct GroupPicture {
    pub id: u32,
    pub crop: [f64; 4],
    pub description: Option<String>,
    pub flip: [bool; 2],
    pub chain: Vec<[f64; 5]>,
}

/// The pictures among a group's descendants whose image the page can decode, for a group box `w` by `h`.
pub fn group_pictures(group: &DrawnShape, w: f64, h: f64) -> Vec<GroupPicture> {
    fn walk(group: &DrawnShape, w: f64, h: f64, chain: &mut Vec<[f64; 5]>, out: &mut Vec<GroupPicture>) {
        for child in &group.children {
            let [rx, ry, rw, rh] = child.rel;
            let (cw, ch) = (w * rw, h * rh);
            chain.push([w * rx, h * ry, cw, ch, child.shape.rotation]);
            match (&child.shape.picture, child.shape.kind.as_str()) {
                (Some(picture), "picture") if picture.placeholder.is_none() => out.push(GroupPicture {
                    id: child.shape.id,
                    crop: picture.crop,
                    description: picture.description.clone(),
                    flip: [child.shape.flip_horizontal, child.shape.flip_vertical],
                    chain: chain.clone(),
                }),
                (_, "group") => walk(&child.shape, cw, ch, chain, out),
                _ => {}
            }
            chain.pop();
        }
    }
    let mut out = Vec::new();
    walk(group, w, h, &mut Vec::new(), &mut out);
    out
}

/// The CSS that mirrors a picture's box for its flips (empty without one).
pub fn flip_style(flip: [bool; 2]) -> String {
    match (flip[0], flip[1]) {
        (false, false) => String::new(),
        (h, v) => format!("transform:scale({},{});", if h { -1 } else { 1 }, if v { -1 } else { 1 }),
    }
}

/// The turn of a shape as the CSS the grid puts on its box (empty without one).
pub fn turn_style(shape: &DrawnShape) -> String {
    rotation_style(shape.rotation)
}

/// The turn of a box by `degrees` clockwise as CSS (empty for none).
pub fn turn_style_of(degrees: f64) -> String {
    rotation_style(degrees)
}

/// The outline of an object (and its text) with no turn: what is drawn over a picture.
pub fn outline_markup(shape: &DrawnShape, w: f64, h: f64, zoom: f64) -> Option<String> {
    inner_markup(shape, w, h, zoom, &gradient_id(&shape.name))
}

fn whole() -> [f64; 2] {
    [1.0, 1.0]
}

/// A group's child: its own (unrotated) box inside the group's, `[x, y, width, height]` as fractions of it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawnChild {
    pub rel: [f64; 4],
    pub shape: DrawnShape,
}

/// A gradient fill: stops at fractions 0 to 1 with `#RRGGBB` colours; linear (`angle` in degrees
/// clockwise from left-to-right) or a path (`circle`, `rect`, `shape`) from a focus `[x, y]` as
/// fractions of the box.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeGradient {
    pub stops: Vec<(f64, String)>,
    pub angle: f64,
    pub scaled: bool,
    pub path: Option<String>,
    pub focus: [f64; 2],
}

/// An arrowhead: `triangle`, `stealth`, `diamond`, `oval` or `arrow`, its `w` and `len` as `sm`, `med`, `lg`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeArrow {
    pub kind: String,
    pub width: String,
    pub length: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeStroke {
    pub width: f64,
    pub colour: String,
    /// `prstDash val`: `solid`, `dash`, `sysDot`, ...
    pub dash: String,
    /// The arrowheads at the start and at the end of a line.
    #[serde(default)]
    pub head: Option<ShapeArrow>,
    #[serde(default)]
    pub tail: Option<ShapeArrow>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeParagraph {
    pub text: String,
    /// `l`, `ctr`, `r` or `just`.
    pub align: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<ShapeParagraphRun>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeParagraphRun {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<String>,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    pub colour: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeText {
    pub paragraphs: Vec<ShapeParagraph>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<String>,
    /// Left, top, right and bottom insets, in pixels at 100%.
    pub inset: [f64; 4],
    /// `t`, `ctr` or `b`.
    pub anchor: String,
    pub wrap: bool,
    /// Font size in pixels at 100%.
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
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

/// The presets this module draws, with their default adjust values (`adj`, `adj1`, ...).
const PRESETS: &[&str] = &[
    "rect",
    "roundRect",
    "ellipse",
    "line",
    "straightConnector1",
    "triangle",
    "rtTriangle",
    "diamond",
    "parallelogram",
    "trapezoid",
    "pentagon",
    "hexagon",
    "octagon",
    "rightArrow",
    "leftArrow",
    "upArrow",
    "downArrow",
    "chevron",
    "wedgeRectCallout",
    "wedgeRoundRectCallout",
    "flowChartProcess",
    "flowChartDecision",
    "flowChartConnector",
    "flowChartTerminator",
    "flowChartDocument",
    "bentConnector2",
    "bentConnector3",
    "bentConnector4",
    "bentConnector5",
    "curvedConnector2",
    "curvedConnector3",
    "curvedConnector4",
    "curvedConnector5",
];

/// Whether the geometry is a preset this module draws.
pub fn drawn_geometry(prst: &str) -> bool {
    PRESETS.contains(&prst)
}

/// Whether the preset is an open path (a line): it has no fill.
pub fn open_geometry(prst: &str) -> bool {
    matches!(prst, "line" | "straightConnector1") || prst.starts_with("bentConnector") || prst.starts_with("curvedConnector")
}

/// The adjust value `name`, or its default; a preset's formulas take them in 100000ths.
fn av(adjust: &[(String, i64)], name: &str, default: f64) -> f64 {
    adjust
        .iter()
        .find(|(n, _)| n == name)
        .map_or(default, |(_, v)| *v as f64)
}

fn pin(low: f64, value: f64, high: f64) -> f64 {
    value.max(low).min(high.max(low))
}

fn polygon(points: &[(f64, f64)]) -> Vec<Seg> {
    let mut out = Vec::with_capacity(points.len() + 1);
    for (i, &(x, y)) in points.iter().enumerate() {
        out.push(if i == 0 { Seg::Move(x, y) } else { Seg::Line(x, y) });
    }
    out.push(Seg::Close);
    out
}

/// A rectangle `w` by `h` with corners of radius `r`, as the arcs of its corners.
fn rounded(w: f64, h: f64, r: f64) -> Vec<Seg> {
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

/// `wedgeRectCallout` and `wedgeRoundRectCallout` (corner radius `r`): the rectangle with a tail to
/// the point `adj1`, `adj2` (fractions of the width and height from the centre) on the side the point
/// lies beyond, its base in the half of that side the point is toward.
fn callout(w: f64, h: f64, adj1: f64, adj2: f64, r: f64) -> Vec<Seg> {
    let dx = w * adj1 / 100_000.0;
    let dy = h * adj2 / 100_000.0;
    let (tip_x, tip_y) = (w / 2.0 + dx, h / 2.0 + dy);
    // The tail leaves the top or bottom when the point is farther off vertically than the box's diagonal says.
    let vertical = dy.abs() > (dx * h / w).abs();
    let (x1, x2) = if dx > 0.0 { (w * 7.0 / 12.0, w * 10.0 / 12.0) } else { (w * 2.0 / 12.0, w * 5.0 / 12.0) };
    let (y1, y2) = if dy > 0.0 { (h * 7.0 / 12.0, h * 10.0 / 12.0) } else { (h * 2.0 / 12.0, h * 5.0 / 12.0) };
    let tip = Seg::Line(tip_x, tip_y);
    let k = r * KAPPA;
    // A corner is an arc of radius `r`; with none, the straight sides meet there.
    let corner = |out: &mut Vec<Seg>, to: (f64, f64), c1: (f64, f64), c2: (f64, f64)| {
        if r > 0.0 {
            out.push(Seg::Cubic(c1, c2, to));
        }
    };
    let mut out = vec![Seg::Move(r, 0.0)];
    // Top, left to right.
    if vertical && dy < 0.0 {
        out.extend([Seg::Line(x1, 0.0), tip, Seg::Line(x2, 0.0)]);
    }
    out.push(Seg::Line(w - r, 0.0));
    corner(&mut out, (w, r), (w - r + k, 0.0), (w, r - k));
    // Right, top to bottom.
    if !vertical && dx > 0.0 {
        out.extend([Seg::Line(w, y1), tip, Seg::Line(w, y2)]);
    }
    out.push(Seg::Line(w, h - r));
    corner(&mut out, (w - r, h), (w, h - r + k), (w - r + k, h));
    // Bottom, right to left.
    if vertical && dy > 0.0 {
        out.extend([Seg::Line(x2, h), tip, Seg::Line(x1, h)]);
    }
    out.push(Seg::Line(r, h));
    corner(&mut out, (0.0, h - r), (r - k, h), (0.0, h - r + k));
    // Left, bottom to top.
    if !vertical && dx < 0.0 {
        out.extend([Seg::Line(0.0, y2), tip, Seg::Line(0.0, y1)]);
    }
    if r > 0.0 {
        out.push(Seg::Line(0.0, r));
        corner(&mut out, (r, 0.0), (0.0, r - k), (r - k, 0.0));
    }
    out.push(Seg::Close);
    out
}

/// The path of preset `prst` in a `w` by `h` box with the adjust values `adjust` (by name, in
/// 100000ths; the preset's default for one not given), `None` for a preset not drawn. The formulas are
/// those of ECMA-376 Part 1's presetShapeDefinitions: `ss` is the shorter side, an adjust is pinned
/// to its range as the definition pins it.
pub fn geometry_path(prst: &str, w: f64, h: f64, adjust: &[(String, i64)]) -> Option<Vec<Seg>> {
    let ss = w.min(h);
    let (hc, vc) = (w / 2.0, h / 2.0);
    Some(match prst {
        "rect" | "flowChartProcess" => polygon(&[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]),
        "roundRect" => {
            let a = pin(0.0, av(adjust, "adj", 16667.0), 50000.0);
            rounded(w, h, ss * a / 100_000.0)
        }
        "ellipse" | "flowChartConnector" => {
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
        "triangle" => {
            let a = pin(0.0, av(adjust, "adj", 50000.0), 100_000.0);
            polygon(&[(0.0, h), (w * a / 100_000.0, 0.0), (w, h)])
        }
        "rtTriangle" => polygon(&[(0.0, h), (0.0, 0.0), (w, h)]),
        "diamond" | "flowChartDecision" => polygon(&[(0.0, vc), (hc, 0.0), (w, vc), (hc, h)]),
        "parallelogram" => {
            let max = if ss > 0.0 { 100_000.0 * w / ss } else { 0.0 };
            let x2 = ss * pin(0.0, av(adjust, "adj", 25000.0), max) / 100_000.0;
            polygon(&[(0.0, h), (x2, 0.0), (w, 0.0), (w - x2, h)])
        }
        "trapezoid" => {
            let max = if ss > 0.0 { 50000.0 * w / ss } else { 0.0 };
            let x2 = ss * pin(0.0, av(adjust, "adj", 25000.0), max) / 100_000.0;
            polygon(&[(0.0, h), (x2, 0.0), (w - x2, 0.0), (w, h)])
        }
        "pentagon" => {
            // A regular pentagon on the ellipse through the box's top: radii stretched by 1.05146 and
            // 1.10557 so its corners touch the box's sides and bottom.
            let (rx, ry) = (hc * 1.05146, vc * 1.10557);
            let centre_y = vc * 1.10557;
            let points: Vec<(f64, f64)> = [-90.0f64, -18.0, 54.0, 126.0, 198.0]
                .iter()
                .map(|deg| {
                    let t = deg.to_radians();
                    (hc + rx * t.cos(), centre_y + ry * t.sin())
                })
                .collect();
            polygon(&points)
        }
        "hexagon" => {
            let max = if ss > 0.0 { 50000.0 * w / ss } else { 0.0 };
            let x1 = ss * pin(0.0, av(adjust, "adj", 25000.0), max) / 100_000.0;
            polygon(&[(0.0, vc), (x1, 0.0), (w - x1, 0.0), (w, vc), (w - x1, h), (x1, h)])
        }
        "octagon" => {
            let x1 = ss * pin(0.0, av(adjust, "adj", 29289.0), 50000.0) / 100_000.0;
            polygon(&[
                (0.0, x1),
                (x1, 0.0),
                (w - x1, 0.0),
                (w, x1),
                (w, h - x1),
                (w - x1, h),
                (x1, h),
                (0.0, h - x1),
            ])
        }
        "rightArrow" | "leftArrow" => {
            let max2 = if ss > 0.0 { 100_000.0 * w / ss } else { 0.0 };
            let a1 = pin(0.0, av(adjust, "adj1", 50000.0), 100_000.0);
            let a2 = pin(0.0, av(adjust, "adj2", 50000.0), max2);
            let head = ss * a2 / 100_000.0;
            let dy = h * a1 / 200_000.0;
            let (y1, y2) = (vc - dy, vc + dy);
            if prst == "rightArrow" {
                let x1 = w - head;
                polygon(&[(0.0, y1), (x1, y1), (x1, 0.0), (w, vc), (x1, h), (x1, y2), (0.0, y2)])
            } else {
                polygon(&[(0.0, vc), (head, 0.0), (head, y1), (w, y1), (w, y2), (head, y2), (head, h)])
            }
        }
        "upArrow" | "downArrow" => {
            let max2 = if ss > 0.0 { 100_000.0 * h / ss } else { 0.0 };
            let a1 = pin(0.0, av(adjust, "adj1", 50000.0), 100_000.0);
            let a2 = pin(0.0, av(adjust, "adj2", 50000.0), max2);
            let head = ss * a2 / 100_000.0;
            let dx = w * a1 / 200_000.0;
            let (x1, x2) = (hc - dx, hc + dx);
            if prst == "upArrow" {
                polygon(&[(0.0, head), (hc, 0.0), (w, head), (x2, head), (x2, h), (x1, h), (x1, head)])
            } else {
                let y2 = h - head;
                polygon(&[(x1, 0.0), (x2, 0.0), (x2, y2), (w, y2), (hc, h), (0.0, y2), (x1, y2)])
            }
        }
        "chevron" => {
            let max = if ss > 0.0 { 100_000.0 * w / ss } else { 0.0 };
            let x1 = ss * pin(0.0, av(adjust, "adj", 50000.0), max) / 100_000.0;
            polygon(&[(0.0, 0.0), (w - x1, 0.0), (w, vc), (w - x1, h), (0.0, h), (x1, vc)])
        }
        "bentConnector2" => vec![Seg::Move(0.0, 0.0), Seg::Line(w, 0.0), Seg::Line(w, h)],
        "bentConnector3" => {
            let x1 = w * av(adjust, "adj1", 50000.0) / 100_000.0;
            vec![Seg::Move(0.0, 0.0), Seg::Line(x1, 0.0), Seg::Line(x1, h), Seg::Line(w, h)]
        }
        "bentConnector4" => {
            let x1 = w * av(adjust, "adj1", 50000.0) / 100_000.0;
            let y2 = h * av(adjust, "adj2", 50000.0) / 100_000.0;
            vec![Seg::Move(0.0, 0.0), Seg::Line(x1, 0.0), Seg::Line(x1, y2), Seg::Line(w, y2), Seg::Line(w, h)]
        }
        "bentConnector5" => {
            let x1 = w * av(adjust, "adj1", 50000.0) / 100_000.0;
            let y2 = h * av(adjust, "adj2", 50000.0) / 100_000.0;
            let x3 = w * av(adjust, "adj3", 50000.0) / 100_000.0;
            vec![
                Seg::Move(0.0, 0.0),
                Seg::Line(x1, 0.0),
                Seg::Line(x1, y2),
                Seg::Line(x3, y2),
                Seg::Line(x3, h),
                Seg::Line(w, h),
            ]
        }
        "curvedConnector2" => vec![Seg::Move(0.0, 0.0), Seg::Cubic((hc, 0.0), (w, vc), (w, h))],
        "curvedConnector3" => {
            let x2 = w * av(adjust, "adj1", 50000.0) / 100_000.0;
            let (x1, x3) = (x2 / 2.0, (w + x2) / 2.0);
            vec![
                Seg::Move(0.0, 0.0),
                Seg::Cubic((x1, 0.0), (x2, h / 4.0), (x2, vc)),
                Seg::Cubic((x2, h * 3.0 / 4.0), (x3, h), (w, h)),
            ]
        }
        "curvedConnector4" => {
            let x2 = w * av(adjust, "adj1", 50000.0) / 100_000.0;
            let y4 = h * av(adjust, "adj2", 50000.0) / 100_000.0;
            let (x1, x3) = (x2 / 2.0, (w + x2) / 2.0);
            let (x4, x5) = ((x2 + x3) / 2.0, (x3 + w) / 2.0);
            let y1 = y4 / 2.0;
            let (y2, y3, y5) = (y1 / 2.0, (y1 + y4) / 2.0, (h + y4) / 2.0);
            vec![
                Seg::Move(0.0, 0.0),
                Seg::Cubic((x1, 0.0), (x2, y2), (x2, y1)),
                Seg::Cubic((x2, y3), (x4, y4), (x3, y4)),
                Seg::Cubic((x5, y4), (w, y5), (w, h)),
            ]
        }
        "curvedConnector5" => {
            let x3 = w * av(adjust, "adj1", 50000.0) / 100_000.0;
            let y4 = h * av(adjust, "adj2", 50000.0) / 100_000.0;
            let x6 = w * av(adjust, "adj3", 50000.0) / 100_000.0;
            let x1 = (x3 + x6) / 2.0;
            let (x2, x4, x5, x7) = (x3 / 2.0, (x3 + x1) / 2.0, (x6 + x1) / 2.0, (x6 + w) / 2.0);
            let y1 = y4 / 2.0;
            let (y2, y3) = (y1 / 2.0, (y1 + y4) / 2.0);
            let y5 = (h + y4) / 2.0;
            let (y6, y7) = ((y4 + y5) / 2.0, (y5 + h) / 2.0);
            vec![
                Seg::Move(0.0, 0.0),
                Seg::Cubic((x2, 0.0), (x3, y2), (x3, y1)),
                Seg::Cubic((x3, y3), (x4, y4), (x1, y4)),
                Seg::Cubic((x5, y4), (x6, y6), (x6, y5)),
                Seg::Cubic((x6, y7), (x7, h), (w, h)),
            ]
        }
        "wedgeRectCallout" => callout(w, h, av(adjust, "adj1", -20833.0), av(adjust, "adj2", 62500.0), 0.0),
        "wedgeRoundRectCallout" => {
            let r = ss * pin(0.0, av(adjust, "adj3", 16667.0), 50000.0) / 100_000.0;
            callout(w, h, av(adjust, "adj1", -20833.0), av(adjust, "adj2", 62500.0), r)
        }
        "flowChartTerminator" => {
            // The definition's 21600-unit path: semicircular ends 3475 units deep.
            let rx = w * 3475.0 / 21600.0;
            let (kx, ky) = (rx * KAPPA, vc * KAPPA);
            vec![
                Seg::Move(rx, 0.0),
                Seg::Line(w - rx, 0.0),
                Seg::Cubic((w - rx + kx, 0.0), (w, vc - ky), (w, vc)),
                Seg::Cubic((w, vc + ky), (w - rx + kx, h), (w - rx, h)),
                Seg::Line(rx, h),
                Seg::Cubic((rx - kx, h), (0.0, vc + ky), (0.0, vc)),
                Seg::Cubic((0.0, vc - ky), (rx - kx, 0.0), (rx, 0.0)),
                Seg::Close,
            ]
        }
        "flowChartDocument" => {
            let (sx, sy) = (w / 21600.0, h / 21600.0);
            vec![
                Seg::Move(0.0, 0.0),
                Seg::Line(w, 0.0),
                Seg::Line(w, 17322.0 * sy),
                Seg::Cubic((10800.0 * sx, 17322.0 * sy), (10800.0 * sx, 23922.0 * sy), (0.0, 20172.0 * sy)),
                Seg::Close,
            ]
        }
        _ => return None,
    })
}

/// The preset's text rectangle `[left, top, right, bottom]` in a `w` by `h` box (the `rect` element of its
/// ECMA-376 definition): the box the text's insets are taken from. A preset whose rectangle is not worked
/// out here uses the whole frame (assumed; Excel decides).
pub fn text_rect(prst: &str, w: f64, h: f64, adjust: &[(String, i64)]) -> [f64; 4] {
    let ss = w.min(h);
    let (hc, vc) = (w / 2.0, h / 2.0);
    let whole = [0.0, 0.0, w, h];
    match prst {
        "roundRect" => {
            let il = ss * pin(0.0, av(adjust, "adj", 16667.0), 50000.0) / 100_000.0 * 0.292_893_218_8;
            [il, il, w - il, h - il]
        }
        "wedgeRoundRectCallout" => {
            let il = ss * pin(0.0, av(adjust, "adj3", 16667.0), 50000.0) / 100_000.0 * 0.292_893_218_8;
            [il, il, w - il, h - il]
        }
        // The square inscribed in the ellipse: cos 45 degrees of each radius.
        "ellipse" | "flowChartConnector" => {
            let (dx, dy) = (hc * std::f64::consts::FRAC_1_SQRT_2, vc * std::f64::consts::FRAC_1_SQRT_2);
            [hc - dx, vc - dy, hc + dx, vc + dy]
        }
        "triangle" => {
            let x1 = w * pin(0.0, av(adjust, "adj", 50000.0), 100_000.0) / 200_000.0;
            [x1, vc, x1 + hc, h]
        }
        "diamond" | "flowChartDecision" => [w / 4.0, h / 4.0, w * 3.0 / 4.0, h * 3.0 / 4.0],
        "octagon" => {
            let il = ss * pin(0.0, av(adjust, "adj", 29289.0), 50000.0) / 100_000.0 / 2.0;
            [il, il, w - il, h - il]
        }
        "trapezoid" => {
            let max = if ss > 0.0 { 50000.0 * w / ss } else { 1.0 };
            let a = pin(0.0, av(adjust, "adj", 25000.0), max);
            [w / 3.0 * a / max, h / 3.0 * a / max, w - w / 3.0 * a / max, h]
        }
        "rightArrow" | "leftArrow" => {
            let max2 = if ss > 0.0 { 100_000.0 * w / ss } else { 0.0 };
            let a1 = pin(0.0, av(adjust, "adj1", 50000.0), 100_000.0);
            let a2 = pin(0.0, av(adjust, "adj2", 50000.0), max2);
            let head = ss * a2 / 100_000.0;
            let dy = h * a1 / 200_000.0;
            let y1 = vc - dy;
            // The text runs into the head along the slope of its edge.
            let into = if vc > 0.0 { y1 * head / vc } else { 0.0 };
            if prst == "rightArrow" { [0.0, y1, w - head + into, vc + dy] } else { [head - into, y1, w, vc + dy] }
        }
        "upArrow" | "downArrow" => {
            let max2 = if ss > 0.0 { 100_000.0 * h / ss } else { 0.0 };
            let a1 = pin(0.0, av(adjust, "adj1", 50000.0), 100_000.0);
            let a2 = pin(0.0, av(adjust, "adj2", 50000.0), max2);
            let head = ss * a2 / 100_000.0;
            let dx = w * a1 / 200_000.0;
            let x1 = hc - dx;
            let into = if hc > 0.0 { x1 * head / hc } else { 0.0 };
            if prst == "upArrow" { [x1, head - into, hc + dx, h] } else { [x1, 0.0, hc + dx, h - head + into] }
        }
        "chevron" => {
            let max = if ss > 0.0 { 100_000.0 * w / ss } else { 0.0 };
            let x1 = ss * pin(0.0, av(adjust, "adj", 50000.0), max) / 100_000.0;
            let x2 = w - x1;
            if x2 - x1 > 0.0 { [x1, 0.0, x2, h] } else { whole }
        }
        "flowChartTerminator" => [w * 1018.0 / 21600.0, h * 3163.0 / 21600.0, w * 20582.0 / 21600.0, h * 18437.0 / 21600.0],
        "flowChartDocument" => [0.0, 0.0, w, h * 17322.0 / 21600.0],
        "rtTriangle" => [w / 12.0, h / 2.0, w * 7.0 / 12.0, h * 11.0 / 12.0],
        _ => whole,
    }
}

/// The point of the box `w` by `h` a shadow's `algn` names (`b` is bottom centre).
fn align_point(align: &str, w: f64, h: f64) -> (f64, f64) {
    let x = match align {
        "tl" | "l" | "bl" => 0.0,
        "tr" | "r" | "br" => w,
        _ => w / 2.0,
    };
    let y = match align {
        "tl" | "t" | "tr" => 0.0,
        "bl" | "b" | "br" => h,
        _ => h / 2.0,
    };
    (x, y)
}

/// The end points of an open path with the direction a stroke leaves each one (unit vectors pointing out
/// of the path): `(start, start_out, end, end_out)`; `None` for a path with no length.
fn path_ends(segments: &[Seg]) -> Option<((f64, f64), (f64, f64), (f64, f64), (f64, f64))> {
    let unit = |from: (f64, f64), to: (f64, f64)| {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = dx.hypot(dy);
        (length > 1e-9).then(|| (dx / length, dy / length))
    };
    let Some(&Seg::Move(sx, sy)) = segments.first() else { return None };
    let start = (sx, sy);
    let mut here = start;
    let mut first = None;
    let mut last = None;
    for segment in &segments[1..] {
        let (points, end): (Vec<(f64, f64)>, (f64, f64)) = match *segment {
            Seg::Line(x, y) => (vec![(x, y)], (x, y)),
            Seg::Cubic(a, b, p) => (vec![a, b, p], p),
            _ => continue,
        };
        if first.is_none() {
            first = points.iter().find_map(|&q| unit(start, q));
        }
        if let Some(direction) = points.iter().rev().find_map(|&q| unit(q, end)).or_else(|| unit(here, end)) {
            last = Some(direction);
        }
        here = end;
    }
    let (first, last) = (first?, last?);
    Some((start, (-first.0, -first.1), here, last))
}

/// `segments` mirrored across the box `w` by `h`.
fn flip_segments(segments: &[Seg], w: f64, h: f64, flip_h: bool, flip_v: bool) -> Vec<Seg> {
    let p = |(x, y): (f64, f64)| (if flip_h { w - x } else { x }, if flip_v { h - y } else { y });
    segments
        .iter()
        .map(|segment| match *segment {
            Seg::Move(x, y) => {
                let (x, y) = p((x, y));
                Seg::Move(x, y)
            }
            Seg::Line(x, y) => {
                let (x, y) = p((x, y));
                Seg::Line(x, y)
            }
            Seg::Cubic(a, b, c) => Seg::Cubic(p(a), p(b), p(c)),
            Seg::Close => Seg::Close,
        })
        .collect()
}

fn number(value: f64) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0".to_owned() } else { text.to_owned() }
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

/// Quote typeface text as CSS escapes, so it cannot escape the style attribute.
fn font_family(face: Option<&str>) -> String {
    face.filter(|face| !face.is_empty() && !face.chars().any(char::is_control)).map(|face| {
        let mut escaped = String::new();
        for c in face.chars() { let _ = write!(escaped, "\\{:x} ", c as u32); }
        format!(";font-family:'{escaped}'")
    }).unwrap_or_default()
}

/// The size of an arrowhead's `w` or `len` in line widths.
fn arrow_factor(size: &str) -> f64 {
    match size {
        "sm" => 2.0,
        "lg" => 5.0,
        _ => 3.0,
    }
}

/// An arrowhead at `tip` of a line running toward it along the unit vector `dir`, for a line `width`
/// pixels wide: the markup of its path and how far the line must stop short of the tip so it does not
/// show through (a triangle's base, a stealth's notch). Lines thinner than two pixels size their heads as a two pixel line.
fn arrowhead(arrow: &ShapeArrow, tip: (f64, f64), dir: (f64, f64), width: f64, colour: &str) -> Option<(String, f64)> {
    let unit = width.max(2.0);
    let (len, wide) = (arrow_factor(&arrow.length) * unit, arrow_factor(&arrow.width) * unit);
    // Local frame: x along `dir`, tip at the origin, y to the right of `dir`.
    let at = |x: f64, y: f64| (tip.0 + dir.0 * x - dir.1 * y, tip.1 + dir.1 * x + dir.0 * y);
    let point = |(x, y): (f64, f64)| format!("{} {}", number(x), number(y));
    let polygon = |points: &[(f64, f64)]| {
        let list: Vec<String> = points.iter().map(|p| point(*p)).collect();
        format!("M{}Z", list.join("L"))
    };
    let (d, filled, trim) = match arrow.kind.as_str() {
        "triangle" => (polygon(&[at(0.0, 0.0), at(-len, wide / 2.0), at(-len, -wide / 2.0)]), true, len),
        "stealth" => {
            let notch = len * 0.75;
            (
                polygon(&[at(0.0, 0.0), at(-len, wide / 2.0), at(-notch, 0.0), at(-len, -wide / 2.0)]),
                true,
                notch,
            )
        }
        "diamond" => (
            polygon(&[at(len / 2.0, 0.0), at(0.0, wide / 2.0), at(-len / 2.0, 0.0), at(0.0, -wide / 2.0)]),
            true,
            0.0,
        ),
        "oval" => {
            let angle = dir.1.atan2(dir.0).to_degrees();
            return Some((
                format!(
                    "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" transform=\"rotate({} {} {})\" fill=\"{colour}\" stroke=\"none\"/>",
                    number(tip.0),
                    number(tip.1),
                    number(len / 2.0),
                    number(wide / 2.0),
                    number(angle),
                    number(tip.0),
                    number(tip.1)
                ),
                0.0,
            ));
        }
        "arrow" => (
            format!("M{}L{}L{}", point(at(-len, wide / 2.0)), point(at(0.0, 0.0)), point(at(-len, -wide / 2.0))),
            false,
            0.0,
        ),
        _ => return None,
    };
    let paint = if filled {
        format!("fill=\"{colour}\" stroke=\"none\"")
    } else {
        format!("fill=\"none\" stroke=\"{colour}\" stroke-width=\"{}\" stroke-linejoin=\"miter\"", number(width))
    };
    Some((format!("<path d=\"{d}\" {paint}/>"), trim))
}

/// A stable identifier for the gradient of the object `name` among the page's inline pictures.
fn gradient_id(name: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in name.bytes() {
        hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
    }
    format!("dcg{hash:08x}")
}

/// The `defs` of `gradient` for a `w` by `h` box drawn flipped as the shape is, and its id.
fn gradient_defs(g: &ShapeGradient, id: &str, w: f64, h: f64, flip_h: bool, flip_v: bool) -> String {
    let stops: String = g
        .stops
        .iter()
        .map(|(pos, colour)| format!("<stop offset=\"{}\" stop-color=\"{colour}\"/>", number(pos.clamp(0.0, 1.0))))
        .collect();
    if let Some(path) = &g.path {
        let _ = path;
        // A rectangle or shape path has no SVG equivalent; all three draw as a radial gradient from the focus
        // out to the farthest corner.
        let fx = if flip_h { 1.0 - g.focus[0] } else { g.focus[0] };
        let fy = if flip_v { 1.0 - g.focus[1] } else { g.focus[1] };
        let reach = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
            .iter()
            .map(|(x, y): &(f64, f64)| ((x - fx).powi(2) + (y - fy).powi(2)).sqrt())
            .fold(0.0, f64::max);
        return format!(
            "<defs><radialGradient id=\"{id}\" cx=\"{}\" cy=\"{}\" fx=\"{}\" fy=\"{}\" r=\"{}\">{stops}</radialGradient></defs>",
            number(fx),
            number(fy),
            number(fx),
            number(fy),
            number(reach)
        );
    }
    // A flipped shape flips its gradient with it.
    let mut angle = g.angle;
    if flip_h {
        angle = 180.0 - angle;
    }
    if flip_v {
        angle = -angle;
    }
    let (sin, cos) = angle.to_radians().sin_cos();
    if g.scaled {
        // In the box's own unit square, where the angle follows the aspect ratio.
        let half = (cos.abs() + sin.abs()) / 2.0;
        format!(
            "<defs><linearGradient id=\"{id}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">{stops}</linearGradient></defs>",
            number(0.5 - cos * half),
            number(0.5 - sin * half),
            number(0.5 + cos * half),
            number(0.5 + sin * half)
        )
    } else {
        // In pixels: the angle is the real one on the screen, the line reaching the corners it projects.
        let half = (w * cos.abs() + h * sin.abs()) / 2.0;
        let (cx, cy) = (w / 2.0, h / 2.0);
        format!(
            "<defs><linearGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">{stops}</linearGradient></defs>",
            number(cx - cos * half),
            number(cy - sin * half),
            number(cx + cos * half),
            number(cy + sin * half)
        )
    }
}

/// Where an object lands when its anchor frame `(left, top, right, bottom)` is the box around the
/// shape rotated: the unrotated shape's box, about the same centre (`extent` are fractions of the frame).
pub fn unrotated(frame: (f64, f64, f64, f64), extent: [f64; 2]) -> (f64, f64, f64, f64) {
    let (cx, cy) = ((frame.0 + frame.2) / 2.0, (frame.1 + frame.3) / 2.0);
    let (w, h) = ((frame.2 - frame.0) * extent[0], (frame.3 - frame.1) * extent[1]);
    (cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0)
}

/// The size of a shape `cx` by `cy` rotated by `degrees`, as fractions of the box around the rotated shape.
pub fn extent_in_rotated_box(cx: f64, cy: f64, degrees: f64) -> [f64; 2] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    // A quarter turn's cosine or sine is zero, not 1e-16.
    let (sin, cos) = (if sin.abs() < 1e-12 { 0.0 } else { sin }, if cos.abs() < 1e-12 { 0.0 } else { cos });
    let (bw, bh) = (cx * cos.abs() + cy * sin.abs(), cx * sin.abs() + cy * cos.abs());
    [if bw > 0.0 { cx / bw } else { 1.0 }, if bh > 0.0 { cy / bh } else { 1.0 }]
}

/// The inside of one object: its geometry as an SVG with the text over it as HTML, then a group's children
/// each in a box of their own. `id` names the gradient among the page's others.
fn inner_markup(shape: &DrawnShape, w: f64, h: f64, zoom: f64, id: &str) -> Option<String> {
    if shape.kind == "group" {
        return Some(finish(shape, w, h, zoom, id, String::new(), ""));
    }
    if let Some(picture) = shape.picture.as_ref().filter(|_| shape.kind == "picture") {
        // The picture's image is the page's own element; markup draws its placeholder, then the outline.
        let label = picture.placeholder.clone().unwrap_or_else(|| "Picture".to_owned());
        let mut out = String::new();
        if picture.placeholder.is_some() {
            let _ = write!(
                out,
                "<div class=\"dc-picture-placeholder\" style=\"position:absolute;left:0;top:0;width:{}px;height:{}px;box-sizing:border-box;border:1px dashed #808080;background:#F2F2F2;color:#404040;font-size:{}px;display:flex;align-items:center;justify-content:center;text-align:center;overflow:hidden\">{}</div>",
                number(w),
                number(h),
                number(11.0 * 96.0 / 72.0 * zoom),
                escape(&label)
            );
        }
        let mut frame = shape.clone();
        frame.picture = None;
        frame.kind = "shape".into();
        frame.fill = None;
        frame.gradient = None;
        frame.text = None;
        if frame.line.is_some() {
            if let Some(path) = inner_markup(&frame, w, h, zoom, id) {
                out.push_str(&path);
            }
        }
        return Some(out);
    }
    let mut segments = geometry_path(&shape.geometry, w, h, &shape.adjust)?;
    let open = open_geometry(&shape.geometry);
    let mut stroke = String::from("stroke=\"none\"");
    // The stroke's width and dashes, for the shadow to repeat in its own colour.
    let mut stroke_rest = String::new();
    let mut heads = String::new();
    // A line is flipped by its end points, so its arrowheads sit on the flipped ends.
    let (mut flip_h, mut flip_v) = (shape.flip_horizontal, shape.flip_vertical);
    if let Some(line) = &shape.line {
        let width = line.width * zoom;
        stroke_rest = format!("stroke-width=\"{}\"", number(width));
        let dashes = dash_pattern(&line.dash);
        if !dashes.is_empty() {
            let list: Vec<String> = dashes.iter().map(|d| number(d * width)).collect();
            let _ = write!(stroke_rest, " stroke-dasharray=\"{}\"", list.join(" "));
        }
        stroke = format!("stroke=\"{}\" {stroke_rest}", line.colour);
        if open {
            // The path is flipped by its points, so its arrowheads sit on the flipped ends and follow the
            // direction the path leaves them in (a bent or curved connector's first and last segments).
            segments = flip_segments(&segments, w, h, flip_h, flip_v);
            (flip_h, flip_v) = (false, false);
            if let Some((tip_a, out_a, tip_b, out_b)) = path_ends(&segments) {
                let trim_a = line.head.as_ref().and_then(|e| arrowhead(e, tip_a, out_a, width, &line.colour));
                let trim_b = line.tail.as_ref().and_then(|e| arrowhead(e, tip_b, out_b, width, &line.colour));
                if let Some((markup, trim)) = trim_a {
                    heads.push_str(&markup);
                    segments[0] = Seg::Move(tip_a.0 - out_a.0 * trim, tip_a.1 - out_a.1 * trim);
                }
                if let Some((markup, trim)) = trim_b {
                    heads.push_str(&markup);
                    let end = (tip_b.0 - out_b.0 * trim, tip_b.1 - out_b.1 * trim);
                    match segments.last_mut() {
                        Some(Seg::Line(x, y)) => (*x, *y) = end,
                        Some(Seg::Cubic(_, _, p)) => *p = end,
                        _ => {}
                    }
                }
            }
        }
    }
    let d = path_data(&segments, w, h, flip_h, flip_v);
    let mut defs = String::new();
    let fill = if open {
        "none".to_owned()
    } else if let Some(gradient) = &shape.gradient {
        defs = gradient_defs(gradient, id, w, h, shape.flip_horizontal, shape.flip_vertical);
        format!("url(#{id})")
    } else {
        shape.fill.clone().unwrap_or_else(|| "none".to_owned())
    };
    let mut shadow = String::new();
    if let Some(s) = &shape.shadow {
        let filled = !open && (shape.gradient.is_some() || shape.fill.is_some());
        let stroked = shape.line.is_some();
        if filled || stroked {
            let (anchor_x, anchor_y) = align_point(&s.align, w, h);
            let (mut dx, mut dy) = (s.dx * zoom, s.dy * zoom);
            if !s.rotate {
                // The offset stays where the file points it while the shape turns.
                let (sin, cos) = (-shape.rotation).to_radians().sin_cos();
                (dx, dy) = (dx * cos - dy * sin, dx * sin + dy * cos);
            }
            let sigma = s.blur * zoom / 2.0;
            let filter = if sigma > 0.0 {
                let _ = write!(
                    defs,
                    "<filter id=\"{id}s\" x=\"-50%\" y=\"-50%\" width=\"200%\" height=\"200%\"><feGaussianBlur stdDeviation=\"{}\"/></filter>",
                    number(sigma)
                );
                format!(" filter=\"url(#{id}s)\"")
            } else {
                String::new()
            };
            let paint = format!(
                "fill=\"{}\" {}",
                if filled { s.colour.as_str() } else { "none" },
                if stroked { format!("stroke=\"{}\" {stroke_rest}", s.colour) } else { "stroke=\"none\"".to_owned() }
            );
            let _ = write!(
                shadow,
                "<path d=\"{d}\" {paint}{filter} transform=\"translate({} {}) translate({} {}) scale({} {}) translate({} {})\"/>",
                number(dx),
                number(dy),
                number(anchor_x),
                number(anchor_y),
                number(s.scale[0]),
                number(s.scale[1]),
                number(-anchor_x),
                number(-anchor_y)
            );
        }
    }
    Some(finish(shape, w, h, zoom, id, defs, &format!("{shadow}<path d=\"{d}\" fill=\"{fill}\" {stroke}/>{heads}")))
}

/// The svg of `paths`, the text of `shape` over it and a group's children, after `out`.
fn finish(shape: &DrawnShape, w: f64, h: f64, zoom: f64, id: &str, defs: String, paths: &str) -> String {
    let mut out = String::new();
    if !paths.is_empty() {
        let _ = write!(
            out,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" overflow=\"visible\">{defs}{paths}</svg>",
            number(w),
            number(h)
        );
    }
    if let Some(text) = &shape.text {
        let [l, t, r, b] = text.inset.map(|v| v * zoom);
        let justify = match text.anchor.as_str() {
            "ctr" => "center",
            "b" => "flex-end",
            _ => "flex-start",
        };
        // A shape flipped vertically carries its text upside down, as Excel's does.
        let turn = if shape.flip_vertical { "transform:rotate(180deg);" } else { "" };
        let [rl, rt, rr, rb] = text_rect(&shape.geometry, w, h, &shape.adjust);
        let _ = write!(
            out,
            "<div class=\"dc-shape-text\" style=\"position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;box-sizing:border-box;padding:{}px {}px {}px {}px;display:flex;flex-direction:column;justify-content:{justify};font-size:{}px;line-height:1.2;color:{};font-weight:{};font-style:{}{}{};white-space:{};overflow:visible;{turn}\">",
            number(rl),
            number(rt),
            number((rr - rl).max(0.0)),
            number((rb - rt).max(0.0)),
            number(t),
            number(r),
            number(b),
            number(l),
            number(text.size * zoom),
            text.colour,
            if text.bold { "bold" } else { "normal" },
            if text.italic { "italic" } else { "normal" },
            font_family(text.face.as_deref()),
            if text.underline { ";text-decoration:underline" } else { "" },
            if text.wrap { "pre-wrap" } else { "pre" },
        );
        for paragraph in &text.paragraphs {
            let align = match paragraph.align.as_str() {
                "ctr" => "center",
                "r" => "right",
                "just" => "justify",
                _ => "left",
            };
            let _ = write!(out, "<p style=\"margin:0;min-height:1.2em;text-align:{align}\">");
            if !paragraph.runs.is_empty() && paragraph.runs.iter().map(|r| r.text.as_str()).collect::<String>() == paragraph.text {
                for run in &paragraph.runs {
                    let face = font_family(run.face.as_deref());
                    let _ = write!(out, "<span style=\"font-size:{}px;font-weight:{};font-style:{};color:{}{}{}\">{}</span>",
                        number(run.size * zoom), if run.bold { "bold" } else { "normal" },
                        if run.italic { "italic" } else { "normal" }, run.colour, face,
                        if run.underline { ";text-decoration:underline" } else { "" }, escape(&run.text));
                }
            } else {
                out.push_str(&escape(&paragraph.text));
            }
            out.push_str("</p>");
        }
        out.push_str("</div>");
    }
    for (index, child) in shape.children.iter().enumerate() {
        let [rx, ry, rw, rh] = child.rel;
        let (cw, ch) = (w * rw, h * rh);
        let (left, top) = (w * rx, h * ry);
        let child_id = format!("{id}c{index}");
        if let Some(body) = inner_markup(&child.shape, cw, ch, zoom, &child_id) {
            let turn = rotation_style(child.shape.rotation);
            let _ = write!(
                out,
                "<div style=\"position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;{turn}\">{body}</div>",
                number(left),
                number(top),
                number(cw),
                number(ch)
            );
        }
    }
    out
}

fn rotation_style(degrees: f64) -> String {
    if degrees.rem_euclid(360.0) == 0.0 {
        String::new()
    } else {
        format!("transform:rotate({}deg);", number(degrees))
    }
}

/// The markup of `shape` for an (unrotated) box `w` by `h` pixels at zoom `zoom`: an SVG of the geometry
/// (with its gradient and arrowheads) with the text over it as HTML, padded by the insets and placed by the
/// vertical anchor, a group's children in boxes inside it, the whole turned by the shape's rotation about its
/// centre. `None` for a geometry not drawn.
pub fn shape_markup(shape: &DrawnShape, w: f64, h: f64, zoom: f64) -> Option<String> {
    let id = gradient_id(&shape.name);
    let body = inner_markup(shape, w, h, zoom, &id)?;
    let turn = rotation_style(shape.rotation);
    if turn.is_empty() {
        Some(body)
    } else {
        Some(format!(
            "<div style=\"position:absolute;left:0;top:0;width:{}px;height:{}px;{turn}\">{body}</div>",
            number(w),
            number(h)
        ))
    }
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
        let rect = geometry_path("rect", 40.0, 20.0, &[]).unwrap();
        assert_eq!(path_data(&rect, 40.0, 20.0, false, false), "M0 0L40 0L40 20L0 20Z");
        // A rounded rectangle's radius is a sixth of the shorter side, 20 * 0.16667 = 3.333.
        let round = path_data(&geometry_path("roundRect", 40.0, 20.0, &[]).unwrap(), 40.0, 20.0, false, false);
        assert!(round.starts_with("M3.333 0L36.667 0C"), "{round}");
        assert!(round.ends_with("Z"));
        // An ellipse touches the middle of each side.
        let oval = path_data(&geometry_path("ellipse", 40.0, 20.0, &[]).unwrap(), 40.0, 20.0, false, false);
        assert!(oval.starts_with("M0 10C0 4.477 "), "{oval}");
        assert!(oval.contains(" 20 0C") && oval.contains(" 40 10C") && oval.contains(" 20 20C"), "{oval}");
        // A line flipped vertically runs from the bottom-left corner to the top-right.
        let line = geometry_path("straightConnector1", 40.0, 20.0, &[]).unwrap();
        assert_eq!(path_data(&line, 40.0, 20.0, false, true), "M0 20L40 0");
        assert_eq!(path_data(&line, 40.0, 20.0, true, false), "M40 0L0 20");
        assert!(geometry_path("star5", 40.0, 20.0, &[]).is_none());
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
            gradient: None,
            rotation: 0.0,
            extent: [1.0, 1.0],
            adjust: Vec::new(),
            children: Vec::new(),
            id: 0,
            picture: None,
            shadow: None,
            line: Some(ShapeStroke { width: 2.0, colour: "#ED7D31".into(), dash: "dash".into(), head: None, tail: None }),
            text: Some(ShapeText {
                face: None,
                paragraphs: vec![
                    ShapeParagraph { text: "a < b".into(), align: "ctr".into(), runs: Vec::new() },
                    ShapeParagraph { text: String::new(), align: "l".into(), runs: Vec::new() },
                ],
                inset: [9.6, 4.8, 9.6, 4.8],
                anchor: "ctr".into(),
                wrap: true,
                size: 14.666,
                bold: true,
                italic: false,
                underline: false,
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
        let mut named = sample();
        named.text.as_mut().unwrap().face = Some("A\";color:red".into());
        named.text.as_mut().unwrap().underline = true;
        let named_markup = shape_markup(&named, 80.0, 40.0, 1.0).unwrap();
        assert!(named_markup.contains("font-family:'\\41 \\22 \\3b "), "{named_markup}");
        assert!(named_markup.contains("text-decoration:underline"), "{named_markup}");
        assert!(!named_markup.contains("color:red"), "{named_markup}");
        // Zoom scales the stroke and the insets with the frame.
        let zoomed = shape_markup(&sample(), 160.0, 80.0, 2.0).unwrap();
        assert!(zoomed.contains("stroke-width=\"4\" stroke-dasharray=\"16 12\""), "{zoomed}");
        assert!(zoomed.contains("padding:9.6px 19.2px 9.6px 19.2px"), "{zoomed}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn mixed_text_runs_keep_their_properties_and_escape_text() {
        let mut shape = sample();
        let paragraph = &mut shape.text.as_mut().unwrap().paragraphs[0];
        paragraph.runs = vec![
            ShapeParagraphRun { text: "a ".into(), face: None, size: 12.0, bold: true, italic: false, underline: false, colour: "#FF0000".into() },
            ShapeParagraphRun { text: "< b".into(), face: None, size: 16.0, bold: false, italic: true, underline: false, colour: "#000000".into() },
        ];
        let markup = shape_markup(&shape, 80.0, 40.0, 1.0).unwrap();
        assert!(markup.contains("font-size:12px;font-weight:bold;font-style:normal;color:#FF0000\">a </span>"), "{markup}");
        assert!(markup.contains("font-size:16px;font-weight:normal;font-style:italic;color:#000000\">&lt; b</span>"), "{markup}");
        shape.text.as_mut().unwrap().paragraphs[0].runs[1].face = Some("X\";color:red".into());
        let markup = shape_markup(&shape, 80.0, 40.0, 1.0).unwrap();
        assert!(markup.contains("font-family:'\\58 \\22 \\3b "), "{markup}");
        assert!(!markup.contains("color:red"), "{markup}");
        shape.text.as_mut().unwrap().paragraphs[0].text = "edited".into();
        let markup = shape_markup(&shape, 80.0, 40.0, 1.0).unwrap();
        assert!(markup.contains(">edited</p>"), "{markup}");
        assert!(!markup.contains("<span style="), "{markup}");
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

    fn d(prst: &str, w: f64, h: f64, adjust: &[(&str, i64)]) -> String {
        let adjust: Vec<(String, i64)> = adjust.iter().map(|(n, v)| ((*n).to_owned(), *v)).collect();
        path_data(&geometry_path(prst, w, h, &adjust).unwrap(), w, h, false, false)
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__polygon_presets_follow_the_definitions_and_their_adjusts() {
        // triangle: the apex at adj/100000 of the width.
        assert_eq!(d("triangle", 40.0, 20.0, &[]), "M0 20L20 0L40 20Z");
        assert_eq!(d("triangle", 40.0, 20.0, &[("adj", 25000)]), "M0 20L10 0L40 20Z");
        assert_eq!(d("rtTriangle", 40.0, 20.0, &[]), "M0 20L0 0L40 20Z");
        assert_eq!(d("diamond", 40.0, 20.0, &[]), "M0 10L20 0L40 10L20 20Z");
        assert_eq!(d("flowChartDecision", 40.0, 20.0, &[]), d("diamond", 40.0, 20.0, &[]));
        assert_eq!(d("flowChartConnector", 40.0, 20.0, &[]), d("ellipse", 40.0, 20.0, &[]));
        assert_eq!(d("flowChartProcess", 40.0, 20.0, &[]), d("rect", 40.0, 20.0, &[]));
        // parallelogram: the slant is ss * adj / 100000, held to 100000 * w / ss.
        assert_eq!(d("parallelogram", 100.0, 40.0, &[]), "M0 40L10 0L100 0L90 40Z");
        assert_eq!(d("parallelogram", 100.0, 40.0, &[("adj", 200000)]), "M0 40L80 0L100 0L20 40Z");
        assert_eq!(d("parallelogram", 100.0, 40.0, &[("adj", 900000)]), "M0 40L100 0L100 0L0 40Z");
        // trapezoid: the top is narrower by ss * adj / 100000 each side; adj is held to 50000 * w / ss.
        assert_eq!(d("trapezoid", 100.0, 40.0, &[]), "M0 40L10 0L90 0L100 40Z");
        assert_eq!(d("trapezoid", 100.0, 40.0, &[("adj", 900000)]), "M0 40L50 0L50 0L100 40Z");
        assert_eq!(d("hexagon", 100.0, 40.0, &[]), "M0 20L10 0L90 0L100 20L90 40L10 40Z");
        // octagon: the corners are cut by ss * 29289 / 100000.
        assert_eq!(
            d("octagon", 100.0, 100.0, &[]),
            "M0 29.289L29.289 0L70.711 0L100 29.289L100 70.711L70.711 100L29.289 100L0 70.711Z"
        );
        assert_eq!(d("octagon", 100.0, 100.0, &[("adj", 90000)]), d("octagon", 100.0, 100.0, &[("adj", 50000)]));
        assert_eq!(d("chevron", 100.0, 40.0, &[]), "M0 0L80 0L100 20L80 40L0 40L20 20Z");
        assert_eq!(d("chevron", 100.0, 40.0, &[("adj", 25000)]), "M0 0L90 0L100 20L90 40L0 40L10 20Z");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__a_regular_pentagon_touches_the_top_the_sides_and_the_bottom() {
        let path = geometry_path("pentagon", 100.0, 100.0, &[]).unwrap();
        let points: Vec<(f64, f64)> = path
            .iter()
            .filter_map(|s| match s {
                Seg::Move(x, y) | Seg::Line(x, y) => Some((*x, *y)),
                _ => None,
            })
            .collect();
        assert_eq!(points.len(), 5);
        let near = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01;
        assert!(near(points[0], (50.0, 0.0)), "{points:?}");
        assert!(near(points[1], (100.0, 38.197)), "{points:?}");
        assert!(near(points[2], (80.902, 100.0)), "{points:?}");
        assert!(near(points[3], (19.098, 100.0)), "{points:?}");
        assert!(near(points[4], (0.0, 38.197)), "{points:?}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__block_arrows_take_shaft_and_head_from_their_adjusts() {
        // adj1 is the shaft's share of the thickness, adj2 the head's length in shorter sides.
        assert_eq!(d("rightArrow", 100.0, 40.0, &[]), "M0 10L80 10L80 0L100 20L80 40L80 30L0 30Z");
        assert_eq!(
            d("rightArrow", 100.0, 40.0, &[("adj1", 100000), ("adj2", 100000)]),
            "M0 0L60 0L60 0L100 20L60 40L60 40L0 40Z"
        );
        assert_eq!(d("leftArrow", 100.0, 40.0, &[]), "M0 20L20 0L20 10L100 10L100 30L20 30L20 40Z");
        assert_eq!(d("upArrow", 40.0, 100.0, &[]), "M0 20L20 0L40 20L30 20L30 100L10 100L10 20Z");
        assert_eq!(d("downArrow", 40.0, 100.0, &[]), "M10 0L30 0L30 80L40 80L20 100L0 80L10 80Z");
        // The head is held to the length of the shape.
        assert_eq!(
            d("rightArrow", 100.0, 40.0, &[("adj2", 9_000_000)]),
            "M0 10L0 10L0 0L100 20L0 40L0 30L0 30Z"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__callouts_put_the_tail_on_the_side_the_point_lies_beyond() {
        // The default point is below and to the left: the tail leaves the bottom, in its left half.
        assert_eq!(d("wedgeRectCallout", 120.0, 60.0, &[]), "M0 0L120 0L120 60L50 60L35 67.5L20 60L0 60Z");
        // Far to the right of the centre: the tail leaves the right side, in its upper half.
        assert_eq!(
            d("wedgeRectCallout", 120.0, 60.0, &[("adj1", 100000), ("adj2", 0)]),
            "M0 0L120 0L120 10L180 30L120 25L120 60L0 60Z"
        );
        // Above and to the right: the top, in its right half.
        assert_eq!(
            d("wedgeRectCallout", 120.0, 60.0, &[("adj1", 20000), ("adj2", -100000)]),
            "M0 0L70 0L84 -30L100 0L120 0L120 60L0 60Z"
        );
        // To the left: the left side, in its lower half (the path runs up it).
        assert_eq!(
            d("wedgeRectCallout", 120.0, 60.0, &[("adj1", -100000), ("adj2", 10000)]),
            "M0 0L120 0L120 60L0 60L0 50L-60 36L0 35Z"
        );
        // The rounded one has corners of adj3 of the shorter side and the same tail.
        let round = d("wedgeRoundRectCallout", 120.0, 60.0, &[]);
        assert!(round.starts_with("M10 0L110 0C"), "{round}");
        assert!(round.contains("L50 60L35 67.5L20 60L10 60C"), "{round}");
        assert!(d("wedgeRoundRectCallout", 120.0, 60.0, &[("adj3", 0)]).starts_with("M0 0L120 0L120 60"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__flowchart_shapes_and_round_rectangle_adjust() {
        assert_eq!(d("roundRect", 40.0, 20.0, &[("adj", 50000)]).get(..11), Some("M10 0L30 0C"));
        assert!(d("roundRect", 40.0, 20.0, &[("adj", 0)]).starts_with("M0 0L40 0C40 0 40 0 40 0L"));
        let stadium = d("flowChartTerminator", 100.0, 40.0, &[]);
        assert!(stadium.starts_with("M16.088 0L83.912 0C"), "{stadium}");
        assert!(stadium.contains(" 100 20C") && stadium.contains(" 0 20C"), "{stadium}");
        let document = d("flowChartDocument", 100.0, 100.0, &[]);
        assert!(document.starts_with("M0 0L100 0L100 80.194C50 80.194 50 110.75 0 93.389Z"), "{document}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__every_listed_preset_has_a_path_and_nothing_else_does() {
        for prst in PRESETS {
            assert!(geometry_path(prst, 50.0, 30.0, &[]).is_some(), "{prst}");
            assert!(drawn_geometry(prst));
        }
        assert!(geometry_path("star5", 50.0, 30.0, &[]).is_none());
    }

    fn line_shape(head: Option<&str>, tail: Option<&str>, width: f64) -> DrawnShape {
        let arrow = |kind: &str| ShapeArrow { kind: kind.into(), width: "med".into(), length: "med".into() };
        let mut shape = sample();
        shape.geometry = "line".into();
        shape.text = None;
        shape.fill = None;
        shape.line = Some(ShapeStroke {
            width,
            colour: "#FF0000".into(),
            dash: "solid".into(),
            head: head.map(arrow),
            tail: tail.map(arrow),
        });
        shape
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__a_triangle_arrowhead_sits_on_the_end_and_shortens_the_line() {
        // A 2 px line: a medium head is 3 line widths long and wide.
        let markup = shape_markup(&line_shape(None, Some("triangle"), 2.0), 100.0, 0.0, 1.0).unwrap();
        assert!(markup.contains("<path d=\"M0 0L94 0\" fill=\"none\" stroke=\"#FF0000\" stroke-width=\"2\"/>"), "{markup}");
        assert!(markup.contains("<path d=\"M100 0L94 3L94 -3Z\" fill=\"#FF0000\" stroke=\"none\"/>"), "{markup}");
        // At the start the head points back along the line.
        let markup = shape_markup(&line_shape(Some("triangle"), None, 2.0), 100.0, 0.0, 1.0).unwrap();
        assert!(markup.contains("d=\"M0 0L6 -3L6 3Z\""), "{markup}");
        assert!(markup.contains("d=\"M6 0L100 0\""), "{markup}");
        // A line flipped horizontally runs right to left: its end is at the left.
        let mut flipped = line_shape(None, Some("triangle"), 2.0);
        flipped.flip_horizontal = true;
        let markup = shape_markup(&flipped, 100.0, 0.0, 1.0).unwrap();
        assert!(markup.contains("d=\"M0 0L6 -3L6 3Z\""), "{markup}");
        assert!(markup.contains("d=\"M100 0L6 0\""), "{markup}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__arrowhead_types_and_sizes() {
        let mut shape = line_shape(None, Some("arrow"), 2.0);
        let markup = shape_markup(&shape, 100.0, 0.0, 1.0).unwrap();
        // An open arrow is two strokes and leaves the line whole.
        assert!(markup.contains("d=\"M94 3L100 0L94 -3\" fill=\"none\" stroke=\"#FF0000\" stroke-width=\"2\""), "{markup}");
        assert!(markup.contains("d=\"M0 0L100 0\""), "{markup}");
        shape.line.as_mut().unwrap().tail = Some(ShapeArrow { kind: "oval".into(), width: "lg".into(), length: "sm".into() });
        let markup = shape_markup(&shape, 100.0, 0.0, 1.0).unwrap();
        // Small is 2 widths, large 5: 4 long and 10 wide, centred on the end.
        assert!(markup.contains("<ellipse cx=\"100\" cy=\"0\" rx=\"2\" ry=\"5\" transform=\"rotate(0 100 0)\""), "{markup}");
        shape.line.as_mut().unwrap().tail = Some(ShapeArrow { kind: "diamond".into(), width: "med".into(), length: "med".into() });
        let markup = shape_markup(&shape, 100.0, 0.0, 1.0).unwrap();
        assert!(markup.contains("d=\"M103 0L100 3L97 0L100 -3Z\""), "{markup}");
        shape.line.as_mut().unwrap().tail = Some(ShapeArrow { kind: "stealth".into(), width: "med".into(), length: "med".into() });
        let markup = shape_markup(&shape, 100.0, 0.0, 1.0).unwrap();
        assert!(markup.contains("d=\"M100 0L94 3L95.5 0L94 -3Z\""), "{markup}");
        assert!(markup.contains("d=\"M0 0L95.5 0\""), "{markup}");
        // A hairline sizes its heads as a 2 px line; zoom scales the line and so the heads.
        let thin = shape_markup(&line_shape(None, Some("triangle"), 0.5), 100.0, 0.0, 1.0).unwrap();
        assert!(thin.contains("d=\"M100 0L94 3L94 -3Z\""), "{thin}");
        let zoomed = shape_markup(&line_shape(None, Some("triangle"), 2.0), 200.0, 0.0, 2.0).unwrap();
        assert!(zoomed.contains("d=\"M200 0L188 6L188 -6Z\""), "{zoomed}");
    }

    fn gradient(angle: f64, scaled: bool) -> ShapeGradient {
        ShapeGradient {
            stops: vec![(0.0, "#FF0000".into()), (0.5, "#00FF00".into()), (1.0, "#0000FF".into())],
            angle,
            scaled,
            path: None,
            focus: [0.5, 0.5],
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__linear_gradients_run_at_their_angle_across_the_box() {
        let g = |angle, scaled, w, h, fh, fv| gradient_defs(&gradient(angle, scaled), "g", w, h, fh, fv);
        // Unscaled, in pixels: 90 degrees runs top to bottom through the centre.
        let down = g(90.0, false, 100.0, 50.0, false, false);
        assert!(down.contains("gradientUnits=\"userSpaceOnUse\" x1=\"50\" y1=\"0\" x2=\"50\" y2=\"50\""), "{down}");
        assert!(down.contains("<stop offset=\"0\" stop-color=\"#FF0000\"/><stop offset=\"0.5\" stop-color=\"#00FF00\"/><stop offset=\"1\" stop-color=\"#0000FF\"/>"), "{down}");
        let across = g(0.0, false, 100.0, 50.0, false, false);
        assert!(across.contains("x1=\"0\" y1=\"25\" x2=\"100\" y2=\"25\""), "{across}");
        // 45 degrees unscaled on a 100 by 50 box reaches the corners it projects: half = (100+50)*0.7071/2 = 53.03, 37.5 along each axis from (50, 25).
        let slant = g(45.0, false, 100.0, 50.0, false, false);
        assert!(slant.contains("x1=\"12.5\" y1=\"-12.5\" x2=\"87.5\" y2=\"62.5\""), "{slant}");
        // Scaled: the unit square's own diagonal, whatever the box.
        let scaled = g(45.0, true, 100.0, 50.0, false, false);
        assert!(scaled.contains("<linearGradient id=\"g\" x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\">"), "{scaled}");
        // A shape flipped horizontally flips its gradient: 0 degrees runs right to left.
        let flipped = g(0.0, true, 100.0, 50.0, true, false);
        assert!(flipped.contains("x1=\"1\" y1=\"0.5\" x2=\"0\" y2=\"0.5\""), "{flipped}");
        let up = g(90.0, true, 100.0, 50.0, false, true);
        assert!(up.contains("x1=\"0.5\" y1=\"1\" x2=\"0.5\" y2=\"0\""), "{up}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__path_gradients_spread_from_their_focus_and_fill_the_shape() {
        let mut g = gradient(0.0, false);
        g.path = Some("circle".into());
        let centred = gradient_defs(&g, "g", 100.0, 50.0, false, false);
        assert!(centred.contains("<radialGradient id=\"g\" cx=\"0.5\" cy=\"0.5\" fx=\"0.5\" fy=\"0.5\" r=\"0.707\">"), "{centred}");
        g.focus = [0.0, 0.0];
        let corner = gradient_defs(&g, "g", 100.0, 50.0, true, false);
        assert!(corner.contains("cx=\"1\" cy=\"0\"") && corner.contains("r=\"1.414\""), "{corner}");
        // The shape is filled by its gradient.
        let mut shape = sample();
        shape.gradient = Some(gradient(90.0, false));
        shape.text = None;
        let markup = shape_markup(&shape, 80.0, 40.0, 1.0).unwrap();
        assert!(markup.contains("<defs><linearGradient id=\"dcg"), "{markup}");
        assert!(markup.contains("fill=\"url(#dcg") && !markup.contains("fill=\"#FF8000\""), "{markup}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__rotation_turns_the_whole_shape_about_its_centre_and_a_vertical_flip_turns_text() {
        let mut shape = sample();
        shape.rotation = 30.0;
        let markup = shape_markup(&shape, 80.0, 40.0, 1.0).unwrap();
        assert!(markup.starts_with("<div style=\"position:absolute;left:0;top:0;width:80px;height:40px;transform:rotate(30deg);\"><svg"), "{markup}");
        assert!(markup.ends_with("</div></div>"), "{markup}");
        shape.rotation = 360.0;
        assert!(shape_markup(&shape, 80.0, 40.0, 1.0).unwrap().starts_with("<svg"));
        let mut flipped = sample();
        flipped.flip_vertical = true;
        let markup = shape_markup(&flipped, 80.0, 40.0, 1.0).unwrap();
        assert!(markup.contains("overflow:visible;transform:rotate(180deg);\""), "{markup}");
        assert!(!shape_markup(&sample(), 80.0, 40.0, 1.0).unwrap().contains("rotate(180deg)"));
        // A horizontal flip mirrors the path and leaves the text as it is.
        let mut mirrored = sample();
        mirrored.flip_horizontal = true;
        assert!(!shape_markup(&mirrored, 80.0, 40.0, 1.0).unwrap().contains("rotate"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__the_anchor_of_a_rotated_shape_is_the_box_around_it() {
        // A 200 by 100 shape turned 90 degrees fills a 100 by 200 box: its own box is 2 by 0.5 of that.
        let [ex, ey] = extent_in_rotated_box(200.0, 100.0, 90.0);
        assert!((ex - 2.0).abs() < 1e-9 && (ey - 0.5).abs() < 1e-9, "{ex} {ey}");
        let own = unrotated((0.0, 0.0, 100.0, 200.0), [ex, ey]);
        assert!((own.0 + 50.0).abs() < 1e-9 && (own.1 - 50.0).abs() < 1e-9, "{own:?}");
        assert!((own.2 - 150.0).abs() < 1e-9 && (own.3 - 150.0).abs() < 1e-9, "{own:?}");
        // 45 degrees: the box around a square of side s is s * 1.4142 wide, so the square is 0.7071 of it.
        let [ex, ey] = extent_in_rotated_box(100.0, 100.0, 45.0);
        assert!((ex - 0.7071).abs() < 1e-4 && (ey - 0.7071).abs() < 1e-4);
        // Not turned: the box is the shape.
        assert_eq!(extent_in_rotated_box(100.0, 40.0, 0.0), [1.0, 1.0]);
        assert_eq!(extent_in_rotated_box(100.0, 40.0, 180.0), [1.0, 1.0]);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__a_group_draws_its_children_in_boxes_of_its_own() {
        let mut left = sample();
        left.name = "Left".into();
        left.text = None;
        let mut right = sample();
        right.name = "Right".into();
        right.text = None;
        right.rotation = 90.0;
        let group = DrawnShape {
            kind: "group".into(),
            geometry: String::new(),
            fill: None,
            line: None,
            text: None,
            children: vec![
                DrawnChild { rel: [0.0, 0.0, 0.5, 1.0], shape: left },
                DrawnChild { rel: [0.5, 0.25, 0.5, 0.5], shape: right },
            ],
            ..sample()
        };
        let markup = shape_markup(&group, 200.0, 100.0, 1.0).unwrap();
        assert!(!markup.starts_with("<svg"), "{markup}");
        assert!(markup.contains("<div style=\"position:absolute;left:0px;top:0px;width:100px;height:100px;\"><svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"100\""), "{markup}");
        assert!(markup.contains("<div style=\"position:absolute;left:100px;top:25px;width:100px;height:50px;transform:rotate(90deg);\"><svg"), "{markup}");
        assert_eq!(markup.matches("<path ").count(), 2);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_3__a_crop_scales_and_shifts_the_image_so_the_kept_part_fills_the_box() {
        // Nothing cropped: the image is the box.
        assert_eq!(crop_style([0.0; 4]), "position:absolute;max-width:none;max-height:none;width:100%;height:100%;left:0%;top:0%");
        // 25% off each side: twice as wide, shifted left by the cropped quarter of the doubled width (50% of the box).
        let both = crop_style([0.25, 0.0, 0.25, 0.0]);
        assert!(both.contains("width:200%;height:100%;left:-50%;top:0%"), "{both}");
        // Left 25%, top 10%, right 25%, bottom 40%: kept 0.5 by 0.5.
        let mixed = crop_style([0.25, 0.1, 0.25, 0.4]);
        assert!(mixed.contains("width:200%;height:200%;left:-50%;top:-20%"), "{mixed}");
        // A negative crop extends the image past its picture: it shrinks inside the box.
        let wide = crop_style([-0.5, 0.0, 0.0, 0.0]);
        assert!(wide.contains("width:66.667%;height:100%;left:33.333%"), "{wide}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_3__a_picture_draws_its_placeholder_and_its_outline_but_leaves_the_image_to_the_page() {
        let mut picture = sample();
        picture.kind = "picture".into();
        picture.text = None;
        picture.fill = None;
        picture.picture = Some(DrawnPicture { crop: [0.0; 4], placeholder: None, description: None });
        let with_image = outline_markup(&picture, 80.0, 40.0, 1.0).unwrap();
        assert!(!with_image.contains("dc-picture-placeholder"), "{with_image}");
        assert!(with_image.contains("stroke=\"#ED7D31\""), "{with_image}");
        picture.picture.as_mut().unwrap().placeholder = Some("EMF picture 96 × 48 <x>".into());
        let placeholder = outline_markup(&picture, 80.0, 40.0, 2.0).unwrap();
        assert!(placeholder.contains("class=\"dc-picture-placeholder\""), "{placeholder}");
        assert!(placeholder.contains(">EMF picture 96 × 48 &lt;x&gt;</div>"), "{placeholder}");
        assert!(placeholder.contains("font-size:29.333px"), "{placeholder}");
        // No outline: just the placeholder.
        picture.line = None;
        assert!(!outline_markup(&picture, 80.0, 40.0, 1.0).unwrap().contains("<svg"));
        // The grid turns the picture's box by its rotation.
        picture.rotation = 30.0;
        assert_eq!(turn_style(&picture), "transform:rotate(30deg);");
    }

    fn adjust(list: &[(&str, i64)]) -> Vec<(String, i64)> {
        list.iter().map(|(n, v)| ((*n).to_owned(), *v)).collect()
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__bent_and_curved_connectors_follow_their_definitions() {
        // bentConnector3 at its default adjust turns at half the width.
        assert_eq!(
            geometry_path("bentConnector3", 100.0, 40.0, &[]).unwrap(),
            vec![Seg::Move(0.0, 0.0), Seg::Line(50.0, 0.0), Seg::Line(50.0, 40.0), Seg::Line(100.0, 40.0)]
        );
        assert_eq!(
            geometry_path("bentConnector3", 100.0, 40.0, &adjust(&[("adj1", 25000)])).unwrap()[1],
            Seg::Line(25.0, 0.0)
        );
        assert_eq!(
            geometry_path("bentConnector2", 100.0, 40.0, &[]).unwrap(),
            vec![Seg::Move(0.0, 0.0), Seg::Line(100.0, 0.0), Seg::Line(100.0, 40.0)]
        );
        let four = geometry_path("bentConnector4", 100.0, 40.0, &adjust(&[("adj1", 30000), ("adj2", 120000)])).unwrap();
        assert_eq!(four.len(), 5);
        assert_eq!((four[2], four[3]), (Seg::Line(30.0, 48.0), Seg::Line(100.0, 48.0)));
        let five = geometry_path("bentConnector5", 100.0, 40.0, &adjust(&[("adj1", 20000), ("adj2", 40000), ("adj3", 80000)])).unwrap();
        assert_eq!(five[2..5], [Seg::Line(20.0, 16.0), Seg::Line(80.0, 16.0), Seg::Line(80.0, 40.0)]);
        // Curved: starts at the corner, ends at the opposite one, three-segment and five-segment forms.
        for (prst, curves) in [("curvedConnector2", 1), ("curvedConnector3", 2), ("curvedConnector4", 3), ("curvedConnector5", 4)] {
            let path = geometry_path(prst, 100.0, 40.0, &[]).unwrap();
            assert_eq!(path.len(), curves + 1, "{prst}");
            assert_eq!(path[0], Seg::Move(0.0, 0.0));
            let Some(Seg::Cubic(_, _, end)) = path.last() else { panic!("{prst}") };
            assert_eq!(*end, (100.0, 40.0), "{prst}");
            assert!(open_geometry(prst) && drawn_geometry(prst));
        }
        // curvedConnector3 passes through the middle of its elbow at (adj1 of the width, half the height).
        let Seg::Cubic(_, _, mid) = geometry_path("curvedConnector3", 100.0, 40.0, &[]).unwrap()[1] else { panic!() };
        assert_eq!(mid, (50.0, 20.0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__a_bent_connectors_arrowheads_follow_its_end_segments() {
        let mut shape = line_shape(Some("triangle"), Some("triangle"), 2.0);
        shape.geometry = "bentConnector2".into();
        let markup = shape_markup(&shape, 100.0, 40.0, 1.0).unwrap();
        // Two heads: the start's points left (the path leaves its start going right), the end's points down.
        assert_eq!(markup.matches("<path ").count(), 3, "{markup}");
        // The body path was trimmed: it starts after the start head and stops above the end head.
        assert!(markup.contains("<path d=\"M"), "{markup}");
        let ends = path_ends(&geometry_path("bentConnector2", 100.0, 40.0, &[]).unwrap()).unwrap();
        assert_eq!(ends, ((0.0, 0.0), (-1.0, 0.0), (100.0, 40.0), (0.0, 1.0)));
        // Flipped horizontally, the start is at the right and leaves the path going right.
        let flipped = flip_segments(&geometry_path("bentConnector2", 100.0, 40.0, &[]).unwrap(), 100.0, 40.0, true, false);
        assert_eq!(path_ends(&flipped).unwrap(), ((100.0, 0.0), (1.0, 0.0), (0.0, 40.0), (0.0, 1.0)));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__a_presets_text_rectangle_is_not_the_whole_frame() {
        assert_eq!(text_rect("rect", 100.0, 40.0, &[]), [0.0, 0.0, 100.0, 40.0]);
        // Ellipse: the inscribed square at cos 45 of each radius.
        let [l, t, r, b] = text_rect("ellipse", 100.0, 40.0, &[]);
        assert_eq!(text_rect("flowChartConnector", 100.0, 40.0, &[]), [l, t, r, b]);
        assert!((l - 14.644).abs() < 0.001 && (t - 5.858).abs() < 0.001 && (r - 85.355).abs() < 0.001 && (b - 34.142).abs() < 0.001);
        assert_eq!(text_rect("diamond", 100.0, 40.0, &[]), [25.0, 10.0, 75.0, 30.0]);
        // Triangle: the lower half between the points a quarter and three quarters across (apex at the middle).
        assert_eq!(text_rect("triangle", 100.0, 40.0, &[]), [25.0, 20.0, 75.0, 40.0]);
        // roundRect at 50000: the corner radius is half the short side, and the text keeps 29.289% of it away.
        let [l, t, ..] = text_rect("roundRect", 100.0, 40.0, &adjust(&[("adj", 50000)]));
        assert!((l - 5.858).abs() < 0.001 && (t - 5.858).abs() < 0.001);
        assert_eq!(text_rect("flowChartDocument", 100.0, 40.0, &[])[3], 40.0 * 17322.0 / 21600.0);
        // The text of a diamond sits in its rectangle: the markup places the text box there.
        let mut shape = sample();
        shape.geometry = "diamond".into();
        let markup = shape_markup(&shape, 100.0, 40.0, 1.0).unwrap();
        assert!(markup.contains("class=\"dc-shape-text\" style=\"position:absolute;left:25px;top:10px;width:50px;height:20px;"), "{markup}");
        // A preset not worked out keeps the whole frame.
        assert_eq!(text_rect("hexagon", 100.0, 40.0, &[]), [0.0, 0.0, 100.0, 40.0]);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__an_outer_shadow_is_a_second_pass_under_the_shape() {
        let mut shape = sample();
        shape.shadow = Some(ShapeShadow {
            dx: 3.0,
            dy: 4.0,
            blur: 6.0,
            scale: [0.9, 0.9],
            align: "t".into(),
            rotate: false,
            colour: "rgba(0,0,0,0.4)".into(),
        });
        let markup = shape_markup(&shape, 100.0, 40.0, 2.0).unwrap();
        let (shadow_at, shape_at) = (markup.find("rgba(0,0,0,0.4)").unwrap(), markup.find("fill=\"#FF8000\"").unwrap());
        assert!(shadow_at < shape_at, "the shadow is drawn first: {markup}");
        // Offset and blur scale with the zoom; the scale is about the top-centre point (50, 0).
        assert!(markup.contains("<feGaussianBlur stdDeviation=\"6\"/>"), "{markup}");
        assert!(markup.contains("transform=\"translate(6 8) translate(50 0) scale(0.9 0.9) translate(-50 0)\""), "{markup}");
        // The shadow keeps the outline's width and dashes in its colour.
        assert!(markup.contains("stroke=\"rgba(0,0,0,0.4)\" stroke-width=\"4\" stroke-dasharray="), "{markup}");
        // A shape without a shadow has no second pass; one without fill or outline has none either.
        shape.shadow = None;
        assert!(!shape_markup(&shape, 100.0, 40.0, 1.0).unwrap().contains("feGaussianBlur"));
        let mut bare = sample();
        bare.fill = None;
        bare.line = None;
        bare.shadow = Some(ShapeShadow { dx: 1.0, dy: 1.0, blur: 0.0, scale: [1.0, 1.0], align: "b".into(), rotate: true, colour: "#000000".into() });
        assert_eq!(shape_markup(&bare, 100.0, 40.0, 1.0).unwrap().matches("<path ").count(), 1);
        // Not rotating with the shape: the offset is turned back by the shape's own turn.
        let mut turned = sample();
        turned.rotation = 90.0;
        turned.shadow = Some(ShapeShadow { dx: 10.0, dy: 0.0, blur: 0.0, scale: [1.0, 1.0], align: "b".into(), rotate: false, colour: "#000000".into() });
        let markup = shape_markup(&turned, 100.0, 40.0, 1.0).unwrap();
        assert!(markup.contains("translate(0 -10)"), "{markup}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__pictures_in_groups_are_placed_in_the_boxes_of_their_ancestors() {
        let picture = |id: u32, flip: bool| DrawnShape {
            kind: "picture".into(),
            id,
            flip_horizontal: flip,
            fill: None,
            line: None,
            text: None,
            picture: Some(DrawnPicture { crop: [0.1, 0.0, 0.0, 0.0], placeholder: None, description: None }),
            ..sample()
        };
        let mut placeholder = picture(9, false);
        placeholder.picture.as_mut().unwrap().placeholder = Some("EMF picture".into());
        let inner = DrawnShape {
            kind: "group".into(),
            fill: None,
            line: None,
            text: None,
            rotation: 20.0,
            children: vec![DrawnChild { rel: [0.5, 0.0, 0.5, 1.0], shape: picture(7, true) }],
            ..sample()
        };
        let outer = DrawnShape {
            kind: "group".into(),
            fill: None,
            line: None,
            text: None,
            children: vec![
                DrawnChild { rel: [0.0, 0.0, 0.5, 0.5], shape: picture(5, false) },
                DrawnChild { rel: [0.5, 0.5, 0.5, 0.5], shape: inner },
                DrawnChild { rel: [0.0, 0.5, 0.5, 0.5], shape: placeholder },
            ],
            ..sample()
        };
        let found = group_pictures(&outer, 200.0, 100.0);
        // The one with a placeholder stays a label; the other two are placed.
        assert_eq!(found.iter().map(|p| p.id).collect::<Vec<_>>(), vec![5, 7]);
        assert_eq!(found[0].chain, vec![[0.0, 0.0, 100.0, 50.0, 0.0]]);
        assert!(!found[0].flip[0] && found[1].flip[0]);
        // The nested one sits in its group's box (turned 20 degrees) inside the outer group's.
        assert_eq!(found[1].chain, vec![[100.0, 50.0, 100.0, 50.0, 20.0], [50.0, 0.0, 50.0, 50.0, 0.0]]);
        assert_eq!(found[0].crop, [0.1, 0.0, 0.0, 0.0]);
        assert_eq!(flip_style([true, false]), "transform:scale(-1,1);");
        assert_eq!(flip_style([true, true]), "transform:scale(-1,-1);");
        assert_eq!(flip_style([false, false]), "");
    }
}
