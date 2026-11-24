//! A worksheet's sparkline groups written after an edit (dc-zjsg.7): the `x14:sparklineGroups` extension of its
//! `extLst` rewritten from the model, in the form Excel writes (attributes and colours in the schema's order, the
//! data and cell references as `xm:f` and `xm:sqref`). The rest of the worksheet stays byte for byte; with no groups
//! left the extension is dropped (and an `extLst` it leaves empty with it).

use super::{SaveRefusal, reader::Reader, text::attribute_value};
use crate::xml::{XmlEvent, XmlPart};
use calc_doc_model::{SparklineGroup, StyleColor};

pub(crate) const SPARKLINE_EXT: &str = "{05C60535-1F16-4fd2-B633-F4F36F0B64E0}";
const X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const XM: &str = "http://schemas.microsoft.com/office/excel/2006/main";
const COLOURS: [&str; 8] = ["colorSeries", "colorNegative", "colorAxis", "colorMarkers", "colorFirst", "colorLast", "colorHigh", "colorLow"];

fn text(value: &str, out: &mut Vec<u8>) {
    for c in value.chars() {
        match c {
            '&' => out.extend_from_slice(b"&amp;"),
            '<' => out.extend_from_slice(b"&lt;"),
            '>' => out.extend_from_slice(b"&gt;"),
            c => {
                let mut buffer = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
}

fn colour(name: &str, c: &StyleColor, out: &mut Vec<u8>) {
    out.extend(format!("<x14:{name}").as_bytes());
    for (attribute, value) in [("rgb", &c.rgb), ("theme", &c.theme), ("tint", &c.tint), ("indexed", &c.indexed)] {
        if let Some(value) = value {
            out.extend(format!(" {attribute}=").as_bytes());
            attribute_value(value, out);
        }
    }
    if c.auto {
        out.extend(b" auto=\"1\"");
    }
    out.extend(b"/>");
}

/// The `x14:sparklineGroups` element of `groups`.
pub(crate) fn groups_xml(groups: &[SparklineGroup]) -> Vec<u8> {
    let mut out = format!("<x14:sparklineGroups xmlns:xm=\"{XM}\">").into_bytes();
    for g in groups {
        out.extend(b"<x14:sparklineGroup");
        let mut attribute = |name: &str, value: &Option<String>| {
            if let Some(value) = value {
                out.extend(format!(" {name}=").as_bytes());
                attribute_value(value, &mut out);
            }
        };
        attribute("manualMax", &g.manual_max);
        attribute("manualMin", &g.manual_min);
        attribute("lineWeight", &g.line_weight);
        attribute("type", &g.kind);
        let flag = |on: bool| on.then(|| "1".to_owned());
        attribute("dateAxis", &flag(g.date_axis));
        attribute("displayEmptyCellsAs", &g.empty_cells);
        attribute("markers", &flag(g.markers));
        attribute("high", &flag(g.high));
        attribute("low", &flag(g.low));
        attribute("first", &flag(g.first));
        attribute("last", &flag(g.last));
        attribute("negative", &flag(g.negative));
        attribute("displayXAxis", &flag(g.axis));
        attribute("displayHidden", &flag(g.hidden));
        attribute("minAxisType", &g.min_axis);
        attribute("maxAxisType", &g.max_axis);
        attribute("rightToLeft", &flag(g.right_to_left));
        out.push(b'>');
        for name in COLOURS {
            if let Some(c) = g.colour(name) {
                colour(name, c, &mut out);
            }
        }
        if let Some(dates) = g.dates.as_deref().filter(|_| g.date_axis) {
            out.extend(b"<xm:f>");
            text(dates, &mut out);
            out.extend(b"</xm:f>");
        }
        out.extend(b"<x14:sparklines>");
        for line in &g.sparklines {
            out.extend(b"<x14:sparkline><xm:f>");
            text(&line.data, &mut out);
            out.extend(b"</xm:f><xm:sqref>");
            text(&line.cell, &mut out);
            out.extend(b"</xm:sqref></x14:sparkline>");
        }
        out.extend(b"</x14:sparklines></x14:sparklineGroup>");
    }
    out.extend(b"</x14:sparklineGroups>");
    out
}

/// The worksheet `xml` with its sparkline extension holding `groups`.
pub(crate) fn rewrite(xml: &XmlPart, groups: &[SparklineGroup]) -> Result<Vec<u8>, SaveRefusal> {
    let source = xml.bytes();
    let mut reader = Reader::new(xml);
    let (root, root_span) = reader.root("worksheet")?;
    let prefix = {
        let tag = &source[root_span.0 + 1..];
        let end = tag.iter().position(|b| matches!(b, b' ' | b'>' | b'/' | b'\t' | b'\r' | b'\n')).unwrap_or(0);
        let name = String::from_utf8_lossy(&tag[..end]).into_owned();
        name.rsplit_once(':').map_or(String::new(), |(p, _)| format!("{p}:"))
    };
    let _ = root;
    // The extLst (its span and the end of its start tag) and the sparkline ext inside it.
    let (mut ext_list, mut ext, mut others) = (None, None, 0usize);
    let close = loop {
        match reader.expect()? {
            (XmlEvent::Start(e), span) => {
                if e.name.local == "extLst" {
                    let tag_end = span.1;
                    loop {
                        match reader.expect()? {
                            (XmlEvent::Start(child), child_span) => {
                                let end = reader.element_end()?;
                                if child.name.local == "ext" && child.attribute("uri") == Some(SPARKLINE_EXT) {
                                    ext = Some((child_span.0, end));
                                } else {
                                    others += 1;
                                }
                            }
                            (XmlEvent::End(_), end_span) => {
                                if source[..tag_end].ends_with(b"/>") {
                                    return Err(reader.invalid("An empty extLst is not rewritten (dc-zjsg.7)"));
                                }
                                ext_list = Some((span.0, tag_end, end_span.0, end_span.1));
                                break;
                            }
                            (XmlEvent::Text(_), _) => {}
                        }
                    }
                } else {
                    reader.element_end()?;
                }
            }
            (XmlEvent::End(_), span) => break span.0,
            (XmlEvent::Text(_), _) => {}
        }
    };
    let new_ext = (!groups.is_empty()).then(|| {
        let mut out = format!("<{prefix}ext uri=\"{SPARKLINE_EXT}\" xmlns:x14=\"{X14}\">").into_bytes();
        out.extend(groups_xml(groups));
        out.extend(format!("</{prefix}ext>").as_bytes());
        out
    });
    let mut out = Vec::with_capacity(source.len() + 512);
    match (ext_list, ext, new_ext) {
        // Replaced in place, or dropped (with its extLst when nothing else is in it).
        (Some((list_start, _, _, list_end)), Some(_), None) if others == 0 => {
            out.extend(&source[..list_start]);
            out.extend(&source[list_end..]);
        }
        (Some(_), Some((start, end)), new) => {
            out.extend(&source[..start]);
            out.extend(new.unwrap_or_default());
            out.extend(&source[end..]);
        }
        // Added at the end of the extLst, or in a new extLst as the worksheet's last child.
        (Some((_, _, list_close, _)), None, Some(new)) => {
            out.extend(&source[..list_close]);
            out.extend(new);
            out.extend(&source[list_close..]);
        }
        (None, None, Some(new)) => {
            out.extend(&source[..close]);
            out.extend(format!("<{prefix}extLst>").as_bytes());
            out.extend(new);
            out.extend(format!("</{prefix}extLst>").as_bytes());
            out.extend(&source[close..]);
        }
        (_, _, None) => out.extend(source),
        (None, Some(_), Some(_)) => unreachable!("an ext is only found inside an extLst"),
    }
    Ok(out)
}
