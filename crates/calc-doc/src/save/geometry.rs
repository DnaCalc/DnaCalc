//! Physical dimensions written through the existing worksheet XML reader and raw-tag writer.
use super::{SaveRefusal, reader::Reader, text::RawTag};
use crate::{
    load::MAIN_NAMESPACE,
    xml::{XmlEvent, XmlPart},
};
use calc_doc_model::value::{MAX_COLUMNS, MAX_ROWS, SheetId};
use calc_doc_model::{Axis, AxisFormat, AxisLayouts};
use std::collections::BTreeMap;

fn attributes(format: AxisFormat, row: bool) -> Vec<(&'static str, Option<String>)> {
    vec![
        (
            if row { "ht" } else { "width" },
            format.size().map(|n| n.to_string()),
        ),
        (
            if row { "customHeight" } else { "customWidth" },
            format.custom().then(|| "1".to_owned()),
        ),
        ("hidden", format.hidden().then(|| "1".to_owned())),
    ]
}
fn row_tag(raw: &[u8], format: AxisFormat, empty: bool, height_changed: bool) -> Vec<u8> {
    let mut attrs = attributes(format, true);
    if height_changed {
        attrs[1].1 = format.size().map(|_| "1".to_owned());
    } else {
        attrs.remove(1);
    }
    RawTag::parse(raw).rebuilt(
        &attrs
            .iter()
            .map(|(n, v)| (*n, v.as_deref()))
            .collect::<Vec<_>>(),
        empty,
    )
}
fn new_row(prefix: &str, index: u32, format: AxisFormat) -> Vec<u8> {
    let raw = format!("<{prefix}row r=\"{index}\"/>");
    row_tag(raw.as_bytes(), format, true, true)
}
fn data(
    reader: &mut Reader<'_>,
    xml: &XmlPart,
    span: (usize, usize),
    sheet: SheetId,
    layout: &AxisLayouts,
) -> Result<Vec<u8>, SaveRefusal> {
    let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
    let prefix = String::from_utf8_lossy(tag.prefix());
    let mut out = tag.rebuilt(&[], false);
    let rows: BTreeMap<_, _> = layout.bands(sheet, Axis::Rows).collect();
    let mut pending = rows.iter().peekable();
    let mut previous = 0;
    loop {
        match reader.expect()? {
            (XmlEvent::Start(child), start) if child.name.is(MAIN_NAMESPACE, "row") => {
                let index = child
                    .attribute("r")
                    .map_or(Some(previous + 1), |n| n.parse::<u32>().ok())
                    .filter(|n| *n > previous && *n <= MAX_ROWS)
                    .ok_or_else(|| {
                        reader.invalid("row indices are outside the ordered worksheet")
                    })?;
                previous = index;
                while pending.peek().is_some_and(|(r, _)| **r < index) {
                    let (r, f) = pending.next().unwrap();
                    out.extend(new_row(&prefix, *r, *f));
                }
                if pending.peek().is_some_and(|(r, _)| **r == index) {
                    pending.next();
                }
                let raw = &xml.bytes()[start.0..start.1];
                let empty = RawTag::parse(raw).is_empty();
                let end = reader.element_end()?;
                let format = layout.get(sheet, Axis::Rows, index);
                let old_height = child.attribute("ht").and_then(|s| s.parse::<f64>().ok());
                out.extend(row_tag(raw, format, empty, old_height != format.size()));
                out.extend_from_slice(&xml.bytes()[start.1..end]);
            }
            (XmlEvent::Start(_), start) => {
                let end = reader.element_end()?;
                out.extend_from_slice(&xml.bytes()[start.0..end]);
            }
            (XmlEvent::Text(_), start) => out.extend_from_slice(&xml.bytes()[start.0..start.1]),
            (XmlEvent::End(_), _) => break,
        }
    }
    for (r, f) in pending {
        out.extend(new_row(&prefix, *r, *f));
    }
    out.extend_from_slice(format!("</{prefix}sheetData>").as_bytes());
    Ok(out)
}
fn columns(
    prefix: &str,
    source: &[Option<(usize, usize)>],
    xml: &XmlPart,
    sheet: SheetId,
    layout: &AxisLayouts,
) -> Vec<u8> {
    let mut spans: Vec<(u32, u32, Option<(usize, usize)>, AxisFormat)> = Vec::new();
    for index in 1..=MAX_COLUMNS {
        let raw = source.get(index as usize).copied().flatten();
        let format = layout.get(sheet, Axis::Columns, index);
        if raw.is_none() && format == AxisFormat::default() {
            continue;
        }
        if let Some((_, last, prev, fmt)) = spans.last_mut()
            && *last + 1 == index
            && *prev == raw
            && *fmt == format
        {
            *last = index;
        } else {
            spans.push((index, index, raw, format));
        }
    }
    let mut out = Vec::new();
    for (first, last, raw, format) in spans {
        let new = format!("<{prefix}col/>");
        let tag = raw.map_or_else(
            || RawTag::parse(new.as_bytes()),
            |r| RawTag::parse(&xml.bytes()[r.0..r.1]),
        );
        let mut attrs = attributes(format, false);
        attrs.push(("min", Some(first.to_string())));
        attrs.push(("max", Some(last.to_string())));
        out.extend(
            tag.rebuilt(
                &attrs
                    .iter()
                    .map(|(n, v)| (*n, v.as_deref()))
                    .collect::<Vec<_>>(),
                true,
            ),
        );
    }
    out
}
fn cols(
    reader: &mut Reader<'_>,
    xml: &XmlPart,
    span: (usize, usize),
    sheet: SheetId,
    layout: &AxisLayouts,
) -> Result<Vec<u8>, SaveRefusal> {
    let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
    let prefix = String::from_utf8_lossy(tag.prefix());
    let mut source = vec![None; (MAX_COLUMNS + 1) as usize];
    let mut other = Vec::new();
    loop {
        match reader.expect()? {
            (XmlEvent::Start(child), start) => {
                let end = reader.element_end()?;
                let range = child
                    .attribute("min")
                    .and_then(|n| n.parse::<u32>().ok())
                    .zip(child.attribute("max").and_then(|n| n.parse::<u32>().ok()))
                    .filter(|(a, b)| *a >= 1 && *a <= *b && *b <= MAX_COLUMNS);
                if child.name.is(MAIN_NAMESPACE, "col")
                    && let Some((first, last)) = range
                {
                    for index in first..=last {
                        source[index as usize].get_or_insert(start);
                    }
                } else {
                    other.extend_from_slice(&xml.bytes()[start.0..end]);
                }
            }
            (XmlEvent::Text(_), start) => other.extend_from_slice(&xml.bytes()[start.0..start.1]),
            (XmlEvent::End(_), _) => break,
        }
    }
    let mut out = tag.rebuilt(&[], false);
    out.extend(columns(&prefix, &source, xml, sheet, layout));
    out.extend(other);
    out.extend_from_slice(format!("</{prefix}cols>").as_bytes());
    Ok(out)
}
pub(super) fn rewrite(
    xml: &XmlPart,
    sheet: SheetId,
    layout: &AxisLayouts,
) -> Result<Vec<u8>, SaveRefusal> {
    let mut reader = Reader::new(xml);
    let (_, span) = reader.root("worksheet")?;
    let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
    let prefix = String::from_utf8_lossy(tag.prefix());
    let mut out = xml.bytes()[..span.1].to_vec();
    let mut have_cols = false;
    loop {
        match reader.expect()? {
            (XmlEvent::Start(child), start) if child.name.is(MAIN_NAMESPACE, "cols") => {
                out.extend(cols(&mut reader, xml, start, sheet, layout)?);
                have_cols = true;
            }
            (XmlEvent::Start(child), start) if child.name.is(MAIN_NAMESPACE, "sheetData") => {
                if !have_cols && layout.bands(sheet, Axis::Columns).next().is_some() {
                    out.extend_from_slice(format!("<{prefix}cols>").as_bytes());
                    out.extend(columns(&prefix, &[], xml, sheet, layout));
                    out.extend_from_slice(format!("</{prefix}cols>").as_bytes());
                }
                out.extend(data(&mut reader, xml, start, sheet, layout)?);
            }
            (XmlEvent::Start(_), start) => {
                let end = reader.element_end()?;
                out.extend_from_slice(&xml.bytes()[start.0..end]);
            }
            (XmlEvent::Text(_), start) => out.extend_from_slice(&xml.bytes()[start.0..start.1]),
            (XmlEvent::End(_), start) => {
                out.extend_from_slice(&xml.bytes()[start.0..]);
                break;
            }
        }
    }
    Ok(out)
}
