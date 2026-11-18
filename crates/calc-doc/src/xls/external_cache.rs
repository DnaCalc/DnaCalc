//! Saved BIFF8 external-cell caches (XCT/CRN, MS-XLS 2.2.7.5). These are retained as
//! external-link cache values, never promoted to calculated values.
use super::{records::Record, workbook::SupBookRecord};
use crate::xlsb::{Cursor, formula};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub(crate) enum Value {
    Empty, Number(f64), Text(Vec<u16>), Boolean(bool), Error(&'static str),
}

pub(crate) type Cells = BTreeMap<(u16, u8), Value>;
#[derive(Debug)]
pub(crate) struct CachedSheet {
    pub(crate) cells: Cells,
    pub(crate) refresh_error: bool,
}
pub(crate) type Caches = BTreeMap<(usize, u16), CachedSheet>;

fn values(record: &Record<'_>, code_page: Option<u16>) -> Option<Cells> {
    if !record.continues.is_empty() { return None; }
    let mut c = Cursor::new(record.body);
    let last = c.u8("CRN").ok()?;
    let first = c.u8("CRN").ok()?;
    let row = c.u16("CRN").ok()?;
    if first > last { return None; }
    let mut cells = Cells::new();
    for col in first..=last {
        let value = match c.u8("CRN value").ok()? {
            0 => { c.skip(8, "CRN empty").ok()?; Some(Value::Empty) }
            1 => {
                let n = c.f64("CRN number").ok()?;
                if !n.is_finite() { return None; }
                Some(Value::Number(n))
            }
            2 => {
                let text = if let Some(page) = code_page {
                    let count = usize::from(c.u8("CRN string").ok()?);
                    super::codepage::decode(page, c.bytes(count, "CRN string").ok()?).ok()?.encode_utf16().collect()
                } else {
                    let count = usize::from(c.u16("CRN string").ok()?);
                    let flags = c.u8("CRN string").ok()?;
                    if flags & !1 != 0 { return None; }
                    if flags & 1 != 0 { c.units(count, "CRN string").ok()? }
                    else { c.bytes(count, "CRN string").ok()?.iter().map(|&b| u16::from(b)).collect() }
                };
                Some(Value::Text(text))
            }
            4 => {
                let b = c.u8("CRN boolean").ok()?;
                c.skip(7, "CRN boolean").ok()?;
                if b > 1 { return None; }
                Some(Value::Boolean(b != 0))
            }
            16 => {
                let code = c.u8("CRN error").ok()?;
                c.skip(7, "CRN error").ok()?;
                Some(Value::Error(formula::error_text(code)?))
            }
            _ => return None,
        };
        if let Some(value) = value { cells.insert((row, col), value); }
    }
    (c.remaining() == 0).then_some(cells)
}

fn consumed(unknown: &mut BTreeMap<u16, u32>, id: u16, n: u32) {
    if let Some(count) = unknown.get_mut(&id) {
        *count = count.saturating_sub(n);
        if *count == 0 { unknown.remove(&id); }
    }
}

/// Only complete, unambiguous BIFF8 groups are retained; all others remain counted unread.
pub(crate) fn read(records: &[Record<'_>], books: &[SupBookRecord], unknown: &mut BTreeMap<u16, u32>) -> Caches {
    let mut candidates: BTreeMap<(usize, u16), Option<(Cells, u32)>> = BTreeMap::new();
    let mut blocked_books = BTreeSet::new();
    let mut invalid_books = BTreeSet::new();
    let mut book = None;
    let mut next_book = 0usize;
    let mut i = 0usize;
    while i < records.len() {
        let r = &records[i];
        if r.id == 0x01AE { book = Some(next_book); next_book += 1; }
        if (r.id == 0x0059 && (r.body.len() != 4 || !r.continues.is_empty())) || r.id == 0x005A {
            if let Some(book) = book { blocked_books.insert(book); }
        }
        if r.id == 0x0059 && r.body.len() == 4 && r.continues.is_empty() {
            let count = i16::from_le_bytes([r.body[0], r.body[1]]);
            let sheet = u16::from_le_bytes([r.body[2], r.body[3]]);
            if let Some(book) = book {
                if count < 0 { invalid_books.insert(book); }
                let valid_book = matches!(books.get(book), Some(SupBookRecord::External { sheets, .. }) if usize::from(sheet) < sheets.len());
                if valid_book {
                    let key = (book, sheet);
                    if candidates.contains_key(&key) { candidates.insert(key, None); i += 1; continue; }
                    let count = usize::from(count.unsigned_abs());
                    if count > records.len().saturating_sub(i + 1) { candidates.insert(key, None); i += 1; continue; }
                    let mut cells = Cells::new();
                    let mut valid = true;
                    for record in &records[i + 1..i + 1 + count] {
                        if record.id != 0x005A { valid = false; break; }
                        let Some(row) = values(record, None) else { valid = false; break };
                        for (key, value) in row {
                            if cells.insert(key, value).is_some() { valid = false; }
                        }
                    }
                    if valid {
                        candidates.insert(key, Some((cells, count as u32)));
                        i += count;
                    } else {
                        candidates.insert(key, None);
                    }
                }
            }
        }
        i += 1;
    }
    let mut out = Caches::new();
    for (key, candidate) in candidates {
        if blocked_books.contains(&key.0) { continue; }
        if let Some((cells, count)) = candidate {
            out.insert(key, CachedSheet { cells, refresh_error: invalid_books.contains(&key.0) });
            consumed(unknown, 0x0059, 1);
            consumed(unknown, 0x005A, count);
        }
    }
    out
}

/// BIFF5 caches follow a sheet-local EXTERNSHEET, and text uses the workbook code page.
/// A sheet appearing in more than one group is left unread until their combination is known.
pub(crate) fn read_biff5<'a>(
    tables: impl IntoIterator<Item = (&'a [Record<'a>], &'a super::v5::Table)>,
    code_page: u16,
) -> (Caches, BTreeMap<u16, u32>) {
    let mut candidates: BTreeMap<(usize, u16), Option<(Cells, u32)>> = BTreeMap::new();
    let mut blocked_books = BTreeSet::new();
    for (records, table) in tables {
        let mut next_target = 0usize;
        let mut target = None;
        let mut i = 0usize;
        while i < records.len() {
            let record = &records[i];
            if record.id == 0x0017 {
                target = table.cache_target(next_target);
                next_target += 1;
            }
            if (record.id == 0x0059 && (record.body.len() != 2 || !record.continues.is_empty())) || record.id == 0x005A {
                if let Some((book, _)) = target { blocked_books.insert(book); }
            }
            if record.id == 0x0059 && record.body.len() == 2 && record.continues.is_empty() {
                let count = u16::from_le_bytes([record.body[0], record.body[1]]) as usize;
                if let Some(key) = target {
                    if candidates.contains_key(&key) || count > i16::MAX as usize || count > records.len().saturating_sub(i + 1) {
                        candidates.insert(key, None); i += 1; continue;
                    }
                    let mut cells = Cells::new();
                    let mut valid = true;
                    for crn in &records[i + 1..i + 1 + count] {
                        if crn.id != 0x005A { valid = false; break; }
                        let Some(row) = values(crn, Some(code_page)) else { valid = false; break };
                        for (cell, value) in row {
                            if cells.insert(cell, value).is_some() { valid = false; }
                        }
                    }
                    if valid {
                        candidates.insert(key, Some((cells, count as u32)));
                        i += count;
                    } else { candidates.insert(key, None); }
                }
            }
            i += 1;
        }
    }
    let mut out = Caches::new();
    let mut consumed = BTreeMap::new();
    for (key, candidate) in candidates {
        if blocked_books.contains(&key.0) { continue; }
        if let Some((cells, count)) = candidate {
            out.insert(key, CachedSheet { cells, refresh_error: false });
            *consumed.entry(0x0059).or_insert(0) += 1;
            *consumed.entry(0x005A).or_insert(0) += count;
        }
    }
    (out, consumed)
}
