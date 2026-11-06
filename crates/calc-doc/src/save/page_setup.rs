//! Source-preserving edits of worksheet print elements, in SpreadsheetML schema order.
use super::{
    SaveRefusal,
    reader::Reader,
    text::{RawTag, xstring},
};
use crate::{
    load::MAIN_NAMESPACE,
    xml::{XmlElement, XmlEvent, XmlPart},
};
use calc_doc_model::{HEADER_FIELDS, PAGE_SECTIONS, PageSetup};
use std::collections::BTreeMap;

fn invalid(detail: impl Into<String>) -> SaveRefusal {
    SaveRefusal::InvalidPart {
        part: "page setup (dc-971p.56)".into(),
        detail: detail.into(),
    }
}
struct Node {
    element: XmlElement,
    start: (usize, usize),
    end: usize,
    closing: usize,
    children: Vec<Node>,
}
fn node(
    reader: &mut Reader<'_>,
    element: XmlElement,
    start: (usize, usize),
) -> Result<Node, SaveRefusal> {
    let container = element.name.namespace.as_deref() == Some(MAIN_NAMESPACE)
        && [
            "worksheet",
            "sheetPr",
            "headerFooter",
            "rowBreaks",
            "colBreaks",
        ]
        .contains(&element.name.local.as_str());
    if !container {
        let mut depth = 1;
        loop {
            match reader.expect()? {
                (XmlEvent::Start(_), _) => depth += 1,
                (XmlEvent::End(_), (closing, end)) => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(Node {
                            element,
                            start,
                            end,
                            closing,
                            children: Vec::new(),
                        });
                    }
                }
                _ => {}
            }
        }
    }
    let mut children = Vec::new();
    loop {
        match reader.expect()? {
            (XmlEvent::Start(e), span) => children.push(node(reader, e, span)?),
            (XmlEvent::End(_), (closing, end)) => {
                return Ok(Node {
                    element,
                    start,
                    end,
                    closing,
                    children,
                });
            }
            _ => {}
        }
    }
}
fn rank(parent: &str, name: &str) -> usize {
    let order: &[&str] = match parent {
        "worksheet" => &[
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
            "autoFilter",
            "sortState",
            "dataConsolidate",
            "customSheetViews",
            "mergeCells",
            "phoneticPr",
            "conditionalFormatting",
            "dataValidations",
            "hyperlinks",
            "printOptions",
            "pageMargins",
            "pageSetup",
            "headerFooter",
            "rowBreaks",
            "colBreaks",
            "customProperties",
            "cellWatches",
            "ignoredErrors",
            "smartTags",
            "drawing",
            "legacyDrawing",
            "legacyDrawingHF",
            "picture",
            "oleObjects",
            "controls",
            "webPublishItems",
            "tableParts",
            "extLst",
        ],
        "sheetPr" => &["tabColor", "outlinePr", "pageSetUpPr"],
        "headerFooter" => HEADER_FIELDS,
        _ => &["brk"],
    };
    order.iter().position(|n| *n == name).unwrap_or(usize::MAX)
}
fn render(
    xml: &XmlPart,
    source: Option<&Node>,
    prefix: &str,
    name: &str,
    attrs: &[(&str, Option<String>)],
    mut edits: BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, SaveRefusal> {
    let attrs = attrs
        .iter()
        .map(|(k, v)| (*k, v.as_deref()))
        .collect::<Vec<_>>();
    let fresh = format!("<{prefix}{name}/>");
    let raw = source
        .map(|n| &xml.bytes()[n.start.0..n.start.1])
        .unwrap_or(fresh.as_bytes());
    let tag = RawTag::parse(raw);
    let mut body = Vec::new();
    if let Some(source) = source {
        let mut cursor = source.start.1;
        let mut seen = std::collections::BTreeSet::new();
        for child in &source.children {
            body.extend_from_slice(&xml.bytes()[cursor..child.start.0]);
            let local = child.element.name.local.as_str();
            let main = child.element.name.namespace.as_deref() == Some(MAIN_NAMESPACE);
            let mut before = edits
                .keys()
                .filter(|k| {
                    rank(name, k) < rank(name, local)
                        && !source
                            .children
                            .iter()
                            .any(|c| c.element.name.is(MAIN_NAMESPACE, k))
                })
                .cloned()
                .collect::<Vec<_>>();
            before.sort_by_key(|k| rank(name, k));
            for key in before {
                body.extend(edits.remove(&key).unwrap());
            }
            if main && let Some(bytes) = edits.get(local) {
                if !seen.insert(local.to_owned()) {
                    return Err(invalid(format!(
                        "Duplicate {local} elements cannot be edited"
                    )));
                }
                body.extend_from_slice(bytes);
            } else {
                body.extend_from_slice(&xml.bytes()[child.start.0..child.end]);
            }
            cursor = child.end;
        }
        if !tag.is_empty() {
            body.extend_from_slice(&xml.bytes()[cursor..source.closing]);
        }
        for key in seen {
            edits.remove(&key);
        }
    }
    let mut rest = edits.into_iter().collect::<Vec<_>>();
    rest.sort_by_key(|(k, _)| rank(name, k));
    for (_, bytes) in rest {
        body.extend(bytes);
    }
    let mut out = tag.rebuilt(&attrs, body.is_empty());
    if !body.is_empty() {
        out.extend(body);
        out.extend_from_slice(
            format!("</{}>", String::from_utf8_lossy(tag.qualified_name())).as_bytes(),
        );
    }
    Ok(out)
}
fn child<'a>(node: Option<&'a Node>, name: &str) -> Result<Option<&'a Node>, SaveRefusal> {
    let found = node
        .into_iter()
        .flat_map(|n| &n.children)
        .filter(|n| n.element.name.is(MAIN_NAMESPACE, name))
        .collect::<Vec<_>>();
    if found.len() > 1 {
        return Err(invalid(format!(
            "Duplicate {name} elements cannot be edited"
        )));
    }
    Ok(found.first().copied())
}
fn attributes(setup: &PageSetup, section: &str) -> Vec<(&'static str, Option<String>)> {
    PAGE_SECTIONS
        .iter()
        .find(|(s, _)| *s == section)
        .unwrap()
        .1
        .iter()
        .map(|name| (*name, setup.get(section, name).map(str::to_owned)))
        .collect()
}
pub(crate) fn rewrite(
    xml: &XmlPart,
    old: &PageSetup,
    new: &PageSetup,
) -> Result<Vec<u8>, SaveRefusal> {
    new.validate().map_err(invalid)?;
    let mut reader = Reader::new(xml);
    let (element, start) = reader.root("worksheet")?;
    let root = node(&mut reader, element, start)?;
    reader.end()?;
    let root_tag = RawTag::parse(&xml.bytes()[root.start.0..root.start.1]);
    let prefix = String::from_utf8_lossy(root_tag.prefix());
    let mut edits = BTreeMap::new();
    for (section, _) in PAGE_SECTIONS {
        if old.sections.get(*section) == new.sections.get(*section)
            && !(*section == "headerFooter" && old.headers != new.headers)
        {
            continue;
        }
        let container = if *section == "pageSetUpPr" {
            child(Some(&root), "sheetPr")?
        } else {
            Some(&root)
        };
        let source = child(container, section)?;
        let local_prefix = source
            .or(container)
            .map(|n| {
                String::from_utf8_lossy(RawTag::parse(&xml.bytes()[n.start.0..n.start.1]).prefix())
                    .into_owned()
            })
            .unwrap_or_else(|| prefix.to_string());
        let mut nested = BTreeMap::new();
        if *section == "headerFooter" {
            for field in HEADER_FIELDS {
                if old.headers.get(*field) == new.headers.get(*field) {
                    continue;
                }
                let mut bytes = Vec::new();
                if let Some(value) = new.headers.get(*field) {
                    bytes.extend_from_slice(format!("<{local_prefix}{field}>").as_bytes());
                    xstring(value.utf16_code_units(), &mut bytes);
                    bytes.extend_from_slice(format!("</{local_prefix}{field}>").as_bytes());
                }
                nested.insert((*field).into(), bytes);
            }
        }
        let bytes = render(
            xml,
            source,
            &local_prefix,
            section,
            &attributes(new, section),
            nested,
        )?;
        if *section == "pageSetUpPr" {
            edits.insert(
                "sheetPr".into(),
                render(
                    xml,
                    container,
                    &prefix,
                    "sheetPr",
                    &[],
                    BTreeMap::from([("pageSetUpPr".into(), bytes)]),
                )?,
            );
        } else {
            edits.insert((*section).into(), bytes);
        }
    }
    for (section, a, b) in [
        ("rowBreaks", &old.row_breaks, &new.row_breaks),
        ("colBreaks", &old.column_breaks, &new.column_breaks),
    ] {
        if a == b {
            continue;
        }
        let source = child(Some(&root), section)?;
        let break_prefix = source
            .map(|n| {
                String::from_utf8_lossy(RawTag::parse(&xml.bytes()[n.start.0..n.start.1]).prefix())
                    .into_owned()
            })
            .unwrap_or_else(|| prefix.to_string());
        let mut bytes = Vec::new();
        for brk in b {
            let original = source.and_then(|s| {
                s.children.iter().find(|n| {
                    n.element.name.is(MAIN_NAMESPACE, "brk")
                        && n.element
                            .attribute("id")
                            .and_then(|v| v.parse::<u32>().ok())
                            == Some(brk.id)
                })
            });
            let attrs = [
                ("id", Some(brk.id.to_string())),
                ("min", Some(brk.min.to_string())),
                ("max", Some(brk.max.to_string())),
                ("man", Some(u8::from(brk.manual).to_string())),
                ("pt", Some(u8::from(brk.pivot).to_string())),
            ];
            bytes.extend(render(
                xml,
                original,
                &break_prefix,
                "brk",
                &attrs,
                BTreeMap::new(),
            )?);
        }
        // Keep unrelated extension children and container attributes.
        let fresh = format!("<{prefix}{section}/>");
        let raw = source
            .map(|n| &xml.bytes()[n.start.0..n.start.1])
            .unwrap_or(fresh.as_bytes());
        let tag = RawTag::parse(raw);
        let count = b.len().to_string();
        let manual = b.iter().filter(|b| b.manual).count().to_string();
        let mut container = tag.rebuilt(
            &[("count", Some(&count)), ("manualBreakCount", Some(&manual))],
            false,
        );
        container.extend(bytes);
        for other in source
            .into_iter()
            .flat_map(|s| &s.children)
            .filter(|n| !n.element.name.is(MAIN_NAMESPACE, "brk"))
        {
            container.extend_from_slice(&xml.bytes()[other.start.0..other.end]);
        }
        container.extend_from_slice(
            format!("</{}>", String::from_utf8_lossy(tag.qualified_name())).as_bytes(),
        );
        edits.insert(section.into(), container);
    }
    let mut out = xml.bytes()[..root.start.0].to_vec();
    out.extend(render(xml, Some(&root), &prefix, "worksheet", &[], edits)?);
    out.extend_from_slice(&xml.bytes()[root.end..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn xml(text: &str) -> XmlPart {
        XmlPart::new(
            "sheet.xml".into(),
            text.as_bytes().to_vec(),
            Default::default(),
        )
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn page_setup_edits_preserve_other_printer_fields_namespaces_and_headers() {
        let source = xml(
            r#"<s:worksheet xmlns:s="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:z="urn:test"><s:sheetPr codeName="SheetCode"><s:outlinePr summaryBelow="0"/><s:pageSetUpPr fitToPage="1" z:extra="keep"/></s:sheetPr><s:sheetData/><s:pageMargins left="0.7000" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><s:pageSetup paperSize="9" orientation="landscape" fitToWidth="1" fitToHeight="0" horizontalDpi="600" r:id="rPrint" z:extra="keep"><z:child/></s:pageSetup><s:headerFooter differentFirst="1"><s:oddHeader>&amp;LLeft&amp;CPage &amp;P of &amp;N</s:oddHeader><s:firstFooter>_x005F_x0041_</s:firstFooter><z:child/></s:headerFooter><s:rowBreaks count="1" manualBreakCount="1"><s:brk id="20" max="16383" man="1" z:keep="yes"/></s:rowBreaks><s:drawing r:id="chart"/></s:worksheet>"#,
        );
        let old = crate::load::page_setup::read(&source).unwrap();
        assert_eq!(old.get("pageMargins", "left"), Some("0.7000"));
        assert_eq!(old.row_breaks[0].id, 20);
        let mut new = old.clone();
        new.set("pageSetup", "orientation", Some("portrait".into()))
            .unwrap();
        new.set("pageSetup", "scale", Some("75".into())).unwrap();
        new.set("pageSetUpPr", "fitToPage", Some("0".into()))
            .unwrap();
        new.set("printOptions", "gridLines", Some("1".into()))
            .unwrap();
        new.headers.insert(
            "evenFooter".into(),
            calc_doc_model::value::ExcelText::from_utf16_code_units(
                "&R<&P>".encode_utf16().collect(),
            ),
        );
        new.row_breaks.push(calc_doc_model::PageBreak {
            id: 40,
            min: 0,
            max: 16383,
            manual: true,
            pivot: false,
        });
        let bytes = rewrite(&source, &old, &new).unwrap();
        let saved = source.with_bytes(bytes.clone());
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(crate::load::page_setup::read(&saved).unwrap(), new);
        for retained in [
            "codeName=\"SheetCode\"",
            "summaryBelow=\"0\"",
            "r:id=\"rPrint\"",
            "z:extra=\"keep\"",
            "z:keep=\"yes\"",
            "<z:child/>",
            "<s:drawing r:id=\"chart\"/>",
            "left=\"0.7000\"",
        ] {
            assert!(text.contains(retained), "Missing {retained}: {text}");
        }
        assert!(text.find("printOptions").unwrap() < text.find("pageMargins").unwrap());
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn page_setup_insertions_follow_schema_order_and_unchanged_source_is_readable() {
        let source = xml(
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData/><drawing xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="r1"/></worksheet>"#,
        );
        let old = PageSetup::default();
        let mut new = old.clone();
        for (section, name, value) in [
            ("pageSetUpPr", "fitToPage", "1"),
            ("pageSetup", "paperSize", "9"),
            ("pageMargins", "left", "0.5"),
            ("printOptions", "headings", "1"),
            ("headerFooter", "differentFirst", "1"),
        ] {
            new.set(section, name, Some(value.into())).unwrap();
        }
        new.headers.insert(
            "oddHeader".into(),
            calc_doc_model::value::ExcelText::from_utf16_code_units(
                "&CAnnual report".encode_utf16().collect(),
            ),
        );
        let bytes = rewrite(&source, &old, &new).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        let names = [
            "sheetPr",
            "sheetData",
            "printOptions",
            "pageMargins",
            "pageSetup",
            "headerFooter",
            "drawing",
        ];
        assert!(
            names
                .windows(2)
                .all(|p| text.find(&format!("<{}", p[0])) < text.find(&format!("<{}", p[1]))),
            "{text}"
        );
        assert_eq!(
            crate::load::page_setup::read(&source.with_bytes(bytes)).unwrap(),
            new
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn page_setup_insertions_use_the_parent_namespace_when_the_root_prefix_is_shadowed() {
        let source = xml(
            r#"<s:worksheet xmlns:s="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><p:sheetPr xmlns:p="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:s="urn:extension"><p:outlinePr summaryBelow="0"/></p:sheetPr><s:sheetData/><p:rowBreaks xmlns:p="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:s="urn:extension" count="0"/><p:colBreaks xmlns:p="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:s="urn:extension" count="0"/></s:worksheet>"#,
        );
        let old = crate::load::page_setup::read(&source).unwrap();
        let mut new = old.clone();
        new.set("pageSetUpPr", "fitToPage", Some("1".into()))
            .unwrap();
        new.row_breaks.push(calc_doc_model::PageBreak {
            id: 20,
            min: 0,
            max: 16383,
            manual: true,
            pivot: false,
        });
        new.column_breaks.push(calc_doc_model::PageBreak {
            id: 4,
            min: 0,
            max: 1048575,
            manual: true,
            pivot: false,
        });
        let bytes = rewrite(&source, &old, &new).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(text.contains("<p:pageSetUpPr"));
        assert_eq!(text.matches("<p:brk ").count(), 2);
        assert_eq!(
            crate::load::page_setup::read(&source.with_bytes(bytes)).unwrap(),
            new
        );
    }
}
