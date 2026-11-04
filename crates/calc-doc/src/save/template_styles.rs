//! Merge template style records without changing any existing workbook index.
use super::{
    SaveRefusal,
    reader::Reader,
    text::{RawTag, Splice},
};
use crate::{
    load::{
        MAIN_NAMESPACE,
        manifest::{Node, tree},
    },
    xml::{XmlEvent, XmlPart},
};
use std::collections::{BTreeMap, BTreeSet};

fn refused(detail: impl Into<String>) -> SaveRefusal {
    SaveRefusal::TemplateNotModelled {
        detail: detail.into(),
        owner: "dc-bctq",
    }
}
fn child<'a>(root: &'a Node, name: &str) -> Option<&'a Node> {
    root.children
        .iter()
        .find(|n| n.element.name.is(MAIN_NAMESPACE, name))
}
fn fragment(xml: &XmlPart, node: &Node, parents: &[&Node]) -> Vec<u8> {
    let mut bytes = xml.bytes()[node.start_tag.0..node.start_tag.1].to_vec();
    for parent in parents.iter().rev() {
        bytes = RawTag::parse(&bytes).with_inherited_namespaces(&RawTag::parse(
            &xml.bytes()[parent.start_tag.0..parent.start_tag.1],
        ));
    }
    bytes.extend_from_slice(&xml.bytes()[node.start_tag.1..node.end_tag.1]);
    bytes
}
fn key(bytes: &[u8]) -> Result<String, SaveRefusal> {
    let xml = XmlPart::new("template-style".into(), bytes.to_vec(), Default::default());
    let mut reader = Reader::new(&xml);
    let mut result = String::new();
    while let Some((event, _)) = reader.next()? {
        match event {
            XmlEvent::Start(e) => {
                let mut attributes: Vec<_> = e
                    .attributes
                    .iter()
                    .map(|a| {
                        (
                            (a.name.namespace.clone(), a.name.local.clone()),
                            a.value.clone(),
                        )
                    })
                    .collect();
                attributes.sort();
                result.push_str(&format!("S{:?}{attributes:?}", e.name));
            }
            XmlEvent::End(e) => result.push_str(&format!("E{e:?}")),
            XmlEvent::Text(text) if !text.trim().is_empty() => {
                result.push_str(&format!("T{text:?}"))
            }
            _ => {}
        }
    }
    Ok(result)
}

#[derive(Default)]
pub(super) struct Maps {
    fonts: Vec<u32>,
    fills: Vec<u32>,
    borders: Vec<u32>,
    style_xfs: Vec<u32>,
    pub(super) xfs: Vec<u32>,
    pub(super) dxfs: Vec<u32>,
    formats: BTreeMap<u32, u32>,
}
fn index(map: &[u32], text: Option<&str>) -> Result<u32, SaveRefusal> {
    let old = text
        .unwrap_or("0")
        .parse::<usize>()
        .map_err(|_| refused("Invalid template style index"))?;
    map.get(old)
        .copied()
        .ok_or_else(|| refused("A template style index has no source record"))
}
fn remap(bytes: &[u8], collection: &str, maps: &Maps) -> Result<Vec<u8>, SaveRefusal> {
    let xml = XmlPart::new("template-style".into(), bytes.to_vec(), Default::default());
    let mut reader = Reader::new(&xml);
    let mut out = Splice::new(xml.bytes());
    while let Some((event, span)) = reader.next()? {
        let XmlEvent::Start(e) = event else {
            continue;
        };
        let mut changes = Vec::<(String, String)>::new();
        if e.name.is(MAIN_NAMESPACE, "xf") {
            for (name, map) in [
                ("fontId", &maps.fonts),
                ("fillId", &maps.fills),
                ("borderId", &maps.borders),
            ] {
                changes.push((name.into(), index(map, e.attribute(name))?.to_string()));
            }
            if collection == "cellXfs" && e.attribute("xfId").is_some() {
                changes.push((
                    "xfId".into(),
                    index(&maps.style_xfs, e.attribute("xfId"))?.to_string(),
                ));
            }
        }
        if e.name.is(MAIN_NAMESPACE, "cellStyle") {
            changes.push((
                "xfId".into(),
                index(&maps.style_xfs, e.attribute("xfId"))?.to_string(),
            ));
        }
        if let Some(id) = e.attribute("numFmtId") {
            let old = id
                .parse::<u32>()
                .map_err(|_| refused("Invalid number-format ID"))?;
            let mapped = maps
                .formats
                .get(&old)
                .copied()
                .or_else(|| (old < 164).then_some(old))
                .ok_or_else(|| refused("Missing custom number-format record"))?;
            changes.push(("numFmtId".into(), mapped.to_string()));
        }
        if let Some(id) = e.attribute("dxfId") {
            changes.push(("dxfId".into(), index(&maps.dxfs, Some(id))?.to_string()));
        }
        if !changes.is_empty() {
            let changes: Vec<_> = changes
                .iter()
                .map(|(n, v)| (n.as_str(), Some(v.as_str())))
                .collect();
            let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
            out.replace(span, &tag.rebuilt(&changes, tag.is_empty()));
        }
    }
    Ok(out.finish())
}
fn append(xml: &XmlPart, node: &Node, extra: &[u8], count: usize) -> Vec<u8> {
    let tag = RawTag::parse(&xml.bytes()[node.start_tag.0..node.start_tag.1]);
    let count = count.to_string();
    let mut bytes = tag.rebuilt(&[("count", Some(&count))], false);
    bytes.extend_from_slice(&xml.bytes()[node.start_tag.1..node.end_tag.0]);
    bytes.extend_from_slice(extra);
    bytes.extend_from_slice(b"</");
    bytes.extend_from_slice(tag.qualified_name());
    bytes.push(b'>');
    bytes
}
pub(super) fn merge(target: &XmlPart, source: &XmlPart) -> Result<(Vec<u8>, Maps), SaveRefusal> {
    let t = tree(target).map_err(|e| refused(e.to_string()))?;
    let s = tree(source).map_err(|e| refused(e.to_string()))?;
    if !super::template_merge::supported_extensions(&s) {
        return Err(refused("Template style extensions need an explicit merge"));
    }
    let order = [
        "numFmts",
        "fonts",
        "fills",
        "borders",
        "cellStyleXfs",
        "cellXfs",
        "cellStyles",
        "dxfs",
        "tableStyles",
        "colors",
        "extLst",
    ];
    if s.children.iter().any(|n| {
        !order.contains(&n.element.name.local.as_str())
            || n.element.name.namespace.as_deref() != Some(MAIN_NAMESPACE)
    }) {
        return Err(refused("Unknown template style collections"));
    }
    match (child(&t, "colors"), child(&s, "colors")) {
        (None, None) => {}
        (Some(a), Some(b))
            if key(&fragment(target, a, &[&t]))? == key(&fragment(source, b, &[&s]))? => {}
        _ => {
            return Err(refused(
                "The template and workbook use different indexed colour palettes",
            ));
        }
    }
    for attr in ["defaultTableStyle", "defaultPivotStyle"] {
        if child(&t, "tableStyles").and_then(|n| n.element.attribute(attr))
            != child(&s, "tableStyles").and_then(|n| n.element.attribute(attr))
        {
            return Err(refused(
                "Template default table styles need an explicit merge",
            ));
        }
    }
    let mut changes = Vec::new();
    let mut maps = Maps::default();
    // Differential formats precede named table styles in the dependency order.
    for name in [
        "numFmts",
        "fonts",
        "fills",
        "borders",
        "cellStyleXfs",
        "cellXfs",
        "cellStyles",
        "dxfs",
        "tableStyles",
        "colors",
        "extLst",
    ] {
        let Some(src) = child(&s, name) else { continue };
        let dst = child(&t, name);
        if matches!(name, "colors" | "extLst") {
            if name == "colors"
                && let Some(dst) = dst
                && key(&fragment(target, dst, &[&t]))? == key(&fragment(source, src, &[&s]))?
            {
                continue;
            }
            return Err(refused(format!("Template {name} needs an explicit merge")));
        }
        let record_name = match name {
            "numFmts" => "numFmt",
            "fonts" => "font",
            "fills" => "fill",
            "borders" => "border",
            "cellStyleXfs" | "cellXfs" => "xf",
            "cellStyles" => "cellStyle",
            "dxfs" => "dxf",
            "tableStyles" => "tableStyle",
            _ => unreachable!(),
        };
        if src
            .children
            .iter()
            .chain(dst.into_iter().flat_map(|n| &n.children))
            .any(|n| !n.element.name.is(MAIN_NAMESPACE, record_name))
        {
            return Err(refused(format!(
                "Unknown {name} records need an explicit merge"
            )));
        }
        let mut records = dst
            .map(|d| {
                d.children
                    .iter()
                    .map(|n| fragment(target, n, &[&t, d]))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let original = records.len();
        let mut mapping = Vec::new();
        let mut used_formats: BTreeSet<u32> = if name == "numFmts" {
            dst.into_iter()
                .flat_map(|d| &d.children)
                .filter_map(|n| n.element.attribute("numFmtId").and_then(|v| v.parse().ok()))
                .collect()
        } else {
            BTreeSet::new()
        };
        for node in &src.children {
            let raw = fragment(source, node, &[&s, src]);
            let mut bytes = if name == "numFmts" {
                raw
            } else {
                remap(&raw, name, &maps)?
            };
            let old_format = if name == "numFmts" {
                Some(
                    node.element
                        .attribute("numFmtId")
                        .and_then(|v| v.parse::<u32>().ok())
                        .ok_or_else(|| refused("Invalid custom number-format record"))?,
                )
            } else {
                None
            };
            if let Some(old) = old_format {
                if maps.formats.contains_key(&old) {
                    return Err(refused("Repeated template number-format ID"));
                }
                let next = (164..=u32::MAX)
                    .find(|id| !used_formats.contains(id))
                    .ok_or_else(|| refused("No free number-format ID"))?;
                used_formats.insert(next);
                maps.formats.insert(old, next);
                let x = XmlPart::new("format".into(), bytes, Default::default());
                let n = tree(&x).map_err(|e| refused(e.to_string()))?;
                let tag = RawTag::parse(&x.bytes()[n.start_tag.0..n.start_tag.1]);
                let id = next.to_string();
                bytes = tag.rebuilt(&[("numFmtId", Some(&id))], tag.is_empty());
                bytes.extend_from_slice(&x.bytes()[n.start_tag.1..]);
            }
            let signature = key(&bytes)?;
            let existing = records
                .iter()
                .position(|r| key(r).is_ok_and(|k| k == signature));
            let id = if let Some(id) = existing {
                id
            } else {
                if matches!(name, "cellStyles" | "tableStyles")
                    && node.element.attribute("name").is_some_and(|name| {
                        records.iter().any(|r| {
                            let x =
                                XmlPart::new("named-style".into(), r.clone(), Default::default());
                            tree(&x)
                                .ok()
                                .and_then(|n| n.element.attribute("name").map(str::to_owned))
                                .is_some_and(|held| {
                                    calc_doc_model::value::workbook_names_equal(name, &held)
                                })
                        })
                    })
                {
                    return Err(refused(
                        "A named template style conflicts with a workbook style",
                    ));
                }
                records.push(bytes);
                records.len() - 1
            };
            mapping.push(u32::try_from(id).map_err(|_| refused("Too many style records"))?);
        }
        match name {
            "fonts" => maps.fonts = mapping,
            "fills" => maps.fills = mapping,
            "borders" => maps.borders = mapping,
            "cellStyleXfs" => maps.style_xfs = mapping,
            "cellXfs" => maps.xfs = mapping,
            "dxfs" => maps.dxfs = mapping,
            _ => {}
        }
        if records.len() == original {
            continue;
        }
        let extra: Vec<_> = records[original..].iter().flatten().copied().collect();
        if let Some(dst) = dst {
            changes.push((dst.whole(), append(target, dst, &extra, records.len())));
        } else {
            let rank = order.iter().position(|n| *n == name).unwrap();
            let at = t
                .children
                .iter()
                .find(|n| {
                    order
                        .iter()
                        .position(|v| *v == n.element.name.local.as_str())
                        .is_some_and(|r| r > rank)
                })
                .map_or(t.end_tag.0, |n| n.start_tag.0);
            let detached = fragment(source, src, &[&s]);
            let x = XmlPart::new("collection".into(), detached, Default::default());
            let n = tree(&x).map_err(|e| refused(e.to_string()))?;
            let tag = RawTag::parse(&x.bytes()[n.start_tag.0..n.start_tag.1]);
            let count = records.len().to_string();
            let mut bytes = tag.rebuilt(&[("count", Some(&count))], false);
            bytes.extend(extra);
            bytes.extend_from_slice(b"</");
            bytes.extend_from_slice(tag.qualified_name());
            bytes.push(b'>');
            changes.push(((at, at), bytes));
        }
    }
    changes.sort_by_key(|(r, _)| *r);
    let mut out = Splice::new(target.bytes());
    for (span, bytes) in changes {
        out.replace(span, &bytes);
    }
    Ok((out.finish(), maps))
}

pub(super) fn sheet(xml: &XmlPart, maps: &Maps) -> Result<Vec<u8>, SaveRefusal> {
    let default = index(&maps.xfs, None)?;
    let root = tree(xml).map_err(|e| refused(e.to_string()))?;
    if root.children.iter().any(|n| {
        n.element.name.local == "extLst"
            || n.element.name.namespace.as_deref() != Some(MAIN_NAMESPACE)
    }) {
        return Err(refused(
            "Template worksheet extensions need an explicit style merge",
        ));
    }
    let mut reader = Reader::new(xml);
    let mut edits = Vec::new();
    let cols = child(&root, "cols");
    while let Some((event, span)) = reader.next()? {
        let XmlEvent::Start(e) = event else { continue };
        if !e
            .name
            .namespace
            .as_deref()
            .is_some_and(|n| n == MAIN_NAMESPACE)
        {
            continue;
        }
        let mut changes = Vec::new();
        if matches!(e.name.local.as_ref(), "c" | "row")
            && let Some(style) = e.attribute("s")
        {
            changes.push(("s", index(&maps.xfs, Some(style))?.to_string()));
        }
        if e.name.local == "phoneticPr"
            && let Some(font) = e.attribute("fontId")
        {
            changes.push(("fontId", index(&maps.fonts, Some(font))?.to_string()));
        }
        if let Some(style) = e.attribute("dxfId") {
            changes.push(("dxfId", index(&maps.dxfs, Some(style))?.to_string()));
        }
        if default == 0
            && e.name.local == "col"
            && let Some(style) = e.attribute("style")
        {
            changes.push(("style", index(&maps.xfs, Some(style))?.to_string()));
        }
        if !changes.is_empty() {
            let changes: Vec<_> = changes
                .iter()
                .map(|(n, v)| (*n, Some(v.as_str())))
                .collect();
            let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
            edits.push((span, tag.rebuilt(&changes, tag.is_empty())));
        }
    }
    if default != 0 {
        // Every previously implicit style-zero column needs the imported default.
        // Keep source column records, including widths and unknown attributes, intact.
        let mut ranges = Vec::new();
        if let Some(cols) = cols {
            for node in &cols.children {
                if !node.element.name.is(MAIN_NAMESPACE, "col") {
                    return Err(refused("Unknown template column metadata"));
                }
                let first = node
                    .element
                    .attribute("min")
                    .and_then(|v| v.parse::<u32>().ok())
                    .ok_or_else(|| refused("Invalid template column span"))?;
                let last = node
                    .element
                    .attribute("max")
                    .and_then(|v| v.parse::<u32>().ok())
                    .ok_or_else(|| refused("Invalid template column span"))?;
                if first == 0 || first > last || last > 16384 {
                    return Err(refused("Invalid template column span"));
                }
                let style = index(&maps.xfs, node.element.attribute("style"))?.to_string();
                let detached = fragment(xml, node, &[&root, cols]);
                let part = XmlPart::new("column".into(), detached, Default::default());
                let n = tree(&part).map_err(|e| refused(e.to_string()))?;
                let tag = RawTag::parse(&part.bytes()[n.start_tag.0..n.start_tag.1]);
                let mut bytes = tag.rebuilt(&[("style", Some(&style))], tag.is_empty());
                bytes.extend_from_slice(&part.bytes()[n.start_tag.1..]);
                ranges.push((first, last, bytes));
            }
        }
        ranges.sort_by_key(|r| r.0);
        let mut first = 1;
        let mut bytes = format!("<cols xmlns=\"{MAIN_NAMESPACE}\">").into_bytes();
        for (start, end, record) in ranges {
            if start < first {
                return Err(refused(
                    "Overlapping template column records need an explicit merge",
                ));
            }
            if start > first {
                bytes.extend_from_slice(
                    format!(
                        "<col min=\"{first}\" max=\"{}\" style=\"{default}\"/>",
                        start - 1
                    )
                    .as_bytes(),
                );
            }
            bytes.extend(record);
            first = end + 1;
        }
        if first <= 16384 {
            bytes.extend_from_slice(
                format!("<col min=\"{first}\" max=\"16384\" style=\"{default}\"/>").as_bytes(),
            );
        }
        bytes.extend_from_slice(b"</cols>");
        if let Some(cols) = cols {
            edits.push((cols.whole(), bytes));
        } else {
            let at = root
                .children
                .iter()
                .find(|n| n.element.name.is(MAIN_NAMESPACE, "sheetData"))
                .ok_or_else(|| refused("Template worksheet has no sheetData"))?
                .start_tag
                .0;
            edits.push(((at, at), bytes));
        }
    }
    edits.sort_by_key(|(span, _)| *span);
    let mut out = Splice::new(xml.bytes());
    for (span, bytes) in edits {
        out.replace(span, &bytes);
    }
    Ok(out.finish())
}

pub(super) fn strings(xml: &XmlPart, maps: &Maps) -> Result<Vec<u8>, SaveRefusal> {
    let mut reader = Reader::new(xml);
    let mut out = Splice::new(xml.bytes());
    while let Some((event, span)) = reader.next()? {
        if let XmlEvent::Start(e) = event
            && e.name.is(MAIN_NAMESPACE, "phoneticPr")
            && let Some(font) = e.attribute("fontId")
        {
            let font = index(&maps.fonts, Some(font))?.to_string();
            let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
            out.replace(
                span,
                &tag.rebuilt(&[("fontId", Some(&font))], tag.is_empty()),
            );
        }
    }
    Ok(out.finish())
}

pub(super) fn table(xml: &XmlPart, maps: &Maps) -> Result<Vec<u8>, SaveRefusal> {
    if !super::template_merge::supported_extensions(&tree(xml).map_err(|e| refused(e.to_string()))?)
    {
        return Err(refused(
            "Template table extensions need an explicit style merge",
        ));
    }
    let mut reader = Reader::new(xml);
    let mut out = Splice::new(xml.bytes());
    while let Some((event, span)) = reader.next()? {
        let XmlEvent::Start(e) = event else { continue };
        let mut changes = Vec::new();
        for name in [
            "dxfId",
            "headerRowDxfId",
            "dataDxfId",
            "totalsRowDxfId",
            "headerRowBorderDxfId",
            "tableBorderDxfId",
            "totalsRowBorderDxfId",
        ] {
            if let Some(style) = e.attribute(name) {
                changes.push((name, index(&maps.dxfs, Some(style))?.to_string()));
            }
        }
        if !changes.is_empty() {
            let changes: Vec<_> = changes
                .iter()
                .map(|(n, v)| (*n, Some(v.as_str())))
                .collect();
            let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
            out.replace(span, &tag.rebuilt(&changes, tag.is_empty()));
        }
    }
    Ok(out.finish())
}
