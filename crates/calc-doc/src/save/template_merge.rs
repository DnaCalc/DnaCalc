//! Import template worksheets by extending the existing OPC source, retaining every admitted part.
use super::{
    SaveRefusal,
    author::relative_target,
    reader::Reader,
    text::{RawTag, Splice},
};
use crate::{
    load::{
        MAIN_NAMESPACE,
        manifest::{Node, tree},
    },
    opc::{Package, TargetMode},
    xml::{XmlEvent, XmlPart},
};
use std::collections::{BTreeMap, BTreeSet};
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
fn refused(detail: impl Into<String>) -> SaveRefusal {
    SaveRefusal::TemplateNotModelled {
        detail: detail.into(),
        owner: "dc-bctq",
    }
}
fn decoded(text: &str) -> Result<String, SaveRefusal> {
    String::from_utf16(&crate::load::strings::decode(text))
        .map_err(|_| refused("Template references with unpaired UTF-16 need an explicit rename"))
}
fn reference_text(raw: &str, xstring: bool) -> Result<String, SaveRefusal> {
    use calc_fml::syntax::{SyntaxChild, SyntaxKind};
    let text = if xstring {
        decoded(raw)?
    } else {
        raw.to_owned()
    };
    let parsed = calc_fml::parse_formula(&text);
    if !parsed.diagnostics().is_empty() {
        return Err(refused(
            "Template reference syntax needs an explicit rename",
        ));
    }
    let mut nodes = vec![parsed.root()];
    while let Some(node) = nodes.pop() {
        if node.kind() == SyntaxKind::QualifiedReferenceExpr
            && calc_fml::reference::qualified_name(node)
                .is_some_and(|(qualifier, _)| qualifier.contains('['))
        {
            return Err(refused(
                "Template workbook-qualified references need an explicit merge",
            ));
        }
        nodes.extend(node.children().iter().filter_map(|c| {
            if let SyntaxChild::Node(n) = c {
                Some(n)
            } else {
                None
            }
        }));
    }
    Ok(text)
}
fn read(p: &Package, n: &str) -> Result<XmlPart, SaveRefusal> {
    super::strings::read_part(p, n)
}
fn child<'a>(n: &'a Node, name: &str) -> Option<&'a Node> {
    n.children
        .iter()
        .find(|c| c.element.name.is(MAIN_NAMESPACE, name))
}
fn raw<'a>(x: &'a XmlPart, n: &Node) -> &'a [u8] {
    &x.bytes()[n.whole().0..n.whole().1]
}
fn detached(x: &XmlPart, n: &Node, ancestors: &[&Node]) -> Vec<u8> {
    let source = raw(x, n);
    let len = n.start_tag.1 - n.start_tag.0;
    let mut out = source[..len].to_vec();
    for a in ancestors.iter().rev() {
        out = RawTag::parse(&out)
            .with_inherited_namespaces(&RawTag::parse(&x.bytes()[a.start_tag.0..a.start_tag.1]));
    }
    out.extend_from_slice(&source[len..]);
    out
}
fn append(x: &XmlPart, n: &Node, extra: &[u8], attrs: &[(&str, Option<&str>)]) -> Vec<u8> {
    let tag = RawTag::parse(&x.bytes()[n.start_tag.0..n.start_tag.1]);
    let mut out = tag.rebuilt(attrs, false);
    out.extend_from_slice(&x.bytes()[n.start_tag.1..n.end_tag.0]);
    out.extend_from_slice(extra);
    out.extend_from_slice(b"</");
    out.extend_from_slice(tag.qualified_name());
    out.push(b'>');
    out
}
fn shared_strings(
    target: Option<&XmlPart>,
    source: &XmlPart,
) -> Result<(Vec<u8>, u32), SaveRefusal> {
    let s = tree(source).map_err(|e| refused(e.to_string()))?;
    let Some(target) = target else {
        return Ok((source.bytes().to_vec(), 0));
    };
    let t = tree(target).map_err(|e| refused(e.to_string()))?;
    if t.children
        .iter()
        .chain(&s.children)
        .any(|n| !n.element.name.is(MAIN_NAMESPACE, "si"))
    {
        return Err(refused("Unknown shared-string children"));
    }
    let offset = u32::try_from(t.children.len()).map_err(|_| refused("Too many shared strings"))?;
    let extra: Vec<_> = s
        .children
        .iter()
        .flat_map(|n| detached(source, n, &[&s]))
        .collect();
    Ok((
        append(
            target,
            &t,
            &extra,
            &[("count", None), ("uniqueCount", None)],
        ),
        offset,
    ))
}
fn strings_in_sheet(xml: &XmlPart, offset: u32, count: usize) -> Result<Vec<u8>, SaveRefusal> {
    if offset == 0 {
        return Ok(xml.bytes().to_vec());
    }
    let mut reader = Reader::new(xml);
    let mut out = Splice::new(xml.bytes());
    let mut shared = false;
    let mut value = None::<(usize, String)>;
    while let Some((event, span)) = reader.next()? {
        match event {
            XmlEvent::Start(e) if e.name.is(MAIN_NAMESPACE, "c") => {
                shared = e.attribute("t") == Some("s")
            }
            XmlEvent::Start(e) if shared && e.name.is(MAIN_NAMESPACE, "v") => {
                value = Some((span.1, String::new()))
            }
            XmlEvent::Text(s) => {
                if let Some((_, text)) = &mut value {
                    text.push_str(&s);
                }
            }
            XmlEvent::End(e) if e.is(MAIN_NAMESPACE, "v") => {
                if let Some((start, text)) = value.take() {
                    let n = text
                        .trim()
                        .parse::<u32>()
                        .map_err(|_| refused("Invalid template shared-string index"))?;
                    if n as usize >= count {
                        return Err(refused("Template shared-string index is out of range"));
                    }
                    out.replace((start, span.0), (n + offset).to_string().as_bytes());
                }
            }
            XmlEvent::End(e) if e.is(MAIN_NAMESPACE, "c") => shared = false,
            _ => {}
        }
    }
    Ok(out.finish())
}
fn workbook_rel<'a>(
    p: &'a Package,
    workbook: &str,
    suffix: &str,
) -> Option<&'a crate::opc::Relationship> {
    p.relationships_of(Some(workbook))?
        .items()
        .iter()
        .find(|r| r.kind.ends_with(suffix))
}

pub(super) fn supported_extensions(root: &Node) -> bool {
    let mut nodes = vec![root];
    while let Some(node) = nodes.pop() {
        if node.element.name.is(MAIN_NAMESPACE, "extLst") {
            let visibility = crate::load::filter::VISIBILITY;
            if !node.children.iter().all(|e| {
                e.element.name.is(MAIN_NAMESPACE, "ext")
                    && e.element.attribute("uri") == Some(visibility)
                    && e.children.iter().all(|v| {
                        v.element.name.is(visibility, "visibility")
                            && v.children.is_empty()
                            && v.element
                                .attributes
                                .iter()
                                .all(|a| matches!(a.name.local.as_str(), "manual" | "filtered"))
                    })
            }) {
                return false;
            }
        } else {
            nodes.extend(&node.children);
        }
    }
    true
}

pub fn merge(
    target: &crate::LoadedWorkbook,
    source: &crate::LoadedWorkbook,
    first_key: u32,
) -> Result<Vec<u8>, SaveRefusal> {
    let target_snapshot = target.snapshot(target.model());
    let source_snapshot = source.snapshot(source.model());
    let tp = target_snapshot.package();
    let sp = source_snapshot.package();
    if target.has_vba_project()
        || source.has_vba_project()
        || sp.parts().iter().any(|p| {
            p.content_type().is_some_and(|t| {
                t.to_ascii_lowercase().contains("vbaproject")
                    || t.to_ascii_lowercase().contains("macrosheet")
            })
        })
    {
        return Err(refused(
            "Inserting a template VBA project needs project merging; create a workbook from this template instead",
        ));
    }
    if [tp, sp].iter().any(|p| {
        p.parts()
            .iter()
            .any(|part| part.name().starts_with("_xmlsignatures/"))
    }) {
        return Err(refused("Signed packages need an explicit signature policy"));
    }
    if source.manifest_part().is_some() {
        return Err(refused("Template view layouts need a block-identity merge"));
    }
    if target.date_system() != source.date_system() {
        return Err(refused(
            "The template and workbook use different date systems",
        ));
    }
    if target.model().sheets().iter().any(|a| {
        source.model().sheets().iter().any(|b| {
            calc_doc_model::value::workbook_names_equal(
                &a.name().text().to_string_lossy(),
                &b.name().text().to_string_lossy(),
            )
        })
    }) {
        return Err(refused("A template worksheet name is already in use"));
    }
    if target.model().definitions().tables().iter().any(|a| {
        source
            .model()
            .definitions()
            .tables()
            .iter()
            .any(|b| calc_doc_model::value::workbook_names_equal(&a.display_name, &b.display_name))
    }) {
        return Err(refused("A template table name is already in use"));
    }
    let mut data: BTreeMap<String, Vec<u8>> = tp
        .parts()
        .iter()
        .map(|p| {
            tp.read_part(p.name())
                .map(|b| (p.name().to_owned(), b))
                .map_err(|error| SaveRefusal::Part {
                    part: p.name().into(),
                    error,
                })
        })
        .collect::<Result<_, _>>()?;
    let chain = workbook_rel(tp, target.workbook_part(), "/calcChain");
    if let Some(part) = chain.and_then(|r| r.resolved.as_deref()) {
        data.remove(part);
    }
    let tw = read(tp, target.workbook_part())?;
    let sw = read(sp, source.workbook_part())?;
    let tr = tree(&tw).map_err(|e| refused(e.to_string()))?;
    let sr = tree(&sw).map_err(|e| refused(e.to_string()))?;
    if ["externalReferences", "pivotCaches", "extLst"]
        .iter()
        .any(|n| child(&sr, n).is_some())
    {
        return Err(refused(
            "Template workbook references or extensions need an explicit merge",
        ));
    }
    let mut names = Vec::new();
    if let Some(src) = child(&sr, "definedNames") {
        for n in &src.children {
            let index = n
                .element
                .attribute("localSheetId")
                .map(|s| s.parse::<usize>())
                .transpose()
                .map_err(|_| refused("Invalid template name scope"))?;
            if index.is_none()
                && target.model().definitions().names().iter().any(|v| {
                    v.scope == calc_doc_model::NameScope::Workbook
                        && n.element.attribute("name").is_some_and(|s| {
                            decoded(s).is_ok_and(|s| {
                                calc_doc_model::value::workbook_names_equal(&s, &v.name)
                            })
                        })
                })
            {
                return Err(refused("A template defined name is already in use"));
            }
            let mut bytes = detached(&sw, n, &[&sr, src]);
            if let Some(index) = index {
                if index >= source.sheets().len() {
                    return Err(refused("Invalid template name scope"));
                }
                let x = XmlPart::new("template-name".into(), bytes, Default::default());
                let node = tree(&x).map_err(|e| refused(e.to_string()))?;
                let number = (target.sheets().len() + index).to_string();
                let tag = RawTag::parse(&x.bytes()[node.start_tag.0..node.start_tag.1]);
                bytes = tag.rebuilt(&[("localSheetId", Some(&number))], tag.is_empty());
                bytes.extend_from_slice(&x.bytes()[node.start_tag.1..]);
            }
            names.extend(bytes);
        }
    }
    let rels = tp
        .relationships_of(Some(target.workbook_part()))
        .ok_or_else(|| refused("Missing workbook relationships"))?;
    let rel_xml = read(tp, rels.part())?;
    let mut ids: BTreeSet<_> = rels.items().iter().map(|r| r.id.clone()).collect();
    let mut new_rels = Vec::<(String, String, String)>::new();
    let mut overrides = Vec::<(String, String)>::new();
    let fresh_id = |ids: &mut BTreeSet<String>| {
        let id = (1u32..)
            .map(|n| format!("dnaTemplate{n}"))
            .find(|id| !ids.contains(id))
            .unwrap();
        ids.insert(id.clone());
        id
    };
    let prefix = (1u32..)
        .map(|n| format!("xl/dnaTemplate{n}/"))
        .find(|p| tp.parts().iter().all(|part| !part.name().starts_with(p)))
        .unwrap();
    let mut mapping = BTreeMap::from([(
        source.workbook_part().to_owned(),
        target.workbook_part().to_owned(),
    )]);
    let mut pending: Vec<String> = source
        .sheets()
        .iter()
        .map(|s| s.part().name().to_owned())
        .collect();
    let mut style_maps = None;
    for suffix in ["theme", "styles"] {
        if workbook_rel(tp, target.workbook_part(), &format!("/{suffix}")).is_some()
            != workbook_rel(sp, source.workbook_part(), &format!("/{suffix}")).is_some()
        {
            return Err(refused(format!(
                "The template and workbook need compatible {suffix} resources"
            )));
        }
    }
    let mut resources: Vec<_> = sp
        .relationships_of(Some(source.workbook_part()))
        .ok_or_else(|| refused("Missing template relationships"))?
        .items()
        .iter()
        .collect();
    resources.sort_by_key(|r| !r.kind.ends_with("/styles"));
    for r in resources {
        let Some(part) = r.resolved.as_deref() else {
            return Err(refused("External template workbook relationship"));
        };
        if r.kind.ends_with("/worksheet") || r.kind.ends_with("/calcChain") {
            continue;
        }
        let suffix = r.kind.rsplit('/').next().unwrap_or("");
        let existing = workbook_rel(tp, target.workbook_part(), &format!("/{suffix}"));
        if !matches!(
            suffix,
            "styles" | "sharedStrings" | "theme" | "sheetMetadata" | "person"
        ) {
            return Err(refused(format!(
                "Template workbook resource {suffix} needs an explicit merge"
            )));
        }
        if let Some(old) = existing {
            let dest = old
                .resolved
                .as_deref()
                .ok_or_else(|| refused("External target workbook relationship"))?;
            let bytes = match suffix {
                "styles" => {
                    let (bytes, maps) =
                        super::template_styles::merge(&read(tp, dest)?, &read(sp, part)?)?;
                    style_maps = Some(maps);
                    bytes
                }
                "sharedStrings" => {
                    let original = read(sp, part)?;
                    let bytes = if let Some(maps) = &style_maps {
                        super::template_styles::strings(&original, maps)?
                    } else {
                        original.bytes().to_vec()
                    };
                    shared_strings(
                        Some(&read(tp, dest)?),
                        &XmlPart::new(part.into(), bytes, Default::default()),
                    )?
                    .0
                }
                _ => {
                    let a = tp.read_part(dest).map_err(|error| SaveRefusal::Part {
                        part: dest.into(),
                        error,
                    })?;
                    let b = sp.read_part(part).map_err(|error| SaveRefusal::Part {
                        part: part.into(),
                        error,
                    })?;
                    if a != b {
                        return Err(refused(format!(
                            "Different {suffix} resources need a merge"
                        )));
                    }
                    a
                }
            };
            mapping.insert(part.into(), dest.into());
            data.insert(dest.into(), bytes);
        } else {
            let dest = format!("{prefix}{part}");
            new_rels.push((
                fresh_id(&mut ids),
                r.kind.clone(),
                relative_target(target.workbook_part(), &dest),
            ));
            pending.push(part.into());
        }
    }
    let mut copied = BTreeSet::new();
    while let Some(part) = pending.pop() {
        if mapping.contains_key(&part) || !copied.insert(part.clone()) {
            continue;
        }
        if sp.part(&part).is_none() {
            return Err(refused("Missing template resource"));
        }
        mapping.insert(part.clone(), format!("{prefix}{part}"));
        if let Some(rels) = sp.relationships_of(Some(&part)) {
            copied.insert(rels.part().into());
            mapping.insert(rels.part().into(), format!("{prefix}{}", rels.part()));
            for r in rels.items() {
                let kind = r.kind.rsplit('/').next().unwrap_or("");
                if !matches!(
                    kind,
                    "comments"
                        | "threadedComment"
                        | "vmlDrawing"
                        | "drawing"
                        | "table"
                        | "hyperlink"
                        | "printerSettings"
                        | "image"
                        | "customProperty"
                        | "chart"
                        | "chartStyle"
                        | "chartColorStyle"
                        | "styles"
                        | "theme"
                        | "sharedStrings"
                        | "sheetMetadata"
                        | "person"
                ) {
                    return Err(refused(format!(
                        "Template resource relationship {kind} needs an explicit merge"
                    )));
                }
                if r.mode == TargetMode::External && kind != "hyperlink" {
                    return Err(refused(
                        "External template resource links need an explicit merge",
                    ));
                }
                if r.mode == TargetMode::Internal {
                    pending.push(
                        r.resolved
                            .clone()
                            .ok_or_else(|| refused("Unresolved template package relationship"))?,
                    );
                }
            }
        }
    }
    let offset = match (
        workbook_rel(tp, target.workbook_part(), "/sharedStrings"),
        workbook_rel(sp, source.workbook_part(), "/sharedStrings"),
    ) {
        (Some(a), Some(b)) => {
            shared_strings(
                Some(&read(tp, a.resolved.as_ref().unwrap())?),
                &read(sp, b.resolved.as_ref().unwrap())?,
            )?
            .1
        }
        _ => 0,
    };
    let count = workbook_rel(sp, source.workbook_part(), "/sharedStrings")
        .map(|r| {
            read(sp, r.resolved.as_ref().unwrap()).and_then(|x| {
                tree(&x)
                    .map(|n| n.children.len())
                    .map_err(|e| refused(e.to_string()))
            })
        })
        .transpose()?
        .unwrap_or(0);
    let mut table_id = target
        .model()
        .definitions()
        .tables()
        .iter()
        .map(|t| t.id)
        .max()
        .unwrap_or(0);
    for part in copied {
        let dest = mapping
            .get(&part)
            .ok_or_else(|| refused("Unmapped template resource"))?
            .clone();
        let p = sp
            .part(&part)
            .ok_or_else(|| refused("Missing template part"))?;
        let mut bytes = sp.read_part(&part).map_err(|error| SaveRefusal::Part {
            part: part.clone(),
            error,
        })?;
        if workbook_rel(sp, source.workbook_part(), "/sharedStrings")
            .and_then(|r| r.resolved.as_deref())
            == Some(part.as_str())
            && let Some(maps) = &style_maps
        {
            bytes = super::template_styles::strings(&read(sp, &part)?, maps)?;
        }
        if source.sheets().iter().any(|s| s.part().name() == part) {
            let xml = read(sp, &part)?;
            let styled = if let Some(maps) = &style_maps {
                super::template_styles::sheet(&xml, maps)?
            } else {
                xml.bytes().to_vec()
            };
            let xml = XmlPart::new(part.clone(), styled, Default::default());
            bytes = strings_in_sheet(&xml, offset, count)?;
        }
        if source
            .model()
            .definitions()
            .tables()
            .iter()
            .any(|t| t.part == part)
        {
            table_id = table_id
                .checked_add(1)
                .ok_or_else(|| refused("No free table ID"))?;
            let xml = XmlPart::new(part.clone(), bytes, Default::default());
            let node = tree(&xml).map_err(|e| refused(e.to_string()))?;
            let tag = RawTag::parse(&xml.bytes()[node.start_tag.0..node.start_tag.1]);
            let id = table_id.to_string();
            bytes = tag.rebuilt(&[("id", Some(&id))], tag.is_empty());
            bytes.extend_from_slice(&xml.bytes()[node.start_tag.1..]);
            if let Some(maps) = &style_maps {
                bytes = super::template_styles::table(
                    &XmlPart::new(part.clone(), bytes, Default::default()),
                    maps,
                )?;
            }
        }
        if p.content_type() == Some(super::manifest::RELATIONSHIPS_CONTENT_TYPE) {
            let x = read(sp, &part)?;
            let mut reader = Reader::new(&x);
            let mut out = Splice::new(x.bytes());
            let owner = sp
                .relationships()
                .iter()
                .find(|r| r.part() == part)
                .ok_or_else(|| refused("Missing relationship owner"))?;
            let original_owner = owner.source().ok_or_else(|| {
                refused("A package relationship cannot be copied as a worksheet resource")
            })?;
            let mapped_owner = mapping
                .get(original_owner)
                .ok_or_else(|| refused("Unmapped relationship owner"))?;
            while let Some((e, span)) = reader.next()? {
                if let XmlEvent::Start(e) = e {
                    if e.name.local == "Relationship"
                        && let Some(r) = e
                            .attribute("Id")
                            .and_then(|id| owner.items().iter().find(|r| r.id == id))
                        && r.mode == TargetMode::Internal
                    {
                        let target = mapping
                            .get(r.resolved.as_ref().unwrap())
                            .ok_or_else(|| refused("Unmapped template relationship target"))?;
                        let target = relative_target(mapped_owner, target);
                        let tag = RawTag::parse(&x.bytes()[span.0..span.1]);
                        out.replace(
                            span,
                            &tag.rebuilt(&[("Target", Some(&target))], tag.is_empty()),
                        );
                    }
                }
            }
            bytes = out.finish();
        }
        let kind = p
            .content_type()
            .ok_or_else(|| refused("A template resource has no content type"))?;
        overrides.push((dest.clone(), kind.into()));
        data.insert(dest, bytes);
    }
    let ts = child(&tr, "sheets").ok_or_else(|| refused("Missing target sheets"))?;
    let ss = child(&sr, "sheets").ok_or_else(|| refused("Missing template sheets"))?;
    let mut extra = Vec::new();
    for (i, (node, sheet)) in ss.children.iter().zip(source.sheets()).enumerate() {
        let id = fresh_id(&mut ids);
        let mapped = mapping
            .get(sheet.part().name())
            .ok_or_else(|| refused("Unmapped template worksheet"))?;
        new_rels.push((
            id.clone(),
            format!("{REL}/worksheet"),
            relative_target(target.workbook_part(), mapped),
        ));
        let bytes = detached(&sw, node, &[&sr, ss]);
        let x = XmlPart::new("template-sheet".into(), bytes, Default::default());
        let n = tree(&x).map_err(|e| refused(e.to_string()))?;
        let tag = RawTag::parse(&x.bytes()[n.start_tag.0..n.start_tag.1]);
        let old = tag
            .attribute_names()
            .find(|name| name.ends_with(b":id") || *name == b"id")
            .ok_or_else(|| refused("Missing template sheet relationship"))?;
        let old =
            std::str::from_utf8(old).map_err(|_| refused("Invalid relationship attribute"))?;
        let key = first_key
            .checked_add(i as u32)
            .ok_or_else(|| refused("Too many worksheet keys"))?
            .to_string();
        extra.extend(tag.rebuilt(
            &[
                (old, None),
                ("xmlns:dnaTemplate", Some(REL)),
                ("dnaTemplate:id", Some(&id)),
                ("sheetId", Some(&key)),
            ],
            true,
        ));
    }
    let mut out = Splice::new(tw.bytes());
    out.replace(ts.whole(), &append(&tw, ts, &extra, &[]));
    if !names.is_empty() {
        if let Some(n) = child(&tr, "definedNames") {
            out.replace(n.whole(), &append(&tw, n, &names, &[]));
        } else {
            let after_names = [
                "calcPr",
                "oleSize",
                "customWorkbookViews",
                "pivotCaches",
                "smartTagPr",
                "smartTagTypes",
                "webPublishing",
                "fileRecoveryPr",
                "webPublishObjects",
                "extLst",
            ];
            let at = tr
                .children
                .iter()
                .find(|n| after_names.contains(&n.element.name.local.as_str()))
                .map_or(tr.end_tag.0, |n| n.start_tag.0);
            let mut bytes = format!("<definedNames xmlns=\"{MAIN_NAMESPACE}\">").into_bytes();
            bytes.extend(names);
            bytes.extend_from_slice(b"</definedNames>");
            out.replace((at, at), &bytes);
        }
    }
    data.insert(target.workbook_part().into(), out.finish());
    let add: Vec<_> = new_rels
        .iter()
        .map(|(id, kind, target)| super::parts::NewRelationship { id, kind, target })
        .collect();
    data.insert(
        rels.part().into(),
        super::parts::relationships(&rel_xml, chain.map(|r| r.kind.as_str()), &add)?,
    );
    let types = tp.content_types_xml().map_err(|error| SaveRefusal::Part {
        part: crate::opc::CONTENT_TYPES_ENTRY.into(),
        error,
    })?;
    let add: Vec<_> = overrides
        .iter()
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .collect();
    data.insert(
        crate::opc::CONTENT_TYPES_ENTRY.into(),
        super::parts::content_types(&types, chain.and_then(|r| r.resolved.as_deref()), &add)?,
    );
    let entries: Vec<_> = data
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    super::new_archive(&entries)
        .ok_or_else(|| refused("Merged template package exceeds ZIP bounds"))
}

/// Rename a template worksheet and its admitted package references before insertion. Formula
/// text uses the host's existing rewriter; raw cell and resource records keep their caches.
pub fn rename_sheet(
    source: &crate::LoadedWorkbook,
    sheet: calc_doc_model::value::SheetId,
    name: calc_doc_model::SheetName,
    rewriter: &dyn calc_doc_model::FormulaRewriter,
) -> Result<Vec<u8>, SaveRefusal> {
    use calc_doc_model::{NameScope, StructuralChange};
    let root = source.model();
    let entry = root
        .sheet_by_id(sheet)
        .ok_or_else(|| refused("Missing template worksheet"))?;
    if source.has_vba_project() || source.manifest_part().is_some() {
        return Err(refused(
            "Template VBA or view identities need an explicit rename",
        ));
    }
    if source.sheets().iter().any(|s| s.charts().unread != 0) {
        return Err(refused(
            "Unread template charts need an explicit reference rename",
        ));
    }
    let change = StructuralChange::RenameSheet {
        sheet,
        name: name.clone(),
    };
    let snapshot = source.snapshot(root);
    let package = snapshot.package();
    let mut data = Vec::<(String, Vec<u8>)>::new();
    for part in package.parts() {
        let mut bytes = package
            .read_part(part.name())
            .map_err(|error| SaveRefusal::Part {
                part: part.name().into(),
                error,
            })?;
        let owner = source
            .sheets()
            .iter()
            .find(|s| s.part().name() == part.name());
        let kind = part.content_type().unwrap_or("");
        let workbook = part.name() == source.workbook_part();
        let formula_part = owner.is_some()
            || workbook
            || kind.ends_with("drawingml.chart+xml")
            || kind.ends_with("spreadsheetml.table+xml");
        if formula_part {
            let xml = XmlPart::new(part.name().into(), bytes, Default::default());
            let tree = tree(&xml).map_err(|e| refused(e.to_string()))?;
            if workbook
                && tree.children.iter().any(|n| {
                    matches!(
                        n.element.name.local.as_str(),
                        "externalReferences" | "pivotCaches" | "extLst"
                    )
                })
            {
                return Err(refused(
                    "Template workbook references or extensions need an explicit rename",
                ));
            }
            if owner.is_some()
                && tree.children.iter().any(|n| {
                    matches!(
                        n.element.name.local.as_str(),
                        "extLst"
                            | "scenarios"
                            | "dataConsolidate"
                            | "customSheetViews"
                            | "cellWatches"
                            | "oleObjects"
                            | "controls"
                            | "smartTags"
                            | "webPublishItems"
                    )
                })
            {
                return Err(refused(
                    "Template worksheet extensions or objects need an explicit rename",
                ));
            }
            if !supported_extensions(&tree) {
                return Err(refused(
                    "Template extensions need an explicit reference rename",
                ));
            }
            let scope = owner.map_or(NameScope::Workbook, |s| NameScope::Sheet(s.id()));
            let mut reader = Reader::new(&xml);
            let mut edits = Vec::<((usize, usize), Vec<u8>)>::new();
            let mut content = None::<(usize, String, NameScope, String, bool)>;
            while let Some((event, span)) = reader.next()? {
                match event {
                    XmlEvent::Start(e) => {
                        let mut attrs = Vec::<(&str, String)>::new();
                        if workbook
                            && e.name.is(MAIN_NAMESPACE, "sheet")
                            && e.attribute("sheetId").and_then(|v| v.parse::<u32>().ok())
                                == Some(entry.key().get())
                        {
                            let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
                            edits.push((
                                span,
                                tag.rebuilt_xstring(
                                    "name",
                                    name.text().utf16_code_units(),
                                    tag.is_empty(),
                                ),
                            ));
                        }
                        if e.name.is(MAIN_NAMESPACE, "hyperlink")
                            && let Some(location) = e.attribute("location")
                            && let Some(value) = rewriter.name(
                                root,
                                &reference_text(location, true)?,
                                scope,
                                &change,
                            )
                        {
                            attrs.push(("location", value));
                        }
                        if e.name.is(MAIN_NAMESPACE, "cfvo")
                            && e.attribute("type") == Some("formula")
                            && let Some(formula) = e.attribute("val")
                            && let Some(value) =
                                rewriter.name(root, &reference_text(formula, true)?, scope, &change)
                        {
                            attrs.push(("val", value));
                        }
                        if !attrs.is_empty() {
                            let mut bytes = xml.bytes()[span.0..span.1].to_vec();
                            for (name, value) in attrs {
                                let tag = RawTag::parse(&bytes);
                                bytes = tag.rebuilt_xstring(
                                    name,
                                    &value.encode_utf16().collect::<Vec<_>>(),
                                    tag.is_empty(),
                                );
                            }
                            edits.push((span, bytes));
                        }
                        let formula = (e.name.namespace.as_deref() == Some(MAIN_NAMESPACE)
                            && matches!(
                                e.name.local.as_str(),
                                "f" | "formula"
                                    | "formula1"
                                    | "formula2"
                                    | "definedName"
                                    | "calculatedColumnFormula"
                                    | "totalsRowFormula"
                            ))
                            || (e.name.namespace.as_deref()
                                == Some("http://schemas.openxmlformats.org/drawingml/2006/chart")
                                && e.name.local == "f");
                        if formula {
                            let name_scope = if e.name.local == "definedName" {
                                e.attribute("localSheetId")
                                    .and_then(|v| v.parse::<usize>().ok())
                                    .and_then(|i| root.sheets().get(i))
                                    .map_or(NameScope::Workbook, |s| NameScope::Sheet(s.id()))
                            } else {
                                scope
                            };
                            content = Some((
                                span.1,
                                String::new(),
                                name_scope,
                                e.name.local.to_string(),
                                e.name.namespace.as_deref() == Some(MAIN_NAMESPACE),
                            ));
                        }
                    }
                    XmlEvent::Text(text) => {
                        if let Some((_, value, _, _, _)) = &mut content {
                            value.push_str(&text);
                        }
                    }
                    XmlEvent::End(e) => {
                        if content
                            .as_ref()
                            .is_some_and(|(_, _, _, local, _)| local.as_str() == e.local.as_str())
                            && let Some((start, text, scope, _, xstring)) = content.take()
                            && !text.is_empty()
                            && let Some(value) = rewriter.name(
                                root,
                                &reference_text(&text, xstring)?,
                                scope,
                                &change,
                            )
                        {
                            let mut bytes = Vec::new();
                            if xstring {
                                super::text::xstring(
                                    &value.encode_utf16().collect::<Vec<_>>(),
                                    &mut bytes,
                                );
                            } else {
                                bytes.extend_from_slice(
                                    quick_xml::escape::escape(&value).as_bytes(),
                                );
                            }
                            edits.push(((start, span.0), bytes));
                        }
                    }
                }
            }
            edits.sort_by_key(|(r, _)| *r);
            let mut out = Splice::new(xml.bytes());
            for (span, bytes) in edits {
                out.replace(span, &bytes);
            }
            bytes = out.finish();
        }
        data.push((part.name().into(), bytes));
    }
    data.push((
        crate::opc::CONTENT_TYPES_ENTRY.into(),
        package
            .content_types_xml()
            .map_err(|error| SaveRefusal::Part {
                part: crate::opc::CONTENT_TYPES_ENTRY.into(),
                error,
            })?
            .bytes()
            .to_vec(),
    ));
    let entries: Vec<_> = data
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    super::new_archive(&entries).ok_or_else(|| refused("Renamed template exceeds ZIP bounds"))
}
