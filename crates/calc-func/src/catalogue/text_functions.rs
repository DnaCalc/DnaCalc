//! The text functions family fragment ported from OxFunc (functions/ at 6fca3518:
//! text_slice_family.rs, text_b_compat_family.rs, text_search_replace_family.rs,
//! text_scalar_misc.rs, text_unicode_fn.rs, excel_casing.rs, clean_fn.rs, exact_fn.rs, t_fn.rs,
//! concat_family.rs and textjoin.rs). The kernels are in [`crate::ported::text_kernels`];
//! arguments are read by worksheet coercion ([`super::kit`]).
//!
//! LEN/MID/FIND/SEARCH/REPLACE read the workbook's CompatibilityVersion: Version 1 counts
//! UTF-16 units and Version 2 counts valid surrogate pairs once. The B functions keep their
//! separate policies: LENB/MIDB/FINDB/SEARCHB/REPLACEB count units, while LEFTB/RIGHTB count
//! characters under a single-byte language.
//! SEARCH's wildcard matcher consumes UTF-16 units in both versions; only its candidate
//! starts and returned positions use the compatibility-dependent character count.
//!
//! A text result longer than a cell holds is #VALUE! for REPT, and #CALC! for CONCAT,
//! CONCATENATE and TEXTJOIN (OxFunc's rule). The casing functions (UPPER, LOWER, PROPER) read
//! characters, and text that is not well-formed UTF-16 is refused there as not modelled.

use super::kit::{Grid, fail, grid, num, out, out_text, text_arg};
use crate::coerce;
use crate::kernel::{Exit, Site, Step};
use crate::locale::LocaleId;
use crate::ported::text_kernels as k;

use crate::value::{
    CalcValue, CoreRef, EXCEL_TEXT_MAX_UTF16_CODE_UNITS, ExcelText, WorksheetErrorCode,
};

/// F2's families take codes from 200 up, clear of the other lanes' families.
const CODE: u16 = 200;

/// Microsoft Compatibility Versions: LEN/MID/FIND/SEARCH/REPLACE change their counting.
/// https://support.microsoft.com/en-gb/excel/compatibility-versions
/// Missing legacy services retain Version 1. An explicit unsupported version is a typed refusal.
fn count_surrogate_pairs(site: &Site<'_>) -> Step<bool> {
    match site.environment().map_or(1, |environment| environment.compatibility_version) {
        1 => Ok(false),
        2 => Ok(true),
        _ => Err(site.not_modelled(None)),
    }
}
/// A text result of a casing kernel, refused when the text is not well-formed UTF-16.
fn cased(site: &Site<'_>, a: &[CalcValue], kernel: fn(&ExcelText) -> ExcelText) -> Step<CalcValue> {
    let text = text_arg(site, a, 0)?;
    if !k::is_well_formed(&text) {
        return Err(site.not_modelled(Some(0)));
    }
    Ok(CalcValue::text(kernel(&text)))
}

/// The byte functions (LENB, LEFTB, RIGHTB, MIDB, FINDB, SEARCHB, REPLACEB) count as their
/// character twins do (Excel 16.0.20527 under en-ZA, the catalogue's sheet dczsn3_bytes), unless
/// the language is a double-byte one, where Excel counts a double-byte character as two bytes:
/// refused under the ja-JP, ko-KR and zh-CN profiles (bead dc-zsn.3).
fn single_byte(site: &Site<'_>) -> Step<()> {
    match site.environment().and_then(|e| e.profile.as_ref()) {
        Some(p) if matches!(p.id, LocaleId::JaJp | LocaleId::KoKr | LocaleId::ZhCn) => {
            Err(site.not_modelled(None))
        }
        _ => Ok(()),
    }
}

/// The optional count of LEFT, RIGHT, LEFTB and RIGHTB: 1 when the call does not reach it.
fn side_count(site: &Site<'_>, a: &[CalcValue]) -> Step<usize> {
    if a.len() < 2 {
        return Ok(1);
    }
    let n = num(site, a, 1)?;
    k::nonnegative_count(n, true).map_err(Exit::Error)
}

fn left(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let text = text_arg(site, a, 0)?;
    let count = side_count(site, a)?;
    Ok(CalcValue::text(k::take_left(&text, count)))
}

fn right(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let text = text_arg(site, a, 0)?;
    let count = side_count(site, a)?;
    Ok(CalcValue::text(k::take_right(&text, count)))
}

/// Version 1 MID and every MIDB count UTF-16 units (dc-fl2 records individual surrogate halves).
/// Version 2 MID counts each valid surrogate pair once.
fn mid(site: &Site<'_>, a: &[CalcValue], bytes: bool) -> Step<CalcValue> {
    let text = text_arg(site, a, 0)?;
    let start = num(site, a, 1)?;
    let count = num(site, a, 2)?;
    let start = k::mid_start(start).map_err(Exit::Error)?;
    let count = k::nonnegative_count(count, false).map_err(Exit::Error)?;
    Ok(CalcValue::text(k::take_mid(&text, start, count, !bytes && count_surrogate_pairs(site)?)))
}

/// The optional start of FIND and SEARCH: 1 when the call does not reach it or leaves it empty.
fn start(site: &Site<'_>, a: &[CalcValue]) -> Step<usize> {
    match a.get(2) {
        None => Ok(1),
        Some(value) if value.is_missing() => Ok(1),
        Some(_) => {
            let n = num(site, a, 2)?;
            k::positive(n).map_err(Exit::Error)
        }
    }
}

fn find(site: &Site<'_>, a: &[CalcValue], bytes: bool) -> Step<CalcValue> {
    let needle = text_arg(site, a, 0)?;
    let hay = text_arg(site, a, 1)?;
    let start = start(site, a)?;
    out(site, k::find(&needle, &hay, start, !bytes && count_surrogate_pairs(site)?))
}

fn search(site: &Site<'_>, a: &[CalcValue], bytes: bool) -> Step<CalcValue> {
    let needle = text_arg(site, a, 0)?;
    let hay = text_arg(site, a, 1)?;
    let start = start(site, a)?;
    out(site, k::search(&needle, &hay, start, !bytes && count_surrogate_pairs(site)?))
}

fn replace(site: &Site<'_>, a: &[CalcValue], bytes: bool) -> Step<CalcValue> {
    let old = text_arg(site, a, 0)?;
    let start = num(site, a, 1)?;
    let start = k::positive(start).map_err(Exit::Error)?;
    let count = num(site, a, 2)?;
    let count = k::nonnegative(count).map_err(Exit::Error)?;
    let new = text_arg(site, a, 3)?;
    capped(k::replace(&old, start, count, &new, !bytes && count_surrogate_pairs(site)?), WorksheetErrorCode::Value)
}

/// A text result, or `code` when it is longer than a cell holds.
fn capped(text: ExcelText, code: WorksheetErrorCode) -> Step<CalcValue> {
    if text.len_utf16_code_units() > EXCEL_TEXT_MAX_UTF16_CODE_UNITS {
        return fail(code);
    }
    Ok(CalcValue::text(text))
}

/// The cells an argument of CONCAT or TEXTJOIN gives, in row-major order: a reference's cells
/// (a blank cell as Empty) or an array's elements, or the argument itself.
fn cells(site: &Site<'_>, a: &[CalcValue], index: usize, sv: &mut crate::call::EvalServices<'_>) -> Step<Grid> {
    grid(site, a, index, sv)
}

/// One cell's text for CONCAT and TEXTJOIN.
fn cell_text(site: &Site<'_>, index: usize, value: &CalcValue) -> Step<Vec<u16>> {
    if value.callable_value().is_some() || matches!(value.core_ref(), CoreRef::Array(_) | CoreRef::Reference(_)) {
        return Err(site.not_modelled(Some(index)));
    }
    Ok(site.argument(index, coerce::text(value))?.into_owned())
}

fn concat(site: &Site<'_>, a: &[CalcValue], sv: &mut crate::call::EvalServices<'_>) -> Step<CalcValue> {
    let mut joined: Vec<u16> = Vec::new();
    for index in 0..a.len() {
        let g = cells(site, a, index, sv)?;
        for value in &g.cells {
            let units = cell_text(site, index, value)?;
            if joined.len() + units.len() > EXCEL_TEXT_MAX_UTF16_CODE_UNITS {
                return fail(WorksheetErrorCode::Calc);
            }
            joined.extend_from_slice(&units);
        }
    }
    Ok(CalcValue::text(ExcelText::from_utf16_code_units(joined)))
}

fn concatenate(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let mut joined: Vec<u16> = Vec::new();
    for index in 0..a.len() {
        let units = cell_text(site, index, &a[index])?;
        joined.extend_from_slice(&units);
    }
    // A result longer than a cell holds is cut to its first 32,767 UTF-16 code units, as &'s is
    // (dc-fl2, Excel 16.0.20527: LEN(CONCATENATE(REPT("a",32767),"b")) is 32,767; CONCAT's is
    // #CALC!).
    joined.truncate(EXCEL_TEXT_MAX_UTF16_CODE_UNITS);
    Ok(CalcValue::text(ExcelText::from_utf16_code_units(joined)))
}

/// TEXTJOIN's delimiters: one text, or each cell of a reference or an array in turn, used in
/// order and again from the first (Microsoft's TEXTJOIN page: a range of delimiters). A reference
/// of several areas (a 3-D reference) gives the cells of each area in its order (Excel 16.0.20430,
/// 2026-10-25, dc-6au, sheets Ja, Fe and Ma holding 1, 2, 3 in A1 and 100, 200, 300 in A2:
/// `=TEXTJOIN(Ja:Ma!A1,1,"a","b")` "a1b", `=TEXTJOIN(Ja:Ma!A1:A2,1,"a","b","c")` "a1b100c",
/// `=TEXTJOIN(Fe:Ma!A1,1,"a","b","c")` "a2b3c").
fn delimiters(site: &Site<'_>, a: &[CalcValue], sv: &mut crate::call::EvalServices<'_>) -> Step<Vec<Vec<u16>>> {
    let mut found = Vec::new();
    match a[0].core_ref() {
        CoreRef::Reference(reference) => {
            for &area in reference.areas() {
                let one = [CalcValue::reference(crate::value::ReferenceLike::area(area))];
                for value in &grid(site, &one, 0, sv)?.cells {
                    found.push(cell_text(site, 0, value)?);
                }
            }
        }
        CoreRef::Array(_) => {
            for value in &cells(site, a, 0, sv)?.cells {
                found.push(cell_text(site, 0, value)?);
            }
        }
        _ => found.push(text_arg(site, a, 0)?.utf16_code_units().to_vec()),
    }
    Ok(found)
}

fn textjoin(site: &Site<'_>, a: &[CalcValue], sv: &mut crate::call::EvalServices<'_>) -> Step<CalcValue> {
    let delimiters = delimiters(site, a, sv)?;
    let ignore_empty = num(site, a, 1)? != 0.0;
    let mut joined: Vec<u16> = Vec::new();
    let mut parts = 0usize;
    for index in 2..a.len() {
        let g = cells(site, a, index, sv)?;
        for value in &g.cells {
            if ignore_empty && matches!(value.core_ref(), CoreRef::Empty | CoreRef::Missing) {
                continue;
            }
            let units = cell_text(site, index, value)?;
            if ignore_empty && units.is_empty() {
                continue;
            }
            let delimiter: &[u16] = if delimiters.is_empty() {
                &[]
            } else {
                &delimiters[parts.saturating_sub(1) % delimiters.len()]
            };
            let extra = units.len() + if parts == 0 { 0 } else { delimiter.len() };
            if joined.len() + extra > EXCEL_TEXT_MAX_UTF16_CODE_UNITS {
                return fail(WorksheetErrorCode::Calc);
            }
            if parts > 0 {
                joined.extend_from_slice(delimiter);
            }
            joined.extend_from_slice(&units);
            parts += 1;
        }
    }
    Ok(CalcValue::text(ExcelText::from_utf16_code_units(joined)))
}

ported_family! {
    code: CODE;
    1 char_fn "CHAR" (1, 1) [V "number"] |s, a, sv| {
        let n = num(s, a, 0)?;
        out_text(k::char_from_number(n))
    }
    2 code_fn "CODE" (1, 1) [V "text"] |s, a, sv| {
        let text = text_arg(s, a, 0)?;
        out(s, k::code_of_text(&text))
    }
    3 unichar_fn "UNICHAR" (1, 1) [V "number"] |s, a, sv| {
        let n = num(s, a, 0)?;
        out_text(k::unichar(n))
    }
    4 unicode_fn "UNICODE" (1, 1) [V "text"] |s, a, sv| {
        let text = text_arg(s, a, 0)?;
        out(s, k::unicode(&text))
    }
    5 len_fn "LEN" (1, 1) [V "text"] |s, a, sv| {
        let text = text_arg(s, a, 0)?;
        let count = if count_surrogate_pairs(s)? {
            k::len_character_count(&text)
        } else {
            text.len_utf16_code_units()
        };
        s.number(count as f64)
    }
    6 left_fn "LEFT" (1, 2) [V "text", V "num_chars"] |s, a, sv| { left(s, a) }
    7 right_fn "RIGHT" (1, 2) [V "text", V "num_chars"] |s, a, sv| { right(s, a) }
    8 mid_fn "MID" (3, 3) [V "text", V "start_num", V "num_chars"] |s, a, sv| { mid(s, a, false) }
    9 lenb_fn "LENB" (1, 1) [V "text"] |s, a, sv| {
        single_byte(s)?;
        let text = text_arg(s, a, 0)?;
        s.number(text.len_utf16_code_units() as f64)
    }
    10 leftb_fn "LEFTB" (1, 2) [V "text", V "num_bytes"] |s, a, sv| { single_byte(s)?; left(s, a) }
    11 rightb_fn "RIGHTB" (1, 2) [V "text", V "num_bytes"] |s, a, sv| { single_byte(s)?; right(s, a) }
    12 midb_fn "MIDB" (3, 3) [V "text", V "start_num", V "num_bytes"] |s, a, sv| { single_byte(s)?; mid(s, a, true) }
    13 upper_fn "UPPER" (1, 1) [V "text"] |s, a, sv| { cased(s, a, k::upper) }
    14 lower_fn "LOWER" (1, 1) [V "text"] |s, a, sv| { cased(s, a, k::lower) }
    15 proper_fn "PROPER" (1, 1) [V "text"] |s, a, sv| { cased(s, a, k::proper) }
    16 trim_fn "TRIM" (1, 1) [V "text"] |s, a, sv| {
        let text = text_arg(s, a, 0)?;
        Ok(CalcValue::text(k::trim_ascii_spaces(&text)))
    }
    17 clean_fn "CLEAN" (1, 1) [V "text"] |s, a, sv| {
        let text = text_arg(s, a, 0)?;
        Ok(CalcValue::text(k::clean(&text)))
    }
    18 rept_fn "REPT" (2, 2) [V "text", V "number_times"] |s, a, sv| {
        let text = text_arg(s, a, 0)?;
        let n = num(s, a, 1)?;
        out_text(k::rept(&text, n))
    }
    19 exact_fn "EXACT" (2, 2) [V "text1", V "text2"] |s, a, sv| {
        let left = text_arg(s, a, 0)?;
        let right = text_arg(s, a, 1)?;
        Ok(CalcValue::logical(left.utf16_code_units() == right.utf16_code_units()))
    }
    20 find_fn "FIND" (2, 3) [V "find_text", V "within_text", V "start_num"] |s, a, sv| { find(s, a, false) }
    21 findb_fn "FINDB" (2, 3) [V "find_text", V "within_text", V "start_num"] |s, a, sv| { single_byte(s)?; find(s, a, true) }
    22 search_fn "SEARCH" (2, 3) [V "find_text", V "within_text", V "start_num"] |s, a, sv| { search(s, a, false) }
    23 searchb_fn "SEARCHB" (2, 3) [V "find_text", V "within_text", V "start_num"] |s, a, sv| { single_byte(s)?; search(s, a, true) }
    24 replace_fn "REPLACE" (4, 4) [V "old_text", V "start_num", V "num_chars", V "new_text"] |s, a, sv| {
        replace(s, a, false)
    }
    25 replaceb_fn "REPLACEB" (4, 4) [V "old_text", V "start_num", V "num_bytes", V "new_text"] |s, a, sv| {
        single_byte(s)?;
        replace(s, a, true)
    }
    26 substitute_fn "SUBSTITUTE" (3, 4) [V "text", V "old_text", V "new_text", V "instance_num"] |s, a, sv| {
        let text = text_arg(s, a, 0)?;
        let old = text_arg(s, a, 1)?;
        let new = text_arg(s, a, 2)?;
        let instance = match a.get(3) {
            None => None,
            Some(value) if value.is_missing() => None,
            Some(_) => {
                let n = num(s, a, 3)?;
                Some(k::positive(n).map_err(Exit::Error)?)
            }
        };
        capped(k::substitute(&text, &old, &new, instance), WorksheetErrorCode::Value)
    }
    27 t_fn "T" (1, 1) [V "value"] |s, a, sv| {
        match a[0].core_ref() {
            CoreRef::Text(_) => Ok(a[0].clone()),
            CoreRef::Error(code) => fail(code),
            CoreRef::Number(_) | CoreRef::Logical(_) | CoreRef::Empty | CoreRef::Missing
                if a[0].callable_value().is_none() =>
            {
                Ok(CalcValue::text(ExcelText::from_utf16_code_units(Vec::new())))
            }
            _ => Err(s.not_modelled(Some(0))),
        }
    }
    28 concatenate_fn "CONCATENATE" (1, 255) [V "text1", V "text2"] |s, a, sv| { concatenate(s, a) }
    29 concat_fn "CONCAT" (1, 254) [R "text1", R "text2"] |s, a, sv| { concat(s, a, sv) }
    30 textjoin_fn "TEXTJOIN" (3, 252) [R "delimiter", V "ignore_empty", R "text1", R "text2"] |s, a, sv| {
        textjoin(s, a, sv)
    }
}
