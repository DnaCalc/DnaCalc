//! New drawing objects as anchors of a spreadsheet drawing (dc-zjsg.1). The anchor of an authored
//! shape, text box or connector is written in the form Excel's own files use (xdr:sp with its
//! `xdr:style` for an auto shape, `txBox="1"` for a text box, xdr:cxnSp for a connector); which
//! defaults Excel writes (names, ids, no `editAs`, the text box's fill and line, text body
//! properties) are those of the XML Excel saved for the report macro (excel-answers.md Q0). Objects
//! read from a file are never written by this module: their XML stays as stored.

use super::{SaveRefusal, text::attribute_value};
use calc_doc_model::{
    ChartAnchor, DrawingPoint, Shape, ShapeColour, ShapeFill, ShapeKind, ShapeLine,
};
use std::collections::BTreeMap;

const XDR: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

fn refuse(shape: &Shape, detail: &str) -> SaveRefusal {
    SaveRefusal::ChartNotModelled {
        part: format!("drawing object {}", shape.name),
        detail: detail.into(),
        owner: "dc-zjsg.4",
    }
}

fn valid_text(s: &str) -> bool {
    s.encode_utf16().count() <= 32767
        && !s.chars().any(|c| {
            (c < ' ' && !matches!(c, '\t' | '\n' | '\r' | '\u{b}'))
                || matches!(c, '\u{fffe}' | '\u{ffff}')
        })
}

fn element_text(value: &str, out: &mut Vec<u8>) {
    for c in value.chars() {
        match c {
            '&' => out.extend_from_slice(b"&amp;"),
            '<' => out.extend_from_slice(b"&lt;"),
            '>' => out.extend_from_slice(b"&gt;"),
            // Excel stores a line feed inside a paragraph as CR LF (dc-piw0 Q8).
            '\n' => out.extend_from_slice(b"\r\n"),
            c => {
                let mut buffer = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
}

fn point(p: &DrawingPoint) -> String {
    format!(
        "<xdr:col>{}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>{}</xdr:rowOff>",
        p.column, p.column_offset, p.row, p.row_offset
    )
}

fn valid_point(p: &DrawingPoint) -> bool {
    p.column < 16384 && p.row < 1_048_576 && p.column_offset >= 0 && p.row_offset >= 0
}

fn colour(c: &ShapeColour, out: &mut Vec<u8>) -> Result<(), &'static str> {
    match c {
        ShapeColour::Rgb(v) if *v <= 0xff_ffff => {
            out.extend(format!("<a:srgbClr val=\"{v:06X}\"/>").as_bytes());
        }
        ShapeColour::Scheme(name)
            if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric()) =>
        {
            out.extend(format!("<a:schemeClr val=\"{name}\"/>").as_bytes());
        }
        _ => return Err("Use an RGB or theme colour"),
    }
    Ok(())
}

fn fill(f: &ShapeFill, out: &mut Vec<u8>) -> Result<(), &'static str> {
    match f {
        ShapeFill::Style => {}
        ShapeFill::None => out.extend(b"<a:noFill/>"),
        ShapeFill::Solid(c) => {
            out.extend(b"<a:solidFill>");
            colour(c, out)?;
            out.extend(b"</a:solidFill>");
        }
        ShapeFill::Other => return Err("Use no fill or a solid fill"),
    }
    Ok(())
}

fn line(l: &ShapeLine, out: &mut Vec<u8>) -> Result<(), &'static str> {
    if *l == ShapeLine::default() {
        return Ok(());
    }
    if l.weight.is_some_and(|w| !(0..=20_116_800).contains(&w)) {
        return Err("Use a line weight from 0 to 1584 points");
    }
    out.extend(b"<a:ln");
    if let Some(w) = l.weight {
        out.extend(format!(" w=\"{w}\"").as_bytes());
    }
    out.push(b'>');
    if l.hidden {
        out.extend(b"<a:noFill/>");
    } else if let Some(c) = &l.colour {
        out.extend(b"<a:solidFill>");
        colour(c, out)?;
        out.extend(b"</a:solidFill>");
    }
    if let Some(dash) = &l.dash {
        if dash.is_empty() || !dash.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err("Use a preset dash");
        }
        out.extend(format!("<a:prstDash val=\"{dash}\"/>").as_bytes());
    }
    out.extend(b"</a:ln>");
    Ok(())
}

/// One authored object's anchor, as a child of `xdr:wsDr`. `remap` gives the id each authored
/// shape is written with (the connectors' glued ids follow it).
pub(crate) fn frame(shape: &Shape, remap: &BTreeMap<u32, u32>) -> Result<Vec<u8>, SaveRefusal> {
    let fail = |detail: &str| refuse(shape, detail);
    if !matches!(
        shape.kind,
        ShapeKind::Shape | ShapeKind::TextBox | ShapeKind::Connector
    ) || !shape.children.is_empty()
        || shape.picture.is_some()
    {
        return Err(fail("New pictures and groups are not written yet"));
    }
    if shape.id == 0 || !valid_text(&shape.name) || shape.name.is_empty() {
        return Err(fail("Use a shape name and a positive identity"));
    }
    let id = remap.get(&shape.id).copied().unwrap_or(shape.id);
    let geometry = shape.geometry.as_deref().unwrap_or("rect");
    if geometry.is_empty() || !geometry.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(fail("Use a preset geometry name"));
    }
    if shape.extent.0 < 0 || shape.extent.1 < 0 || shape.offset.0 < 0 || shape.offset.1 < 0 {
        return Err(fail("Use a non-negative position and size"));
    }
    let (edit_as, from_to) = match shape.anchor {
        ChartAnchor::TwoCell { from, to } if valid_point(&from) && valid_point(&to) => (
            "twoCellAnchor",
            format!(
                "<xdr:from>{}</xdr:from><xdr:to>{}</xdr:to>",
                point(&from),
                point(&to)
            ),
        ),
        ChartAnchor::OneCell {
            from,
            width,
            height,
        } if valid_point(&from)
            && (0..=100_000_000).contains(&width)
            && (0..=100_000_000).contains(&height) =>
        {
            (
                "oneCellAnchor",
                format!(
                    "<xdr:from>{}</xdr:from><xdr:ext cx=\"{width}\" cy=\"{height}\"/>",
                    point(&from)
                ),
            )
        }
        ChartAnchor::Absolute {
            x,
            y,
            width,
            height,
        } if x >= 0 && y >= 0 && width >= 0 && height >= 0 => (
            "absoluteAnchor",
            format!("<xdr:pos x=\"{x}\" y=\"{y}\"/><xdr:ext cx=\"{width}\" cy=\"{height}\"/>"),
        ),
        _ => return Err(fail("The shape's anchor must fit the worksheet")),
    };
    let mut out = format!("<xdr:{edit_as} xmlns:xdr=\"{XDR}\" xmlns:a=\"{A}\"").into_bytes();
    match shape.edit_as.as_deref() {
        None => {}
        Some(v @ ("twoCell" | "oneCell" | "absolute")) if edit_as == "twoCellAnchor" => {
            out.extend(format!(" editAs=\"{v}\"").as_bytes());
        }
        Some(_) => {
            return Err(fail(
                "editAs belongs to a two-cell anchor: twoCell, oneCell or absolute",
            ));
        }
    }
    out.push(b'>');
    out.extend(from_to.as_bytes());
    let mut xfrm = String::from("<a:xfrm");
    if shape.rotation != 0 {
        xfrm.push_str(&format!(" rot=\"{}\"", shape.rotation));
    }
    if shape.flip_horizontal {
        xfrm.push_str(" flipH=\"1\"");
    }
    if shape.flip_vertical {
        xfrm.push_str(" flipV=\"1\"");
    }
    xfrm.push_str(&format!(
        "><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>",
        shape.offset.0, shape.offset.1, shape.extent.0, shape.extent.1
    ));
    let mut properties =
        format!("<xdr:spPr>{xfrm}<a:prstGeom prst=\"{geometry}\"><a:avLst/></a:prstGeom>")
            .into_bytes();
    let text_box = shape.kind == ShapeKind::TextBox;
    if text_box && shape.fill == ShapeFill::Style {
        // A text box Excel adds has a white fill and a thin line of the theme's own (Q0).
        properties.extend(b"<a:solidFill><a:schemeClr val=\"lt1\"/></a:solidFill>");
    } else {
        fill(&shape.fill, &mut properties).map_err(fail)?;
    }
    if text_box && shape.line == ShapeLine::default() {
        properties.extend(b"<a:ln w=\"9525\" cmpd=\"sng\"><a:solidFill><a:schemeClr val=\"lt1\"><a:shade val=\"50000\"/></a:schemeClr></a:solidFill></a:ln>");
    } else {
        line(&shape.line, &mut properties).map_err(fail)?;
    }
    properties.extend(b"</xdr:spPr>");
    let hidden = if shape.hidden { " hidden=\"1\"" } else { "" };
    let mut name = Vec::new();
    attribute_value(&shape.name, &mut name);
    let name = String::from_utf8(name).expect("names are UTF-8");
    if shape.kind == ShapeKind::Connector {
        out.extend(
            format!(
                "<xdr:cxnSp macro=\"\"><xdr:nvCxnSpPr><xdr:cNvPr id=\"{id}\" name={name}{hidden}/><xdr:cNvCxnSpPr>"
            )
            .as_bytes(),
        );
        for (tag, end) in [("stCxn", &shape.begin), ("endCxn", &shape.end)] {
            if let Some(end) = end {
                let glued = remap.get(&end.shape).copied().unwrap_or(end.shape);
                out.extend(format!("<a:{tag} id=\"{glued}\" idx=\"{}\"/>", end.site).as_bytes());
            }
        }
        out.extend(b"</xdr:cNvCxnSpPr></xdr:nvCxnSpPr>");
        out.extend(&properties);
        out.extend(b"<xdr:style><a:lnRef idx=\"2\"><a:schemeClr val=\"accent1\"/></a:lnRef><a:fillRef idx=\"0\"><a:schemeClr val=\"accent1\"/></a:fillRef><a:effectRef idx=\"1\"><a:schemeClr val=\"accent1\"/></a:effectRef><a:fontRef idx=\"minor\"><a:schemeClr val=\"tx1\"/></a:fontRef></xdr:style>");
        out.extend(b"</xdr:cxnSp>");
    } else {
        out.extend(
            format!(
                "<xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"{id}\" name={name}{hidden}/><xdr:cNvSpPr{}/></xdr:nvSpPr>",
                if text_box { " txBox=\"1\"" } else { "" }
            )
            .as_bytes(),
        );
        out.extend(&properties);
        if text_box {
            out.extend(b"<xdr:style><a:lnRef idx=\"0\"><a:scrgbClr r=\"0\" g=\"0\" b=\"0\"/></a:lnRef><a:fillRef idx=\"0\"><a:scrgbClr r=\"0\" g=\"0\" b=\"0\"/></a:fillRef><a:effectRef idx=\"0\"><a:scrgbClr r=\"0\" g=\"0\" b=\"0\"/></a:effectRef><a:fontRef idx=\"minor\"><a:schemeClr val=\"dk1\"/></a:fontRef></xdr:style>");
        } else {
            out.extend(b"<xdr:style><a:lnRef idx=\"2\"><a:schemeClr val=\"accent1\"><a:shade val=\"15000\"/></a:schemeClr></a:lnRef><a:fillRef idx=\"1\"><a:schemeClr val=\"accent1\"/></a:fillRef><a:effectRef idx=\"0\"><a:schemeClr val=\"accent1\"/></a:effectRef><a:fontRef idx=\"minor\"><a:schemeClr val=\"lt1\"/></a:fontRef></xdr:style>");
        }
        out.extend(
            format!(
                "<xdr:txBody><a:bodyPr vertOverflow=\"clip\" horzOverflow=\"clip\"{} rtlCol=\"0\" anchor=\"t\"/><a:lstStyle/>",
                if text_box { " vert=\"horz\"" } else { "" }
            )
            .as_bytes(),
        );
        let align = if text_box {
            ""
        } else {
            "<a:pPr algn=\"l\"/>"
        };
        let paragraphs = shape.text.as_deref().unwrap_or(&[]);
        if paragraphs.is_empty() {
            out.extend(
                format!("<a:p>{align}<a:endParaRPr lang=\"en-US\" sz=\"1100\"/></a:p>").as_bytes(),
            );
        }
        for paragraph in paragraphs {
            if !valid_text(paragraph) {
                return Err(fail("The text holds a character a drawing cannot store"));
            }
            out.extend(format!("<a:p>{align}").as_bytes());
            if paragraph.is_empty() {
                out.extend(b"<a:endParaRPr lang=\"en-US\" sz=\"1100\"/>");
            } else {
                out.extend(b"<a:r><a:rPr lang=\"en-US\" sz=\"1100\"/><a:t>");
                element_text(paragraph, &mut out);
                out.extend(b"</a:t></a:r>");
            }
            out.extend(b"</a:p>");
        }
        out.extend(b"</xdr:txBody></xdr:sp>");
    }
    out.extend(b"<xdr:clientData/>");
    out.extend(format!("</xdr:{edit_as}>").as_bytes());
    Ok(out)
}
