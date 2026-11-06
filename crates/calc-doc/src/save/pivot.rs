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
const M: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
pub(crate) fn missing_output(detail: &str) -> SaveRefusal {
    bad("pivot outputs", detail)
}
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
fn item(item: &PivotItem, out: &mut Vec<u8>) -> Result<(), SaveRefusal> {
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
    out.extend_from_slice(b"/>");
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
        out.extend_from_slice(b"><sharedItems");
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
        let mixed = u8::from(text) + u8::from(number) + u8::from(logical) + u8::from(error) > 1;
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
        attr("containsNonDate", "1", &mut out);
        attr("containsDate", "0", &mut out);
        attr(
            "containsMixedTypes",
            if mixed { "1" } else { "0" },
            &mut out,
        );
        attr(
            "containsSemiMixedTypes",
            if text { "1" } else { "0" },
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
        out.push(b'>');
        for value in &field.items {
            item(value, &mut out)?;
        }
        out.extend_from_slice(b"</sharedItems></cacheField>");
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
        attrs(&field.attributes, &mut out);
        if field.items.is_empty() {
            out.extend_from_slice(b"/>");
            continue;
        }
        out.extend_from_slice(format!("><items count=\"{}\">", field.items.len()).as_bytes());
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
            out.extend_from_slice(b"/>");
        }
        out.extend_from_slice(b"</items></pivotField>");
    }
    out.extend_from_slice(b"</pivotFields>");
    out
}
fn row_items(table: &PivotTable) -> Vec<u8> {
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
    let area = table.area;
    a.insert(
        "ref".into(),
        format!(
            "{}:{}",
            text::a1(area.first().row(), area.first().column()),
            text::a1(area.last().row(), area.last().column())
        ),
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
                if active {
                    let local = e.name.local.as_str();
                    let parent = path.last().map(String::as_str).unwrap_or("");
                    let allowed = match parent {
                        "pivotTableDefinition" => {
                            local == "location" || local == "pivotFields" || local == "rowItems"
                        }
                        "pivotFields" => local == "pivotField",
                        "pivotField" => local == "items",
                        "items" => local == "item",
                        "rowItems" => local == "i",
                        "i" => local == "x",
                        "pivotCacheDefinition" => local == "cacheFields",
                        "cacheFields" => local == "cacheField",
                        "cacheField" => local == "sharedItems",
                        "sharedItems" => matches!(local, "n" | "s" | "b" | "e" | "m"),
                        "pivotCacheRecords" => local == "r",
                        "r" => matches!(local, "x" | "n" | "s" | "b" | "e" | "m"),
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
                            "pivotFields" | "rowItems" | "items" | "cacheFields" => {
                                a.name.local.as_str() == "count"
                            }
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
                                    | "longText"
                            ),
                            "item" => matches!(a.name.local.as_str(), "x" | "t" | "h"),
                            "i" => matches!(a.name.local.as_str(), "t" | "r" | "i"),
                            "x" | "n" | "s" | "b" | "e" => a.name.local.as_str() == "v",
                            _ => false,
                        };
                        if !allowed || a.name.namespace.is_some() {
                            return Err(bad(
                                xml.name(),
                                "Nested pivot attributes cannot be discarded during refresh",
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
        body.extend_from_slice(format!("<r xmlns=\"{M}\">").as_bytes());
        for value in row {
            item(value, &mut body)?;
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
pub(super) fn finish(
    package: &Package,
    before: &PivotState,
    after: &PivotState,
    out: &mut Changes,
) -> Result<(), SaveRefusal> {
    if before == after {
        return Ok(());
    }
    if before.tables().count() != after.tables().count() {
        return Err(bad(
            "pivot tables",
            "Pivot creation needs the package relationship writer",
        ));
    }
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
    for table in after.tables() {
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
                &["location", "pivotFields", "rowItems"],
            )?;
        }
        if old == table {
            continue;
        }
        let mut retained = table.clone();
        retained.area = old.area;
        retained.location = old.location.clone();
        retained.fields = old.fields.clone();
        retained.row_items = old.row_items.clone();
        if &retained != old {
            return Err(bad(
                &table.part,
                "Changing these pivot properties is not modelled",
            ));
        }
        let xml = read_part(package, &table.part)?;
        preflight(&xml, &["location", "pivotFields", "rowItems"])?;
        let replacements = BTreeMap::from([
            ("location", location(table)),
            ("pivotFields", fields(table)),
            ("rowItems", row_items(table)),
        ]);
        out.rewritten.insert(
            table.part.clone(),
            rewrite(&xml, "pivotTableDefinition", &[], &replacements)?,
        );
    }
    for (id, cache) in &after.caches.caches {
        let old = before.caches.caches.get(id).ok_or_else(|| {
            bad(
                &cache.part,
                "A pivot cache was added without a package relationship",
            )
        })?;
        if old == cache {
            continue;
        }
        let Some(snapshot) = cache.snapshot.as_ref() else {
            return Err(bad(
                &cache.part,
                "Discarding refreshed cache records is not modelled",
            ));
        };
        let mut retained = (**cache).clone();
        retained.record_count = old.record_count;
        retained.fields = old.fields.clone();
        retained.snapshot = old.snapshot.clone();
        if &retained != old.as_ref() {
            return Err(bad(
                &cache.part,
                "Changing this cache source is not modelled",
            ));
        }
        let xml = read_part(package, &cache.part)?;
        preflight(&xml, &["cacheFields"])?;
        let count = snapshot.records.len().to_string();
        let children = BTreeMap::from([("cacheFields", cache_fields(cache)?)]);
        let bytes = rewrite(
            &xml,
            "pivotCacheDefinition",
            &[
                ("recordCount", Some(&count)),
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
    Ok(())
}

pub(crate) fn check(
    package: &Package,
    before: &PivotState,
    after: &PivotState,
) -> Result<(), SaveRefusal> {
    finish(package, before, after, &mut Changes::default())
}
