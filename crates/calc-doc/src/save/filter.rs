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
/// Retain the imported filter identity while replacing owned criteria and visibility.
pub(super) fn render_with_source(
    prefix: &str,
    filter: &AutoFilter,
    definitions: &Definitions,
    source: Option<&[u8]>,
) -> Result<Vec<u8>, SaveRefusal> {
    let bytes = render(prefix, filter, definitions);
    let end = bytes
        .iter()
        .position(|b| *b == b'>')
        .expect("rendered opening")
        + 1;
    let tag = RawTag::parse(&bytes[..end]);
    let mut changes = Vec::<(String, Option<String>)>::new();
    let namespace = if prefix.is_empty() {
        "xmlns".into()
    } else {
        format!("xmlns:{}", prefix.trim_end_matches(':'))
    };
    changes.push((namespace, Some(MAIN_NAMESPACE.into())));
    let mut source_tag = None;
    if let Some(source) = source {
        let xml = XmlPart::new(
            "filter-source.xml".into(),
            source.to_vec(),
            crate::xml::XmlLimits::default(),
        );
        let mut r = Reader::new(&xml);
        let (e, span) = r.root("autoFilter")?;
        let raw = source[span.0..span.1].to_vec();
        for a in &e.attributes {
            if a.name.is(
                "http://schemas.microsoft.com/office/spreadsheetml/2014/revision",
                "uid",
            ) {
                let revision_prefix = if prefix == "dnarev:" {
                    "dnarev2"
                } else {
                    "dnarev"
                };
                changes.push((
                    format!("xmlns:{revision_prefix}"),
                    Some("http://schemas.microsoft.com/office/spreadsheetml/2014/revision".into()),
                ));
                changes.push((format!("{revision_prefix}:uid"), Some(a.value.to_string())));
            }
        }
        source_tag = Some(raw);
    }
    let changes = changes
        .iter()
        .map(|(n, v)| (n.as_str(), v.as_deref()))
        .collect::<Vec<_>>();
    let opening = tag.rebuilt(&changes, false);
    let mut out = if let Some(source) = source_tag {
        RawTag::parse(&opening).with_inherited_namespaces(&RawTag::parse(&source))
    } else {
        opening
    };
    out.extend_from_slice(&bytes[end..]);
    Ok(out)
}
pub(super) fn render(prefix: &str, filter: &AutoFilter, definitions: &Definitions) -> Vec<u8> {
    let a = filter.area;
    let mut out = format!(
        "<{prefix}autoFilter ref=\"{}{}:{}{}\" {}=\"{MAIN_NAMESPACE}\">",
        column_letters(a.first().column()),
        a.first().row(),
        column_letters(a.last().column()),
        a.last().row(),
        if prefix.is_empty() {
            "xmlns".into()
        } else {
            format!("xmlns:{}", prefix.trim_end_matches(':'))
        }
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
            FilterCriteria::Dates {
                groups,
                values,
                blank,
            } => {
                out.extend(format!("<{prefix}filters blank=\"{}\">", u8::from(*blank)).bytes());
                for v in values {
                    out.extend(format!("<{prefix}filter val=").bytes());
                    xstring_attribute(&v.encode_utf16().collect::<Vec<_>>(), &mut out);
                    out.extend(b"/>");
                }
                let fields = ["year", "month", "day", "hour", "minute", "second"];
                for group in groups {
                    out.extend(
                        format!(
                            "<{prefix}dateGroupItem dateTimeGrouping=\"{}\"",
                            fields[group.len() - 1]
                        )
                        .bytes(),
                    );
                    for (field, value) in fields.iter().zip(group) {
                        out.extend(format!(" {field}=\"{value}\"").bytes());
                    }
                    out.extend(b"/>");
                }
                out.extend(format!("</{prefix}filters>").bytes());
            }
            FilterCriteria::Top {
                top,
                percent,
                value,
            } => {
                out.extend(
                    format!(
                        "<{prefix}top10 top=\"{}\" percent=\"{}\" val=",
                        u8::from(*top),
                        u8::from(*percent)
                    )
                    .bytes(),
                );
                xstring_attribute(&value.encode_utf16().collect::<Vec<_>>(), &mut out);
                out.extend(b"/>");
            }
            FilterCriteria::Color { dxf_id, cell_color } => {
                out.extend(
                    format!(
                        "<{prefix}colorFilter dxfId=\"{dxf_id}\" cellColor=\"{}\"/>",
                        u8::from(*cell_color)
                    )
                    .bytes(),
                );
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
    let source = super::reader::element_sources(xml, |e| e.name.is(MAIN_NAMESPACE, "autoFilter"))?
        .into_iter()
        .next();
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
                        out.extend(render_with_source(
                            &prefix,
                            f,
                            definitions,
                            source.as_deref(),
                        )?);
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
                        out.extend(render_with_source(
                            &prefix,
                            f,
                            definitions,
                            source.as_deref(),
                        )?);
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
                        out.extend(render_with_source(
                            &prefix,
                            f,
                            definitions,
                            source.as_deref(),
                        )?);
                    }
                }
                out.extend_from_slice(&xml.bytes()[span.0..]);
                break;
            }
        }
    }
    Ok(out)
}
