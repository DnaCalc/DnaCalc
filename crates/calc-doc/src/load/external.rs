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
use calc_doc_model::{CachedValue, CellText, ExternalBook, ExternalSheet, FiniteNumber};
use std::collections::BTreeMap;

/// The relationship type of a workbook part's link to an externalLink part.
pub(crate) const EXTERNAL_LINK: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink";

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
                let file_name = id.and_then(|id| {
                    let relationships = package.relationships_of(Some(part))?;
                    let item = relationships.items().iter().find(|item| item.id == id)?;
                    file_name_of(&item.target)
                });
                let mut read = read_external_book(&mut events)?;
                read.file_name = file_name;
                book = Some(read);
            }
            Some(XmlEvent::Start(_)) => events.skip().ok()?,
            Some(XmlEvent::Text(_)) => {}
            Some(XmlEvent::End(_)) => break,
            None => return None,
        }
    }
    book
}

/// The `externalBook` whose start tag was just read: its sheet names and cached sheets.
fn read_external_book(events: &mut Events<'_>) -> Option<ExternalBook> {
    let mut names = Vec::new();
    let mut data: BTreeMap<usize, (bool, BTreeMap<(u32, u32), CachedValue>)> = BTreeMap::new();
    loop {
        match events.next().ok()? {
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "sheetNames") => {
                loop {
                    match events.next().ok()? {
                        Some(XmlEvent::Start(name)) if name.name.is(MAIN_NAMESPACE, "sheetName") => {
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
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "sheetDataSet") => {
                loop {
                    match events.next().ok()? {
                        Some(XmlEvent::Start(sheet)) if sheet.name.is(MAIN_NAMESPACE, "sheetData") => {
                            let index: usize = collapsed(sheet.attribute("sheetId")?).parse().ok()?;
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
    Some(ExternalBook {
        file_name: None,
        sheets,
    })
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
