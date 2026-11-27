//! Resolves direct addresses against the sheet and name information a view already has.

use crate::address::{self, SheetNames, quoted_sheet};
use crate::grid::{Area, Key, Sheet};
use crate::{NameScope, SourceRef, TablePart};
use calc_value::{CellKey, GridArea, SheetId, fold_workbook_name};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddressBook {
    pub sheets: Vec<(Sheet, String)>,
    pub names: Vec<(NameScope, String)>,
    pub tables: Vec<String>,
    pub active: Option<Key>,
}

impl SheetNames for AddressBook {
    fn sheet_name_of(&self, sheet: SheetId) -> Option<String> {
        self.sheets
            .iter()
            .find(|(id, _)| id.id() == sheet)
            .map(|(_, name)| name.clone())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notation {
    A1,
    R1C1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddressError {
    Invalid,
    UnknownSheet,
    AmbiguousName,
    RequiresResolution,
}

fn sheet_text(text: &str) -> Option<String> {
    if let Some(quoted) = text.strip_prefix('\'') {
        Some(quoted.strip_suffix('\'')?.replace("''", "'"))
    } else {
        Some(text.to_owned())
    }
}

fn qualified(text: &str) -> Option<(&str, &str)> {
    let mut quoted = false;
    for (index, ch) in text.char_indices() {
        match ch {
            '\'' => quoted = !quoted,
            '!' if !quoted => return Some((&text[..index], &text[index + 1..])),
            _ => {}
        }
    }
    None
}

fn axis(text: &str, base: Option<u32>) -> Option<u32> {
    if text.is_empty() {
        return base;
    }
    if let Some(offset) = text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
        return base?.checked_add_signed(offset.parse().ok()?);
    }
    text.parse().ok()
}

fn coords(text: &str, notation: Notation, active: Option<Key>) -> Option<(u32, u32)> {
    match notation {
        Notation::A1 => address::parse_address(text),
        Notation::R1C1 => {
            let upper = text.to_ascii_uppercase();
            let (row, col) = upper.strip_prefix('R')?.split_once('C')?;
            Some((
                axis(row, active.map(|at| at.2))?,
                axis(col, active.map(|at| at.3))?,
            ))
        }
    }
}

impl SourceRef {
    pub fn parse(text: &str, notation: Notation, book: &AddressBook) -> Result<Self, AddressError> {
        let same = |left: &str, right: &str| fold_workbook_name(left) == fold_workbook_name(right);
        let text = text.trim();
        if text.is_empty() {
            return Err(AddressError::Invalid);
        }
        let (qualifier, reference) = if let Some((qualifier, reference)) = qualified(text) {
            let name = sheet_text(qualifier).ok_or(AddressError::Invalid)?;
            let id = book
                .sheets
                .iter()
                .find(|(_, sheet)| same(sheet, &name))
                .map(|(id, _)| *id)
                .ok_or(AddressError::UnknownSheet)?;
            (Some(id), reference)
        } else {
            (None, text)
        };
        let sheet = qualifier.or(book.active.map(|at| Sheet(at.0, at.1)));
        if let Some(sheet) = sheet {
            let cell = |input: &str| {
                let (row, column) = coords(input.trim(), notation, book.active)?;
                CellKey::new(sheet.id(), row, column).ok()
            };
            if let Some(anchor) = reference.strip_suffix('#').and_then(cell) {
                return Ok(Self::Spill {
                    anchor: Key::of(anchor),
                });
            }
            let areas: Option<Vec<_>> = reference
                .split(',')
                .map(|range| {
                    let (first, last) = range.split_once(':').unwrap_or((range, range));
                    let (first, last) = (cell(first)?, cell(last)?);
                    let a = CellKey::new(
                        sheet.id(),
                        first.row().min(last.row()),
                        first.column().min(last.column()),
                    )
                    .ok()?;
                    let b = CellKey::new(
                        sheet.id(),
                        first.row().max(last.row()),
                        first.column().max(last.column()),
                    )
                    .ok()?;
                    GridArea::new(a, b).ok().map(Area::of)
                })
                .collect();
            if let Some(areas) = areas {
                return Ok(Self::Cells { sheet, areas });
            }
            if let Some(name) = reference
                .strip_prefix('"')
                .and_then(|name| name.strip_suffix('"'))
            {
                return Ok(Self::Object {
                    sheet,
                    name: name.replace("\"\"", "\""),
                });
            }
            if reference.is_empty() && qualifier.is_some() {
                return Ok(Self::Sheet(sheet));
            }
        }
        let mut names = book.names.iter().filter(|(scope, name)| {
            let visible = match scope {
                NameScope::Workbook => qualifier.is_none(),
                NameScope::Sheet(local) => Some(*local) == sheet,
            };
            visible && same(name, reference)
        });
        if let Some((scope, name)) = names.next() {
            if names.next().is_some() {
                return Err(AddressError::AmbiguousName);
            }
            return Ok(Self::Name {
                scope: scope.clone(),
                name: name.clone(),
            });
        }
        if qualifier.is_none() {
            let (name, selector) = reference
                .split_once('[')
                .map_or((reference, None), |(name, tail)| (name, Some(tail)));
            if let Some(name) = book.tables.iter().find(|t| same(t, name)) {
                let part = match selector {
                    None => TablePart::Data,
                    Some("#All]") => TablePart::All,
                    Some("#Data]") => TablePart::Data,
                    Some("#Headers]") => TablePart::Headers,
                    Some("#Totals]") => TablePart::Totals,
                    Some(selector) => {
                        let column = selector.strip_suffix(']').ok_or(AddressError::Invalid)?;
                        if column.contains(['[', ']', ':', '@']) {
                            return Err(AddressError::RequiresResolution);
                        }
                        TablePart::Columns {
                            first: column.to_owned(),
                            last: column.to_owned(),
                        }
                    }
                };
                return Ok(Self::Table {
                    name: name.clone(),
                    part,
                });
            }
        }
        Err(AddressError::RequiresResolution)
    }

    pub fn display(&self, book: &AddressBook) -> Option<String> {
        let label = |sheet: Sheet| {
            book.sheet_name_of(sheet.id())
                .map(|name| quoted_sheet(&name))
        };
        let cell_text = |cell: CellKey| address::address(cell.row(), cell.column());
        Some(match self {
            Self::Cells { sheet, areas } if !areas.is_empty() => {
                let addresses: Option<Vec<_>> = areas
                    .iter()
                    .map(|area| {
                        let area = area.area()?;
                        if area.sheet() != sheet.id() {
                            return None;
                        }
                        let first = cell_text(area.first());
                        Some(if area.first() == area.last() {
                            first
                        } else {
                            format!("{first}:{}", cell_text(area.last()))
                        })
                    })
                    .collect();
                format!("{}!{}", label(*sheet)?, addresses?.join(","))
            }
            Self::Cells { .. } => return None,
            Self::Spill { anchor } => {
                let at = anchor.cell()?;
                format!("{}!{}#", label(Sheet::of(at.sheet()))?, cell_text(at))
            }
            Self::Name {
                scope: NameScope::Workbook,
                name,
            } => name.clone(),
            Self::Name {
                scope: NameScope::Sheet(sheet),
                name,
            } => format!("{}!{name}", label(*sheet)?),
            Self::Table { name, part } => match part {
                TablePart::All => format!("{name}[#All]"),
                TablePart::Data => name.clone(),
                TablePart::Headers => format!("{name}[#Headers]"),
                TablePart::Totals => format!("{name}[#Totals]"),
                TablePart::Columns { first, last } if first == last => format!("{name}[{first}]"),
                TablePart::Columns { first, last } => format!("{name}[[{first}]:[{last}]]"),
            },
            Self::Object { sheet, name } => {
                format!("{}!\"{}\"", label(*sheet)?, name.replace('"', "\"\""))
            }
            Self::Sheet(sheet) => format!("{}!", label(*sheet)?),
            Self::Workbook => return None,
        })
    }
}
