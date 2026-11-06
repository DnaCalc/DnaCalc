//! Worksheet lifecycle on the existing package writer. Existing sheet parts keep their identity.
use super::{
    SaveRefusal,
    author::Changes,
    parts::{self, NewRelationship},
    reader::Reader,
    strings::read_part,
    text::{RawTag, Splice, attribute_value, xstring_attribute},
};
use crate::{
    load::{LoadedSheet, MAIN_NAMESPACE},
    opc::Package,
    xml::{XmlEvent, XmlLimits, XmlPart},
};
use calc_doc_model::{ModelRoot, SheetVisibility};
use std::borrow::Cow;

const WORKSHEET: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
const CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
const EMPTY: &[u8] = b"<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData/></worksheet>";

pub(super) fn expanded<'a>(
    package: &Package,
    workbook: &str,
    loaded: &'a [LoadedSheet],
    model: &ModelRoot,
) -> Vec<Cow<'a, LoadedSheet>> {
    let mut sheets: Vec<_> = loaded.iter().map(Cow::Borrowed).collect();
    let parent = workbook
        .rsplit_once('/')
        .map_or(String::new(), |(parent, _)| format!("{parent}/"));
    for entry in model.sheets() {
        if sheets.iter().any(|s| s.id() == entry.id()) {
            continue;
        }
        let part = (1u32..)
            .map(|n| format!("{parent}worksheets/dnaSheet{n}.xml"))
            .find(|name| {
                package.part(name).is_none() && sheets.iter().all(|s| s.part().name() != name)
            })
            .expect("a free part");
        let relationship = (1u32..)
            .map(|n| format!("dnaWorksheet{n}"))
            .find(|id| {
                package
                    .relationships_of(Some(workbook))
                    .is_none_or(|rels| rels.items().iter().all(|r| &r.id != id))
                    && sheets.iter().all(|s| s.relationship() != id)
            })
            .expect("a free relationship");
        sheets.push(Cow::Owned(LoadedSheet::empty(
            entry.id(),
            relationship,
            part,
        )));
    }
    sheets
}

pub(crate) fn read_sheet(package: &Package, name: &str) -> Result<XmlPart, SaveRefusal> {
    if package.part(name).is_some() {
        read_part(package, name)
    } else {
        Ok(XmlPart::new(
            name.to_owned(),
            EMPTY.to_vec(),
            XmlLimits::default(),
        ))
    }
}

pub(super) fn workbook(
    xml: &XmlPart,
    base: &ModelRoot,
    model: &ModelRoot,
    sheets: &[Cow<'_, LoadedSheet>],
) -> Result<Vec<u8>, SaveRefusal> {
    let mut reader = Reader::new(xml);
    let mut splice = Splice::new(xml.bytes());
    while let Some((event, span)) = reader.next()? {
        let XmlEvent::Start(element) = event else {
            continue;
        };
        if element.name.is(MAIN_NAMESPACE, "sheets") {
            let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
            let prefix = String::from_utf8_lossy(tag.prefix());
            let mut bytes = tag.rebuilt(&[], false);
            let mut originals = std::collections::BTreeMap::new();
            let end = loop {
                match reader.expect()? {
                    (XmlEvent::Start(child), child_span) => {
                        if child.name.is(MAIN_NAMESPACE, "sheet") {
                            if let Some(id) = child
                                .attributes
                                .iter()
                                .find(|a| a.name.local == "id")
                                .map(|a| a.value.clone())
                            {
                                originals.insert(id, child_span);
                            }
                        }
                        reader.element_end()?;
                    }
                    (XmlEvent::End(_), (_, end)) => break end,
                    _ => {}
                }
            };
            for entry in model.sheets() {
                let loaded = sheets
                    .iter()
                    .find(|s| s.id() == entry.id())
                    .expect("expanded sheet");
                let state = match entry.visibility() {
                    SheetVisibility::Visible => "visible",
                    SheetVisibility::Hidden => "hidden",
                    SheetVisibility::VeryHidden => "veryHidden",
                };
                if let Some(&(start, end)) = originals.get(loaded.relationship()) {
                    let tag = RawTag::parse(&xml.bytes()[start..end]);
                    let changed = tag.rebuilt(&[("state", Some(state))], true);
                    bytes.extend_from_slice(&RawTag::parse(&changed).rebuilt_xstring(
                        "name",
                        entry.name().text().utf16_code_units(),
                        true,
                    ));
                    continue;
                }
                bytes.extend_from_slice(format!("<{prefix}sheet name=").as_bytes());
                xstring_attribute(entry.name().text().utf16_code_units(), &mut bytes);
                bytes.extend_from_slice(format!(" sheetId=\"{}\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=", entry.key().get()).as_bytes());
                attribute_value(loaded.relationship(), &mut bytes);
                let state = match entry.visibility() {
                    SheetVisibility::Visible => "visible",
                    SheetVisibility::Hidden => "hidden",
                    SheetVisibility::VeryHidden => "veryHidden",
                };
                bytes.extend_from_slice(format!(" state=\"{state}\"/>").as_bytes());
            }
            bytes.extend_from_slice(format!("</{prefix}sheets>").as_bytes());
            splice.replace((span.0, end), &bytes);
        } else if element.name.is(MAIN_NAMESPACE, "definedName") {
            if let Some(index) = element
                .attribute("localSheetId")
                .and_then(|s| s.parse::<usize>().ok())
            {
                let destination = base
                    .sheets()
                    .get(index)
                    .and_then(|old| model.sheets().iter().position(|s| s.id() == old.id()));
                if let Some(index) = destination {
                    let value = index.to_string();
                    let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
                    splice.replace(
                        span,
                        &tag.rebuilt(&[("localSheetId", Some(&value))], tag.is_empty()),
                    );
                } else {
                    let end = reader.element_end()?;
                    splice.replace((span.0, end), &[]);
                }
            }
        } else if element.name.is(MAIN_NAMESPACE, "workbookView") {
            // Imported activeTab/firstSheet indexes may point past the new list or at a hidden sheet.
            let active = model
                .sheets()
                .iter()
                .position(|s| s.visibility() == SheetVisibility::Visible)
                .unwrap_or(0)
                .to_string();
            let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
            splice.replace(
                span,
                &tag.rebuilt(
                    &[("activeTab", Some(&active)), ("firstSheet", Some("0"))],
                    tag.is_empty(),
                ),
            );
        }
    }
    Ok(splice.finish())
}

pub(super) fn finish(
    package: &Package,
    workbook: &str,
    base: &ModelRoot,
    model: &ModelRoot,
    sheets: &[Cow<'_, LoadedSheet>],
    out: &mut Changes,
) -> Result<(), SaveRefusal> {
    let added: Vec<_> = sheets
        .iter()
        .filter(|s| base.sheet_by_id(s.id()).is_none())
        .collect();
    let removed: Vec<_> = sheets
        .iter()
        .filter(|s| model.sheet_by_id(s.id()).is_none())
        .collect();
    for sheet in &added {
        let bytes = out
            .rewritten
            .remove(sheet.part().name())
            .or_else(||out.added.iter().find(|(name,_)|name==sheet.part().name()).map(|(_,bytes)|bytes.clone()))
            .unwrap_or_else(|| EMPTY.to_vec());
        if let Some((_,current))=out.added.iter_mut().find(|(name,_)|name==sheet.part().name()){*current=bytes;}else{out.added.push((sheet.part().name().to_owned(),bytes));}
    }
    for sheet in &removed {
        out.rewritten.remove(sheet.part().name());
        out.removed.push(sheet.part().name().to_owned());
    }
    let rels =
        package
            .relationships_of(Some(workbook))
            .ok_or_else(|| SaveRefusal::InvalidPart {
                part: workbook.to_owned(),
                detail: "missing workbook relationships".to_owned(),
            })?;
    let original = read_part(package, rels.part())?;
    let current = out
        .rewritten
        .remove(rels.part())
        .map(|bytes| original.with_bytes(bytes));
    let xml = current.as_ref().unwrap_or(&original);
    let targets: Vec<_> = added
        .iter()
        .map(|s| super::author::relative_target(workbook, s.part().name()))
        .collect();
    let add: Vec<_> = added
        .iter()
        .zip(&targets)
        .map(|(s, target)| NewRelationship {
            id: s.relationship(),
            kind: WORKSHEET,
            target,
        })
        .collect();
    let bytes = parts::relationships(xml, None, &add)?;
    let xml = xml.with_bytes(bytes);
    let mut reader = Reader::new(&xml);
    let mut splice = Splice::new(xml.bytes());
    while let Some((event, span)) = reader.next()? {
        if let XmlEvent::Start(e) = event {
            if e.name.local == "Relationship"
                && removed
                    .iter()
                    .any(|s| e.attribute("Id") == Some(s.relationship()))
            {
                let end = reader.element_end()?;
                splice.replace((span.0, end), &[]);
            }
        }
    }
    out.rewritten
        .insert(rels.part().to_owned(), splice.finish());
    let original = package
        .content_types_xml()
        .map_err(|error| SaveRefusal::Part {
            part: "[Content_Types].xml".to_owned(),
            error,
        })?;
    let mut xml = out
        .content_types
        .take()
        .map(|bytes| original.with_bytes(bytes))
        .unwrap_or(original);
    let overrides: Vec<_> = added
        .iter()
        .map(|s| (s.part().name(), CONTENT_TYPE))
        .collect();
    let bytes = parts::content_types(&xml, None, &overrides)?;
    xml = xml.with_bytes(bytes);
    for sheet in removed {
        xml = xml.with_bytes(parts::content_types(&xml, Some(sheet.part().name()), &[])?);
    }
    out.content_types = Some(xml.bytes().to_vec());
    Ok(())
}
