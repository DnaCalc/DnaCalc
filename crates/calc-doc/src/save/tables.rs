//! Authored table parts share the package, relationship and child-element writer.
use super::{
    SaveRefusal,
    author::Changes,
    parts,
    reader::Reader,
    text::{RawTag, Splice, xstring_attribute},
};
use crate::{
    load::{LoadedSheet, MAIN_NAMESPACE},
    opc::Package,
    xml::{XmlEvent, XmlLimits, XmlPart},
};
use calc_doc_model::{ModelRoot, Table};
use std::{borrow::Cow, collections::BTreeSet};
const TABLE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/table";
const REL_ATTR: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
fn area(a: calc_doc_model::value::GridArea) -> String {
    format!(
        "{}:{}",
        super::text::a1(a.first().row(), a.first().column()),
        super::text::a1(a.last().row(), a.last().column())
    )
}
fn root(xml: &XmlPart, table: &Table) -> Result<Vec<u8>, SaveRefusal> {
    let mut reader = Reader::new(xml);
    let (_, span) = reader.root("table")?;
    let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
    let id = table.id.to_string();
    let reference = area(table.area);
    let header = table.header_rows.to_string();
    let total = table.totals_rows.to_string();
    let tag = tag.rebuilt(
        &[
            ("id", Some(&id)),
            ("ref", Some(&reference)),
            ("headerRowCount", Some(&header)),
            ("totalsRowCount", Some(&total)),
            (
                "totalsRowShown",
                Some(if table.totals_rows > 0 { "1" } else { "0" }),
            ),
        ],
        tag.is_empty(),
    );
    let tag = RawTag::parse(&tag).rebuilt_xstring(
        "name",
        &table.name.encode_utf16().collect::<Vec<_>>(),
        false,
    );
    let tag = RawTag::parse(&tag).rebuilt_xstring(
        "displayName",
        &table.display_name.encode_utf16().collect::<Vec<_>>(),
        false,
    );
    let mut splice = Splice::new(xml.bytes());
    splice.replace(span, &tag);
    Ok(splice.finish())
}
fn columns(xml: &XmlPart, table: &Table) -> Result<Vec<u8>, SaveRefusal> {
    let records =
        super::reader::element_sources(xml, |e| e.name.is(MAIN_NAMESPACE, "tableColumn"))?;
    let mut used = BTreeSet::new();
    for record in &records {
        let part = XmlPart::new(
            "table-column.xml".into(),
            record.clone(),
            XmlLimits::default(),
        );
        let mut r = Reader::new(&part);
        if let (XmlEvent::Start(e), _) = r.expect()? {
            if let Some(id) = e.attribute("id").and_then(|s| s.parse::<u32>().ok()) {
                used.insert(id);
            }
        }
    }
    let mut out = Vec::new();
    for (index, name) in table.columns.iter().enumerate() {
        let fresh = (1..=u32::MAX)
            .find(|id| !used.contains(id))
            .ok_or_else(|| SaveRefusal::InvalidPart {
                part: xml.name().into(),
                detail: "Table column IDs are exhausted".into(),
            })?;
        used.insert(fresh);
        let default = format!("<tableColumn xmlns=\"{MAIN_NAMESPACE}\" id=\"{}\"/>", fresh);
        let bytes = records
            .get(index)
            .map(Vec::as_slice)
            .unwrap_or(default.as_bytes());
        let part = XmlPart::new(
            "table-column.xml".into(),
            bytes.to_vec(),
            XmlLimits::default(),
        );
        let mut reader = Reader::new(&part);
        let (_, span) = reader.root("tableColumn")?;
        let tag = RawTag::parse(&part.bytes()[span.0..span.1]);
        let function = table.totals_functions.get(index).and_then(|v| v.as_deref());
        let tag = tag.rebuilt(
            &[
                ("totalsRowFunction", function),
                (
                    "totalsRowLabel",
                    table.totals_labels.get(index).and_then(|v| v.as_deref()),
                ),
            ],
            tag.is_empty(),
        );
        let tag = if let Some(label) = table.totals_labels.get(index).and_then(|v| v.as_deref()) {
            RawTag::parse(&tag).rebuilt_xstring(
                "totalsRowLabel",
                &label.encode_utf16().collect::<Vec<_>>(),
                RawTag::parse(&tag).is_empty(),
            )
        } else {
            tag
        };
        let tag = RawTag::parse(&tag).rebuilt_xstring(
            "name",
            &name.encode_utf16().collect::<Vec<_>>(),
            RawTag::parse(&tag).is_empty(),
        );
        let mut splice = Splice::new(part.bytes());
        splice.replace(span, &tag);
        out.extend(splice.finish());
    }
    let old = super::reader::element_sources(xml, |e| e.name.is(MAIN_NAMESPACE, "tableColumns"))?
        .into_iter()
        .next();
    let empty = format!("<tableColumns xmlns=\"{MAIN_NAMESPACE}\"/>");
    let container = XmlPart::new(
        "table-columns.xml".into(),
        old.unwrap_or_else(|| empty.into_bytes()),
        XmlLimits::default(),
    );
    let updated = parts::root_child(&container, "tableColumns", "tableColumn", &out, &["extLst"])?;
    let container = container.with_bytes(updated);
    let mut reader = Reader::new(&container);
    let (_, span) = reader.root("tableColumns")?;
    let tag = RawTag::parse(&container.bytes()[span.0..span.1]);
    let count = table.columns.len().to_string();
    let mut splice = Splice::new(container.bytes());
    splice.replace(
        span,
        &tag.rebuilt(&[("count", Some(&count))], tag.is_empty()),
    );
    Ok(splice.finish())
}
fn style(xml: &XmlPart, table: &Table) -> Result<Vec<u8>, SaveRefusal> {
    let old = super::reader::element_sources(xml, |e| e.name.is(MAIN_NAMESPACE, "tableStyleInfo"))?
        .into_iter()
        .next();
    let empty = format!("<tableStyleInfo xmlns=\"{MAIN_NAMESPACE}\"/>");
    let bytes = old.as_deref().unwrap_or(empty.as_bytes());
    let part = XmlPart::new(
        "table-style.xml".into(),
        bytes.to_vec(),
        XmlLimits::default(),
    );
    let mut r = Reader::new(&part);
    let (_, span) = r.root("tableStyleInfo")?;
    let tag = RawTag::parse(&part.bytes()[span.0..span.1]);
    let s = &table.style;
    let changes = [
        ("name", s.name.as_deref()),
        (
            "showFirstColumn",
            Some(if s.first_column { "1" } else { "0" }),
        ),
        (
            "showLastColumn",
            Some(if s.last_column { "1" } else { "0" }),
        ),
        (
            "showRowStripes",
            Some(if s.row_stripes { "1" } else { "0" }),
        ),
        (
            "showColumnStripes",
            Some(if s.column_stripes { "1" } else { "0" }),
        ),
    ];
    let mut splice = Splice::new(part.bytes());
    splice.replace(span, &tag.rebuilt(&changes, tag.is_empty()));
    Ok(splice.finish())
}
pub(super) fn finish(
    package: &Package,
    workbook: &str,
    sheets: &[Cow<'_, LoadedSheet>],
    base: &ModelRoot,
    model: &ModelRoot,
    out: &mut Changes,
) -> Result<(), SaveRefusal> {
    for table in model.definitions().tables() {
        let old = base
            .definitions()
            .tables()
            .iter()
            .find(|t| t.part == table.part);
        let visibility_changed =
            base.definitions().axis_layouts() != model.definitions().axis_layouts();
        if old == Some(table)
            && (!visibility_changed || table.filter.as_ref().is_none_or(|f| f.unsupported))
        {
            continue;
        }
        let empty =
            format!("<table xmlns=\"{MAIN_NAMESPACE}\"><tableColumns count=\"0\"/></table>");
        let mut xml = parts::current_part(package, out, &table.part, empty.as_bytes())?;
        xml = xml.with_bytes(root(&xml, table)?);
        if old.is_none_or(|t| {
            t.columns != table.columns
                || t.totals_functions != table.totals_functions
                || t.totals_labels != table.totals_labels
        }) {
            xml = xml.with_bytes(parts::root_child(
                &xml,
                "table",
                "tableColumns",
                &columns(&xml, table)?,
                &["tableStyleInfo", "extLst"],
            )?);
        }
        if !table.filter.as_ref().is_some_and(|f| f.unsupported)
            && (old.is_none_or(|t| t.filter != table.filter)
                || (visibility_changed && table.filter.is_some()))
        {
            let source =
                super::reader::element_sources(&xml, |e| e.name.is(MAIN_NAMESPACE, "autoFilter"))?
                    .into_iter()
                    .next();
            let body = if let Some(f) = &table.filter {
                super::filter::render_with_source("", f, model.definitions(), source.as_deref())?
            } else {
                Vec::new()
            };
            xml = xml.with_bytes(parts::root_child(
                &xml,
                "table",
                "autoFilter",
                &body,
                &["sortState", "tableColumns", "tableStyleInfo", "extLst"],
            )?);
        }
        if old.is_none_or(|t| t.style != table.style) {
            xml = xml.with_bytes(parts::root_child(
                &xml,
                "table",
                "tableStyleInfo",
                &style(&xml, table)?,
                &["extLst"],
            )?);
        }
        parts::put_part(package, out, &table.part, xml.bytes().to_vec());
        parts::ensure_content_type(
            package,
            out,
            &table.part,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml",
        )?;
    }
    for sheet in sheets
        .iter()
        .filter(|s| model.sheet_by_id(s.id()).is_some())
    {
        let old = base
            .definitions()
            .tables()
            .iter()
            .filter(|t| t.sheet == sheet.id())
            .map(|t| t.part.as_str())
            .collect::<Vec<_>>();
        let current = model
            .definitions()
            .tables()
            .iter()
            .filter(|t| t.sheet == sheet.id())
            .collect::<Vec<_>>();
        if old == current.iter().map(|t| t.part.as_str()).collect::<Vec<_>>() {
            continue;
        }
        let mut ids = BTreeSet::new();
        let mut body = format!(
            "<tableParts xmlns=\"{MAIN_NAMESPACE}\" xmlns:r=\"{REL_ATTR}\" count=\"{}\">",
            current.len()
        )
        .into_bytes();
        for table in current {
            let id = parts::ensure_relationship(
                package,
                out,
                sheet.part().name(),
                TABLE_REL,
                &table.part,
            )?;
            body.extend(b"<tablePart r:id=");
            xstring_attribute(&id.encode_utf16().collect::<Vec<_>>(), &mut body);
            body.extend(b"/>");
            ids.insert(id);
        }
        body.extend(b"</tableParts>");
        if ids.is_empty() {
            body.clear();
        }
        let xml = parts::current_part(
            package,
            out,
            sheet.part().name(),
            super::sheets::read_sheet(package, sheet.part().name())?.bytes(),
        )?;
        parts::put_part(
            package,
            out,
            sheet.part().name(),
            parts::worksheet_child(&xml, "tableParts", &body, &["extLst"])?,
        );
        let (parent, file) = sheet
            .part()
            .name()
            .rsplit_once('/')
            .unwrap_or(("", sheet.part().name()));
        let relname = format!("{parent}/_rels/{file}.rels")
            .trim_start_matches('/')
            .to_owned();
        let empty = format!(
            "<Relationships xmlns=\"{}\"/>",
            crate::opc::RELATIONSHIPS_NAMESPACE
        );
        let xml = parts::current_part(package, out, &relname, empty.as_bytes())?;
        let mut reader = Reader::new(&xml);
        let mut splice = Splice::new(xml.bytes());
        while let Some((event, span)) = reader.next()? {
            if let XmlEvent::Start(e) = event
                && e.name
                    .is(crate::opc::RELATIONSHIPS_NAMESPACE, "Relationship")
                && e.attribute("Type") == Some(TABLE_REL)
                && !e.attribute("Id").is_some_and(|id| ids.contains(id))
            {
                let end = reader.element_end()?;
                splice.replace((span.0, end), &[]);
            }
        }
        parts::put_part(package, out, &relname, splice.finish());
    }
    let _ = workbook;
    Ok(())
}
