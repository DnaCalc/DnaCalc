//! The string-valued core document properties in an OLE SummaryInformation stream (MS-OLEPS).
//! Invalid, unsupported or ambiguous properties are left out; they are never cell values.

use std::collections::BTreeMap;

const SUMMARY_FMTID: [u8; 16] = [0xe0, 0x85, 0x9f, 0xf2, 0xf9, 0x4f, 0x68, 0x10,
    0xab, 0x91, 0x08, 0x00, 0x2b, 0x27, 0xb3, 0xd9];
const DOCUMENT_FMTID: [u8; 16] = [0x02, 0xd5, 0xcd, 0xd5, 0x9c, 0x2e, 0x1b, 0x10,
    0x93, 0x97, 0x08, 0x00, 0x2b, 0x2c, 0xf9, 0xae];

fn word(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(at..at.checked_add(2)?)?.try_into().ok()?))
}
fn dword(data: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(data.get(at..at.checked_add(4)?)?.try_into().ok()?) as usize)
}

fn xml_text(text: &str) -> bool {
    text.chars().all(|c| matches!(c, '\t' | '\n' | '\r') || ('\u{20}'..='\u{fffd}').contains(&c) || c >= '\u{10000}')
}

/// FILETIME is a UTC count of 100-nanosecond ticks since 1601-01-01.
fn filetime(ticks: u64) -> Option<String> {
    if ticks == 0 { return None; } // Some writers use zero for an unset property.
    let seconds = ticks / 10_000_000;
    let mut days = seconds / 86_400;
    let time = seconds % 86_400;
    let mut year = 1601u64.checked_add((days / 146_097).checked_mul(400)?)?;
    days %= 146_097;
    if year > 9999 { return None; }
    let leap = |year: u64| year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    loop {
        let length = if leap(year) { 366 } else { 365 };
        if days < length { break; }
        days -= length; year += 1;
        if year > 9999 { return None; }
    }
    let months = [31u64, if leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1;
    for length in months {
        if days < length { break; }
        days -= length; month += 1;
    }
    let mut result = format!("{year:04}-{month:02}-{:02}T{:02}:{:02}:{:02}", days + 1,
        time / 3600, (time / 60) % 60, time % 60);
    let fraction = ticks % 10_000_000;
    if fraction != 0 {
        result.push('.');
        result.push_str(format!("{fraction:07}").trim_end_matches('0'));
    }
    result.push('Z');
    Some(result)
}

/// Return the properties we can decode without making up a code page or accepting overlapping data.
pub(crate) fn summary(data: &[u8]) -> Option<BTreeMap<u32, String>> {
    read(data, &SUMMARY_FMTID, &[2, 3, 4, 5, 6, 8, 9, 18], true)
}

pub(crate) fn document(data: &[u8]) -> Option<BTreeMap<u32, String>> {
    read(data, &DOCUMENT_FMTID, &[2, 14, 15], false)
}

fn read(data: &[u8], fmtid: &[u8; 16], ids: &[u32], dates: bool) -> Option<BTreeMap<u32, String>> {
    if data.len() > 2 * 1024 * 1024 || word(data, 0)? != 0xfffe || word(data, 2)? > 1 { return None; }
    let set_count = dword(data, 24)?;
    if !(1..=2).contains(&set_count) || (dates && set_count != 1) { return None; }
    let header_end = 28 + set_count * 20;
    let mut ranges = Vec::new();
    let mut wanted = None;
    for i in 0..set_count {
        let at = 28 + i * 20;
        let kind = data.get(at..at + 16)?;
        let offset = dword(data, at + 16)?;
        if offset < header_end || offset % 4 != 0 { return None; }
        let end = offset.checked_add(dword(data, offset)?)?;
        if end > data.len() || ranges.iter().any(|&(start, stop)| offset < stop && start < end) { return None; }
        ranges.push((offset, end));
        if kind == fmtid && wanted.replace(offset).is_some() { return None; }
    }
    let offset = wanted?;
    let size = dword(data, offset)?;
    let section = data.get(offset..offset.checked_add(size)?)?;
    let count = dword(section, 4)?;
    let table_end = 8usize.checked_add(count.checked_mul(8)?)?;
    if count > 1024 || table_end > size { return None; }
    let mut offsets = BTreeMap::new();
    for i in 0..count {
        let id = dword(section, 8 + i * 8)? as u32;
        let at = dword(section, 12 + i * 8)?;
        if at < table_end || at % 4 != 0 || at.checked_add(4)? > size || offsets.insert(id, at).is_some() { return None; }
    }
    let mut ordered: Vec<usize> = offsets.values().copied().collect();
    ordered.sort_unstable();
    if ordered.windows(2).any(|pair| pair[0] == pair[1]) { return None; }
    let value = |id: u32| -> Option<&[u8]> {
        let at = *offsets.get(&id)?;
        let end = ordered.iter().copied().find(|offset| *offset > at).unwrap_or(size);
        section.get(at..end)
    };
    let cp = value(1).and_then(|bytes| (word(bytes, 0) == Some(2) && word(bytes, 2) == Some(0))
        .then(|| word(bytes, 4)).flatten());
    let mut result = BTreeMap::new();
    for &id in ids {
        let Some(bytes) = value(id) else { continue };
        if word(bytes, 2) != Some(0) { continue; }
        let Some(length) = dword(bytes, 4) else { continue };
        let text = match word(bytes, 0) {
            Some(0x1e) if length > 0 => {
                let Some(raw) = bytes.get(8..8usize.saturating_add(length)) else { continue };
                if cp == Some(1200) {
                    if raw.len() % 2 != 0 { continue; }
                    let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                    if units.last() != Some(&0) || units[..units.len()-1].contains(&0) { continue; }
                    if let Ok(text) = String::from_utf16(&units[..units.len()-1]) && xml_text(&text) { result.insert(id, text); }
                    continue;
                }
                if raw.last() != Some(&0) || raw[..raw.len()-1].contains(&0) { continue; }
                let raw = &raw[..raw.len()-1];
                match cp {
                    Some(65001) => std::str::from_utf8(raw).ok().map(str::to_owned),
                    Some(page) if page == 932 || super::codepage::high_half(page).is_some()
                        || (matches!(page, 936 | 949 | 950) && raw.is_ascii()) => super::codepage::decode(page, raw).ok(),
                    _ => None,
                }
            }
            Some(0x1f) if length > 0 => {
                let Some(raw) = length.checked_mul(2).and_then(|len| bytes.get(8..8usize.checked_add(len)?)) else { continue };
                let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                if units.last() != Some(&0) || units[..units.len()-1].contains(&0) { continue; }
                String::from_utf16(&units[..units.len()-1]).ok()
            }
            _ => None,
        };
        if let Some(text) = text.filter(|text| xml_text(text)) { result.insert(id, text); }
    }
    for id in if dates { &[12, 13][..] } else { &[][..] } {
        let Some(bytes) = value(*id) else { continue };
        if word(bytes, 0) != Some(0x40) || word(bytes, 2) != Some(0) { continue; }
        let Some(raw) = bytes.get(4..12) else { continue };
        if let Some(text) = filetime(u64::from_le_bytes(raw.try_into().expect("eight bytes"))) { result.insert(*id, text); }
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn codepage_unicode_strings_are_byte_counted() {
        let mut data = vec![0u8; 96];
        data[0..2].copy_from_slice(&0xfffeu16.to_le_bytes());
        data[24..28].copy_from_slice(&1u32.to_le_bytes());
        data[28..44].copy_from_slice(&SUMMARY_FMTID);
        data[44..48].copy_from_slice(&48u32.to_le_bytes());
        let section = &mut data[48..];
        section[0..4].copy_from_slice(&48u32.to_le_bytes());
        section[4..8].copy_from_slice(&2u32.to_le_bytes());
        section[8..12].copy_from_slice(&1u32.to_le_bytes());
        section[12..16].copy_from_slice(&24u32.to_le_bytes());
        section[16..20].copy_from_slice(&2u32.to_le_bytes());
        section[20..24].copy_from_slice(&32u32.to_le_bytes());
        section[24..28].copy_from_slice(&2u32.to_le_bytes());
        section[28..30].copy_from_slice(&1200u16.to_le_bytes());
        section[32..36].copy_from_slice(&30u32.to_le_bytes());
        section[36..40].copy_from_slice(&6u32.to_le_bytes());
        section[40..46].copy_from_slice(&[b'A', 0, 0xe9, 0, 0, 0]);
        assert_eq!(summary(&data).unwrap().get(&2).map(String::as_str), Some("Aé"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn filetime_keeps_utc_and_fractional_seconds() {
        assert_eq!(filetime(116_444_736_000_000_000).as_deref(), Some("1970-01-01T00:00:00Z"));
        assert_eq!(filetime(125_963_423_991_234_500).as_deref(), Some("2000-02-29T23:59:59.12345Z"));
        assert_eq!(filetime(2_650_467_743_990_000_000).as_deref(), Some("9999-12-31T23:59:59Z"));
        assert_eq!(filetime(0), None);
        assert_eq!(filetime(u64::MAX), None);
    }
}
