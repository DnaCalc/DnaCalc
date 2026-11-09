//! Rewrite refreshed caches and table indexes, retaining unrelated XML and relationships.
use super::{
    SaveRefusal,
    author::Changes,
    reader::Reader,
    strings::read_part,
    text::{self, RawTag, Splice},
};
use crate::{
    opc::Package,
    xml::{XmlEvent, XmlPart},
};
use calc_doc_model::pivot::*;
use std::collections::{BTreeMap, BTreeSet};
use std::borrow::Borrow;
const M: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
pub(crate) fn missing_output(detail: &str) -> SaveRefusal {
    bad("pivot outputs", detail)
}
const X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const FILL_DOWN_URI: &str = "{2946ED86-A175-432a-8AC1-64E0C546D7DE}";
fn bad(part: &str, detail: impl Into<String>) -> SaveRefusal {
    SaveRefusal::PivotNotModelled {
        part: part.into(),
        detail: detail.into(),
        owner: "dc-8wp",
    }
}
fn attr(name: &str, value: &str, out: &mut Vec<u8>) {
    out.push(b' ');
    out.extend_from_slice(name.as_bytes());
    out.push(b'=');
    text::attribute_value(value, out);
}
fn string_attr(name: &str, value: &str, out: &mut Vec<u8>) {
    out.push(b' ');
    out.extend_from_slice(name.as_bytes());
    out.push(b'=');
    text::xstring_attribute(&value.encode_utf16().collect::<Vec<_>>(), out);
}
fn attrs(values: &BTreeMap<String, String>, out: &mut Vec<u8>) {
    for (k, v) in values {
        attr(k, v, out);
    }
}
fn item(item: &PivotItem, unused: bool, calculated: bool, out: &mut Vec<u8>) -> Result<(), SaveRefusal> {
    let (kind, value) = match item {
        PivotItem::Missing => ("m", None),
        PivotItem::Number(v) => ("n", Some(v.clone())),
        PivotItem::Text(v) => ("s", Some(v.clone())),
        PivotItem::Boolean(v) => ("b", Some(u8::from(*v).to_string())),
        PivotItem::Date(v) => ("d", Some(v.clone())),
        PivotItem::Error(v) => ("e", Some(v.clone())),
        PivotItem::Index(v) => ("x", Some(v.to_string())),
        _ => {
            return Err(bad(
                "pivot cache",
                "Unknown cache items cannot be rewritten",
            ));
        }
    };
    out.push(b'<');
    out.extend_from_slice(kind.as_bytes());
    if let Some(value) = value {
        if kind == "s" {
            string_attr("v", &value, out);
        } else {
            attr("v", &value, out);
        }
    }
    if unused { attr("u", "1", out); }
    if calculated { attr("f", "1", out); }
    out.extend_from_slice(b"/>");
    Ok(())
}
fn cache_group(field: &PivotCacheField, out: &mut Vec<u8>) -> Result<(), SaveRefusal> {
    let Some(group) = &field.group else { return Ok(()); };
    out.extend_from_slice(b"<fieldGroup");
    attrs(&group.attributes, out);
    if group.range.is_some() || group.discrete.is_some() {
        out.push(b'>');
        if let Some(range) = &group.range {
            out.extend_from_slice(b"<rangePr");
            attrs(range, out);
            out.extend_from_slice(b"/>");
        }
        if let Some(discrete) = &group.discrete {
            out.extend_from_slice(b"<discretePr");
            attr("count", &discrete.len().to_string(), out);
            out.push(b'>');
            for index in discrete { out.extend_from_slice(format!("<x v=\"{index}\"/>").as_bytes()); }
            out.extend_from_slice(b"</discretePr>");
        }
        let items = group.numeric_items.as_deref().unwrap_or(&field.items);
        out.extend_from_slice(b"<groupItems");
        attr("count", &items.len().to_string(), out);
        out.push(b'>');
        for value in items { item(value, false, false, out)?; }
        out.extend_from_slice(b"</groupItems></fieldGroup>");
    } else {
        out.extend_from_slice(b"/>");
    }
    Ok(())
}
fn cache_fields(cache: &PivotCache) -> Result<Vec<u8>, SaveRefusal> {
    let mut out = format!(
        "<cacheFields xmlns=\"{M}\" count=\"{}\">",
        cache.fields.len()
    )
    .into_bytes();
    for field in &cache.fields {
        out.extend_from_slice(b"<cacheField");
        let mut a = field.attributes.clone();
        a.remove("name");
        attrs(&a, &mut out);
        string_attr("name", &field.name, &mut out);
        out.push(b'>');
        if matches!(field.attributes.get("databaseField").map(String::as_str), Some("0" | "false")) {
            cache_group(field, &mut out)?;
            out.extend_from_slice(b"</cacheField>");
            continue;
        }
        out.extend_from_slice(b"<sharedItems");
        attr("count", &field.items.len().to_string(), &mut out);
        let missing = field.items.iter().any(|i| matches!(i, PivotItem::Missing));
        let text = field.items.iter().any(|i| matches!(i, PivotItem::Text(_)));
        let number = field
            .items
            .iter()
            .any(|i| matches!(i, PivotItem::Number(_)));
        // ISO/IEC 29500 sharedItems flags describe every retained and newly captured item.
        // https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.shareditems
        let logical = field
            .items
            .iter()
            .any(|i| matches!(i, PivotItem::Boolean(_)));
        let error = field.items.iter().any(|i| matches!(i, PivotItem::Error(_)));
        let dates = field
            .items
            .iter()
            .filter_map(|i| {
                if let PivotItem::Date(d) = i {
                    Some(d.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let date = !dates.is_empty();
        // Excel refuses (through automation) a cache whose date items are flagged as non-dates:
        // date items carry containsDate, with minDate and maxDate.
        let non_date = missing || text || number || logical || error || !date;
        let mixed = u8::from(text)
            + u8::from(number)
            + u8::from(logical)
            + u8::from(error)
            + u8::from(date)
            > 1;
        attr(
            "containsInteger",
            if number
                && !mixed
                && field
                    .items
                    .iter()
                    .filter_map(|i| {
                        if let PivotItem::Number(n) = i {
                            n.parse::<f64>().ok()
                        } else {
                            None
                        }
                    })
                    .all(|n| n.fract() == 0.)
            {
                "1"
            } else {
                "0"
            },
            &mut out,
        );
        attr("containsBlank", if missing { "1" } else { "0" }, &mut out);
        attr("containsString", if text { "1" } else { "0" }, &mut out);
        attr("containsNumber", if number { "1" } else { "0" }, &mut out);
        attr("containsNonDate", if non_date { "1" } else { "0" }, &mut out);
        attr("containsDate", if date { "1" } else { "0" }, &mut out);
        attr(
            "containsMixedTypes",
            if mixed { "1" } else { "0" },
            &mut out,
        );
        // Text or blank (ISO/IEC 29500): Excel refuses (Workbooks.Open 0x800A03EC) a numeric field
        // with a blank item that says 0 here (measured, dc-8wp.16 replay of a source row insert).
        attr(
            "containsSemiMixedTypes",
            if text || missing { "1" } else { "0" },
            &mut out,
        );
        attr(
            "longText",
            if field
                .items
                .iter()
                .any(|i| matches!(i,PivotItem::Text(t) if t.encode_utf16().count()>255))
            {
                "1"
            } else {
                "0"
            },
            &mut out,
        );
        let numbers = field
            .items
            .iter()
            .filter_map(|i| {
                if let PivotItem::Number(n) = i {
                    n.parse::<f64>().ok()
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if let Some(min) = numbers.iter().copied().reduce(f64::min) {
            attr("minValue", &min.to_string(), &mut out);
        }
        if let Some(max) = numbers.iter().copied().reduce(f64::max) {
            attr("maxValue", &max.to_string(), &mut out);
        }
        if let (Some(min), Some(max)) = (dates.iter().min(), dates.iter().max()) {
            attr("minDate", min, &mut out);
            attr("maxDate", max, &mut out);
        }
        out.push(b'>');
        for (index, value) in field.items.iter().enumerate() {
            item(value, field.unused_items.contains(&(index as u32)), field.formula_items.contains(&(index as u32)), &mut out)?;
        }
        out.extend_from_slice(b"</sharedItems>");
        cache_group(field, &mut out)?;
        out.extend_from_slice(b"</cacheField>");
    }
    out.extend_from_slice(b"</cacheFields>");
    Ok(out)
}
fn fields(table: &PivotTable) -> Vec<u8> {
    let mut out = format!(
        "<pivotFields xmlns=\"{M}\" count=\"{}\">",
        table.fields.len()
    )
    .into_bytes();
    for field in &table.fields {
        out.extend_from_slice(b"<pivotField");
        let mut a = field.attributes.clone();
        let fill_down = a.remove("fillDownLabels");
        attrs(&a, &mut out);
        let extension = fill_down.map(|v| {
            let mut ext = format!(
                "<extLst><ext uri=\"{FILL_DOWN_URI}\" xmlns:x14=\"{X14}\"><x14:pivotField"
            )
            .into_bytes();
            attr("fillDownLabels", &v, &mut ext);
            ext.extend_from_slice(b"/></ext></extLst>");
            ext
        });
        if field.items.is_empty() && field.sort_measure.is_none() {
            match extension {
                Some(ext) => {
                    out.push(b'>');
                    out.extend_from_slice(&ext);
                    out.extend_from_slice(b"</pivotField>");
                }
                None => out.extend_from_slice(b"/>"),
            }
            continue;
        }
        out.push(b'>');
        if !field.items.is_empty() {
            out.extend_from_slice(format!("<items count=\"{}\">", field.items.len()).as_bytes());
        }
        for item in &field.items {
            out.extend_from_slice(b"<item");
            if let Some(i) = item.index {
                attr("x", &i.to_string(), &mut out);
            }
            if item.kind != "data" {
                attr("t", &item.kind, &mut out);
            }
            if item.hidden {
                attr("h", "1", &mut out);
            }
            if item.missing {
                attr("m", "1", &mut out);
            }
            if item.calculated {
                attr("f", "1", &mut out);
            }
            out.extend_from_slice(b"/>");
        }
        if !field.items.is_empty() { out.extend_from_slice(b"</items>"); }
        if let Some(measure) = field.sort_measure {
            out.extend_from_slice(format!("<autoSortScope><pivotArea dataOnly=\"0\" outline=\"0\" fieldPosition=\"0\"><references count=\"1\"><reference field=\"4294967294\" count=\"1\" selected=\"0\"><x v=\"{measure}\"/></reference></references></pivotArea></autoSortScope>").as_bytes());
        }
        if let Some(ext) = extension {
            out.extend_from_slice(&ext);
        }
        out.extend_from_slice(b"</pivotField>");
    }
    out.extend_from_slice(b"</pivotFields>");
    out
}
/// The `dataFields` container from the model (a refresh that dropped a source column drops its value fields).
fn data_fields(table: &PivotTable) -> Vec<u8> {
    // A container with no entry is not written: Excel's own file for a pivot with no field has none.
    if table.data_fields.is_empty() { return Vec::new(); }
    let mut out = format!("<dataFields xmlns=\"{M}\" count=\"{}\">", table.data_fields.len()).into_bytes();
    for data in &table.data_fields {
        out.extend_from_slice(b"<dataField");
        let mut a = data.attributes.clone();
        a.remove("name");
        attrs(&a, &mut out);
        if let Some(name) = &data.name {
            string_attr("name", name, &mut out);
        }
        out.extend_from_slice(b"/>");
    }
    out.extend_from_slice(b"</dataFields>");
    out
}
fn axis_fields(name: &str, fields: &[i32]) -> Vec<u8> {
    if fields.is_empty() { return Vec::new(); }
    let mut out = format!("<{name} xmlns=\"{M}\" count=\"{}\">", fields.len()).into_bytes();
    for field in fields { out.extend_from_slice(format!("<field x=\"{field}\"/>").as_bytes()); }
    out.extend_from_slice(format!("</{name}>").as_bytes());
    out
}
fn col_items(table: &PivotTable) -> Vec<u8> {
    if table.col_items.is_empty() { return Vec::new(); }
    let mut out = format!(
        "<colItems xmlns=\"{M}\" count=\"{}\">",
        table.col_items.len()
    )
    .into_bytes();
    for item in &table.col_items {
        out.extend_from_slice(b"<i");
        if item.kind != "data" {
            attr("t", &item.kind, &mut out);
        }
        if item.repeat != 0 {
            attr("r", &item.repeat.to_string(), &mut out);
        }
        if item.data_index != 0 {
            attr("i", &item.data_index.to_string(), &mut out);
        }
        if item.indexes.is_empty() {
            out.extend_from_slice(b"/>");
            continue;
        }
        out.push(b'>');
        for x in &item.indexes {
            out.extend_from_slice(format!("<x v=\"{x}\"/>").as_bytes());
        }
        out.extend_from_slice(b"</i>");
    }
    out.extend_from_slice(b"</colItems>");
    out
}
fn row_items(table: &PivotTable) -> Vec<u8> {
    if table.row_items.is_empty() { return Vec::new(); }
    let mut out = format!(
        "<rowItems xmlns=\"{M}\" count=\"{}\">",
        table.row_items.len()
    )
    .into_bytes();
    for item in &table.row_items {
        out.extend_from_slice(b"<i");
        if item.kind != "data" {
            attr("t", &item.kind, &mut out);
        }
        if item.repeat != 0 {
            attr("r", &item.repeat.to_string(), &mut out);
        }
        if item.data_index != 0 {
            attr("i", &item.data_index.to_string(), &mut out);
        }
        out.push(b'>');
        for x in &item.indexes {
            out.extend_from_slice(format!("<x v=\"{x}\"/>").as_bytes());
        }
        out.extend_from_slice(b"</i>");
    }
    out.extend_from_slice(b"</rowItems>");
    out
}
fn location(table: &PivotTable) -> Vec<u8> {
    let mut out = format!("<location xmlns=\"{M}\"").into_bytes();
    let mut a = table.location.clone();
    if table.collapsed {
        // Excel's file for a pivot that took one cell for lack of room (m7-names col-*, 16.0.20527): the
        // location is that cell with no header, data row or data column; the fields and items stay.
        for name in ["firstHeaderRow", "firstDataRow", "firstDataCol"] {
            a.insert(name.into(), "0".into());
        }
    }
    let area = table.area;
    // One cell is written as Excel writes it, `C4`, not `C4:C4`.
    a.insert(
        "ref".into(),
        if area.first() == area.last() {
            text::a1(area.first().row(), area.first().column())
        } else {
            format!(
                "{}:{}",
                text::a1(area.first().row(), area.first().column()),
                text::a1(area.last().row(), area.last().column())
            )
        },
    );
    attrs(&a, &mut out);
    out.extend_from_slice(b"/>");
    out
}
/// Refuse nested metadata the replacement containers cannot conserve. Root siblings stay raw.
fn preflight(xml: &XmlPart, containers: &[&str]) -> Result<(), SaveRefusal> {
    let mut reader = Reader::new(xml);
    let mut path = Vec::<String>::new();
    let mut active = false;
    while let Some((event, _)) = reader.next()? {
        match event {
            XmlEvent::Start(e) => {
                if path.len() == 1 {
                    active = e.name.namespace.as_deref() == Some(M)
                        && containers.contains(&e.name.local.as_str());
                }
                let in_field_ext = active
                    && path.len() >= 3
                    && path[path.len() - 1] == "ext"
                    && path[path.len() - 2] == "extLst"
                    && path[path.len() - 3] == "pivotField"
                    && e.name.namespace.as_deref() == Some(X14)
                    && e.name.local == "pivotField"
                    && e.attributes.iter().all(|a| a.name.local == "fillDownLabels");
                if active && !in_field_ext {
                    let local = e.name.local.as_str();
                    let parent = path.last().map(String::as_str).unwrap_or("");
                    let allowed = match parent {
                        "pivotTableDefinition" => {
                            local == "location"
                                || local == "pivotFields"
                                || local == "rowItems"
                                || local == "colItems"
                                || local == "rowFields"
                                || local == "colFields"
                                || local == "dataFields"
                        }
                        "dataFields" => local == "dataField",
                        "pivotFields" => local == "pivotField",
                        "pivotField" => matches!(local, "items" | "extLst" | "autoSortScope"),
                        "autoSortScope" => local == "pivotArea",
                        "pivotArea" => local == "references",
                        "references" => local == "reference",
                        "reference" => local == "x",
                        "extLst" => local == "ext",
                        "items" => local == "item",
                        "rowFields" | "colFields" => local == "field",
                        "rowItems" | "colItems" => local == "i",
                        "i" => local == "x",
                        "pivotCacheDefinition" => local == "cacheFields",
                        "cacheFields" => local == "cacheField",
                        "cacheField" => matches!(local, "sharedItems" | "fieldGroup"),
                        "fieldGroup" => matches!(local, "rangePr" | "groupItems" | "discretePr"),
                        "discretePr" => local == "x",
                        "sharedItems" | "groupItems" => matches!(local, "n" | "s" | "b" | "e" | "m" | "d"),
                        "pivotCacheRecords" => local == "r",
                        "r" => matches!(local, "x" | "n" | "s" | "b" | "e" | "m" | "d"),
                        _ => false,
                    };
                    if !allowed || e.name.namespace.as_deref() != Some(M) {
                        return Err(bad(
                            xml.name(),
                            "Nested pivot extensions need a preserving writer before refresh",
                        ));
                    }
                    for a in &e.attributes {
                        let allowed = match local {
                            "location" | "pivotField" | "cacheField" => true,
                            "pivotFields" | "rowFields" | "colFields" | "rowItems" | "colItems" | "items" | "cacheFields" | "dataFields" => {
                                a.name.local.as_str() == "count"
                            }
                            "dataField" => true,
                            "sharedItems" => matches!(
                                a.name.local.as_str(),
                                "count"
                                    | "containsBlank"
                                    | "containsString"
                                    | "containsNumber"
                                    | "containsNonDate"
                                    | "containsDate"
                                    | "containsMixedTypes"
                                    | "containsSemiMixedTypes"
                                    | "containsInteger"
                                    | "minValue"
                                    | "maxValue"
                                    | "minDate"
                                    | "maxDate"
                                    | "longText"
                            ),
                            "autoSortScope" => false,
                            "pivotArea" => matches!(a.name.local.as_str(), "dataOnly" | "outline" | "fieldPosition"),
                            "references" => a.name.local.as_str() == "count",
                            "reference" => matches!(a.name.local.as_str(), "field" | "count" | "selected"),
                            "fieldGroup" => matches!(a.name.local.as_str(), "base" | "par"),
                            "rangePr" => matches!(a.name.local.as_str(), "groupBy" | "startDate" | "endDate" | "autoStart" | "autoEnd" | "groupInterval" | "startNum" | "endNum"),
                            "groupItems" | "discretePr" => a.name.local.as_str() == "count",
                            "ext" => a.name.local.as_str() == "uri" && a.value == FILL_DOWN_URI,
                            "extLst" => false,
                            "field" => a.name.local.as_str() == "x",
                            "item" => matches!(a.name.local.as_str(), "x" | "t" | "h" | "m" | "f"),
                            "i" => matches!(a.name.local.as_str(), "t" | "r" | "i"),
                            "x" | "n" | "s" | "b" | "e" | "d" => a.name.local.as_str() == "v"
                                || (parent == "sharedItems" && matches!(a.name.local.as_str(), "u" | "f")),
                            _ => false,
                        };
                        if !allowed || a.name.namespace.is_some() {
                            return Err(bad(
                                xml.name(),
                                &format!(
                                    "Nested pivot attribute {}@{} cannot be discarded during refresh",
                                    local, a.name.local
                                ),
                            ));
                        }
                    }
                }
                path.push(e.name.local.to_string());
            }
            XmlEvent::End(_) => {
                path.pop();
                if path.len() == 1 {
                    active = false;
                }
            }
            XmlEvent::Text(t) if active && !t.trim().is_empty() => {
                return Err(bad(
                    xml.name(),
                    "Nested pivot text cannot be discarded during refresh",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}
/// Replace named direct children and root attributes. Unknown siblings retain their exact bytes.
fn rewrite(
    xml: &XmlPart,
    root_name: &str,
    attributes: &[(&str, Option<&str>)],
    children: &BTreeMap<&str, Vec<u8>>,
) -> Result<Vec<u8>, SaveRefusal> {
    let mut reader = Reader::new(xml);
    let (_, root_span) = reader.root(root_name)?;
    let mut splice = Splice::new(xml.bytes());
    let root = RawTag::parse(&xml.bytes()[root_span.0..root_span.1]);
    if root.is_empty() {
        return Err(bad(
            xml.name(),
            "An empty pivot definition cannot be rewritten",
        ));
    }
    splice.replace(root_span, &root.rebuilt(attributes, false));
    let mut seen = BTreeSet::new();
    loop {
        match reader.expect()? {
            (XmlEvent::Start(e), span) => {
                let end = reader.element_end()?;
                if root_name == "pivotTableDefinition" && e.name.namespace.as_deref() == Some(M) {
                    let local = e.name.local.as_str();
                    for (missing, before) in [
                        ("rowFields", local == "rowItems"),
                        ("colFields", matches!(local, "colItems" | "pageFields" | "dataFields")),
                        ("colItems", matches!(local, "pageFields" | "dataFields")),
                    ] {
                        if before && !seen.contains(missing) && let Some(bytes) = children.get(missing) {
                            splice.replace((span.0, span.0), bytes);
                            seen.insert(missing.to_owned());
                        }
                    }
                }
                if e.name.namespace.as_deref() == Some(M)
                    && let Some(bytes) = children.get(e.name.local.as_str())
                {
                    if !seen.insert(e.name.local.to_string()) {
                        return Err(bad(xml.name(), "Duplicate pivot metadata containers"));
                    }
                    splice.replace((span.0, end), bytes);
                }
            }
            (XmlEvent::End(_), _) => break,
            _ => {}
        }
    }
    reader.end()?;
    if children.keys().any(|k| !seen.contains(*k)) {
        return Err(bad(
            xml.name(),
            "A required pivot metadata container is missing",
        ));
    }
    Ok(splice.finish())
}
fn records(xml: &XmlPart, cache: &PivotCache) -> Result<Vec<u8>, SaveRefusal> {
    let snapshot = cache
        .snapshot
        .as_ref()
        .ok_or_else(|| bad(&cache.part, "No refreshed source records"))?;
    let mut body = format!("<pivotCacheRecords xmlns=\"{M}\"").into_bytes();
    attr("count", &snapshot.records.len().to_string(), &mut body);
    body.push(b'>');
    for row in &snapshot.records {
        if row.len() != cache.fields.len() { return Err(bad(&cache.part, "A record has an invalid field count")); }
        body.extend_from_slice(format!("<r xmlns=\"{M}\">").as_bytes());
        for (i, value) in row.iter().enumerate() {
            let field = cache.fields.get(i).ok_or_else(|| bad(&cache.part, "A record has an invalid field count"))?;
            if !matches!(field.attributes.get("databaseField").map(String::as_str), Some("0" | "false")) {
                item(value, false, false, &mut body)?;
            }
        }
        body.extend_from_slice(b"</r>");
    }
    body.extend_from_slice(b"</pivotCacheRecords>");
    // Retain unknown direct children, root attributes and namespace declarations. Records are
    // the only replaced content; the old source never supplies an aggregate answer.
    let mut reader = Reader::new(xml);
    let (_, span) = reader.root("pivotCacheRecords")?;
    let raw = RawTag::parse(&xml.bytes()[span.0..span.1]);
    let count = snapshot.records.len().to_string();
    let mut splice = Splice::new(xml.bytes());
    splice.replace(span, &raw.rebuilt(&[("count", Some(&count))], false));
    let mut inserted = false;
    let start = body.iter().position(|&b| b == b'>').unwrap() + 1;
    let end = body.len() - "</pivotCacheRecords>".len();
    let body = &body[start..end];
    if raw.is_empty() {
        splice.put(body);
        splice.put(b"</");
        splice.put(raw.qualified_name());
        splice.put(b">");
        return Ok(splice.finish());
    }
    loop {
        match reader.expect()? {
            (XmlEvent::Start(e), span) => {
                let end = reader.element_end()?;
                if e.name.is(M, "r") {
                    splice.replace(
                        (span.0, end),
                        if inserted {
                            b""
                        } else {
                            inserted = true;
                            body
                        },
                    );
                } else if !inserted {
                    splice.replace((span.0, span.0), body);
                    inserted = true;
                }
            }
            (XmlEvent::End(_), span) => {
                if !inserted {
                    splice.replace((span.0, span.0), body);
                }
                break;
            }
            _ => {}
        }
    }
    reader.end()?;
    Ok(splice.finish())
}
pub(super) fn finish<S:Borrow<crate::load::LoadedSheet>>(
    package: &Package,
    before: &PivotState,
    after: &PivotState,
    out: &mut Changes,
    workbook:&str,sheets:&[S],
) -> Result<(), SaveRefusal> {
    if before == after {
        return Ok(());
    }
    if before.caches.caches.keys().any(|id|!after.caches.caches.contains_key(id)){return Err(bad("pivot tables","Removing imported pivot caches is not modelled"));}
    remove_tables(package,before,after,out,sheets)?;
    for cache in after
        .caches
        .caches
        .values()
        .filter(|c| c.snapshot.is_some())
    {
        if after.caches.caches.values().any(|other| {
            other.id != cache.id
                && (other.part == cache.part
                    || cache
                        .records_part
                        .as_ref()
                        .is_some_and(|p| other.records_part.as_ref() == Some(p)))
        }) {
            return Err(bad(
                &cache.part,
                "Aliased cache definition or records parts cannot be refreshed independently",
            ));
        }
    }
    for table in after.tables().filter(|t|before.tables().any(|old|old.part==t.part)) {
        if let Some(cache) = after
            .caches
            .caches
            .get(&table.cache_id)
            .filter(|c| c.snapshot.is_some())
        {
            if after.tables().filter(|t| t.part == table.part).count() != 1 {
                return Err(bad(
                    &table.part,
                    "A pivot definition is shared between worksheets",
                ));
            }
            let links=package.relationships_of(Some(&table.part)).into_iter().flat_map(|r|r.items()).filter(|r|r.kind=="http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition").collect::<Vec<_>>();
            if links.len() != 1
                || links[0].mode != crate::opc::TargetMode::Internal
                || links[0].resolved.as_deref() != Some(cache.part.as_str())
            {
                return Err(bad(
                    &table.part,
                    "The pivot cache ID and definition relationship disagree",
                ));
            }
        }

        let old = before
            .tables()
            .find(|old| old.part == table.part)
            .ok_or_else(|| {
                bad(
                    &table.part,
                    "A pivot was added without a package relationship",
                )
            })?;
        if after
            .caches
            .caches
            .get(&table.cache_id)
            .is_some_and(|c| c.snapshot.is_some())
        {
            preflight(
                &read_part(package, &table.part)?,
                &["location", "pivotFields", "rowItems", "colItems"],
            )?;
        }
        if old == table {
            continue;
        }
        let mut retained = table.clone();
        retained.area = old.area;
        retained.collapsed = old.collapsed;
        retained.location = old.location.clone();
        retained.fields = old.fields.clone();
        retained.row_items = old.row_items.clone();
        retained.col_items = old.col_items.clone();
        retained.row_fields = old.row_fields.clone();
        retained.column_fields = old.column_fields.clone();
        let data_changed = old.data_fields != table.data_fields;
        if data_changed {
            // A source column that went takes its value fields (dc-8wp.16); only plain ones are rewritten.
            if old.data_fields.iter().chain(&table.data_fields).any(|d| d.extended_show.is_some()) {
                return Err(bad(&table.part, "Changing value fields with a display calculation is not modelled"));
            }
            retained.data_fields = old.data_fields.clone();
        }
        if &retained != old {
            return Err(bad(
                &table.part,
                "Changing these pivot properties is not modelled",
            ));
        }
        if old.unsupported.iter().any(|item| item == "unsupported value sort") {
            return Err(bad(&table.part, "An unsupported value-sort scope cannot be rewritten"));
        }
        let xml = read_part(package, &table.part)?;
        let mut containers = vec!["location", "pivotFields", "rowItems", "colItems"];
        if data_changed { containers.push("dataFields"); }
        if old.row_fields != table.row_fields { containers.push("rowFields"); }
        if old.column_fields != table.column_fields { containers.push("colFields"); }
        preflight(&xml, &containers)?;
        let mut replacements = BTreeMap::from([
            ("location", location(table)),
            ("pivotFields", fields(table)),
            ("rowItems", row_items(table)),
        ]);
        // Older and synthetic definitions can lack the container; leave such a part alone.
        if !old.col_items.is_empty() || !table.col_items.is_empty() {
            replacements.insert("colItems", col_items(table));
        }
        if data_changed { replacements.insert("dataFields", data_fields(table)); }
        if old.row_fields != table.row_fields { replacements.insert("rowFields", axis_fields("rowFields", &table.row_fields)); }
        if old.column_fields != table.column_fields { replacements.insert("colFields", axis_fields("colFields", &table.column_fields)); }
        out.rewritten.insert(
            table.part.clone(),
            rewrite(&xml, "pivotTableDefinition", &[], &replacements)?,
        );
    }
    for (id, cache) in after.caches.caches.iter().filter(|(id,_)|before.caches.caches.contains_key(id)) {
        let old = before.caches.caches.get(id).ok_or_else(|| {
            bad(
                &cache.part,
                "A pivot cache was added without a package relationship",
            )
        })?;
        if old == cache {
            continue;
        }
        // A worksheet source that a row or column edit moved (dc-8wp.16): only `cacheSource` is rewritten.
        let source_changed = cache.source_range != old.source_range || cache.source_sheet != old.source_sheet;
        let cache_source = if source_changed {
            let (Some(sheet), Some(range)) = (&cache.source_sheet, &cache.source_range) else {
                return Err(bad(&cache.part, "A moved cache source needs a worksheet and a range"));
            };
            let mut source = format!("<cacheSource xmlns=\"{M}\" type=\"{}\"><worksheetSource", cache.source_type).into_bytes();
            string_attr("sheet", sheet, &mut source);
            attr("ref", range, &mut source);
            source.extend_from_slice(b"/></cacheSource>");
            Some(source)
        } else {
            None
        };
        if let Some(source) = &cache_source {
            let mut retained = (**cache).clone();
            retained.source_range = old.source_range.clone();
            retained.source_sheet = old.source_sheet.clone();
            if retained == **old {
                let xml = read_part(package, &cache.part)?;
                preflight(&xml, &["cacheSource"])?;
                let bytes = rewrite(&xml, "pivotCacheDefinition", &[], &BTreeMap::from([("cacheSource", source.clone())]))?;
                out.rewritten.insert(cache.part.clone(), bytes);
                continue;
            }
        }
        let Some(snapshot) = cache.snapshot.as_ref() else {
            return Err(bad(
                &cache.part,
                "Discarding refreshed cache records is not modelled",
            ));
        };
        if matches!(&snapshot.origin, PivotSnapshotOrigin::DataModel { .. }) {
            let mut retained = (**cache).clone();
            retained.snapshot = old.snapshot.clone();
            if &retained != old.as_ref() {
                return Err(bad(&cache.part, "Changing this model-pivot cache is not modelled"));
            }
            continue;
        }
        let mut retained = (**cache).clone();
        retained.record_count = old.record_count;
        retained.fields = old.fields.clone();
        retained.snapshot = old.snapshot.clone();
        retained.source_range = old.source_range.clone();
        retained.source_sheet = old.source_sheet.clone();
        if retained.attributes.get("missingItemsLimit") != old.attributes.get("missingItemsLimit") {
            if !matches!(retained.attributes.get("missingItemsLimit").map(String::as_str), None | Some("0" | "32500")) {
                return Err(bad(&cache.part, "This item retention setting is not modelled"));
            }
            match old.attributes.get("missingItemsLimit") {
                Some(value) => { retained.attributes.insert("missingItemsLimit".into(), value.clone()); }
                None => { retained.attributes.remove("missingItemsLimit"); }
            }
        }
        if &retained != old.as_ref() {
            return Err(bad(
                &cache.part,
                "Changing this cache source is not modelled",
            ));
        }
        let xml = read_part(package, &cache.part)?;
        preflight(&xml, &["cacheFields"])?;
        let count = snapshot.records.len().to_string();
        let mut children = BTreeMap::from([("cacheFields", cache_fields(cache)?)]);
        if let Some(source) = cache_source {
            children.insert("cacheSource", source);
        }
        let bytes = rewrite(
            &xml,
            "pivotCacheDefinition",
            &[
                ("recordCount", Some(&count)),
                ("missingItemsLimit", cache.attributes.get("missingItemsLimit").map(String::as_str)),
                ("refreshOnLoad", Some("0")),
                ("invalid", Some("0")),
                ("refreshedBy", None),
                ("refreshedDate", None),
                ("refreshedDateIso", None),
            ],
            &children,
        )?;
        out.rewritten.insert(cache.part.clone(), bytes);
        let part = cache.records_part.as_deref().ok_or_else(|| {
            bad(
                &cache.part,
                "Creating a missing records relationship is not modelled",
            )
        })?;
        let xml = read_part(package, part)?;
        preflight(&xml, &["r"])?;
        out.rewritten.insert(part.into(), records(&xml, cache)?);
    }
    add_new(package,before,after,out,workbook,sheets)?;
    Ok(())
}

/// A pivot table the model no longer holds (all of its output was deleted, as Excel removes it): its part, its
/// own relationships, the worksheet's relationship to it and its content type go. Its cache stays in the workbook.
fn remove_tables<S: Borrow<crate::load::LoadedSheet>>(
    package: &Package,
    before: &PivotState,
    after: &PivotState,
    out: &mut Changes,
    sheets: &[S],
) -> Result<(), SaveRefusal> {
    use super::parts;
    for old in before.tables().filter(|old| !after.tables().any(|t| t.part == old.part)) {
        if before.tables().filter(|t| t.part == old.part).count() != 1 {
            return Err(bad(&old.part, "A pivot definition shared between worksheets cannot be removed"));
        }
        let sheet = sheets.iter().map(Borrow::borrow).find(|s| s.id() == old.area.sheet())
            .ok_or_else(|| bad(&old.part, "The removed pivot's worksheet is unavailable"))?;
        let sheet_part = sheet.part().name().to_owned();
        let (parent, file) = sheet_part.rsplit_once('/').unwrap_or(("", sheet_part.as_str()));
        let rels = if parent.is_empty() { format!("_rels/{file}.rels") } else { format!("{parent}/_rels/{file}.rels") };
        let empty = format!("<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"/>");
        let xml = parts::current_part(package, out, &rels, empty.as_bytes())?;
        let mut reader = Reader::new(&xml);
        let mut splice = Splice::new(xml.bytes());
        let mut found = false;
        while let Some((event, span)) = reader.next()? {
            if let XmlEvent::Start(e) = event
                && e.name.local == "Relationship"
                && e.attribute("TargetMode") != Some("External")
                && e.attribute("Type").is_some_and(|t| t.ends_with("/pivotTable"))
                && e.attribute("Target").is_some_and(|target| {
                    package.relationships_of(Some(&sheet_part)).into_iter().flat_map(|r| r.items())
                        .any(|r| r.target == target && r.resolved.as_deref() == Some(old.part.as_str()))
                })
            {
                let end = reader.element_end()?;
                splice.replace((span.0, end), &[]);
                found = true;
            }
        }
        if !found {
            return Err(bad(&old.part, "The worksheet has no relationship to the removed pivot"));
        }
        parts::put_part(package, out, &rels, splice.finish());
        let content = match &out.content_types {
            Some(bytes) => XmlPart::new(crate::opc::CONTENT_TYPES_ENTRY.into(), bytes.clone(), crate::xml::XmlLimits::default()),
            None => package.content_types_xml().map_err(|error| SaveRefusal::Part { part: crate::opc::CONTENT_TYPES_ENTRY.into(), error })?,
        };
        out.content_types = Some(parts::content_types(&content, Some(&old.part), &[])?);
        if let Some(own) = package.relationships_of(Some(&old.part)) {
            out.removed.push(own.part().to_owned());
        }
        out.rewritten.remove(&old.part);
        out.removed.push(old.part.clone());
    }
    Ok(())
}
pub(crate) fn check(
    package: &Package,
    before: &PivotState,
    after: &PivotState,workbook:&str,sheets:&[crate::load::LoadedSheet],
) -> Result<(), SaveRefusal> {
    finish(package, before, after, &mut Changes::default(),workbook,sheets)
}

const REL:&str="http://schemas.openxmlformats.org/officeDocument/2006/relationships";
/// Extend the existing collection through the shared child writer, preserving every old entry.
fn append_collection(package:&Package,out:&mut Changes,part:&str,root:&str,name:&str,entry:&[u8],before:&[&str])->Result<(),SaveRefusal>{
    let xml=super::parts::current_part(package,out,part,b"")?;
    let mut reader=Reader::new(&xml);let (_,root_span)=reader.root(root)?;let root_tag=RawTag::parse(&xml.bytes()[root_span.0..root_span.1]);let mut children=Vec::new();
    loop{match reader.expect()?{(XmlEvent::Start(e),span)=>{let end=reader.element_end()?;if e.name.is(M,name){let mut child=RawTag::parse(&xml.bytes()[span.0..span.1]).with_inherited_namespaces(&root_tag);child.extend_from_slice(&xml.bytes()[span.1..end]);children.push(child);}},(XmlEvent::End(_),_)=>break,_=>{}}}reader.end()?;
    if children.len()>1{return Err(bad(part,"Duplicate pivot collections cannot be extended"));}
    let bytes=if let Some(bytes)=children.first(){
        let container=XmlPart::new(part.into(),bytes.clone(),crate::xml::XmlLimits::default());
        super::parts::append_root(&container,entry)?
    }else{
        let mut bytes=format!("<{name} xmlns=\"{M}\">").into_bytes();bytes.extend(entry);bytes.extend(format!("</{name}>").as_bytes());bytes
    };
    let bytes=super::parts::root_child(&xml,root,name,&bytes,before)?;super::parts::put_part(package,out,part,bytes);Ok(())
}
fn fresh_cache(cache:&PivotCache,records_id:&str)->Result<Vec<u8>,SaveRefusal>{
    let snapshot=cache.snapshot.as_ref().ok_or_else(||bad(&cache.part,"A new cache needs actual source records"))?;
    let mut out=format!("<pivotCacheDefinition xmlns=\"{M}\" xmlns:r=\"{REL}\"").into_bytes();attr("r:id",records_id,&mut out);attr("recordCount",&snapshot.records.len().to_string(),&mut out);attrs(&BTreeMap::from([("saveData".into(),"1".into()),("refreshOnLoad".into(),"0".into()),("createdVersion".into(),"6".into()),("refreshedVersion".into(),"6".into()),("minRefreshableVersion".into(),"3".into())]),&mut out);
    out.extend(b"><cacheSource type=\"worksheet\"><worksheetSource");
    let (Some(sheet),Some(range))=(&cache.source_sheet,&cache.source_range)else{return Err(bad(&cache.part,"New pivots need an explicit worksheet source"));};string_attr("sheet",sheet,&mut out);attr("ref",range,&mut out);out.extend(b"/></cacheSource>");out.extend(cache_fields(cache)?);out.extend(b"</pivotCacheDefinition>");Ok(out)
}
fn fresh_table(table:&PivotTable)->Vec<u8>{
    let mut out=format!("<pivotTableDefinition xmlns=\"{M}\"").into_bytes();
    let mut a=table.attributes.clone(); a.remove("name"); attrs(&a,&mut out);
    string_attr("name",&table.name,&mut out); out.push(b'>');
    out.extend(location(table)); out.extend(fields(table));
    if !table.row_fields.is_empty() { out.extend(axis_fields("rowFields", &table.row_fields)); }
    out.extend(row_items(table));
    if !table.column_fields.is_empty() { out.extend(axis_fields("colFields", &table.column_fields)); }
    if !table.col_items.is_empty() { out.extend(col_items(table)); }
    out.extend(format!("<dataFields count=\"{}\">",table.data_fields.len()).as_bytes());
    for data in &table.data_fields {
        out.extend(b"<dataField"); let mut a=data.attributes.clone(); a.remove("name"); attrs(&a,&mut out);
        if let Some(name)=&data.name { string_attr("name",name,&mut out); } out.extend(b"/>");
    }
    out.extend(b"</dataFields><pivotTableStyleInfo name=\"PivotStyleLight16\" showRowHeaders=\"1\" showColHeaders=\"1\" showRowStripes=\"0\" showColStripes=\"0\" showLastColumn=\"1\"/></pivotTableDefinition>"); out
}
fn add_new<S:Borrow<crate::load::LoadedSheet>>(package:&Package,before:&PivotState,after:&PivotState,out:&mut Changes,workbook:&str,sheets:&[S])->Result<(),SaveRefusal>{
    use super::parts;
    for (id,cache)in after.caches.caches.iter().filter(|(id,_)|!before.caches.caches.contains_key(id)){
        if package.part(&cache.part).is_some(){return Err(bad(&cache.part,"The new cache part name is already used"));}
        let records_part=cache.records_part.as_deref().ok_or_else(||bad(&cache.part,"A new cache needs a records part"))?;
        if package.part(records_part).is_some(){return Err(bad(records_part,"The new records part name is already used"));}
        let record_id=parts::ensure_relationship(package,out,&cache.part,&format!("{REL}/pivotCacheRecords"),records_part)?;
        parts::put_part(package,out,&cache.part,fresh_cache(cache,&record_id)?);
        let empty=XmlPart::new(records_part.into(),format!("<pivotCacheRecords xmlns=\"{M}\" count=\"0\"/>").into_bytes(),crate::xml::XmlLimits::default());parts::put_part(package,out,records_part,records(&empty,cache)?);
        parts::ensure_content_type(package,out,&cache.part,"application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml")?;
        parts::ensure_content_type(package,out,records_part,"application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheRecords+xml")?;
        let rel=parts::ensure_relationship(package,out,workbook,&format!("{REL}/pivotCacheDefinition"),&cache.part)?;
        let mut entry=format!("<pivotCache xmlns=\"{M}\" xmlns:r=\"{REL}\" cacheId=\"{id}\"").into_bytes();attr("r:id",&rel,&mut entry);entry.extend(b"/>");
        append_collection(package,out,workbook,"workbook","pivotCaches",&entry,&["smartTagPr","smartTagTypes","webPublishing","fileRecoveryPr","webPublishObjects","extLst"])?;
    }
    for table in after.tables().filter(|table|!before.tables().any(|old|old.part==table.part)){
        let source=sheets.iter().map(Borrow::borrow).find(|s|s.id()==table.area.sheet()).ok_or_else(||bad(&table.part,"The destination worksheet is unavailable"))?;
        if sheets.iter().map(Borrow::borrow).filter(|s|s.part().name()==source.part().name()).count()!=1||package.part(&table.part).is_some(){return Err(bad(&table.part,"A pivot definition or destination part is shared"));}
        let cache=after.caches.caches.get(&table.cache_id).ok_or_else(||bad(&table.part,"The new pivot cache is unavailable"))?;
        // Use the engine's same layout validation; the codec only writes neutral definitions.
        // Host/engine already validated the supported row/value layout and actual source snapshot.
        if table.row_fields.is_empty()||!table.page_fields.is_empty()||table.data_fields.is_empty()||cache.snapshot.is_none(){return Err(bad(&table.part,"The new pivot layout is not supported"));}
        parts::put_part(package,out,&table.part,fresh_table(table));
        parts::ensure_relationship(package,out,&table.part,&format!("{REL}/pivotCacheDefinition"),&cache.part)?;
        parts::ensure_content_type(package,out,&table.part,"application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml")?;
        let sheet=source.part().name();let rel=parts::ensure_relationship(package,out,sheet,&format!("{REL}/pivotTable"),&table.part)?;
        // Worksheet-to-pivot links are implicit relationships; SpreadsheetML has no pivotParts child.
        let _=rel;

    }Ok(())
}

#[cfg(test)]
mod date_flags {
    use super::*;
    /// UI30 P11: Excel refused a refreshed cache whose date items were flagged as non-dates
    /// (it opened once the flags were Excel's: containsDate with minDate and maxDate).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn date_items_are_flagged_as_dates() {
        let field = PivotCacheField {
            name: "Month".into(),
            shared_items: None,
            group: None,
            unused_items: Vec::new(),
            formula_items: Vec::new(),
            items: vec![
                PivotItem::Date("2024-02-15T00:00:00".into()),
                PivotItem::Date("2024-01-15T00:00:00".into()),
            ],
            attributes: BTreeMap::new(),
        };
        let cache = PivotCache {
            id: 1,
            part: String::new(),
            records_part: None,
            record_count: None,
            source_type: "worksheet".into(),
            source_connection_id: None,
            source_sheet: None,
            source_range: None,
            source_name: None,
            fields: vec![field],
            calculated_items: Vec::new(),
            attributes: BTreeMap::new(),
            unsupported: Vec::new(),
            snapshot: None,
        };
        let xml = String::from_utf8(cache_fields(&cache).unwrap()).unwrap();
        assert!(xml.contains(r#"containsDate="1""#), "{xml}");
        assert!(xml.contains(r#"containsNonDate="0""#), "{xml}");
        assert!(xml.contains(r#"containsNumber="0""#), "{xml}");
        assert!(xml.contains(r#"containsMixedTypes="0""#), "{xml}");
        assert!(xml.contains(r#"minDate="2024-01-15T00:00:00""#), "{xml}");
        assert!(xml.contains(r#"maxDate="2024-02-15T00:00:00""#), "{xml}");
    }
}
