//! Outline settings and independent hiding supplement the existing geometry writer.
use super::{SaveRefusal, reader::Reader, text::RawTag};
use crate::{
    load::{MAIN_NAMESPACE, outline::NAMESPACE},
    xml::{XmlEvent, XmlPart},
};
use calc_doc_model::value::SheetId;
use calc_doc_model::{Axis, AxisLayouts};
fn outline_tag(raw: &[u8], empty: bool, sheet: SheetId, layout: &AxisLayouts) -> Vec<u8> {
    RawTag::parse(raw).rebuilt(
        &[
            (
                "summaryBelow",
                Some(if layout.summary_after(sheet, Axis::Rows) {
                    "1"
                } else {
                    "0"
                }),
            ),
            (
                "summaryRight",
                Some(if layout.summary_after(sheet, Axis::Columns) {
                    "1"
                } else {
                    "0"
                }),
            ),
        ],
        empty,
    )
}
fn settings(
    reader: &mut Reader<'_>,
    xml: &XmlPart,
    span: (usize, usize),
    sheet: SheetId,
    layout: &AxisLayouts,
) -> Result<Vec<u8>, SaveRefusal> {
    let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
    let prefix = String::from_utf8_lossy(tag.prefix());
    let mut out = tag.rebuilt(&[], false);
    let mut written = false;
    loop {
        match reader.expect()? {
            (XmlEvent::Start(e), start) if e.name.is(MAIN_NAMESPACE, "outlinePr") => {
                let end = reader.element_end()?;
                let raw = &xml.bytes()[start.0..start.1];
                out.extend(outline_tag(
                    raw,
                    RawTag::parse(raw).is_empty(),
                    sheet,
                    layout,
                ));
                out.extend_from_slice(&xml.bytes()[start.1..end]);
                written = true;
            }
            (XmlEvent::Start(e), start) => {
                if !written && !e.name.is(MAIN_NAMESPACE, "tabColor") {
                    out.extend(outline_tag(
                        format!("<{prefix}outlinePr/>").as_bytes(),
                        true,
                        sheet,
                        layout,
                    ));
                    written = true;
                }
                let end = reader.element_end()?;
                out.extend_from_slice(&xml.bytes()[start.0..end]);
            }
            (XmlEvent::Text(_), start) => out.extend_from_slice(&xml.bytes()[start.0..start.1]),
            (XmlEvent::End(_), _) => break,
        }
    }
    if !written {
        out.extend(outline_tag(
            format!("<{prefix}outlinePr/>").as_bytes(),
            true,
            sheet,
            layout,
        ));
    }
    out.extend(format!("</{prefix}sheetPr>").bytes());
    Ok(out)
}
fn state(prefix: &str, source: u32, sheet: SheetId, layout: &AxisLayouts) -> Vec<u8> {
    let list = |axis: Axis, manual: bool| {
        layout
            .bands(sheet, axis)
            .filter_map(|(i, f)| {
                if manual {
                    f.hidden().then(|| i.to_string())
                } else {
                    (f.collapsed() != 0).then(|| format!("{i}:{}", f.collapsed()))
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let (rows, columns, manual_rows, manual_columns) = (
        list(Axis::Rows, false),
        list(Axis::Columns, false),
        list(Axis::Rows, true),
        list(Axis::Columns, true),
    );
    let integrity = crate::load::outline::state_checksum(
        source,
        &rows,
        &columns,
        &manual_rows,
        &manual_columns,
    );
    format!("<{prefix}ext uri=\"{NAMESPACE}\"><dc:state xmlns:dc=\"{NAMESPACE}\" source=\"{source}\" integrity=\"{integrity}\" rows=\"{rows}\" columns=\"{columns}\" manualRows=\"{manual_rows}\" manualColumns=\"{manual_columns}\"/></{prefix}ext>").into_bytes()
}
fn extensions(
    reader: &mut Reader<'_>,
    xml: &XmlPart,
    span: (usize, usize),
    addition: &[u8],
) -> Result<Vec<u8>, SaveRefusal> {
    let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
    let prefix = String::from_utf8_lossy(tag.prefix());
    let mut out = tag.rebuilt(&[], false);
    loop {
        match reader.expect()? {
            (XmlEvent::Start(e), start) => {
                let end = reader.element_end()?;
                if !(e.name.is(MAIN_NAMESPACE, "ext") && e.attribute("uri") == Some(NAMESPACE)) {
                    out.extend_from_slice(&xml.bytes()[start.0..end]);
                }
            }
            (XmlEvent::Text(_), start) => out.extend_from_slice(&xml.bytes()[start.0..start.1]),
            (XmlEvent::End(_), _) => break,
        }
    }
    out.extend_from_slice(addition);
    out.extend(format!("</{prefix}extLst>").bytes());
    Ok(out)
}
pub(super) fn rewrite(
    xml: &XmlPart,
    sheet: SheetId,
    layout: &AxisLayouts,
) -> Result<Vec<u8>, SaveRefusal> {
    let active = [Axis::Rows, Axis::Columns].into_iter().any(|axis| {
        !layout.summary_after(sheet, axis)
            || layout
                .bands(sheet, axis)
                .any(|(_, f)| f.outline_level() > 0 || f.collapsed() != 0)
    });
    let mut reader = Reader::new(xml);
    let (_, root) = reader.root("worksheet")?;
    let tag = RawTag::parse(&xml.bytes()[root.0..root.1]);
    let prefix = String::from_utf8_lossy(tag.prefix());
    let mut out = xml.bytes()[..root.1].to_vec();
    let mut have_pr = false;
    let mut have_format = false;
    let row_level = layout
        .bands(sheet, Axis::Rows)
        .map(|(_, f)| f.outline_level())
        .max()
        .unwrap_or(0)
        .to_string();
    let col_level = layout
        .bands(sheet, Axis::Columns)
        .map(|(_, f)| f.outline_level())
        .max()
        .unwrap_or(0)
        .to_string();
    loop {
        match reader.expect()? {
            (XmlEvent::Start(e), start) => {
                if active && !have_pr && !e.name.is(MAIN_NAMESPACE, "sheetPr") {
                    out.extend(format!("<{prefix}sheetPr>").bytes());
                    out.extend(outline_tag(
                        format!("<{prefix}outlinePr/>").as_bytes(),
                        true,
                        sheet,
                        layout,
                    ));
                    out.extend(format!("</{prefix}sheetPr>").bytes());
                    have_pr = true;
                }
                if !have_format
                    && (e.name.is(MAIN_NAMESPACE, "cols") || e.name.is(MAIN_NAMESPACE, "sheetData"))
                {
                    if active {
                        out.extend(format!("<{prefix}sheetFormatPr defaultRowHeight=\"15\" outlineLevelRow=\"{row_level}\" outlineLevelCol=\"{col_level}\"/>").bytes());
                    }
                    have_format = true;
                }
                if e.name.is(MAIN_NAMESPACE, "sheetPr") {
                    have_pr = true;
                    out.extend(settings(&mut reader, xml, start, sheet, layout)?);
                } else if e.name.is(MAIN_NAMESPACE, "sheetFormatPr") {
                    have_format = true;
                    let end = reader.element_end()?;
                    let raw = &xml.bytes()[start.0..start.1];
                    out.extend(RawTag::parse(raw).rebuilt(
                        &[
                            (
                                "outlineLevelRow",
                                (row_level != "0").then_some(row_level.as_str()),
                            ),
                            (
                                "outlineLevelCol",
                                (col_level != "0").then_some(col_level.as_str()),
                            ),
                        ],
                        RawTag::parse(raw).is_empty(),
                    ));
                    out.extend_from_slice(&xml.bytes()[start.1..end]);
                } else {
                    let end = reader.element_end()?;
                    out.extend_from_slice(&xml.bytes()[start.0..end]);
                }
            }
            (XmlEvent::Text(_), start) => out.extend_from_slice(&xml.bytes()[start.0..start.1]),
            (XmlEvent::End(_), start) => {
                out.extend_from_slice(&xml.bytes()[start.0..]);
                break;
            }
        }
    }
    let interim = xml.with_bytes(out);
    let physical =
        crate::load::outline::physical(&interim).map_err(|_| SaveRefusal::InvalidPart {
            part: xml.name().to_owned(),
            detail: "outline geometry could not be read".into(),
        })?;
    let addition = if active {
        state(
            &prefix,
            crate::load::outline::signature(&physical),
            sheet,
            layout,
        )
    } else {
        Vec::new()
    };
    let mut reader = Reader::new(&interim);
    let (_, root) = reader.root("worksheet")?;
    let mut out = interim.bytes()[..root.1].to_vec();
    let mut written = false;
    loop {
        match reader.expect()? {
            (XmlEvent::Start(e), start) if e.name.is(MAIN_NAMESPACE, "extLst") => {
                out.extend(extensions(&mut reader, &interim, start, &addition)?);
                written = true;
            }
            (XmlEvent::Start(_), start) => {
                let end = reader.element_end()?;
                out.extend_from_slice(&interim.bytes()[start.0..end]);
            }
            (XmlEvent::Text(_), start) => out.extend_from_slice(&interim.bytes()[start.0..start.1]),
            (XmlEvent::End(_), start) => {
                if !written && !addition.is_empty() {
                    out.extend(format!("<{prefix}extLst>").bytes());
                    out.extend_from_slice(&addition);
                    out.extend(format!("</{prefix}extLst>").bytes());
                }
                out.extend_from_slice(&interim.bytes()[start.0..]);
                break;
            }
        }
    }
    Ok(out)
}
