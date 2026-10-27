//! The styles part of an Excel binary workbook (MS-XLSB 2.1.7.50): number formats (BrtFmt),
//! fonts (BrtFont), fills (BrtFill), borders (BrtBorder), cell style XFs and cell XFs (BrtXF),
//! cell styles (BrtStyle), differential formats (BrtDXF), the indexed color palette and the most
//! recently used colors, written as the SpreadsheetML styles part. A table style or a slicer style
//! the part defines beyond the built-in ones is not carried.

use super::records::{Cursor, Malformed, Read, records, text};
use super::{MAIN_NAMESPACE, Xml, refusal};
use crate::OpenError;

const BRT_FONT: u16 = 43;
const BRT_FMT: u16 = 44;
const BRT_FILL: u16 = 45;
const BRT_BORDER: u16 = 46;
const BRT_XF: u16 = 47;
const BRT_STYLE: u16 = 48;
const BRT_INDEXED_COLOR: u16 = 475;
const BRT_MRU_COLOR: u16 = 572;
const BRT_DXF: u16 = 507;
const BRT_BEGIN_CELL_STYLE_XFS: u16 = 626;
const BRT_BEGIN_CELL_XFS: u16 = 617;
const BRT_BEGIN_TABLE_STYLES: u16 = 508;

/// A BrtColor (2.4.337) as SpreadsheetML color attributes; empty for an automatic color.
pub(crate) fn color(c: &mut Cursor<'_>) -> Read<Vec<(&'static str, String)>> {
    let first = c.u8("a color")?;
    let kind = first >> 1;
    let index = c.u8("a color")?;
    let tint = c.u16("a color")? as i16;
    let rgba = c.bytes(4, "a color")?;
    let mut out = Vec::new();
    match kind {
        0 => out.push(("auto", "1".to_owned())),
        1 => out.push(("indexed", index.to_string())),
        2 => out.push((
            "rgb",
            format!(
                "{:02X}{:02X}{:02X}{:02X}",
                rgba[3], rgba[0], rgba[1], rgba[2]
            ),
        )),
        3 => out.push(("theme", index.to_string())),
        other => return Err(Malformed(format!("the color type {other}"))),
    }
    if tint != 0 {
        out.push(("tint", tint_text(f64::from(tint) / 32767.0)));
    }
    Ok(out)
}

/// A tint as Excel writes it: 17 significant digits (`0.79998168889431442`).
fn tint_text(value: f64) -> String {
    let scientific = format!("{value:.16e}");
    let (mantissa, exponent) = scientific.split_once('e').expect("an exponent");
    let exponent: i32 = exponent.parse().expect("an exponent");
    let sign = if mantissa.starts_with('-') { "-" } else { "" };
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let text = if exponent >= 0 {
        let whole = exponent as usize + 1;
        if digits.len() <= whole {
            format!("{sign}{digits}{}", "0".repeat(whole - digits.len()))
        } else {
            format!("{sign}{}.{}", &digits[..whole], &digits[whole..])
        }
    } else {
        format!("{sign}0.{}{digits}", "0".repeat((-exponent - 1) as usize))
    };
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// A font (BrtFont, 2.4.690), as the children of `<font>` or `<rPr>`.
#[derive(Clone, Debug, Default)]
pub(crate) struct Font {
    pub(crate) height: u16,
    pub(crate) italic: bool,
    pub(crate) strike: bool,
    pub(crate) outline: bool,
    pub(crate) shadow: bool,
    pub(crate) condense: bool,
    pub(crate) extend: bool,
    pub(crate) weight: u16,
    pub(crate) script: u16,
    pub(crate) underline: u8,
    pub(crate) family: u8,
    pub(crate) charset: u8,
    pub(crate) color: Vec<(&'static str, String)>,
    pub(crate) scheme: u8,
    pub(crate) name: String,
}

pub(crate) fn font(body: &[u8]) -> Read<Font> {
    let mut c = Cursor::new(body);
    let height = c.u16("BrtFont")?;
    let flags = c.u16("BrtFont")?;
    let weight = c.u16("BrtFont")?;
    let script = c.u16("BrtFont")?;
    let underline = c.u8("BrtFont")?;
    let family = c.u8("BrtFont")?;
    let charset = c.u8("BrtFont")?;
    c.skip(1, "BrtFont")?;
    let color = color(&mut c)?;
    let scheme = c.u8("BrtFont")?;
    let name = text(&c.wide("BrtFont")?);
    Ok(Font {
        height,
        italic: flags & 0x02 != 0,
        strike: flags & 0x08 != 0,
        outline: flags & 0x10 != 0,
        shadow: flags & 0x20 != 0,
        condense: flags & 0x40 != 0,
        extend: flags & 0x80 != 0,
        weight,
        script,
        underline,
        family,
        charset,
        color,
        scheme,
        name,
    })
}

/// Writes a font's properties in SpreadsheetML order; `run` names the font element `rFont` (a
/// rich text run's) instead of `name`.
pub(crate) fn write_font(x: &mut Xml, f: &Font, run: bool) {
    if f.weight >= 700 {
        x.empty("b", &[]);
    }
    if f.italic {
        x.empty("i", &[]);
    }
    if f.strike {
        x.empty("strike", &[]);
    }
    if f.condense {
        x.empty("condense", &[]);
    }
    if f.extend {
        x.empty("extend", &[]);
    }
    if f.outline {
        x.empty("outline", &[]);
    }
    if f.shadow {
        x.empty("shadow", &[]);
    }
    match f.underline {
        0x01 => x.empty("u", &[]),
        0x02 => x.empty("u", &[("val", "double".to_owned())]),
        0x21 => x.empty("u", &[("val", "singleAccounting".to_owned())]),
        0x22 => x.empty("u", &[("val", "doubleAccounting".to_owned())]),
        _ => {}
    }
    match f.script {
        1 => x.empty("vertAlign", &[("val", "superscript".to_owned())]),
        2 => x.empty("vertAlign", &[("val", "subscript".to_owned())]),
        _ => {}
    }
    x.empty(
        "sz",
        &[(
            "val",
            super::formula::number_text(f64::from(f.height) / 20.0),
        )],
    );
    // An automatic color (the system's window text) is the default, which SpreadsheetML leaves
    // out.
    if f.color.first().is_some_and(|(name, _)| *name != "auto") {
        x.empty("color", &f.color);
    }
    x.empty(
        if run { "rFont" } else { "name" },
        &[("val", f.name.clone())],
    );
    if f.family != 0 {
        x.empty("family", &[("val", f.family.to_string())]);
    }
    if f.charset != 0 {
        x.empty("charset", &[("val", f.charset.to_string())]);
    }
    match f.scheme {
        1 => x.empty("scheme", &[("val", "major".to_owned())]),
        2 => x.empty("scheme", &[("val", "minor".to_owned())]),
        _ => {}
    }
}

/// The fonts of a styles part, for rich text runs elsewhere.
pub(crate) fn fonts(data: &[u8]) -> Read<Vec<Font>> {
    records(data)?
        .iter()
        .filter(|r| r.kind == BRT_FONT)
        .map(|r| font(r.body))
        .collect()
}

const PATTERNS: [&str; 19] = [
    "none",
    "solid",
    "mediumGray",
    "darkGray",
    "lightGray",
    "darkHorizontal",
    "darkVertical",
    "darkDown",
    "darkUp",
    "darkGrid",
    "darkTrellis",
    "lightHorizontal",
    "lightVertical",
    "lightDown",
    "lightUp",
    "lightGrid",
    "lightTrellis",
    "gray125",
    "gray0625",
];

const BORDER_STYLES: [&str; 14] = [
    "none",
    "thin",
    "medium",
    "dashed",
    "dotted",
    "thick",
    "double",
    "hair",
    "mediumDashed",
    "dashDot",
    "mediumDashDot",
    "dashDotDot",
    "mediumDashDotDot",
    "slantDashDot",
];

fn fill(x: &mut Xml, body: &[u8]) -> Read<()> {
    let mut c = Cursor::new(body);
    let pattern = c.u32("BrtFill")?;
    let fore = color(&mut c)?;
    let back = color(&mut c)?;
    x.open("fill", &[]);
    if pattern == 0x28 {
        let kind = c.u32("BrtFill")?;
        let degree = c.f64("BrtFill")?;
        let edges = [
            c.f64("BrtFill")?,
            c.f64("BrtFill")?,
            c.f64("BrtFill")?,
            c.f64("BrtFill")?,
        ];
        let stops = c.u32("BrtFill")?;
        let mut attributes = Vec::new();
        if kind == 1 {
            attributes.push(("type", "path".to_owned()));
            for (name, value) in ["left", "right", "top", "bottom"].iter().zip(edges) {
                if value != 0.0 {
                    attributes.push((name, super::formula::number_text(value)));
                }
            }
        } else if degree != 0.0 {
            attributes.push(("degree", super::formula::number_text(degree)));
        }
        x.open("gradientFill", &attributes);
        for _ in 0..stops {
            let stop_color = color(&mut c)?;
            let position = c.f64("GradientStop")?;
            x.open(
                "stop",
                &[("position", super::formula::number_text(position))],
            );
            x.empty("color", &stop_color);
            x.close("stop");
        }
        x.close("gradientFill");
    } else {
        let name = PATTERNS
            .get(pattern as usize)
            .ok_or_else(|| Malformed(format!("the fill pattern {pattern}")))?;
        // The system foreground (indexed 64) and background (indexed 65) are the defaults, which
        // Excel leaves out; it writes the background beside a foreground with a tint (measured in
        // Excel-saved twins).
        let fore_default = fore
            .first()
            .is_some_and(|c| *c == ("indexed", "64".to_owned()));
        let back_default = back
            .first()
            .is_some_and(|c| *c == ("indexed", "65".to_owned()))
            && !fore.iter().any(|(name, _)| *name == "tint");
        let fore = (pattern != 0 && !fore.is_empty() && !fore_default).then_some(fore);
        let back = (pattern != 0 && !back.is_empty() && !back_default).then_some(back);
        if fore.is_none() && back.is_none() {
            x.empty("patternFill", &[("patternType", (*name).to_owned())]);
        } else {
            x.open("patternFill", &[("patternType", (*name).to_owned())]);
            if let Some(fore) = &fore {
                x.empty("fgColor", fore);
            }
            if let Some(back) = &back {
                x.empty("bgColor", back);
            }
            x.close("patternFill");
        }
    }
    x.close("fill");
    Ok(())
}

fn border(x: &mut Xml, body: &[u8]) -> Read<()> {
    let mut c = Cursor::new(body);
    let flags = c.u8("BrtBorder")?;
    let mut attributes = Vec::new();
    if flags & 0x01 != 0 {
        attributes.push(("diagonalDown", "1".to_owned()));
    }
    if flags & 0x02 != 0 {
        attributes.push(("diagonalUp", "1".to_owned()));
    }
    x.open("border", &attributes);
    for name in ["left", "right", "top", "bottom", "diagonal"] {
        // The record holds top, bottom, left, right, diagonal; SpreadsheetML writes left,
        // right, top, bottom, diagonal.
        let at = 1 + 10
            * match name {
                "top" => 0,
                "bottom" => 1,
                "left" => 2,
                "right" => 3,
                _ => 4,
            };
        let mut b = Cursor::new(body);
        b.skip(at, "BrtBorder")?;
        let style = b.u8("a border")?;
        b.skip(1, "a border")?;
        let side_color = color(&mut b)?;
        let style_name = BORDER_STYLES
            .get(usize::from(style))
            .ok_or_else(|| Malformed(format!("the border style {style}")))?;
        if style == 0 {
            x.empty(name, &[]);
        } else {
            x.open(name, &[("style", (*style_name).to_owned())]);
            if !side_color.is_empty() {
                x.empty("color", &side_color);
            }
            x.close(name);
        }
    }
    let _ = c;
    x.close("border");
    Ok(())
}

const HORIZONTAL: [&str; 8] = [
    "general",
    "left",
    "center",
    "right",
    "fill",
    "justify",
    "centerContinuous",
    "distributed",
];
const VERTICAL: [&str; 5] = ["top", "center", "bottom", "justify", "distributed"];

fn xf(x: &mut Xml, body: &[u8], style: bool) -> Read<()> {
    let mut c = Cursor::new(body);
    let parent = c.u16("BrtXF")?;
    let format = c.u16("BrtXF")?;
    let font = c.u16("BrtXF")?;
    let fill = c.u16("BrtXF")?;
    let border = c.u16("BrtXF")?;
    let rotation = c.u8("BrtXF")?;
    let indent = c.u8("BrtXF")?;
    let a = c.u16("BrtXF")?;
    let b = c.u16("BrtXF")?;
    let horizontal = a & 0x7;
    let vertical = (a >> 3) & 0x7;
    let wrap = a & (1 << 6) != 0;
    let justify_last = a & (1 << 7) != 0;
    let shrink = a & (1 << 8) != 0;
    let reading = (a >> 10) & 0x3;
    let locked = a & (1 << 12) != 0;
    let hidden = a & (1 << 13) != 0;
    let quote = a & (1 << 15) != 0;
    let applied = b & 0x3F;
    let mut attributes = vec![
        ("numFmtId", format.to_string()),
        ("fontId", font.to_string()),
        ("fillId", fill.to_string()),
        ("borderId", border.to_string()),
    ];
    if !style {
        attributes.push(("xfId", parent.to_string()));
    }
    if quote {
        attributes.push(("quotePrefix", "1".to_owned()));
    }
    for (bit, name) in [
        (0, "applyNumberFormat"),
        (1, "applyFont"),
        (4, "applyFill"),
        (3, "applyBorder"),
        (2, "applyAlignment"),
        (5, "applyProtection"),
    ] {
        let set = applied & (1 << bit) != 0;
        if style {
            if set {
                attributes.push((name, "0".to_owned()));
            }
        } else if set {
            attributes.push((name, "1".to_owned()));
        }
    }
    let mut alignment = Vec::new();
    if horizontal != 0 {
        alignment.push(("horizontal", HORIZONTAL[horizontal as usize].to_owned()));
    }
    if vertical != 2 {
        alignment.push((
            "vertical",
            VERTICAL
                .get(vertical as usize)
                .copied()
                .unwrap_or("bottom")
                .to_owned(),
        ));
    }
    if rotation != 0 {
        alignment.push(("textRotation", rotation.to_string()));
    }
    if wrap {
        alignment.push(("wrapText", "1".to_owned()));
    }
    if indent != 0 {
        alignment.push(("indent", indent.to_string()));
    }
    if justify_last {
        alignment.push(("justifyLastLine", "1".to_owned()));
    }
    if shrink {
        alignment.push(("shrinkToFit", "1".to_owned()));
    }
    if reading != 0 {
        alignment.push(("readingOrder", reading.to_string()));
    }
    let mut protection = Vec::new();
    if !locked {
        protection.push(("locked", "0".to_owned()));
    }
    if hidden {
        protection.push(("hidden", "1".to_owned()));
    }
    if alignment.is_empty() && protection.is_empty() {
        x.empty("xf", &attributes);
    } else {
        x.open("xf", &attributes);
        if !alignment.is_empty() {
            x.empty("alignment", &alignment);
        }
        if !protection.is_empty() {
            x.empty("protection", &protection);
        }
        x.close("xf");
    }
    Ok(())
}

/// An XFPropColor (2.5.161) as color attributes.
fn prop_color(c: &mut Cursor<'_>) -> Read<Vec<(&'static str, String)>> {
    let first = c.u8("a property color")?;
    let kind = first >> 1;
    let index = c.u8("a property color")?;
    let tint = c.u16("a property color")? as i16;
    let rgba = c.bytes(4, "a property color")?;
    let mut out = Vec::new();
    match kind {
        0 => out.push(("auto", "1".to_owned())),
        1 => out.push(("indexed", index.to_string())),
        2 => out.push((
            "rgb",
            format!(
                "{:02X}{:02X}{:02X}{:02X}",
                rgba[3], rgba[0], rgba[1], rgba[2]
            ),
        )),
        3 => out.push(("theme", index.to_string())),
        _ => {}
    }
    if tint != 0 {
        out.push(("tint", tint_text(f64::from(tint) / 32767.0)));
    }
    Ok(out)
}

/// A differential format (BrtDXF, 2.4.359) as a `<dxf>`: its font, number format, fill,
/// alignment, border and protection properties.
fn dxf(x: &mut Xml, body: &[u8]) -> Read<()> {
    let mut c = Cursor::new(body);
    c.skip(2, "BrtDXF")?;
    c.skip(2, "XFProps")?;
    let count = c.u16("XFProps")?;
    let mut font: Vec<(String, Vec<(&'static str, String)>)> = Vec::new();
    let mut number: Option<(u16, Option<String>)> = None;
    let mut pattern: Option<u8> = None;
    let mut fore = None;
    let mut back = None;
    let mut borders: Vec<(&str, u16, Vec<(&'static str, String)>)> = Vec::new();
    let mut alignment = Vec::new();
    let mut protection = Vec::new();
    for _ in 0..count {
        let kind = c.u16("XFProp")?;
        let size = c.u16("XFProp")? as usize;
        let data = c.bytes(size.saturating_sub(4), "XFProp")?;
        let mut d = Cursor::new(data);
        match kind {
            0x00 => pattern = Some(d.u8("FillPattern")?),
            0x01 => fore = Some(prop_color(&mut d)?),
            0x02 => back = Some(prop_color(&mut d)?),
            0x05 => font.push(("color".to_owned(), prop_color(&mut d)?)),
            0x06..=0x0A => {
                let side = match kind {
                    0x06 => "top",
                    0x07 => "bottom",
                    0x08 => "left",
                    0x09 => "right",
                    _ => "diagonal",
                };
                let color = prop_color(&mut d)?;
                let style = d.u16("XFPropBorder")?;
                borders.push((side, style, color));
            }
            0x0F => alignment.push((
                "horizontal",
                HORIZONTAL
                    .get(usize::from(d.u8("HorizAlign")?))
                    .copied()
                    .unwrap_or("general")
                    .to_owned(),
            )),
            0x10 => alignment.push((
                "vertical",
                VERTICAL
                    .get(usize::from(d.u8("VertAlign")?))
                    .copied()
                    .unwrap_or("bottom")
                    .to_owned(),
            )),
            0x14 => {
                if d.u8("wrap")? != 0 {
                    alignment.push(("wrapText", "1".to_owned()));
                }
            }
            0x18 => {
                let count = d.u16("LPWideString")? as usize;
                font.push((
                    "name".to_owned(),
                    vec![("val", text(&d.units(count, "a font name")?))],
                ));
            }
            0x19 => {
                if d.u16("Bold")? >= 700 {
                    font.push(("b".to_owned(), Vec::new()));
                } else {
                    font.push(("b".to_owned(), vec![("val", "0".to_owned())]));
                }
            }
            0x1A => {
                let value = d.u16("Underline")?;
                let name = match value {
                    0 => "none",
                    1 => "single",
                    2 => "double",
                    0x21 => "singleAccounting",
                    _ => "doubleAccounting",
                };
                font.push(("u".to_owned(), vec![("val", name.to_owned())]));
            }
            0x1B => match d.u16("Script")? {
                1 => font.push((
                    "vertAlign".to_owned(),
                    vec![("val", "superscript".to_owned())],
                )),
                2 => font.push((
                    "vertAlign".to_owned(),
                    vec![("val", "subscript".to_owned())],
                )),
                _ => font.push(("vertAlign".to_owned(), vec![("val", "baseline".to_owned())])),
            },
            0x25 => match d.u8("FontScheme")? {
                1 => font.push(("scheme".to_owned(), vec![("val", "major".to_owned())])),
                2 => font.push(("scheme".to_owned(), vec![("val", "minor".to_owned())])),
                _ => font.push(("scheme".to_owned(), vec![("val", "none".to_owned())])),
            },
            0x1C => {
                let on = d.u8("italic")? != 0;
                font.push((
                    "i".to_owned(),
                    if on {
                        Vec::new()
                    } else {
                        vec![("val", "0".to_owned())]
                    },
                ));
            }
            0x1D => {
                let on = d.u8("strike")? != 0;
                font.push((
                    "strike".to_owned(),
                    if on {
                        Vec::new()
                    } else {
                        vec![("val", "0".to_owned())]
                    },
                ));
            }
            0x24 => {
                let twips = d.u32("size")?;
                font.push((
                    "sz".to_owned(),
                    vec![("val", super::formula::number_text(f64::from(twips) / 20.0))],
                ));
            }
            0x26 => {
                let count = d.u16("a number format")? as usize;
                let code = text(&d.units(count, "a number format")?);
                number = Some((number.map_or(0, |n| n.0), Some(code)));
            }
            0x29 => {
                let id = d.u16("Ifmt")?;
                number = Some((id, number.and_then(|n| n.1)));
            }
            0x2B => protection.push((
                "locked",
                if d.u8("locked")? != 0 { "1" } else { "0" }.to_owned(),
            )),
            0x2C => protection.push((
                "hidden",
                if d.u8("hidden")? != 0 { "1" } else { "0" }.to_owned(),
            )),
            _ => {}
        }
    }
    x.open("dxf", &[]);
    if !font.is_empty() {
        x.open("font", &[]);
        for (name, attributes) in &font {
            x.empty(name, attributes);
        }
        x.close("font");
    }
    if let Some((id, code)) = number {
        x.empty(
            "numFmt",
            &[
                ("numFmtId", id.to_string()),
                ("formatCode", code.unwrap_or_default()),
            ],
        );
    }
    if pattern.is_some() || fore.is_some() || back.is_some() {
        x.open("fill", &[]);
        let mut attributes = Vec::new();
        if let Some(p) = pattern {
            attributes.push((
                "patternType",
                PATTERNS
                    .get(usize::from(p))
                    .copied()
                    .unwrap_or("solid")
                    .to_owned(),
            ));
        }
        x.open("patternFill", &attributes);
        if let Some(fore) = &fore {
            x.empty("fgColor", fore);
        }
        if let Some(back) = &back {
            x.empty("bgColor", back);
        }
        x.close("patternFill");
        x.close("fill");
    }
    if !alignment.is_empty() {
        x.empty("alignment", &alignment);
    }
    if !borders.is_empty() {
        x.open("border", &[]);
        for side in ["left", "right", "top", "bottom", "diagonal"] {
            if let Some((_, style, color)) = borders.iter().find(|b| b.0 == side) {
                let name = BORDER_STYLES
                    .get(usize::from(*style))
                    .copied()
                    .unwrap_or("thin");
                if *style == 0 {
                    x.empty(side, &[]);
                } else {
                    x.open(side, &[("style", name.to_owned())]);
                    x.empty("color", color);
                    x.close(side);
                }
            }
        }
        x.close("border");
    }
    if !protection.is_empty() {
        x.empty("protection", &protection);
    }
    x.close("dxf");
    Ok(())
}

pub(crate) fn convert(part: &str, data: &[u8]) -> Result<Vec<u8>, OpenError> {
    write(data).map_err(|error| refusal(part, error.0))
}

fn write(data: &[u8]) -> Read<Vec<u8>> {
    let records = records(data)?;
    let of = |kind: u16| records.iter().filter(move |r| r.kind == kind);
    let mut x = Xml::new();
    x.open("styleSheet", &[("xmlns", MAIN_NAMESPACE.to_owned())]);
    let formats: Vec<_> = of(BRT_FMT).collect();
    if !formats.is_empty() {
        x.open("numFmts", &[("count", formats.len().to_string())]);
        for r in formats {
            let mut c = Cursor::new(r.body);
            let id = c.u16("BrtFmt")?;
            let code = text(&c.wide("BrtFmt")?);
            x.empty(
                "numFmt",
                &[("numFmtId", id.to_string()), ("formatCode", code)],
            );
        }
        x.close("numFmts");
    }
    let fonts: Vec<_> = of(BRT_FONT).collect();
    x.open("fonts", &[("count", fonts.len().to_string())]);
    for r in fonts {
        let f = font(r.body)?;
        x.open("font", &[]);
        write_font(&mut x, &f, false);
        x.close("font");
    }
    x.close("fonts");
    let fills: Vec<_> = of(BRT_FILL).collect();
    x.open("fills", &[("count", fills.len().to_string())]);
    for r in fills {
        fill(&mut x, r.body)?;
    }
    x.close("fills");
    let borders: Vec<_> = of(BRT_BORDER).collect();
    x.open("borders", &[("count", borders.len().to_string())]);
    for r in borders {
        border(&mut x, r.body)?;
    }
    x.close("borders");
    // The XFs after BrtBeginCellStyleXFs are style XFs, those after BrtBeginCellXFs cell XFs.
    let mut style_xfs = Vec::new();
    let mut cell_xfs = Vec::new();
    let mut current: Option<bool> = None;
    for r in &records {
        match r.kind {
            BRT_BEGIN_CELL_STYLE_XFS => current = Some(true),
            BRT_BEGIN_CELL_XFS => current = Some(false),
            BRT_XF => match current {
                Some(true) => style_xfs.push(r.body),
                Some(false) => cell_xfs.push(r.body),
                None => {}
            },
            _ => {}
        }
    }
    x.open("cellStyleXfs", &[("count", style_xfs.len().to_string())]);
    for body in style_xfs {
        xf(&mut x, body, true)?;
    }
    x.close("cellStyleXfs");
    x.open("cellXfs", &[("count", cell_xfs.len().to_string())]);
    for body in cell_xfs {
        xf(&mut x, body, false)?;
    }
    x.close("cellXfs");
    let styles: Vec<_> = of(BRT_STYLE).collect();
    x.open("cellStyles", &[("count", styles.len().to_string())]);
    for r in styles {
        let mut c = Cursor::new(r.body);
        let ixf = c.u32("BrtStyle")?;
        let flags = c.u16("BrtStyle")?;
        let builtin = c.u8("BrtStyle")?;
        let level = c.u8("BrtStyle")?;
        let name = text(&c.wide("BrtStyle")?);
        let mut attributes = vec![("name", name), ("xfId", ixf.to_string())];
        if flags & 1 != 0 {
            attributes.push(("builtinId", builtin.to_string()));
            if builtin == 1 || builtin == 2 {
                attributes.push(("iLevel", level.to_string()));
            }
        }
        if flags & 2 != 0 {
            attributes.push(("hidden", "1".to_owned()));
        }
        if flags & 4 != 0 {
            attributes.push(("customBuiltin", "1".to_owned()));
        }
        x.empty("cellStyle", &attributes);
    }
    x.close("cellStyles");
    let dxfs: Vec<_> = of(BRT_DXF).collect();
    x.open("dxfs", &[("count", dxfs.len().to_string())]);
    for r in dxfs {
        dxf(&mut x, r.body)?;
    }
    x.close("dxfs");
    if let Some(r) = of(BRT_BEGIN_TABLE_STYLES).next() {
        let mut c = Cursor::new(r.body);
        let _count = c.u32("BrtBeginTableStyles")?;
        let table = c.nullable_wide("BrtBeginTableStyles")?.map(|u| text(&u));
        let pivot = c.nullable_wide("BrtBeginTableStyles")?.map(|u| text(&u));
        let mut attributes = vec![("count", "0".to_owned())];
        if let Some(table) = table {
            attributes.push(("defaultTableStyle", table));
        }
        if let Some(pivot) = pivot {
            attributes.push(("defaultPivotStyle", pivot));
        }
        x.empty("tableStyles", &attributes);
    }
    let indexed: Vec<_> = of(BRT_INDEXED_COLOR).collect();
    let recent: Vec<_> = of(BRT_MRU_COLOR).collect();
    if !indexed.is_empty() || !recent.is_empty() {
        x.open("colors", &[]);
        if !indexed.is_empty() {
            x.open("indexedColors", &[]);
            for r in indexed {
                let b = r.body;
                if b.len() >= 4 {
                    x.empty(
                        "rgbColor",
                        &[(
                            "rgb",
                            format!("{:02X}{:02X}{:02X}{:02X}", b[3], b[0], b[1], b[2]),
                        )],
                    );
                }
            }
            x.close("indexedColors");
        }
        if !recent.is_empty() {
            x.open("mruColors", &[]);
            for r in recent {
                let mut c = Cursor::new(r.body);
                let attributes = color(&mut c)?;
                x.empty("color", &attributes);
            }
            x.close("mruColors");
        }
        x.close("colors");
    }
    x.close("styleSheet");
    Ok(x.out)
}
