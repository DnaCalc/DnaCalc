//! A drawing object as the page draws it (dc-zjsg.2): the model's [`calc_host::Shape`] with its colours
//! resolved, its lengths in pixels at 100% and the style's defaults filled in where the file leaves
//! a field to the style.
//!
//! What the style gives is Excel's own for a shape inserted from the ribbon (`lnRef idx=2
//! accent1 shade 50%`, `fillRef idx=1 accent1`, `fontRef lt1`): the fill is accent 1, the outline
//! accent 1 shaded, a point wide, the text white. A text box has no style: no fill, no outline,
//! black text. Scheme colours come from the Office theme; a workbook's own theme is not read here.

use crate::wire::{ChartPlace, DrawnShape, ShapeParagraph, ShapeStroke, ShapeText};
use calc_host::{Shape, ShapeColour, ShapeFill, ShapeKind};

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

/// The object as drawn; `None` for one this slice does not draw (a picture, a group, a hidden object,
/// a geometry that is not drawn).
pub(crate) fn drawn(shape: &Shape, place: ChartPlace) -> Option<DrawnShape> {
    if shape.hidden {
        return None;
    }
    let (kind, default_geometry, styled) = match shape.kind {
        ShapeKind::Shape => ("shape", "rect", true),
        ShapeKind::TextBox => ("textbox", "rect", false),
        ShapeKind::Connector => ("connector", "line", true),
        ShapeKind::Picture | ShapeKind::Group => return None,
    };
    let geometry = shape.geometry.as_deref().unwrap_or(default_geometry);
    if !dna_calc_model::drawn_shapes::drawn_geometry(geometry) {
        return None;
    }
    let connector = shape.kind == ShapeKind::Connector;
    let fill = match &shape.fill {
        ShapeFill::None => None,
        ShapeFill::Solid(colour) => css(colour),
        // A gradient, pattern or picture is not drawn: a shape with one draws as if it had none.
        ShapeFill::Other => None,
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
        line,
        text,
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
    fn dc_zjsg_2__hidden_objects_pictures_groups_and_other_geometries_are_not_drawn() {
        let mut hidden = shape(ShapeKind::Shape);
        hidden.hidden = true;
        assert!(drawn(&hidden, place()).is_none());
        assert!(drawn(&shape(ShapeKind::Picture), place()).is_none());
        assert!(drawn(&shape(ShapeKind::Group), place()).is_none());
        let mut star = shape(ShapeKind::Shape);
        star.geometry = Some("star5".into());
        assert!(drawn(&star, place()).is_none());
        // A gradient fill is not drawn (the open rest of the bead), and the outline still is.
        let mut gradient = shape(ShapeKind::Shape);
        gradient.fill = ShapeFill::Other;
        assert_eq!(drawn(&gradient, place()).unwrap().fill, None);
    }
}
