//! Complete BIFF8 scenario definitions retained as unsupported worksheet metadata.
//! MS-XLS 2.4.244 and 2.4.246; the application does not apply these scenarios.
use super::{records::Record, strings::Pieces};
use crate::xlsb::Xml;
use std::collections::BTreeSet;

struct Input { cell: String, value: Vec<u16> }
struct Scenario { name: Vec<u16>, user: Vec<u16>, comment: Vec<u16>, locked: bool, hidden: bool, inputs: Vec<Input> }
pub(crate) struct Scenarios { current: Option<u16>, shown: Option<u16>, results: Vec<String>, scenarios: Vec<Scenario> }

fn word(bytes: &[u8], at: usize) -> Option<u16> {
    let pair = bytes.get(at..at + 2)?;
    Some(u16::from_le_bytes([pair[0], pair[1]]))
}
fn cell(row: u16, col: u16) -> String {
    format!("{}{}", super::sheet::column_name(u32::from(col)), u32::from(row) + 1)
}
fn unicode(p: &mut Pieces<'_>, count: usize) -> Option<Vec<u16>> {
    let flags = p.u8("scenario string").ok()?;
    if flags & !1 != 0 { return None; }
    p.characters(count, flags & 1 != 0, "scenario string").ok()
}
fn full_string(p: &mut Pieces<'_>, expected: Option<usize>) -> Option<Vec<u16>> {
    let count = usize::from(p.u16("scenario string length").ok()?);
    if expected.is_some_and(|length| count != length) { return None; }
    unicode(p, count)
}
fn scenario(record: &Record<'_>) -> Option<Scenario> {
    if !record.continues.is_empty() { return None; }
    let mut p = Pieces::new(record);
    let count = usize::from(p.u16("scenario cells").ok()?);
    let locked = p.u8("scenario locked").ok()?;
    let hidden = p.u8("scenario hidden").ok()?;
    let name_len = usize::from(p.u8("scenario name length").ok()?);
    let comment_len = usize::from(p.u8("scenario comment length").ok()?);
    let user_len = usize::from(p.u8("scenario user length").ok()?);
    if !(1..=32).contains(&count) || locked > 1 || hidden > 1 || name_len == 0 || user_len > 52 { return None; }
    let name = unicode(&mut p, name_len)?;
    String::from_utf16(&name).ok()?;
    let user = if user_len == 0 { Vec::new() } else { full_string(&mut p, Some(user_len))? };
    let comment = if comment_len == 0 { Vec::new() } else { full_string(&mut p, Some(comment_len))? };
    let mut cells = Vec::with_capacity(count);
    let mut distinct = BTreeSet::new();
    for _ in 0..count {
        let row = p.u16("scenario cell row").ok()?;
        let col = p.u16("scenario cell column").ok()?;
        // Deleted references have no trustworthy OOXML r attribute.
        if col & 0x4000 != 0 || col & 0x3fff > 255 { return None; }
        let address = cell(row, col & 0x3fff);
        if !distinct.insert(address.clone()) { return None; }
        cells.push(address);
    }
    let mut inputs = Vec::with_capacity(count);
    for cell in cells { inputs.push(Input { cell, value: full_string(&mut p, None)? }); }
    p.skip(count.checked_mul(2)?, "scenario unused bytes").ok()?;
    if !p.done() { return None; }
    Some(Scenario { name, user, comment, locked: locked != 0, hidden: hidden != 0, inputs })
}

/// Only a complete, unambiguous BIFF8 collection is emitted. Its records remain counted unread
/// because the application cannot yet apply or edit scenarios.
pub(crate) fn read(records: &[Record<'_>], biff5: bool) -> Option<Scenarios> {
    if biff5 { return None; }
    let mut depth = 0usize;
    let mut custom = 0usize;
    let mut manager = None;
    let mut entries = Vec::new();
    for record in records {
        match record.id {
            super::workbook::BOF => { depth += 1; continue; },
            super::workbook::EOF => { depth = depth.checked_sub(1)?; continue; },
            0x01AA if depth == 1 => { custom += 1; continue; },
            0x01AB if depth == 1 => { custom = custom.checked_sub(1)?; continue; },
            _ => {}
        }
        if depth != 1 || custom != 0 { continue; }
        match record.id {
            0x00AE => { if manager.replace(record).is_some() { return None; } },
            0x00AF => entries.push(record),
            _ => {}
        }
    }
    let manager = manager?;
    if !manager.continues.is_empty() { return None; }
    let b = manager.body;
    let count = usize::from(word(b, 0)?);
    let current = word(b, 2)?;
    let shown = word(b, 4)?;
    let result_count = usize::from(word(b, 6)?);
    if count == 0 || count > i16::MAX as usize || count != entries.len() || result_count > 32 || b.len() != 8 + result_count * 8 { return None; }
    let index = |v: u16| if v == 0xffff { Some(None) } else if usize::from(v) < count { Some(Some(v)) } else { None };
    let mut results = Vec::new();
    for i in 0..result_count {
        let at = 8 + i * 8;
        let (r1, r2, c1, c2) = (word(b, at)?, word(b, at+2)?, word(b, at+4)?, word(b, at+6)?);
        if r1 > r2 || c1 > c2 || c2 > 255 { return None; }
        let first = cell(r1, c1); let last = cell(r2, c2);
        results.push(if first == last { first } else { format!("{first}:{last}") });
    }
    let mut names = BTreeSet::new();
    let mut scenarios = Vec::with_capacity(count);
    for record in entries {
        let item = scenario(record)?;
        if !names.insert(String::from_utf16(&item.name).ok()?.to_lowercase()) { return None; }
        scenarios.push(item);
    }
    Some(Scenarios { current: index(current)?, shown: index(shown)?, results, scenarios })
}

fn text_attribute(x: &mut Xml, name: &str, units: &[u16]) {
    x.out.push(b' '); x.out.extend_from_slice(name.as_bytes()); x.out.push(b'=');
    crate::save::text::xstring_attribute(units, &mut x.out);
}
impl Scenarios {
    pub(crate) fn write(&self, x: &mut Xml) {
        let mut attributes = Vec::new();
        if let Some(current) = self.current { attributes.push(("current", current.to_string())); }
        if let Some(shown) = self.shown { attributes.push(("show", shown.to_string())); }
        if !self.results.is_empty() { attributes.push(("sqref", self.results.join(" "))); }
        x.open("scenarios", &attributes);
        for item in &self.scenarios {
            x.out.extend_from_slice(b"<scenario"); text_attribute(x, "name", &item.name);
            if item.locked { x.out.extend_from_slice(b" locked=\"1\""); }
            if item.hidden { x.out.extend_from_slice(b" hidden=\"1\""); }
            x.out.extend_from_slice(format!(" count=\"{}\"", item.inputs.len()).as_bytes());
            if !item.user.is_empty() { text_attribute(x, "user", &item.user); }
            if !item.comment.is_empty() { text_attribute(x, "comment", &item.comment); }
            x.out.push(b'>');
            for input in &item.inputs {
                x.out.extend_from_slice(b"<inputCells r=\""); x.out.extend_from_slice(input.cell.as_bytes());
                x.out.push(b'\"'); text_attribute(x, "val", &input.value); x.out.extend_from_slice(b"/>");
            }
            x.out.extend_from_slice(b"</scenario>");
        }
        x.close("scenarios");
    }
}
