//! New drawing objects as anchors of a spreadsheet drawing (dc-zjsg.1). The anchor of an authored
//! shape, text box or connector is written in the form Excel's own files use (xdr:sp with its
//! `xdr:style` for an auto shape, `txBox="1"` for a text box, xdr:cxnSp for a connector); which
//! defaults Excel writes (names, ids, no `editAs`, the text box's fill and line, text body
//! properties) are those of the XML Excel saved for the report macro (excel-answers.md Q0). Objects
//! read from a file are never written by this module: their XML stays as stored.

use super::{SaveRefusal, text::attribute_value};
use calc_doc_model::{
    ChartAnchor, DrawingPoint, LineEnd, Shape, ShapeColour, ShapeFill, ShapeGradient, ShapeKind, ShapeShadow,
    ShapeLine,
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

pub(super) fn valid_text(s: &str) -> bool {
    s.encode_utf16().count() <= 32767
        && !s.chars().any(|c| {
            (c < ' ' && !matches!(c, '\t' | '\n' | '\r' | '\u{b}'))
                || matches!(c, '\u{fffe}' | '\u{ffff}')
        })
}

/// An enumeration attribute's value (`ctr`, `square`): letters and digits only.
fn attribute_text(value: &str) -> String {
    value.chars().filter(char::is_ascii_alphanumeric).collect()
}

pub(super) fn element_text(value: &str, out: &mut Vec<u8>) {
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

/// The colour element: `a:srgbClr` or `a:schemeClr`, with the modifiers a modified colour carries as its children.
fn colour(c: &ShapeColour, out: &mut Vec<u8>) -> Result<(), &'static str> {
    let (base, modifiers): (&ShapeColour, &[(String, i64)]) = match c {
        ShapeColour::Modified(base, modifiers) => (base, modifiers),
        other => (other, &[]),
    };
    let tag = match base {
        ShapeColour::Rgb(v) if *v <= 0xff_ffff => {
            out.extend(format!("<a:srgbClr val=\"{v:06X}\"").as_bytes());
            "a:srgbClr"
        }
        ShapeColour::Scheme(name)
            if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric()) =>
        {
            out.extend(format!("<a:schemeClr val=\"{name}\"").as_bytes());
            "a:schemeClr"
        }
        _ => return Err("Use an RGB or theme colour"),
    };
    if modifiers.is_empty() {
        out.extend(b"/>");
        return Ok(());
    }
    out.push(b'>');
    for (name, value) in modifiers {
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err("Use a colour modifier of letters and digits");
        }
        out.extend(format!("<a:{name} val=\"{value}\"/>").as_bytes());
    }
    out.extend(format!("</{tag}>").as_bytes());
    Ok(())
}

/// `a:effectLst` with the shape's outer shadow.
fn effects(shadow: &ShapeShadow, out: &mut Vec<u8>) -> Result<(), &'static str> {
    if shadow.blur < 0 || shadow.distance < 0 {
        return Err("Use a non-negative shadow blur and distance");
    }
    out.extend(
        format!(
            "<a:effectLst><a:outerShdw blurRad=\"{}\" dist=\"{}\" dir=\"{}\"",
            shadow.blur, shadow.distance, shadow.direction
        )
        .as_bytes(),
    );
    if shadow.scale.0 != 100_000 {
        out.extend(format!(" sx=\"{}\"", shadow.scale.0).as_bytes());
    }
    if shadow.scale.1 != 100_000 {
        out.extend(format!(" sy=\"{}\"", shadow.scale.1).as_bytes());
    }
    if let Some(align) = &shadow.align {
        if align.is_empty() || !align.bytes().all(|b| b.is_ascii_alphabetic()) {
            return Err("Use a shadow alignment such as b or tl");
        }
        out.extend(format!(" algn=\"{align}\"").as_bytes());
    }
    out.extend(format!(" rotWithShape=\"{}\">", u8::from(shadow.rotate_with_shape)).as_bytes());
    colour(&shadow.colour, out)?;
    out.extend(b"</a:outerShdw></a:effectLst>");
    Ok(())
}

pub(super) fn fill(f: &ShapeFill, out: &mut Vec<u8>) -> Result<(), &'static str> {
    match f {
        ShapeFill::Style => {}
        ShapeFill::None => out.extend(b"<a:noFill/>"),
        ShapeFill::Solid(c) => {
            out.extend(b"<a:solidFill>");
            colour(c, out)?;
            out.extend(b"</a:solidFill>");
        }
        ShapeFill::Gradient(g) => gradient(g, out)?,
        ShapeFill::Pattern(p) => {
            if p.preset.is_empty() || !p.preset.chars().all(|c| c.is_ascii_alphanumeric()) {
                return Err("Use a pattern preset such as pct50 or smCheck");
            }
            out.extend(format!("<a:pattFill prst=\"{}\">", p.preset).as_bytes());
            for (tag, c) in [("fgClr", &p.foreground), ("bgClr", &p.background)] {
                if let Some(c) = c {
                    out.extend(format!("<a:{tag}>").as_bytes());
                    colour(c, out)?;
                    out.extend(format!("</a:{tag}>").as_bytes());
                }
            }
            out.extend(b"</a:pattFill>");
        }
        ShapeFill::Picture(_) | ShapeFill::Other => return Err("Use no fill, a solid fill, a gradient or a pattern"),
    }
    Ok(())
}

fn gradient(g: &ShapeGradient, out: &mut Vec<u8>) -> Result<(), &'static str> {
    if g.stops.len() < 2 || g.stops.iter().any(|(pos, _)| !(0..=100_000).contains(pos)) {
        return Err("Use two or more gradient stops at 0 to 100 percent");
    }
    out.extend(b"<a:gradFill rotWithShape=\"1\"><a:gsLst>");
    for (pos, c) in &g.stops {
        out.extend(format!("<a:gs pos=\"{pos}\">").as_bytes());
        colour(c, out)?;
        out.extend(b"</a:gs>");
    }
    out.extend(b"</a:gsLst>");
    if let Some(path) = &g.path {
        if !matches!(path.as_str(), "circle" | "rect" | "shape") {
            return Err("Use a circle, rect or shape gradient path");
        }
        out.extend(format!("<a:path path=\"{path}\">").as_bytes());
        if let Some([l, t, r, b]) = g.focus {
            out.extend(
                format!("<a:fillToRect l=\"{l}\" t=\"{t}\" r=\"{r}\" b=\"{b}\"/>").as_bytes(),
            );
        }
        out.extend(b"</a:path>");
    } else {
        out.extend(
            format!(
                "<a:lin ang=\"{}\" scaled=\"{}\"/>",
                g.angle.unwrap_or(0),
                u8::from(g.scaled)
            )
            .as_bytes(),
        );
    }
    out.extend(b"</a:gradFill>");
    Ok(())
}

fn line_end(tag: &str, end: &LineEnd, out: &mut Vec<u8>) -> Result<(), &'static str> {
    let kind = matches!(
        end.kind.as_str(),
        "none" | "triangle" | "stealth" | "diamond" | "oval" | "arrow"
    );
    let size = |v: &Option<String>| v.as_deref().is_none_or(|v| matches!(v, "sm" | "med" | "lg"));
    if !kind || !size(&end.width) || !size(&end.length) {
        return Err("Use an arrowhead type of none, triangle, stealth, diamond, oval or arrow, sized sm, med or lg");
    }
    out.extend(format!("<a:{tag} type=\"{}\"", end.kind).as_bytes());
    if let Some(w) = &end.width {
        out.extend(format!(" w=\"{w}\"").as_bytes());
    }
    if let Some(l) = &end.length {
        out.extend(format!(" len=\"{l}\"").as_bytes());
    }
    out.extend(b"/>");
    Ok(())
}

pub(super) fn line(l: &ShapeLine, out: &mut Vec<u8>) -> Result<(), &'static str> {
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
    if let Some(end) = &l.head {
        line_end("headEnd", end, out)?;
    }
    if let Some(end) = &l.tail {
        line_end("tailEnd", end, out)?;
    }
    out.extend(b"</a:ln>");
    Ok(())
}

/// The file extension of a picture's media content type, `None` for one a picture may not have.
pub(crate) fn media_extension(content_type: &str) -> Option<&'static str> {
    Some(match content_type {
        "image/png" => "png",
        "image/jpeg" => "jpeg",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        "image/x-emf" => "emf",
        "image/x-wmf" => "wmf",
        "image/svg+xml" => "svg",
        _ => return None,
    })
}

/// The relationship ids of an authored picture's image and SVG parts, by the shape's written id.
pub(crate) type MediaRels = BTreeMap<u32, (String, Option<String>)>;

/// What every object, a group's children too, must state to be written.
fn check(shape: &Shape) -> Result<(), SaveRefusal> {
    let fail = |detail: &str| refuse(shape, detail);
    if !matches!(
        shape.kind,
        ShapeKind::Shape | ShapeKind::TextBox | ShapeKind::Connector | ShapeKind::Group | ShapeKind::Picture
    ) || (shape.kind != ShapeKind::Group && !shape.children.is_empty())
    {
        return Err(fail("This kind of object is not written"));
    }
    if shape.kind == ShapeKind::Picture {
        let image = shape.media.as_ref().and_then(|m| m.image.as_ref());
        if image.is_none_or(|i| i.bytes.is_empty() || !media_extension(&i.content_type).is_some()) {
            return Err(fail("A new picture needs its image bytes, as PNG, JPEG, GIF, BMP, TIFF, EMF or WMF"));
        }
        if shape.media.as_ref().and_then(|m| m.svg.as_ref()).is_some_and(|s| s.bytes.is_empty()) {
            return Err(fail("A new picture's SVG needs its bytes"));
        }
    }
    if shape.id == 0 || !valid_text(&shape.name) || shape.name.is_empty() {
        return Err(fail("Use a shape name and a positive identity"));
    }
    let geometry = shape.geometry.as_deref().unwrap_or("rect");
    if geometry.is_empty() || !geometry.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(fail("Use a preset geometry name"));
    }
    if shape.adjust.iter().any(|(name, _)| name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric())) {
        return Err(fail("Use adjust names of letters and digits"));
    }
    if shape.extent.0 < 0 || shape.extent.1 < 0 || shape.child_extent.0 < 0 || shape.child_extent.1 < 0 {
        return Err(fail("Use a non-negative size"));
    }
    // A top-level object sits at a non-negative offset; a group's children are given in the
    // group's child space, where Excel writes any value.
    Ok(())
}

fn check_tree(shape: &Shape) -> Result<(), SaveRefusal> {
    check(shape)?;
    shape.children.iter().try_for_each(check_tree)
}

/// One authored object's anchor, as a child of `xdr:wsDr`. `remap` gives the id each authored
/// shape is written with (the connectors' glued ids follow it).
/// What a new object that is a stored one writes for its element (`None`: written from the model).
pub(crate) type StoredElement<'a> = dyn Fn(&Shape) -> Option<Result<Vec<u8>, SaveRefusal>> + 'a;

pub(crate) fn frame(shape: &Shape, remap: &BTreeMap<u32, u32>, media: &MediaRels, stored: &StoredElement<'_>) -> Result<Vec<u8>, SaveRefusal> {
    let fail = |detail: &str| refuse(shape, detail);
    let element = stored(shape).transpose()?;
    if element.is_none() {
        check(shape)?;
    }
    // A turned object's unturned box may reach past the sheet's edge while its anchor stays on it (a:off is signed).
    if shape.rotation == 0 && (shape.offset.0 < 0 || shape.offset.1 < 0) {
        return Err(fail("Use a non-negative position"));
    }
    if element.is_none() {
        for child in &shape.children {
            if stored(child).is_none() {
                check_tree(child)?;
            }
        }
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
    match element {
        Some(element) => out.extend(element),
        None => object(shape, remap, media, stored, &mut out)?,
    }
    out.extend(b"<xdr:clientData/>");
    out.extend(format!("</xdr:{edit_as}>").as_bytes());
    Ok(out)
}

/// One object's element (`xdr:sp`, `xdr:cxnSp` or `xdr:grpSp`), a group's children inside it.
fn object(shape: &Shape, remap: &BTreeMap<u32, u32>, media: &MediaRels, stored: &StoredElement<'_>, out: &mut Vec<u8>) -> Result<(), SaveRefusal> {
    if let Some(element) = stored(shape) {
        out.extend(element?);
        return Ok(());
    }
    let fail = |detail: &str| refuse(shape, detail);
    let id = remap.get(&shape.id).copied().unwrap_or(shape.id);
    let geometry = shape.geometry.as_deref().unwrap_or("rect");
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
        "><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/>",
        shape.offset.0, shape.offset.1, shape.extent.0, shape.extent.1
    ));
    if shape.kind == ShapeKind::Group {
        xfrm.push_str(&format!(
            "<a:chOff x=\"{}\" y=\"{}\"/><a:chExt cx=\"{}\" cy=\"{}\"/>",
            shape.child_offset.0, shape.child_offset.1, shape.child_extent.0, shape.child_extent.1
        ));
    }
    xfrm.push_str("</a:xfrm>");
    if shape.kind == ShapeKind::Group {
        let mut name = Vec::new();
        attribute_value(&shape.name, &mut name);
        let name = String::from_utf8(name).expect("names are UTF-8");
        let hidden = if shape.hidden { " hidden=\"1\"" } else { "" };
        out.extend(
            format!(
                "<xdr:grpSp><xdr:nvGrpSpPr><xdr:cNvPr id=\"{id}\" name={name}{hidden}/><xdr:cNvGrpSpPr/></xdr:nvGrpSpPr><xdr:grpSpPr>{xfrm}</xdr:grpSpPr>"
            )
            .as_bytes(),
        );
        for child in &shape.children {
            object(child, remap, media, stored, out)?;
        }
        out.extend(b"</xdr:grpSp>");
        return Ok(());
    }
    // Excel (20527) repairs an avLst that names some of a multi-guide preset's guides but not all: the dc-8ari6 bisect
    // of shapes-paint2/3 failed on one hexagon with `adj` alone and one wedgeRoundRectCallout with `adj3` alone, and
    // opened every other anchor. Excel itself lists every guide, so a partial list is completed in the preset's order
    // with the preset's own defaults.
    let guides = complete_guides(geometry, &shape.adjust);
    let mut adjust = String::new();
    for (name, value) in &guides {
        adjust.push_str(&format!("<a:gd name=\"{name}\" fmla=\"val {value}\"/>"));
    }
    let avlst = if adjust.is_empty() { "<a:avLst/>".to_owned() } else { format!("<a:avLst>{adjust}</a:avLst>") };
    let mut properties =
        format!("<xdr:spPr>{xfrm}<a:prstGeom prst=\"{geometry}\">{avlst}</a:prstGeom>").into_bytes();
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
    if let Some(shadow) = &shape.shadow {
        effects(shadow, &mut properties).map_err(fail)?;
    }
    properties.extend(b"</xdr:spPr>");
    let hidden = if shape.hidden { " hidden=\"1\"" } else { "" };
    let mut name = Vec::new();
    attribute_value(&shape.name, &mut name);
    let name = String::from_utf8(name).expect("names are UTF-8");
    if shape.kind == ShapeKind::Picture {
        let (image, svg) = media.get(&id).ok_or_else(|| fail("The picture's image part was not written"))?;
        let r = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
        let mut blip = format!("<a:blip xmlns:r=\"{r}\" r:embed=\"{image}\"");
        match svg {
            Some(svg) => blip.push_str(&format!("><a:extLst><a:ext uri=\"{{96DAC541-7B7A-43D3-8B79-37D633B846F1}}\"><asvg:svgBlip xmlns:asvg=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\" r:embed=\"{svg}\"/></a:ext></a:extLst></a:blip>")),
            None => blip.push_str("/>"),
        }
        let picture = shape.media.as_ref();
        let crop = picture.map_or([0; 4], |m| m.crop);
        let crop = if crop == [0; 4] {
            String::new()
        } else {
            format!("<a:srcRect l=\"{}\" t=\"{}\" r=\"{}\" b=\"{}\"/>", crop[0], crop[1], crop[2], crop[3])
        };
        let mut descr = Vec::new();
        if let Some(text) = picture.and_then(|m| m.description.as_deref()) {
            descr.extend(b" descr=");
            attribute_value(text, &mut descr);
        }
        let descr = String::from_utf8(descr).expect("descriptions are UTF-8");
        out.extend(
            format!(
                "<xdr:pic macro=\"\"><xdr:nvPicPr><xdr:cNvPr id=\"{id}\" name={name}{descr}{hidden}/><xdr:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></xdr:cNvPicPr></xdr:nvPicPr><xdr:blipFill>{blip}{crop}<a:stretch><a:fillRect/></a:stretch></xdr:blipFill>"
            )
            .as_bytes(),
        );
        out.extend(&properties);
        out.extend(b"</xdr:pic>");
    } else if shape.kind == ShapeKind::Connector {
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
        // What the object states of its body (insets, wrap, anchor), after the attributes Excel writes
        // for every body; an object that states none writes what it always did.
        let body = &shape.body;
        let mut body_attributes = String::new();
        if let Some(wrap) = body.wrap.as_deref() {
            body_attributes.push_str(&format!(" wrap=\"{}\"", attribute_text(wrap)));
        }
        for (name, value) in [
            ("lIns", body.left_inset),
            ("tIns", body.top_inset),
            ("rIns", body.right_inset),
            ("bIns", body.bottom_inset),
        ] {
            if let Some(value) = value {
                body_attributes.push_str(&format!(" {name}=\"{value}\""));
            }
        }
        out.extend(
            format!(
                "<xdr:txBody><a:bodyPr vertOverflow=\"clip\" horzOverflow=\"clip\"{}{body_attributes} rtlCol=\"0\" anchor=\"{}\"/><a:lstStyle/>",
                if text_box { " vert=\"horz\"" } else { "" },
                attribute_text(body.anchor.as_deref().unwrap_or("t")),
            )
            .as_bytes(),
        );
        let default_align = if text_box { None } else { Some("l") };
        let align_of = |index: usize| -> String {
            match body
                .alignment
                .get(index)
                .and_then(|a| a.as_deref())
                .or(default_align)
            {
                Some(align) => format!("<a:pPr algn=\"{}\"/>", attribute_text(align)),
                None => String::new(),
            }
        };
        let size = body.size.unwrap_or(1100);
        let run_font = {
            let mut attributes = String::new();
            if body.bold {
                attributes.push_str(" b=\"1\"");
            }
            if body.italic {
                attributes.push_str(" i=\"1\"");
            }
            if body.underline {
                attributes.push_str(" u=\"sng\"");
            }
            let colour = match &body.colour {
                Some(c) => {
                    let mut element = Vec::new();
                    colour(c, &mut element).map_err(fail)?;
                    format!("<a:solidFill>{}</a:solidFill>", String::from_utf8(element).expect("colour elements are ASCII"))
                }
                None => String::new(),
            };
            let face = match &body.face {
                Some(face) if !face.is_empty() && !face.starts_with('+')
                    && !face.chars().any(|c| c.is_control() || matches!(c, '\u{fffe}' | '\u{ffff}')) => {
                    let mut element = b"<a:latin typeface=".to_vec();
                    attribute_value(face, &mut element);
                    element.extend(b"/>");
                    String::from_utf8(element).expect("typeface is UTF-8")
                }
                Some(_) => return Err(fail("The typeface cannot be represented")),
                None => String::new(),
            };
            (attributes, colour, face)
        };
        let lang = shape.lang.as_deref().unwrap_or("en-US");
        let paragraphs = shape.text.as_deref().unwrap_or(&[]);
        let rich_runs = body.rich_runs_for(paragraphs);
        if paragraphs.is_empty() {
            out.extend(
                format!(
                    "<a:p>{}<a:endParaRPr lang=\"{lang}\" sz=\"{size}\"/></a:p>",
                    align_of(0)
                )
                .as_bytes(),
            );
        }
        for (index, paragraph) in paragraphs.iter().enumerate() {
            let align = align_of(index);
            if !valid_text(paragraph) {
                return Err(fail("The text holds a character a drawing cannot store"));
            }
            out.extend(format!("<a:p>{align}").as_bytes());
            if paragraph.is_empty() {
                out.extend(format!("<a:endParaRPr lang=\"{lang}\" sz=\"{size}\"/>").as_bytes());
            } else if let Some(runs) = rich_runs.and_then(|runs| runs.get(index)) {
                for run in runs {
                    out.extend(format!("<a:r><a:rPr lang=\"{lang}\" sz=\"{}\" b=\"{}\" i=\"{}\" u=\"{}\">",
                        run.size, u8::from(run.bold), u8::from(run.italic), if run.underline { "sng" } else { "none" }).as_bytes());
                    if let Some(c) = &run.colour {
                        out.extend(b"<a:solidFill>");
                        colour(c, out).map_err(fail)?;
                        out.extend(b"</a:solidFill>");
                    }
                    if let Some(face) = &run.face {
                        out.extend(b"<a:latin typeface=");
                        attribute_value(face, out);
                        out.extend(b"/>");
                    }
                    out.extend(b"</a:rPr><a:t xml:space=\"preserve\">");
                    element_text(&run.text, out);
                    out.extend(b"</a:t></a:r>");
                }
            } else {
                let (attributes, colour, face) = &run_font;
                if colour.is_empty() && face.is_empty() {
                    out.extend(format!("<a:r><a:rPr lang=\"{lang}\" sz=\"{size}\"{attributes}/><a:t>").as_bytes());
                } else {
                    out.extend(format!("<a:r><a:rPr lang=\"{lang}\" sz=\"{size}\"{attributes}>{colour}{face}</a:rPr><a:t>").as_bytes());
                }
                element_text(paragraph, out);
                out.extend(b"</a:t></a:r>");
            }
            out.extend(b"</a:p>");
        }
        out.extend(b"</xdr:txBody></xdr:sp>");
    }
    Ok(())
}

/// The guides of the presets that have more than one, in the preset's order, with the default of each.
const MULTI_GUIDE_PRESETS: &[(&str, &[(&str, i64)])] = &[
    ("hexagon", &[("adj", 25_000), ("vf", 115_470)]),
    ("rightArrow", &[("adj1", 50_000), ("adj2", 50_000)]),
    ("leftArrow", &[("adj1", 50_000), ("adj2", 50_000)]),
    ("upArrow", &[("adj1", 50_000), ("adj2", 50_000)]),
    ("downArrow", &[("adj1", 50_000), ("adj2", 50_000)]),
    ("wedgeRectCallout", &[("adj1", -20_833), ("adj2", 62_500)]),
    ("wedgeRoundRectCallout", &[("adj1", -20_833), ("adj2", 62_500), ("adj3", 16_667)]),
];

/// The guides to write: the shape's own, and for a multi-guide preset with any guide set, all of its guides.
fn complete_guides(geometry: &str, own: &[(String, i64)]) -> Vec<(String, i64)> {
    let Some((_, all)) = MULTI_GUIDE_PRESETS.iter().find(|(name, _)| *name == geometry) else {
        return own.to_vec();
    };
    if own.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<(String, i64)> = all
        .iter()
        .map(|(name, default)| ((*name).to_owned(), own.iter().find(|(n, _)| n == name).map_or(*default, |(_, v)| *v)))
        .collect();
    out.extend(own.iter().filter(|(n, _)| !all.iter().any(|(name, _)| name == n)).cloned());
    out
}
