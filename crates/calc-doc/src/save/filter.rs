//! Write authored criteria and explicit filtered visibility through the existing worksheet writer.
use super::{
    SaveRefusal,
    reader::Reader,
    text::{RawTag, xstring_attribute},
};
use crate::{
    load::{MAIN_NAMESPACE, address::column_letters, filter::VISIBILITY},
    xml::{XmlEvent, XmlPart},
};
use calc_doc_model::{AutoFilter, Axis, Definitions, FilterCriteria};
fn render(prefix: &str, filter: &AutoFilter, definitions: &Definitions) -> Vec<u8> {
    let a = filter.area;
    let mut out = format!(
        "<{prefix}autoFilter ref=\"{}{}:{}{}\">",
        column_letters(a.first().column()),
        a.first().row(),
        column_letters(a.last().column()),
        a.last().row()
    )
    .into_bytes();
    for (col, criteria) in &filter.columns {
        out.extend(
            format!(
                "<{prefix}filterColumn colId=\"{}\">",
                col - a.first().column()
            )
            .bytes(),
        );
        match criteria {
            FilterCriteria::Values { values, blank } => {
                out.extend(format!("<{prefix}filters blank=\"{}\">", u8::from(*blank)).bytes());
                for v in values {
                    out.extend(format!("<{prefix}filter val=").bytes());
                    xstring_attribute(&v.encode_utf16().collect::<Vec<_>>(), &mut out);
                    out.extend(b"/>");
                }
                out.extend(format!("</{prefix}filters>").bytes());
            }
            FilterCriteria::Custom { and, predicates } => {
                out.extend(format!("<{prefix}customFilters and=\"{}\">", u8::from(*and)).bytes());
                for p in predicates {
                    out.extend(
                        format!(
                            "<{prefix}customFilter operator=\"{}\" val=",
                            p.operator.xml()
                        )
                        .bytes(),
                    );
                    xstring_attribute(&p.value.encode_utf16().collect::<Vec<_>>(), &mut out);
                    out.extend(b"/>");
                }
                out.extend(format!("</{prefix}customFilters>").bytes());
            }
        }
        out.extend(format!("</{prefix}filterColumn>").bytes());
    }
    // XLSX has one physical hidden bit. Keep its two causes in an allowed extension so a
    // DNA save/reopen can clear filtering without erasing independently authored manual hiding.
    let manual = definitions
        .axis_layouts()
        .bands(a.sheet(), Axis::Rows)
        .filter(|(r, f)| *r > a.first().row() && *r <= a.last().row() && f.hidden())
        .map(|(r, _)| r.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let filtered = filter
        .filtered_rows
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    out.extend(format!("<{prefix}extLst><{prefix}ext uri=\"{VISIBILITY}\"><dc:visibility xmlns:dc=\"{VISIBILITY}\" manual=\"{manual}\" filtered=\"{filtered}\"/></{prefix}ext></{prefix}extLst></{prefix}autoFilter>").bytes());
    out
}
pub(super) fn rewrite(
    xml: &XmlPart,
    filter: Option<&AutoFilter>,
    definitions: &Definitions,
) -> Result<Vec<u8>, SaveRefusal> {
    let mut reader = Reader::new(xml);
    let (_, root) = reader.root("worksheet")?;
    let tag = RawTag::parse(&xml.bytes()[root.0..root.1]);
    let prefix = String::from_utf8_lossy(tag.prefix());
    let mut out = xml.bytes()[..root.1].to_vec();
    let mut written = false;
    let mut sheet_pr = false;
    loop {
        match reader.expect()? {
            (XmlEvent::Start(e), _span) if e.name.is(MAIN_NAMESPACE, "autoFilter") => {
                reader.element_end()?;
                if !written {
                    if let Some(f) = filter {
                        out.extend(render(&prefix, f, definitions));
                    }
                    written = true;
                }
            }
            (XmlEvent::Start(e), span) => {
                if !sheet_pr && !e.name.is(MAIN_NAMESPACE, "sheetPr") {
                    if filter.is_some_and(|f| !f.columns.is_empty()) {
                        out.extend(format!("<{prefix}sheetPr filterMode=\"1\"/>").bytes());
                    }
                    sheet_pr = true;
                }
                // autoFilter follows sheetData/protection/scenarios and precedes sortState/merges/rules.
                if !written
                    && ![
                        "sheetPr",
                        "dimension",
                        "sheetViews",
                        "sheetFormatPr",
                        "cols",
                        "sheetData",
                        "sheetCalcPr",
                        "sheetProtection",
                        "protectedRanges",
                        "scenarios",
                    ]
                    .iter()
                    .any(|n| e.name.is(MAIN_NAMESPACE, n))
                {
                    if let Some(f) = filter {
                        out.extend(render(&prefix, f, definitions));
                    }
                    written = true;
                }
                let end = reader.element_end()?;
                if e.name.is(MAIN_NAMESPACE, "sheetPr") {
                    sheet_pr = true;
                    let raw = RawTag::parse(&xml.bytes()[span.0..span.1]);
                    out.extend(raw.rebuilt(
                        &[(
                            "filterMode",
                            filter.filter(|f| !f.columns.is_empty()).map(|_| "1"),
                        )],
                        raw.is_empty(),
                    ));
                    out.extend_from_slice(&xml.bytes()[span.1..end]);
                } else {
                    out.extend_from_slice(&xml.bytes()[span.0..end]);
                }
            }
            (XmlEvent::Text(_), span) => out.extend_from_slice(&xml.bytes()[span.0..span.1]),
            (XmlEvent::End(_), span) => {
                if !written {
                    if let Some(f) = filter {
                        out.extend(render(&prefix, f, definitions));
                    }
                }
                out.extend_from_slice(&xml.bytes()[span.0..]);
                break;
            }
        }
    }
    Ok(out)
}
