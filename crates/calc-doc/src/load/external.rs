//! A workbook's external links (dc-ylr): each `externalReference` of the workbook part names an
//! externalLink part, whose `externalBook` caches the values of the other workbook's cells that
//! the workbook's formulas read. A file's formula writes such a reference as `[N]Sheet1!A1`, N
//! counting the workbook's `externalReference` elements from 1. Excel, its links not updated,
//! reads those cached values: Excel 16.0.20527 gave `=[1]Sheet1!A1` 77 where the link's cache
//! holds 77 for A1, and #REF! where the link's `sheetData` is marked `refreshError` and caches
//! nothing (the local corpus's link workbooks, dc-ifc's captures).
//!
//! A link this loader does not read (a DDE or OLE link, a part that is missing or not shaped as it
//! expects) is kept as unread: a reference to it is refused, never guessed. A cell of an
//! unsupported type (a shared string index) is left out of the cache, so a reference to it is
//! refused too.

use super::sheet::{ERRORS, collapsed};
use super::{Events, MAIN_NAMESPACE, address, read_xml, strings};
use crate::opc::{Package, TargetMode};
use crate::xml::XmlEvent;
use calc_doc_model::value::ExcelText;
use calc_doc_model::{CachedValue, CellText, ExternalBook, ExternalName, ExternalSheet, FiniteNumber};
use std::collections::BTreeMap;

/// The relationship type of a workbook part's link to an externalLink part.
pub(crate) const EXTERNAL_LINK: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink";
const ALTERNATE_URL_NAMESPACE: &str =
    "http://schemas.microsoft.com/office/spreadsheetml/2021/extlinks2021";

/// The books the workbook part's `externalReference` relationships `ids` name, in order: each the
/// externalLink part's cached book, or `None` when it is not read.
pub(crate) fn read_external_books(
    package: &Package,
    workbook: &str,
    ids: &[String],
) -> Vec<Option<ExternalBook>> {
    let relationships = package.relationships_of(Some(workbook));
    ids.iter()
        .map(|id| {
            let relationship = relationships?
                .items()
                .iter()
                .find(|item| &item.id == id && item.kind == EXTERNAL_LINK)?;
            if relationship.mode != TargetMode::Internal {
                return None;
            }
            read_book(package, relationship.resolved.as_deref()?)
        })
        .collect()
}

/// The file name a link's relationship target names: its last part after a `/` or `\`
/// (`file:///C:\Data\Book2.xlsx` and `../Book2.xlsx` both name `Book2.xlsx`).
fn file_name_of(target: &str) -> Option<String> {
    let name = target.rsplit(['/', '\\']).next()?;
    (!name.is_empty()).then(|| name.to_owned())
}

/// The cached book of the externalLink part `part`, or `None` when it holds none or is not read.
fn read_book(package: &Package, part: &str) -> Option<ExternalBook> {
    let xml = read_xml(package, part).ok()?;
    let mut events = Events::new(&xml);
    events
        .root(MAIN_NAMESPACE, "externalLink", "not an externalLink")
        .ok()?;
    let mut book = None;
    loop {
        match events.next().ok()? {
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "externalBook") => {
                let id = super::relationship_attribute(&element, "id").map(str::to_owned);
                let target = |id: &str| {
                    let relationships = package.relationships_of(Some(part))?;
                    let item = relationships.items().iter().find(|item| item.id == id)?;
                    (item.mode == TargetMode::External).then(|| item.target.clone())
                };
                let (mut read, absolute_id) = read_external_book(&mut events)?;
                read.target = id.as_deref().and_then(target);
                read.absolute_target = absolute_id.as_deref().and_then(target);
                read.file_name = read.target.as_deref().and_then(file_name_of);
                book = Some(read);
            }
            // A DDE or OLE link (`ddeLink`, dc-x8v4): its items' cached values, as a book of one
            // synthetic sheet whose names are the items.
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "ddeLink") => {
                book = Some(read_dde_items(&mut events)?);
            }
            Some(XmlEvent::Start(_)) => events.skip().ok()?,
            Some(XmlEvent::Text(_)) => {}
            Some(XmlEvent::End(_)) => break,
            None => return None,
        }
    }
    book
}

/// The sheet a DDE link's items are cached on: no worksheet of a workbook can be named so.
const DDE_SHEET: &str = "(dde)";

/// The `ddeLink` whose start tag was just read, as an [`ExternalBook`]: one sheet holding each
/// item's first cached value in column A, and a defined name per item (`[1]!'ENE,LAST,1'`) that
/// refers to it. An item cached with no value is #REF! (Excel, saved caches of a closed
/// TWINDDE link); an item cached as several values has no name, so stays refused.
fn read_dde_items(events: &mut Events<'_>) -> Option<ExternalBook> {
    let mut cells = BTreeMap::new();
    let mut names = Vec::new();
    loop {
        match events.next().ok()? {
            Some(XmlEvent::Start(items)) if items.name.is(MAIN_NAMESPACE, "ddeItems") => loop {
                match events.next().ok()? {
                    Some(XmlEvent::Start(item)) if item.name.is(MAIN_NAMESPACE, "ddeItem") => {
                        let name =
                            String::from_utf16_lossy(&strings::decode(item.attribute("name")?));
                        let mut values = Vec::new();
                        loop {
                            match events.next().ok()? {
                                Some(XmlEvent::Start(held)) if held.name.is(MAIN_NAMESPACE, "values") => {
                                    loop {
                                        match events.next().ok()? {
                                            Some(XmlEvent::Start(value))
                                                if value.name.is(MAIN_NAMESPACE, "value") =>
                                            {
                                                let kind =
                                                    value.attribute("t").map(|t| collapsed(t).to_owned());
                                                let mut text = None;
                                                loop {
                                                    match events.next().ok()? {
                                                        Some(XmlEvent::Start(val))
                                                            if val.name.is(MAIN_NAMESPACE, "val") =>
                                                        {
                                                            text = Some(events.text().ok()?);
                                                        }
                                                        Some(XmlEvent::Start(_)) => events.skip().ok()?,
                                                        Some(XmlEvent::Text(_)) => {}
                                                        Some(XmlEvent::End(_)) => break,
                                                        None => return None,
                                                    }
                                                }
                                                values.push(
                                                    text.and_then(|text| cached(kind.as_deref(), &text)),
                                                );
                                            }
                                            Some(XmlEvent::Start(_)) => events.skip().ok()?,
                                            Some(XmlEvent::Text(_)) => {}
                                            Some(XmlEvent::End(_)) => break,
                                            None => return None,
                                        }
                                    }
                                }
                                Some(XmlEvent::Start(_)) => events.skip().ok()?,
                                Some(XmlEvent::Text(_)) => {}
                                Some(XmlEvent::End(_)) => break,
                                None => return None,
                            }
                        }
                        let row = names.len() as u32 + 1;
                        let refers_to = match values.as_slice() {
                            [] => Some("=#REF!".to_owned()),
                            [Some(value)] => {
                                cells.insert((row, 1), value.clone());
                                Some(format!("='{DDE_SHEET}'!$A${row}"))
                            }
                            _ => None,
                        };
                        names.push(ExternalName {
                            name,
                            sheet: None,
                            refers_to,
                        });
                    }
                    Some(XmlEvent::Start(_)) => events.skip().ok()?,
                    Some(XmlEvent::Text(_)) => {}
                    Some(XmlEvent::End(_)) => break,
                    None => return None,
                }
            },
            Some(XmlEvent::Start(_)) => events.skip().ok()?,
            Some(XmlEvent::Text(_)) => {}
            Some(XmlEvent::End(_)) => break,
            None => return None,
        }
    }
    Some(ExternalBook {
        target: None,
        absolute_target: None,
        file_name: None,
        sheets: vec![ExternalSheet {
            name: DDE_SHEET.to_owned(),
            id: None,
            cached: true,
            refresh_error: false,
            cells,
        }],
        defined_names: names,
    })
}

/// The `externalBook` whose start tag was just read: its sheet names and cached sheets.
fn read_external_book(events: &mut Events<'_>) -> Option<(ExternalBook, Option<String>)> {
    let mut names = Vec::new();
    let mut defined = Vec::new();
    let mut absolute_id = None;
    let mut data: BTreeMap<usize, (bool, BTreeMap<(u32, u32), CachedValue>)> = BTreeMap::new();
    loop {
        match events.next().ok()? {
            Some(XmlEvent::Start(element))
                if element.name.is(ALTERNATE_URL_NAMESPACE, "alternateUrls") =>
            {
                loop {
                    match events.next().ok()? {
                        Some(XmlEvent::Start(element)) => {
                            if element.name.is(ALTERNATE_URL_NAMESPACE, "absoluteUrl") {
                                absolute_id = super::relationship_attribute(&element, "id")
                                    .map(str::to_owned);
                            }
                            events.skip().ok()?;
                        }
                        Some(XmlEvent::Text(_)) => {}
                        Some(XmlEvent::End(_)) => break,
                        None => return None,
                    }
                }
            }
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "sheetNames") => {
                loop {
                    match events.next().ok()? {
                        Some(XmlEvent::Start(name))
                            if name.name.is(MAIN_NAMESPACE, "sheetName") =>
                        {
                            names.push(String::from_utf16_lossy(&strings::decode(
                                name.attribute("val").unwrap_or_default(),
                            )));
                            events.skip().ok()?;
                        }
                        Some(XmlEvent::Start(_)) => events.skip().ok()?,
                        Some(XmlEvent::Text(_)) => {}
                        Some(XmlEvent::End(_)) => break,
                        None => return None,
                    }
                }
            }
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "definedNames") => {
                loop {
                    match events.next().ok()? {
                        Some(XmlEvent::Start(name))
                            if name.name.is(MAIN_NAMESPACE, "definedName") =>
                        {
                            let text = |attribute: &str| {
                                name.attribute(attribute)
                                    .map(|value| String::from_utf16_lossy(&strings::decode(value)))
                            };
                            if let Some(held) = text("name") {
                                defined.push(ExternalName {
                                    name: held,
                                    sheet: name
                                        .attribute("sheetId")
                                        .and_then(|value| collapsed(value).parse().ok()),
                                    refers_to: text("refersTo"),
                                });
                            }
                            events.skip().ok()?;
                        }
                        Some(XmlEvent::Start(_)) => events.skip().ok()?,
                        Some(XmlEvent::Text(_)) => {}
                        Some(XmlEvent::End(_)) => break,
                        None => return None,
                    }
                }
            }
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "sheetDataSet") => {
                loop {
                    match events.next().ok()? {
                        Some(XmlEvent::Start(sheet))
                            if sheet.name.is(MAIN_NAMESPACE, "sheetData") =>
                        {
                            let index: usize =
                                collapsed(sheet.attribute("sheetId")?).parse().ok()?;
                            let refresh_error = matches!(
                                sheet.attribute("refreshError").map(collapsed),
                                Some("1" | "true")
                            );
                            let cells = read_sheet_data(events)?;
                            data.insert(index, (refresh_error, cells));
                        }
                        Some(XmlEvent::Start(_)) => events.skip().ok()?,
                        Some(XmlEvent::Text(_)) => {}
                        Some(XmlEvent::End(_)) => break,
                        None => return None,
                    }
                }
            }
            Some(XmlEvent::Start(_)) => events.skip().ok()?,
            Some(XmlEvent::Text(_)) => {}
            Some(XmlEvent::End(_)) => break,
            None => return None,
        }
    }
    let sheets = names
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            let held = data.remove(&index);
            let cached = held.is_some();
            let (refresh_error, cells) = held.unwrap_or_default();
            ExternalSheet {
                name,
                id: None,
                cached,
                refresh_error,
                cells,
            }
        })
        .collect();
    Some((
        ExternalBook {
            target: None,
            absolute_target: None,
            file_name: None,
            sheets,
            defined_names: defined,
        },
        absolute_id,
    ))
}

/// The cells of the `sheetData` whose start tag was just read, by row and column.
fn read_sheet_data(events: &mut Events<'_>) -> Option<BTreeMap<(u32, u32), CachedValue>> {
    let mut cells = BTreeMap::new();
    loop {
        match events.next().ok()? {
            Some(XmlEvent::Start(row)) if row.name.is(MAIN_NAMESPACE, "row") => loop {
                match events.next().ok()? {
                    Some(XmlEvent::Start(cell)) if cell.name.is(MAIN_NAMESPACE, "cell") => {
                        let at = cell.attribute("r").and_then(address::cell);
                        let kind = cell.attribute("t").map(|t| collapsed(t).to_owned());
                        let mut text = None;
                        loop {
                            match events.next().ok()? {
                                Some(XmlEvent::Start(v)) if v.name.is(MAIN_NAMESPACE, "v") => {
                                    text = Some(events.text().ok()?);
                                }
                                Some(XmlEvent::Start(_)) => events.skip().ok()?,
                                Some(XmlEvent::Text(_)) => {}
                                Some(XmlEvent::End(_)) => break,
                                None => return None,
                            }
                        }
                        if let (Some(at), Some(text)) = (at, text)
                            && let Some(value) = cached(kind.as_deref(), &text)
                        {
                            cells.insert(at, value);
                        }
                    }
                    Some(XmlEvent::Start(_)) => events.skip().ok()?,
                    Some(XmlEvent::Text(_)) => {}
                    Some(XmlEvent::End(_)) => break,
                    None => return None,
                }
            },
            Some(XmlEvent::Start(_)) => events.skip().ok()?,
            Some(XmlEvent::Text(_)) => {}
            Some(XmlEvent::End(_)) => break,
            None => return None,
        }
    }
    Some(cells)
}

/// A cached cell's value, its `<v>` text read as type `kind` (`n` when absent); `None` for a type
/// this loader does not read.
fn cached(kind: Option<&str>, text: &str) -> Option<CachedValue> {
    match kind {
        None | Some("n") => collapsed(text)
            .parse::<f64>()
            .ok()
            .and_then(FiniteNumber::new)
            .map(CachedValue::Number),
        Some("str") => CellText::new(ExcelText::from_utf16_code_units(strings::decode(text)))
            .map(CachedValue::Text),
        Some("b") => match collapsed(text) {
            "0" | "false" => Some(CachedValue::Logical(false)),
            "1" | "true" => Some(CachedValue::Logical(true)),
            _ => None,
        },
        Some("e") => ERRORS
            .iter()
            .find(|(spelling, _)| *spelling == collapsed(text))
            .map(|(_, code)| CachedValue::Error(*code)),
        Some(_) => None,
    }
}
