//! A drawing object as the page draws it (dc-zjsg.2): the model's [`calc_host::Shape`] with its colours
//! resolved, its lengths in pixels at 100% and the style's defaults filled in where the file leaves
//! a field to the style.
//!
//! What the style gives is Excel's own for a shape inserted from the ribbon (`lnRef idx=2
//! accent1 shade 50%`, `fillRef idx=1 accent1`, `fontRef lt1`): the fill is accent 1, the outline
//! accent 1 shaded, a point wide, the text white. A text box has no style: no fill, no outline,
//! black text. Scheme colours come from the Office theme; a workbook's own theme is not read here.

use crate::wire::{ChartPlace, DrawnShape, ShapeParagraph, ShapeParagraphRun, ShapeStroke, ShapeText};
use calc_host::{LineEnd, Shape, ShapeColour, ShapeFill, ShapeKind};
use calc_host::ShapeShadow as FileShadow;
use dna_calc_model::drawn_shapes::{DrawnChild, DrawnPicture, ShapeArrow, ShapeGradient, ShapeShadow};
use dna_calc_model::shape_colour::{self, Resolved};

const EMU_PER_PIXEL: f64 = 9_525.0;

/// The Office theme's colours by their scheme names, for a workbook that carries no theme part.
fn office_scheme(name: &str) -> Option<u32> {
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

/// A scheme colour name's slot in the theme part's colour scheme (dk1, lt1, dk2, lt2, accent1 to
/// accent6, hlink, folHlink): `tx1` and `bg1` are dk1 and lt1, as a drawing maps them.
fn slot(name: &str) -> Option<usize> {
    Some(match name {
        "dk1" | "tx1" => 0,
        "lt1" | "bg1" => 1,
        "dk2" | "tx2" => 2,
        "lt2" | "bg2" => 3,
        "accent1" => 4,
        "accent2" => 5,
        "accent3" => 6,
        "accent4" => 7,
        "accent5" => 8,
        "accent6" => 9,
        "hlink" => 10,
        "folHlink" => 11,
        _ => return None,
    })
}

/// A scheme colour of the workbook's theme (`theme`: its twelve `RRGGBB`s, empty without a theme part);
/// a colour the theme does not give is the Office theme's.
fn scheme(name: &str, theme: &[String]) -> Option<u32> {
    slot(name)
        .and_then(|i| theme.get(i))
        .filter(|c| c.len() == 6)
        .and_then(|c| u32::from_str_radix(c, 16).ok())
        .or_else(|| office_scheme(name))
}

/// The colour with its modifiers applied; `None` for a scheme name no theme gives (`phClr`).
fn resolve(colour: &ShapeColour, theme: &[String]) -> Option<Resolved> {
    match colour {
        ShapeColour::Rgb(rgb) => Some(Resolved { rgb: *rgb, alpha: 1.0 }),
        ShapeColour::Scheme(name) => Some(Resolved { rgb: scheme(name, theme)?, alpha: 1.0 }),
        ShapeColour::Modified(base, modifiers) => {
            let base = resolve(base, theme)?;
            let mut out = shape_colour::modify(base.rgb, modifiers);
            out.alpha *= base.alpha;
            Some(out)
        }
    }
}

fn css(colour: &ShapeColour, theme: &[String]) -> Option<String> {
    resolve(colour, theme).map(shape_colour::css)
}

/// A point wide, the weight of a style's `lnRef idx=2`; a connector's `idx=1` is 0.75 pt.
const STYLE_LINE_WEIGHT: i64 = 12_700;
const STYLE_CONNECTOR_WEIGHT: i64 = 9_525;

/// What a ribbon shape's style gives: accent 1 as the fill, accent 1 shaded 50% as the outline.
fn style_fill(theme: &[String]) -> String {
    shape_colour::css(Resolved { rgb: scheme("accent1", theme).unwrap_or(0x4472C4), alpha: 1.0 })
}

fn style_line(theme: &[String]) -> String {
    let modifiers = [("shade".to_owned(), 50_000)];
    shape_colour::css(shape_colour::modify(scheme("accent1", theme).unwrap_or(0x4472C4), &modifiers))
}

/// A gradient's stops and direction as the page draws them; `None` for one with a colour not resolved.
fn gradient(g: &calc_host::ShapeGradient, theme: &[String]) -> Option<ShapeGradient> {
    let stops = g
        .stops
        .iter()
        .map(|(pos, colour)| Some((*pos as f64 / 100_000.0, css(colour, theme)?)))
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

/// An outer shadow with its lengths in pixels; `None` for a colour not resolved.
fn shadow(s: &FileShadow, theme: &[String]) -> Option<ShapeShadow> {
    let angle = (s.direction as f64 / 60_000.0).to_radians();
    let distance = s.distance as f64 / EMU_PER_PIXEL;
    Some(ShapeShadow {
        dx: distance * angle.cos(),
        dy: distance * angle.sin(),
        blur: s.blur as f64 / EMU_PER_PIXEL,
        scale: [s.scale.0 as f64 / 100_000.0, s.scale.1 as f64 / 100_000.0],
        align: s.align.clone().unwrap_or_else(|| "b".into()),
        rotate: s.rotate_with_shape,
        colour: css(&s.colour, theme)?,
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
pub(crate) fn drawn(shape: &Shape, place: ChartPlace, theme: &[String]) -> Option<DrawnShape> {
    drawn_with(shape, place, theme, &[])
}

/// The line width of a style's `a:lnRef idx` (1, 2, 3) in the theme's line styles (`lines`, EMU), or the
/// Office theme's (6350, 12700, 19050) where the workbook gives none.
fn style_line_weight(index: i64, lines: &[i64]) -> Option<i64> {
    let i = usize::try_from(index.checked_sub(1)?).ok()?;
    lines.get(i).copied().filter(|&w| w > 0).or_else(|| [6_350, 12_700, 19_050].get(i).copied())
}

/// [`drawn`] with the theme's line style widths (`lines`, EMU) for a shape's `a:lnRef`.
pub(crate) fn drawn_with(shape: &Shape, place: ChartPlace, theme: &[String], lines: &[i64]) -> Option<DrawnShape> {
    if shape.hidden {
        return None;
    }
    let (kind, default_geometry, styled) = match shape.kind {
        ShapeKind::Shape => ("shape", "rect", true),
        ShapeKind::TextBox => ("textbox", "rect", false),
        ShapeKind::Connector => ("connector", "line", true),
        ShapeKind::Group => ("group", "", false),
        ShapeKind::Picture => ("picture", "rect", false),
    };
    let mut geometry = shape.geometry.as_deref().unwrap_or(default_geometry);
    // A custom geometry, or a preset the model's own paths do not draw, from its ECMA-376 definition (a picture
    // keeps its rectangle).
    let custom_geometry = match shape.custom_geometry.as_ref() {
        Some(g) if shape.geometry.is_none() && kind != "group" => Some(g.clone()),
        _ if kind != "group" && kind != "picture" && !dna_calc_model::drawn_shapes::drawn_geometry(geometry) => {
            crate::custom_geometry::preset(geometry, &shape.adjust)
        }
        _ => None,
    };
    if custom_geometry.is_some() {
        geometry = "custom";
    }
    if kind == "picture" && !dna_calc_model::drawn_shapes::drawn_geometry(geometry) {
        // A picture cropped to a shape this module does not draw is drawn as a rectangle.
        geometry = "rect";
    }
    if kind != "group" && !dna_calc_model::drawn_shapes::drawn_geometry(geometry) {
        return None;
    }
    // A picture's image, or the image a shape is filled with (an equation's fallback picture, dc-zjsg.9). A kept
    // object the page cannot show names its kind in the placeholder.
    let picture = shape
        .media
        .as_ref()
        .filter(|_| kind == "picture" || matches!(shape.fill, ShapeFill::Picture(_)))
        .map(|media| DrawnPicture {
            crop: media.crop.map(|v| v as f64 / 100_000.0),
            placeholder: crate::picture::source(media).err().map(|label| match &shape.kept {
                Some(kept) => format!("{kept}: {label}"),
                None => label,
            }),
            description: media.description.clone(),
        });
    let degrees = shape.rotation as f64 / 60_000.0;
    let (custom, custom_text) = match custom_geometry.as_ref() {
        Some(g) => {
            let (paths, text) = crate::custom_geometry::custom_paths(g, shape.extent.0 as f64, shape.extent.1 as f64);
            (paths, Some(text))
        }
        // A preset the model draws takes its text rectangle from the standard's definition too.
        None if kind == "shape" || kind == "textbox" => (
            Vec::new(),
            crate::custom_geometry::preset(geometry, &shape.adjust)
                .map(|g| crate::custom_geometry::custom_paths(&g, shape.extent.0 as f64, shape.extent.1 as f64).1),
        ),
        None => (Vec::new(), None),
    };
    let mut children = Vec::new();
    if kind == "group" {
        // The children's boxes as fractions of the group's frame, through the group's child space; a group
        // flipped mirrors them and each child's own flip and turn with it.
        let (cx, cy) = (shape.child_offset.0 as f64, shape.child_offset.1 as f64);
        let (cw, ch) = (shape.child_extent.0 as f64, shape.child_extent.1 as f64);
        for child in &shape.children {
            let Some(mut part) = drawn_with(child, ChartPlace::Absolute { x: 0.0, y: 0.0, width: 0.0, height: 0.0 }, theme, lines) else {
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
            children.push(DrawnChild { rel, shape: part });
        }
    }
    let connector = shape.kind == ShapeKind::Connector;
    // The file's `xdr:style` decides what the shape's own properties leave out: a reference of index 0 is
    // none, another takes the reference's colour (the theme's fill styles are drawn as that colour, solid;
    // its line styles give the width). Without one, what Excel's ribbon gives (see the module's head).
    let style = shape.style.as_ref();
    let fill = match &shape.fill {
        ShapeFill::None => None,
        ShapeFill::Solid(colour) => css(colour, theme),
        // A gradient and a pattern are drawn from their own fields; a picture fill is not drawn: a shape
        // with one draws as if it had none.
        ShapeFill::Gradient(_) | ShapeFill::Pattern(_) | ShapeFill::Picture(_) | ShapeFill::Other => None,
        ShapeFill::Style => match style {
            Some(style) if style.fill.index != 0 => style.fill.colour.as_ref().and_then(|c| css(c, theme)),
            Some(_) => None,
            None => styled.then(|| style_fill(theme)).filter(|_| !connector),
        }
        .filter(|_| !connector),
    };
    let line = if shape.line.hidden {
        None
    } else {
        let colour = match (&shape.line.colour, style) {
            (Some(colour), _) => css(colour, theme),
            (None, Some(style)) if style.line.index != 0 => style.line.colour.as_ref().and_then(|c| css(c, theme)),
            (None, Some(_)) => None,
            (None, None) => styled.then(|| style_line(theme)),
        };
        let weight = shape.line.weight.or_else(|| style.and_then(|s| style_line_weight(s.line.index, lines))).unwrap_or(if connector {
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
    // Text whose runs are unfilled is not drawn (an equation's fallback carries it in its picture).
    let text = shape.text.as_ref().filter(|p| !body.text_hidden && p.iter().any(|t| !t.is_empty())).map(|paragraphs| {
        let inset = |value: Option<i64>, default: i64| value.unwrap_or(default) as f64 / EMU_PER_PIXEL;
        let rich_runs = body.rich_runs_for(paragraphs);
        let default_colour = || {
            if let Some(colour) = style.and_then(|s| s.font.as_ref()).and_then(|c| css(c, theme)) {
                return colour;
            }
            let name = if styled && style.is_none() { "lt1" } else { "dk1" };
            shape_colour::css(Resolved { rgb: scheme(name, theme).unwrap_or(0), alpha: 1.0 })
        };
        ShapeText {
            face: body.face.clone(),
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
                    runs: rich_runs.and_then(|runs| runs.get(i)).map_or_else(Vec::new, |runs| {
                        runs.iter().map(|run| ShapeParagraphRun {
                            text: run.text.clone(), face: run.face.clone(), size: run.size as f64 / 100.0 * 96.0 / 72.0,
                            bold: run.bold, italic: run.italic, underline: run.underline,
                            colour: run.colour.as_ref().and_then(|c| css(c, theme)).unwrap_or_else(default_colour),
                        }).collect()
                    }),
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
            underline: body.underline && rich_runs.is_none(),
            colour: body
                .colour
                .as_ref()
                .and_then(|c| css(c, theme))
                .unwrap_or_else(default_colour),
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
            ShapeFill::Gradient(g) => gradient(g, theme),
            _ => None,
        },
        line,
        text,
        rotation: degrees,
        pattern: match &shape.fill {
            ShapeFill::Pattern(p) => Some(dna_calc_model::drawn_shapes::ShapePattern {
                preset: p.preset.clone(),
                foreground: p.foreground.as_ref().and_then(|c| css(c, theme)).unwrap_or_else(|| "#000000".into()),
                background: p.background.as_ref().and_then(|c| css(c, theme)).unwrap_or_else(|| "#FFFFFF".into()),
            }),
            _ => None,
        },
        custom,
        custom_text,
        kept: shape.kept.clone(),
        adjust: shape.adjust.clone(),
        children,
        id: shape.id,
        picture,
        shadow: shape.shadow.as_ref().and_then(|s| shadow(s, theme)),
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
        s.body.face = Some("Arial".into());
        s.body.bold = true;
        s.body.underline = true;
        let d = drawn(&s, place(), &[]).unwrap();
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
        assert_eq!(text.face.as_deref(), Some("Arial"));
        assert!(text.underline);
        // A ribbon shape's text is white.
        assert_eq!(text.colour, "#FFFFFF");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn explicit_rich_runs_reach_the_shape_renderer() {
        use calc_host::ShapeTextRun;
        let mut s = shape(ShapeKind::TextBox);
        s.text = Some(vec!["AB".into()]); s.body.size = Some(1200); s.body.bold = true;
        s.body.colour = Some(ShapeColour::Rgb(0xff0000));
        s.body.underline = true;
        s.body.runs = vec![vec![
            ShapeTextRun { text: "A".into(), face: None, size: 1200, bold: true, italic: false, underline: true, colour: Some(ShapeColour::Rgb(0xff0000)) },
            ShapeTextRun { text: "B".into(), face: Some("Courier".into()), size: 900, bold: false, italic: true, underline: false, colour: None },
        ]];
        let d = drawn(&s, place(), &[]).unwrap();
        let text = d.text.as_ref().unwrap();
        assert_eq!(text.paragraphs[0].runs.len(), 2);
        assert_eq!(text.paragraphs[0].runs[0].colour, "#FF0000");
        assert_eq!(text.paragraphs[0].runs[1].colour, "#000000");
        assert_eq!(text.paragraphs[0].runs[1].face.as_deref(), Some("Courier"));
        assert!(!text.underline && text.paragraphs[0].runs[0].underline && !text.paragraphs[0].runs[1].underline);
        let markup = dna_calc_model::drawn_shapes::shape_markup(&d, 100.0, 60.0, 1.0).unwrap();
        assert!(markup.contains("font-weight:bold;font-style:normal;color:#FF0000;text-decoration:underline\">A</span>"), "{markup}");
        assert!(markup.contains("font-weight:normal;font-style:italic;color:#000000;font-family:'\\43 \\6f \\75 \\72 \\69 \\65 \\72 '\">B</span>"), "{markup}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__what_the_file_leaves_to_the_style_takes_the_ribbon_shapes_style() {
        let d = drawn(&shape(ShapeKind::Shape), place(), &[]).unwrap();
        assert_eq!(d.fill.as_deref(), Some("#4472C4"));
        let line = d.line.unwrap();
        assert_eq!((line.colour.as_str(), line.width), ("#2F528F", 12_700.0 / 9_525.0));
        // A text box has no style: nothing is drawn but its text, in black.
        let mut b = shape(ShapeKind::TextBox);
        b.text = Some(vec!["a".into()]);
        let d = drawn(&b, place(), &[]).unwrap();
        assert_eq!((d.fill, d.line), (None, None));
        assert_eq!(d.text.unwrap().colour, "#000000");
        // A connector has no fill.
        let c = drawn(&shape(ShapeKind::Connector), place(), &[]).unwrap();
        assert_eq!(c.fill, None);
        assert_eq!(c.line.unwrap().width, 1.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__hidden_objects_and_other_geometries_are_not_drawn() {
        let mut hidden = shape(ShapeKind::Shape);
        hidden.hidden = true;
        assert!(drawn(&hidden, place(), &[]).is_none());
        // A preset the model's paths lack is drawn from its ECMA-376 definition; a name no standard defines is not.
        let mut star = shape(ShapeKind::Shape);
        star.geometry = Some("star5".into());
        star.extent = (100, 100);
        let d = drawn(&star, place(), &[]).unwrap();
        assert_eq!((d.geometry.as_str(), d.custom.len()), ("custom", 1));
        star.geometry = Some("noSuchShape".into());
        assert!(drawn(&star, place(), &[]).is_none());
        // A pattern or picture fill is not drawn, and the outline still is.
        let mut pattern = shape(ShapeKind::Shape);
        pattern.fill = ShapeFill::Other;
        let d = drawn(&pattern, place(), &[]).unwrap();
        assert_eq!((d.fill, d.gradient), (None, None));
    }

    fn theme() -> Vec<String> {
        // A workbook theme that is not Office's: dk1 000000, lt1 FFFFFF, dk2 112233, lt2 EEEEEE, accents 1 to 6, links.
        ["000000", "FFFFFF", "112233", "EEEEEE", "C00000", "00B050", "0070C0", "7030A0", "FFC000", "92D050", "0563C1", "954F72"]
            .iter()
            .map(|c| (*c).to_owned())
            .collect()
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__the_workbooks_theme_colours_replace_the_fixed_office_ones() {
        let mut s = shape(ShapeKind::Shape);
        s.fill = ShapeFill::Solid(ShapeColour::Scheme("accent2".into()));
        // Without a theme part the Office accent 2; with the workbook's theme its own.
        assert_eq!(drawn(&s, place(), &[]).unwrap().fill.as_deref(), Some("#ED7D31"));
        assert_eq!(drawn(&s, place(), &theme()).unwrap().fill.as_deref(), Some("#00B050"));
        // tx1 and bg1 are the theme's dk1 and lt1; the shape's style takes accent 1 (red here) and its shade for the outline.
        let styled = drawn(&shape(ShapeKind::Shape), place(), &theme()).unwrap();
        assert_eq!(styled.fill.as_deref(), Some("#C00000"));
        assert_ne!(styled.line.as_ref().unwrap().colour, "#2F528F");
        assert_eq!(styled.text, None);
        // A theme part short of a colour falls back to Office's for that one.
        let short: Vec<String> = vec![String::new(); 12];
        assert_eq!(drawn(&s, place(), &short).unwrap().fill.as_deref(), Some("#ED7D31"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__modified_colours_and_shadows_resolve() {
        let mut s = shape(ShapeKind::Shape);
        let lighter = ShapeColour::Modified(
            Box::new(ShapeColour::Scheme("accent1".into())),
            vec![("lumMod".into(), 60_000), ("lumOff".into(), 40_000)],
        );
        s.fill = ShapeFill::Solid(lighter.clone());
        s.line = ShapeLine {
            weight: Some(9_525),
            colour: Some(ShapeColour::Modified(Box::new(ShapeColour::Rgb(0xFF0000)), vec![("alpha".into(), 50_000)])),
            ..Default::default()
        };
        s.shadow = Some(FileShadow {
            blur: 50_800,
            distance: 76_200,
            direction: 5_400_000,
            scale: (90_000, 100_000),
            align: None,
            rotate_with_shape: false,
            colour: ShapeColour::Modified(Box::new(ShapeColour::Rgb(0)), vec![("alpha".into(), 40_000)]),
        });
        let d = drawn(&s, place(), &[]).unwrap();
        // Excel's "Accent 1, Lighter 40%".
        assert_eq!(d.fill.as_deref(), Some("#8FAADC"));
        assert_eq!(d.line.unwrap().colour, "rgba(255,0,0,0.5)");
        let shadow = d.shadow.unwrap();
        assert!(shadow.dx.abs() < 1e-9 && (shadow.dy - 8.0).abs() < 1e-9 && (shadow.blur - 5.333_333).abs() < 1e-5);
        assert_eq!((shadow.scale, shadow.align.as_str(), shadow.rotate, shadow.colour.as_str()), ([0.9, 1.0], "b", false, "rgba(0,0,0,0.4)"));
        // A modified colour in a gradient stop and in text resolves too.
        let mut g = shape(ShapeKind::Shape);
        g.fill = ShapeFill::Gradient(calc_host::ShapeGradient { stops: vec![(0, lighter.clone()), (100_000, ShapeColour::Rgb(0))], angle: Some(0), ..Default::default() });
        g.body.colour = Some(lighter);
        g.text = Some(vec!["t".into()]);
        let d = drawn(&g, place(), &[]).unwrap();
        assert_eq!(d.gradient.unwrap().stops[0].1, "#8FAADC");
        assert_eq!(d.text.unwrap().colour, "#8FAADC");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_3__a_picture_in_a_group_keeps_its_image_and_its_flips() {
        let mut group = shape(ShapeKind::Group);
        group.child_extent = (100, 100);
        let mut picture = shape(ShapeKind::Picture);
        picture.id = 5;
        picture.extent = (50, 50);
        picture.flip_vertical = true;
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        picture.media = Some(calc_host::ShapePicture {
            image: Some(calc_host::PictureMedia {
                part: String::new(),
                content_type: "image/png".into(),
                size: png.len() as u64,
                bytes: png.to_vec().into(),
            }),
            ..Default::default()
        });
        group.children = vec![picture];
        let d = drawn(&group, place(), &[]).unwrap();
        let child = &d.children[0].shape;
        // No typed placeholder: the page draws the image, flipped.
        assert_eq!(child.picture.as_ref().unwrap().placeholder, None);
        assert!(child.flip_vertical && !child.flip_horizontal);
        // A group flipped horizontally mirrors the picture inside it.
        group.flip_horizontal = true;
        let d = drawn(&group, place(), &[]).unwrap();
        assert!(d.children[0].shape.flip_horizontal);
    }
}
