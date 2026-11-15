//! A drawing object as the page draws it (dc-zjsg.2): the model's [`calc_host::Shape`] with its colours
//! resolved, its lengths in pixels at 100% and the style's defaults filled in where the file leaves
//! a field to the style.
//!
//! What the style gives is Excel's own for a shape inserted from the ribbon (`lnRef idx=2
//! accent1 shade 50%`, `fillRef idx=1 accent1`, `fontRef lt1`): the fill is accent 1, the outline
//! accent 1 shaded, a point wide, the text white. A text box has no style: no fill, no outline,
//! black text. Scheme colours come from the Office theme; a workbook's own theme is not read here.

use crate::wire::{ChartPlace, DrawnShape, ShapeParagraph, ShapeStroke, ShapeText};
use calc_host::{LineEnd, Shape, ShapeColour, ShapeFill, ShapeKind};
use dna_calc_model::drawn_shapes::{DrawnChild, ShapeArrow, ShapeGradient};

const EMU_PER_PIXEL: f64 = 9_525.0;

/// The Office theme's colours by their scheme names (the document's theme is not consulted).
fn scheme(name: &str) -> Option<u32> {
    Some(match name {
        "dk1" | "tx1" => 0x000000,
        "lt1" | "bg1" => 0xFFFFFF,
        "dk2" | "tx2" => 0x44546A,
        "lt2" | "bg2" => 0xE7E6E6,
        "accent1" => 0x4472C4,
        "accent2" => 0xED7D31,
        "accent3" => 0xA5A5A5,
        "accent4" => 0xFFC000,
        "accent5" => 0x5B9BD5,
        "accent6" => 0x70AD47,
        "hlink" => 0x0563C1,
        "folHlink" => 0x954F72,
        _ => return None,
    })
}

fn css(colour: &ShapeColour) -> Option<String> {
    let rgb = match colour {
        ShapeColour::Rgb(rgb) => *rgb,
        ShapeColour::Scheme(name) => scheme(name)?,
    };
    Some(format!("#{rgb:06X}"))
}

/// Accent 1 shaded 50%, the outline a ribbon shape's style gives.
const STYLE_LINE: &str = "#2F528F";
const STYLE_FILL: &str = "#4472C4";
/// A point, the weight of a style's `lnRef idx=2`; a connector's `idx=1` is 0.75 pt.
const STYLE_LINE_WEIGHT: i64 = 12_700;
const STYLE_CONNECTOR_WEIGHT: i64 = 9_525;

/// A gradient's stops and direction as the page draws them; `None` for one with a colour not resolved.
fn gradient(g: &calc_host::ShapeGradient) -> Option<ShapeGradient> {
    let stops = g
        .stops
        .iter()
        .map(|(pos, colour)| Some((*pos as f64 / 100_000.0, css(colour)?)))
        .collect::<Option<Vec<_>>>()?;
    // The focus of a path gradient is the middle of its fill-to rectangle.
    let focus = g.focus.map_or([0.5, 0.5], |[l, t, r, b]| {
        [(l + 100_000 - r) as f64 / 200_000.0, (t + 100_000 - b) as f64 / 200_000.0]
    });
    Some(ShapeGradient {
        stops,
        angle: g.angle.unwrap_or(0) as f64 / 60_000.0,
        scaled: g.scaled,
        path: g.path.clone(),
        focus,
    })
}

fn arrow(end: &Option<LineEnd>) -> Option<ShapeArrow> {
    let end = end.as_ref().filter(|e| e.kind != "none")?;
    Some(ShapeArrow {
        kind: end.kind.clone(),
        width: end.width.clone().unwrap_or_else(|| "med".into()),
        length: end.length.clone().unwrap_or_else(|| "med".into()),
    })
}

/// The object as drawn; `None` for one this slice does not draw (a picture, a hidden object, a
/// geometry that is not drawn). A group carries its children, each as a box inside the group's.
pub(crate) fn drawn(shape: &Shape, place: ChartPlace) -> Option<DrawnShape> {
    if shape.hidden {
        return None;
    }
    let (kind, default_geometry, styled) = match shape.kind {
        ShapeKind::Shape => ("shape", "rect", true),
        ShapeKind::TextBox => ("textbox", "rect", false),
        ShapeKind::Connector => ("connector", "line", true),
        ShapeKind::Group => ("group", "", false),
        ShapeKind::Picture => return None,
    };
    let geometry = shape.geometry.as_deref().unwrap_or(default_geometry);
    if kind != "group" && !dna_calc_model::drawn_shapes::drawn_geometry(geometry) {
        return None;
    }
    let degrees = shape.rotation as f64 / 60_000.0;
    let extent = if degrees.rem_euclid(360.0) == 0.0 {
        [1.0, 1.0]
    } else {
        dna_calc_model::drawn_shapes::extent_in_rotated_box(shape.extent.0 as f64, shape.extent.1 as f64, degrees)
    };
    let mut children = Vec::new();
    if kind == "group" {
        // The children's boxes as fractions of the group's frame, through the group's child space; a group
        // flipped mirrors them and each child's own flip and turn with it.
        let (cx, cy) = (shape.child_offset.0 as f64, shape.child_offset.1 as f64);
        let (cw, ch) = (shape.child_extent.0 as f64, shape.child_extent.1 as f64);
        for child in &shape.children {
            let Some(mut part) = drawn(child, ChartPlace::Absolute { x: 0.0, y: 0.0, width: 0.0, height: 0.0 }) else {
                continue;
            };
            let fraction = |value: f64, origin: f64, whole: f64| if whole > 0.0 { (value - origin) / whole } else { 0.0 };
            let size = |value: f64, whole: f64| if whole > 0.0 { value / whole } else { 1.0 };
            let mut rel = [
                fraction(child.offset.0 as f64, cx, cw),
                fraction(child.offset.1 as f64, cy, ch),
                size(child.extent.0 as f64, cw),
                size(child.extent.1 as f64, ch),
            ];
            if shape.flip_horizontal {
                rel[0] = 1.0 - rel[0] - rel[2];
                part.flip_horizontal = !part.flip_horizontal;
                part.rotation = -part.rotation;
            }
            if shape.flip_vertical {
                rel[1] = 1.0 - rel[1] - rel[3];
                part.flip_vertical = !part.flip_vertical;
                part.rotation = -part.rotation;
            }
            part.extent = [1.0, 1.0];
            children.push(DrawnChild { rel, shape: part });
        }
    }
    let connector = shape.kind == ShapeKind::Connector;
    let fill = match &shape.fill {
        ShapeFill::None => None,
        ShapeFill::Solid(colour) => css(colour),
        // A gradient is drawn from its own field; a pattern or picture is not drawn: a shape with one
        // draws as if it had none.
        ShapeFill::Gradient(_) | ShapeFill::Other => None,
        ShapeFill::Style => styled.then(|| STYLE_FILL.to_owned()).filter(|_| !connector),
    };
    let line = if shape.line.hidden {
        None
    } else {
        let colour = match &shape.line.colour {
            Some(colour) => css(colour),
            None => styled.then(|| STYLE_LINE.to_owned()),
        };
        let weight = shape.line.weight.unwrap_or(if connector {
            STYLE_CONNECTOR_WEIGHT
        } else {
            STYLE_LINE_WEIGHT
        });
        // A text box without an outline of its own has none; an `a:ln` with a width and no colour
        // takes the style's, which a text box lacks.
        colour.map(|colour| ShapeStroke {
            width: weight as f64 / EMU_PER_PIXEL,
            colour,
            dash: shape.line.dash.clone().unwrap_or_else(|| "solid".into()),
            head: arrow(&shape.line.head),
            tail: arrow(&shape.line.tail),
        })
    };
    let body = &shape.body;
    let text = shape.text.as_ref().filter(|p| p.iter().any(|t| !t.is_empty())).map(|paragraphs| {
        let inset = |value: Option<i64>, default: i64| value.unwrap_or(default) as f64 / EMU_PER_PIXEL;
        ShapeText {
            paragraphs: paragraphs
                .iter()
                .enumerate()
                .map(|(i, text)| ShapeParagraph {
                    text: text.clone(),
                    align: body
                        .alignment
                        .get(i)
                        .cloned()
                        .flatten()
                        .unwrap_or_else(|| "l".into()),
                })
                .collect(),
            inset: [
                inset(body.left_inset, 91_440),
                inset(body.top_inset, 45_720),
                inset(body.right_inset, 91_440),
                inset(body.bottom_inset, 45_720),
            ],
            anchor: body.anchor.clone().unwrap_or_else(|| "t".into()),
            wrap: body.wrap.as_deref() != Some("none"),
            // 11 pt, Excel's default, at 96 dots per inch.
            size: body.size.map_or(11.0, |s| s as f64 / 100.0) * 96.0 / 72.0,
            bold: body.bold,
            italic: body.italic,
            colour: body
                .colour
                .as_ref()
                .and_then(css)
                .unwrap_or_else(|| if styled { "#FFFFFF" } else { "#000000" }.to_owned()),
        }
    });
    Some(DrawnShape {
        name: shape.name.clone(),
        kind: kind.to_owned(),
        place,
        geometry: geometry.to_owned(),
        flip_horizontal: shape.flip_horizontal,
        flip_vertical: shape.flip_vertical,
        fill,
        gradient: match &shape.fill {
            ShapeFill::Gradient(g) => gradient(g),
            _ => None,
        },
        line,
        text,
        rotation: degrees,
        extent,
        adjust: shape.adjust.clone(),
        children,
    })
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use calc_host::ShapeLine;

    fn place() -> ChartPlace {
        ChartPlace::Absolute { x: 0.0, y: 0.0, width: 10.0, height: 10.0 }
    }

    fn shape(kind: ShapeKind) -> Shape {
        let anchor = calc_doc::ChartAnchor::Absolute { x: 0, y: 0, width: 95_250, height: 95_250 };
        Shape::new(2, "S".into(), kind, anchor)
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__a_stated_fill_outline_dash_and_text_are_drawn() {
        let mut s = shape(ShapeKind::Shape);
        s.geometry = Some("roundRect".into());
        s.fill = ShapeFill::Solid(ShapeColour::Rgb(0xFF8000));
        s.line = ShapeLine {
            weight: Some(19_050),
            hidden: false,
            colour: Some(ShapeColour::Scheme("accent2".into())),
            dash: Some("dash".into()),
            ..Default::default()
        };
        s.text = Some(vec!["Hello shape".into(), String::new()]);
        s.body.anchor = Some("ctr".into());
        s.body.alignment = vec![Some("ctr".into()), None];
        s.body.size = Some(1100);
        s.body.bold = true;
        let d = drawn(&s, place()).unwrap();
        assert_eq!(d.fill.as_deref(), Some("#FF8000"));
        let line = d.line.unwrap();
        assert_eq!((line.width, line.colour.as_str(), line.dash.as_str()), (2.0, "#ED7D31", "dash"));
        let text = d.text.unwrap();
        assert_eq!(text.inset, [9.6, 4.8, 9.6, 4.8]);
        assert_eq!(text.anchor, "ctr");
        assert_eq!(text.paragraphs[0].align, "ctr");
        assert_eq!(text.paragraphs[1].align, "l");
        assert!((text.size - 11.0 * 96.0 / 72.0).abs() < 1e-9);
        assert!(text.bold && text.wrap);
        // A ribbon shape's text is white.
        assert_eq!(text.colour, "#FFFFFF");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__what_the_file_leaves_to_the_style_takes_the_ribbon_shapes_style() {
        let d = drawn(&shape(ShapeKind::Shape), place()).unwrap();
        assert_eq!(d.fill.as_deref(), Some("#4472C4"));
        let line = d.line.unwrap();
        assert_eq!((line.colour.as_str(), line.width), ("#2F528F", 12_700.0 / 9_525.0));
        // A text box has no style: nothing is drawn but its text, in black.
        let mut b = shape(ShapeKind::TextBox);
        b.text = Some(vec!["a".into()]);
        let d = drawn(&b, place()).unwrap();
        assert_eq!((d.fill, d.line), (None, None));
        assert_eq!(d.text.unwrap().colour, "#000000");
        // A connector has no fill.
        let c = drawn(&shape(ShapeKind::Connector), place()).unwrap();
        assert_eq!(c.fill, None);
        assert_eq!(c.line.unwrap().width, 1.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__hidden_objects_pictures_and_other_geometries_are_not_drawn() {
        let mut hidden = shape(ShapeKind::Shape);
        hidden.hidden = true;
        assert!(drawn(&hidden, place()).is_none());
        assert!(drawn(&shape(ShapeKind::Picture), place()).is_none());
        let mut star = shape(ShapeKind::Shape);
        star.geometry = Some("star5".into());
        assert!(drawn(&star, place()).is_none());
        // A pattern or picture fill is not drawn, and the outline still is.
        let mut pattern = shape(ShapeKind::Shape);
        pattern.fill = ShapeFill::Other;
        let d = drawn(&pattern, place()).unwrap();
        assert_eq!((d.fill, d.gradient), (None, None));
    }
}
