//! Saved BIFF8 pivot-cache source records. Unsupported or ambiguous caches are left unread.
use super::{records::{self, Record}, strings::Pieces, workbook::{BoundSheet, NameRecord}};
use calc_doc_model::pivot::{PivotCacheField, PivotItem};
use std::collections::BTreeMap;

pub(crate) struct Cache {
    pub fields: Vec<PivotCacheField>,
    pub rows: Vec<Vec<PivotItem>>,
}

pub(crate) enum Source {
    Range { sheet: String, range: String, columns: usize },
    WorkbookName(String),
}

/// An explicit local range or unambiguous workbook-scoped name.
pub(crate) fn source(record: &Record<'_>, sheets: &[BoundSheet], names: &[NameRecord]) -> Option<Source> {
    if record.id == 0x0052 && record.continues.is_empty() {
        let mut p = Pieces::new(record);
        let name = String::from_utf16(&p.unicode_string("pivot source name").ok()?).ok()?;
        if name.is_empty() || p.u16("pivot source workbook").ok()? != 0 || !p.done() { return None; }
        let matches: Vec<_> = names.iter().filter(|n| n.scope.is_none() && n.name.eq_ignore_ascii_case(&name)
            && !n.rgce.is_empty()).collect();
        return (matches.len() == 1).then(|| Source::WorkbookName(matches[0].name.clone()));
    }
    if record.id == 0x01B5 && record.body.len() == 6 && record.continues.is_empty() {
        let b = record.body;
        if b[1..] != [0; 5] { return None; }
        let name = super::workbook::built_in_name(u16::from(b[0]))?;
        let matches: Vec<_> = names.iter().filter(|n| n.scope.is_none() && n.name.eq_ignore_ascii_case(name)
            && !n.rgce.is_empty()).collect();
        return (matches.len() == 1).then(|| Source::WorkbookName(matches[0].name.clone()));
    }
    if record.id != 0x0051 || record.body.len() < 11 || !record.continues.is_empty() { return None; }
    let b = record.body;
    let first_row = u16::from_le_bytes([b[0], b[1]]);
    let last_row = u16::from_le_bytes([b[2], b[3]]);
    let (first_col, last_col) = (b[4], b[5]);
    if first_row > last_row || first_col > last_col { return None; }
    let mut p = Pieces::new(record);
    p.skip(6, "pivot source range").ok()?;
    let count = usize::from(p.u16("pivot source sheet").ok()?);
    let flag = p.u8("pivot source sheet").ok()?;
    if count < 2 || flag & !1 != 0 { return None; }
    let units = p.characters(count, flag & 1 != 0, "pivot source sheet").ok()?;
    if units.first().copied()? != 2 { return None; }
    p.skip(if flag & 1 != 0 { 2 } else { 1 }, "pivot source terminator").ok()?;
    if !p.done() { return None; }
    let sheet = String::from_utf16(&units[1..]).ok()?;
    let matching: Vec<_> = sheets.iter().filter(|s| s.name.eq_ignore_ascii_case(&sheet)).collect();
    if matching.len() != 1 || matching[0].kind != super::workbook::SheetKind::Worksheet { return None; }
    let cell = |row: u16, col: u8| format!("{}{}", super::sheet::column_name(u32::from(col)), u32::from(row) + 1);
    let range = format!("{}:{}", cell(first_row, first_col), cell(last_row, last_col));
    Some(Source::Range { sheet, range, columns: usize::from(last_col - first_col) + 1 })
}

fn item(record: &Record<'_>) -> Option<PivotItem> {
    let b = record.body;
    if !record.continues.is_empty() { return None; }
    let word = || (b.len() == 2).then(|| u16::from_le_bytes([b[0], b[1]]));
    match record.id {
        0x00C9 if b.len() == 8 => {
            let n = f64::from_le_bytes(b.try_into().ok()?);
            n.is_finite().then(|| PivotItem::Number(crate::xlsb::formula::number_text(n)))
        }
        0x00CA => match word()? { 0 => Some(PivotItem::Boolean(false)), 1 => Some(PivotItem::Boolean(true)), _ => None },
        0x00CB => Some(PivotItem::Error(crate::xlsb::formula::error_text(u8::try_from(word()?).ok()?)?.to_owned())),
        0x00CC => Some(PivotItem::Number((word()? as i16).to_string())),
        0x00CD => {
            let mut p = Pieces::new(record);
            let text = p.unicode_string("pivot cache string").ok()?;
            if !p.done() { return None; }
            Some(PivotItem::Text(String::from_utf16(&text).ok()?))
        }
        0x00CE if b.len() == 8 => {
            let year = u16::from_le_bytes([b[0], b[1]]);
            let month = u16::from_le_bytes([b[2], b[3]]);
            let (day, hour, minute, second) = (b[4], b[5], b[6], b[7]);
            let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
            let days = match month { 1 | 3 | 5 | 7 | 8 | 10 | 12 => 31, 4 | 6 | 9 | 11 => 30,
                2 if leap => 29, 2 => 28, _ => return None };
            if !(1900..=9999).contains(&year) || day == 0 || day > days || hour > 23 || minute > 59 || second > 59 { return None; }
            Some(PivotItem::Date(format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}")))
        }
        0x00CF if b.is_empty() => Some(PivotItem::Missing),
        _ => None,
    }
}

/// The bounded, complete worksheet-source cache subset: no grouped, calculated, invalid or
/// refresh-on-load caches. Its values remain file records, not calculated pivot output.
pub(crate) fn read(bytes: &[u8], stream_id: u16) -> Option<Cache> {
    let records = records::read_all(bytes).ok()?;
    let header = records.first()?;
    if header.id != 0x00C6 || header.body.len() < 20 || !header.continues.is_empty() { return None; }
    let h = header.body;
    let rows = usize::try_from(i32::from_le_bytes(h[0..4].try_into().ok()?)).ok()?;
    let id = u16::from_le_bytes([h[4], h[5]]);
    let flags = u16::from_le_bytes([h[6], h[7]]);
    let sources = usize::from(u16::from_le_bytes([h[10], h[11]]));
    let fields = usize::from(u16::from_le_bytes([h[12], h[13]]));
    let source_type = u16::from_le_bytes([h[16], h[17]]);
    if id != stream_id || flags & 0x27 != 0x21 || source_type != 1 || fields == 0 || sources != fields
        || rows > 100_000 || rows.checked_mul(fields)? > 1_000_000 { return None; }
    let mut at = 1usize;
    if records.get(at)?.id != 0x0122 { return None; }
    at += 1;
    let mut out = Vec::with_capacity(fields);
    let mut sizes = Vec::with_capacity(fields);
    for _ in 0..fields {
        let r = records.get(at)?;
        if r.id != 0x00C7 || r.body.len() < 17 || !r.continues.is_empty() { return None; }
        let flags = u16::from_le_bytes([r.body[0], r.body[1]]);
        if flags & 0xE018 != 0 { return None; }
        let all_shared = flags & 1 != 0;
        if all_shared && flags & 2 != 0 { return None; }
        let short = flags & 0x0200 != 0;
        let count = usize::from(u16::from_le_bytes([r.body[12], r.body[13]]));
        if (!all_shared && count != 0) || short != (count > 255) { return None; }
        let mut p = Pieces::new(r);
        p.skip(14, "pivot cache field").ok()?;
        let name = String::from_utf16(&p.unicode_string("pivot cache field name").ok()?).ok()?;
        if name.is_empty() || !p.done() { return None; }
        at += 1;
        if records.get(at)?.id != 0x01BB || records[at].body.len() != 2 || !records[at].continues.is_empty() { return None; }
        at += 1;
        let mut items = Vec::with_capacity(count);
        for _ in 0..count { items.push(item(records.get(at)?)?); at += 1; }
        sizes.push((all_shared, short));
        out.push(PivotCacheField { name, shared_items: Some(count as u32), group: None, items,
            unused_items: Vec::new(), formula_items: Vec::new(), attributes: BTreeMap::new() });
    }
    let mut data = Vec::with_capacity(rows);
    for _ in 0..rows {
        let shared = if sizes.iter().any(|(a, _)| *a) {
            let r = records.get(at)?;
            if r.id != 0x00C8 || !r.continues.is_empty() { return None; }
            at += 1;
            r.body
        } else { &[] };
        let mut offset = 0usize;
        let mut row = Vec::with_capacity(fields);
        for (i, &(all, short)) in sizes.iter().enumerate() {
            if all {
                let index = if short {
                    let pair = shared.get(offset..offset + 2)?; offset += 2; u16::from_le_bytes([pair[0], pair[1]])
                } else { let value = u16::from(*shared.get(offset)?); offset += 1; value };
                if usize::from(index) >= out[i].items.len() { return None; }
                row.push(PivotItem::Index(u32::from(index)));
            } else {
                row.push(item(records.get(at)?)?); at += 1;
            }
        }
        if offset != shared.len() { return None; }
        data.push(row);
    }
    if records.get(at)?.id != 0x000A || at + 1 != records.len() { return None; }
    for (column, field) in out.iter_mut().enumerate() {
        let mut used = vec![false; field.items.len()];
        for row in &data {
            if let PivotItem::Index(index) = row[column] { used[index as usize] = true; }
        }
        field.unused_items = used.iter().enumerate().filter_map(|(i, used)| (!used).then_some(i as u32)).collect();
    }
    Some(Cache { fields: out, rows: data })
}

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../tests/xls_kit/mod.rs"]
mod kit;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xls::cfb_write::{self, Node};
    use calc_doc_model::value::{IdAllocator, SheetId};
    use std::sync::Arc;

    fn cache(inline: bool) -> Vec<u8> {
        let mut header = 2i32.to_le_bytes().to_vec();
        let fields = if inline { 2 } else { 1 };
        header.extend(kit::u16s(&[7, 0x21, 0, fields, fields, 2, 1, 0xFFFF]));
        let mut field = kit::u16s(&[1, 0, 0, 0, 0, 0, 3]);
        field.extend([1, 0, 0, b'X']);
        let mut out = [kit::record(0x00C6, &header), kit::record(0x0122, &[0; 12]),
         kit::record(0x00C7, &field), kit::record(0x01BB, &[0, 0]),
         kit::record(0x00CD, &[1, 0, 0, b'A']), kit::record(0x00CD, &[1, 0, 0, b'B']),
         kit::record(0x00CD, &[1, 0, 0, b'C'])].concat();
        if inline {
            let mut second = kit::u16s(&[0, 0, 0, 0, 0, 0, 0]); second.extend([1, 0, 0, b'Y']);
            out.extend(kit::record(0x00C7, &second)); out.extend(kit::record(0x01BB, &[0, 0]));
        }
        for (index, value) in [(0, 3.5_f64), (1, -2.0)] {
            out.extend(kit::record(0x00C8, &[index]));
            if inline { out.extend(kit::record(0x00C9, &value.to_le_bytes())); }
        }
        out.extend(kit::record(0x000A, &[]));
        out
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn worksheet_cache_is_visible_as_saved_source_records_after_open_and_save() {
        let mut source = kit::u16s(&[0, 2]); source.extend([0, 0]);
        source.extend(kit::u16s(&[2])); source.extend([0, 2, b'S', 0]);
        let globals = [kit::record(0x00D5, &kit::u16s(&[7])), kit::record(0x00E3, &kit::u16s(&[1])),
            kit::record(0x0051, &source)].concat();
        let workbook = kit::workbook_stream(false, &globals,
            &[kit::Sheet::worksheet("S", [kit::number(0, 0, 1.0), kit::record(0x00B0, &[])].concat())]);
        let bytes = cfb_write::write(vec![("Workbook".into(), Node::Stream(workbook)),
            ("_SX_DB_CUR".into(), Node::Storage(vec![("0007".into(), Node::Stream(cache(false)))]))]).unwrap();
        let source = |bytes: &[u8]| Arc::new(crate::MemorySource::new(bytes.to_vec())) as Arc<dyn crate::PackageRead>;
        let document = crate::open(source(&bytes), &crate::OpenPolicy::default()).unwrap();
        let report = crate::legacy_workbook_report(source(&bytes), &crate::OpenPolicy::default()).unwrap().unwrap();
        assert_eq!(report.pivot_tables(), 1); // The table itself is still unsupported.
        let check = |doc: &crate::OpenedDocument| {
            let mut ids = IdAllocator::<SheetId>::new();
            let loaded = crate::load(doc, &mut ids).unwrap();
            let cached = loaded.pivot_caches().caches.get(&0).unwrap();
            assert_eq!(cached.source_sheet.as_deref(), Some("S"));
            assert_eq!(cached.source_range.as_deref(), Some("A1:A3"));
            assert_eq!(cached.fields[0].items, vec![PivotItem::Text("A".into()), PivotItem::Text("B".into()), PivotItem::Text("C".into())]);
            assert_eq!(cached.fields[0].unused_items, vec![2]);
            assert_eq!(loaded.pivot_records(0, 10).unwrap().records,
                vec![vec![PivotItem::Index(0)], vec![PivotItem::Index(1)]]);
            assert_eq!(loaded.sheets()[0].pivots().tables.len(), 0);
        };
        check(&document);
        let plan = crate::prepare_save(&document.snapshot(), &crate::SavePolicy::default()).unwrap();
        let mut saved = Vec::new(); crate::write(plan, &mut saved).unwrap();
        check(&crate::open(source(&saved), &crate::OpenPolicy::default()).unwrap());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn invalid_cache_or_grouped_fields_are_not_interpreted() {
        let bytes = cache(false);
        assert!(read(&bytes, 7).is_some());
        assert!(read(&bytes, 8).is_none());
        let mut invalid = bytes.clone(); invalid[4 + 6] |= 2;
        assert!(read(&invalid, 7).is_none());
        let mut grouped = bytes;
        let start = grouped.windows(2).position(|b| b == [0xC7, 0]).unwrap();
        grouped[start + 4] |= 8;
        assert!(read(&grouped, 7).is_none());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cache_with_inline_values_keeps_record_types_and_indexes() {
        let mut source = kit::u16s(&[0, 2]); source.extend([0, 1]);
        source.extend(kit::u16s(&[2])); source.extend([0, 2, b'S', 0]);
        let globals = [kit::record(0x00D5, &kit::u16s(&[7])), kit::record(0x00E3, &kit::u16s(&[1])),
            kit::record(0x0051, &source)].concat();
        let workbook = kit::workbook_stream(false, &globals, &[kit::Sheet::worksheet("S", Vec::new())]);
        let bytes = cfb_write::write(vec![("Workbook".into(), Node::Stream(workbook)),
            ("_SX_DB_CUR".into(), Node::Storage(vec![("0007".into(), Node::Stream(cache(true)))]))]).unwrap();
        let source = |bytes: &[u8]| Arc::new(crate::MemorySource::new(bytes.to_vec())) as Arc<dyn crate::PackageRead>;
        let document = crate::open(source(&bytes), &crate::OpenPolicy::default()).unwrap();
        let check = |doc: &crate::OpenedDocument| {
            let mut ids = IdAllocator::<SheetId>::new();
            let loaded = crate::load(doc, &mut ids).unwrap();
            assert_eq!(loaded.pivot_records(0, 10).unwrap().records, vec![
                vec![PivotItem::Index(0), PivotItem::Number("3.5".into())],
                vec![PivotItem::Index(1), PivotItem::Number("-2".into())]]);
            assert!(loaded.pivot_caches().caches[&0].fields[1].items.is_empty());
        };
        check(&document);
        let plan = crate::prepare_save(&document.snapshot(), &crate::SavePolicy::default()).unwrap();
        let mut saved = Vec::new(); crate::write(plan, &mut saved).unwrap();
        check(&crate::open(source(&saved), &crate::OpenPolicy::default()).unwrap());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn workbook_scoped_cache_sources_remain_names() {
        for builtin in [false, true] { for have_name in [false, true] {
            let (flags, name, expected, source_record) = if builtin {
                (0x20, "\u{4}", "_xlnm.Database", kit::record(0x01B5, &[4, 0, 0, 0, 0, 0]))
            } else {
                let mut body = kit::u16s(&[6]); body.extend([0, b'S', b'o', b'u', b'r', b'c', b'e']); body.extend([0, 0]);
                (0, "Source", "Source", kit::record(0x0052, &body))
            };
            let named = kit::name_record(flags, 0, name, &kit::ptg::area3d(0, (0, 2), (0, 1), 0));
            let globals = [kit::supbook_self(1), kit::extern_sheet(&[(0, 0, 0)]),
                if have_name { named.clone() } else { Vec::new() },
                kit::record(0x00D5, &kit::u16s(&[7])), kit::record(0x00E3, &kit::u16s(&[1])),
                source_record].concat();
            let workbook = kit::workbook_stream(false, &globals, &[kit::Sheet::worksheet("S", Vec::new())]);
            let bytes = cfb_write::write(vec![("Workbook".into(), Node::Stream(workbook)),
                ("_SX_DB_CUR".into(), Node::Storage(vec![("0007".into(), Node::Stream(cache(true)))]))]).unwrap();
            let source = |bytes: &[u8]| Arc::new(crate::MemorySource::new(bytes.to_vec())) as Arc<dyn crate::PackageRead>;
            let document = crate::open(source(&bytes), &crate::OpenPolicy::default()).unwrap();
            let check = |doc: &crate::OpenedDocument| {
                let mut ids = IdAllocator::<SheetId>::new();
                let loaded = crate::load(doc, &mut ids).unwrap();
                if have_name {
                    let cached = loaded.pivot_caches().caches.get(&0).unwrap();
                    assert_eq!(cached.source_name.as_deref(), Some(expected));
                    assert_eq!(loaded.pivot_records(0, 10).unwrap().records.len(), 2);
                } else { assert!(loaded.pivot_caches().caches.is_empty()); }
            };
            check(&document);
            let plan = crate::prepare_save(&document.snapshot(), &crate::SavePolicy::default()).unwrap();
            let mut saved = Vec::new(); crate::write(plan, &mut saved).unwrap();
            check(&crate::open(source(&saved), &crate::OpenPolicy::default()).unwrap());
        }}
    }
}
