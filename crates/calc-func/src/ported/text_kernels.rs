//! The text kernels ported from OxFunc (crates/oxfunc_core/src/functions/ at 6fca3518):
//! text_slice_family.rs, text_search_replace_family.rs, text_scalar_misc.rs, text_unicode_fn.rs,
//! excel_casing.rs, clean_fn.rs, exact_fn.rs and date_fn.rs's `date_integer_floor`, with the
//! OxFunc surface layer (argument preparation, lifting, metadata) removed. The text family
//! fragment (`catalogue/text_functions.rs`) reads and coerces the arguments.
//!
//! Two kernels depart from OxFunc where Excel on this machine (16.0.20430, Windows locale en-ZA,
//! ANSI code page 1252) gave another result in a spot check: CHAR and CODE read and write the
//! code page 1252 (CHAR(128) is U+20AC, CODE of a character outside the code page is 63), and
//! SEARCH folds case beyond ASCII (SEARCH("é","XÉ") is 2).

use crate::value::{EXCEL_TEXT_MAX_UTF16_CODE_UNITS, ExcelText, WorksheetErrorCode};

pub(crate) fn text(units: Vec<u16>) -> ExcelText {
    ExcelText::from_utf16_code_units(units)
}

pub(crate) fn text_from_str(s: &str) -> ExcelText {
    ExcelText::from_utf16_code_units(s.encode_utf16().collect())
}

/// Observed date-argument conversion (OxFunc date_fn.rs): the inclusive boundary is a two-step
/// binary rounding rule. LEFT and RIGHT share it.
pub(crate) fn date_integer_floor(number: f64) -> f64 {
    let upper = number.ceil();
    let threshold = if upper > 0.0 {
        2.0_f64.powi(-22) + 2.0_f64.powi(-33)
    } else {
        2.0_f64.powi(-23) + 2.0_f64.powi(-34)
    };
    if upper - number <= threshold {
        upper
    } else {
        number.floor()
    }
}

// --- slices (text_slice_family.rs) --------------------------------------------------------------

/// A character count from a number: LEFT and RIGHT (`tolerant`) use the date-argument floor,
/// MID plain truncation with signed 32-bit admission.
pub(crate) fn nonnegative_count(n: f64, tolerant: bool) -> Result<usize, WorksheetErrorCode> {
    if !n.is_finite() || n < 0.0 || (!tolerant && n >= 2147483648.0) {
        return Err(WorksheetErrorCode::Value);
    }
    let truncated = if tolerant {
        date_integer_floor(n)
    } else {
        n.trunc()
    };
    if truncated < 0.0 {
        return Err(WorksheetErrorCode::Value);
    }
    if truncated > usize::MAX as f64 {
        return Ok(usize::MAX);
    }
    Ok(truncated as usize)
}

/// MID's one-based start.
pub(crate) fn mid_start(n: f64) -> Result<usize, WorksheetErrorCode> {
    if !n.is_finite() || n >= 2147483648.0 {
        return Err(WorksheetErrorCode::Value);
    }
    let truncated = n.trunc();
    if truncated < 1.0 {
        return Err(WorksheetErrorCode::Value);
    }
    Ok(truncated as usize)
}

fn unit_width(units: &[u16], offset: usize) -> usize {
    if (0xD800..=0xDBFF).contains(&units[offset])
        && units.get(offset + 1).is_some_and(|next| (0xDC00..=0xDFFF).contains(next)) {
        2
    } else {
        1
    }
}

fn character_boundary(text: &ExcelText, count: usize) -> usize {
    let units = text.utf16_code_units();
    let (mut offset, mut seen) = (0, 0);
    while offset < units.len() && seen < count {
        offset += unit_width(units, offset);
        seen += 1;
    }
    offset
}

pub(crate) fn slice_character_count(text: &ExcelText) -> usize {
    std::char::decode_utf16(text.utf16_code_units().iter().copied()).count()
}

fn character_scan_end(text: &ExcelText) -> usize {
    text.len_utf16_code_units()
        - usize::from(
            text.utf16_code_units()
                .last()
                .is_some_and(|unit| (0xD800..=0xDBFF).contains(unit)),
        )
}

/// The characters LEFT counts: a surrogate pair once, and a trailing unmatched high surrogate not
/// at all.
fn len_character_count(text: &ExcelText) -> usize {
    slice_character_count(text)
        - usize::from(character_scan_end(text) < text.len_utf16_code_units())
}

pub(crate) fn take_left(text: &ExcelText, count: usize) -> ExcelText {
    // A forward scan that stops at an unmatched final high surrogate before reaching count falls
    // back to the raw unit count, as Excel does.
    let end = if character_scan_end(text) < text.len_utf16_code_units()
        && count > len_character_count(text)
    {
        count.min(text.len_utf16_code_units())
    } else {
        character_boundary(text, count)
    };
    ExcelText::from_utf16_code_units(text.utf16_code_units()[..end].to_vec())
}

pub(crate) fn take_right(text: &ExcelText, count: usize) -> ExcelText {
    let start = character_boundary(text, slice_character_count(text).saturating_sub(count));
    ExcelText::from_utf16_code_units(text.utf16_code_units()[start..].to_vec())
}

/// MID counts valid surrogate pairs once in Version 2; MIDB keeps its UTF-16 unit policy.
pub(crate) fn take_mid(text: &ExcelText, start_one_based: usize, count: usize, pairs: bool) -> ExcelText {
    let units = text.utf16_code_units();
    let first = start_one_based.saturating_sub(1);
    let start = if pairs { character_boundary(text, first) } else { first };
    if count == 0 || start >= units.len() {
        return ExcelText::from_utf16_code_units(Vec::new());
    }
    let end = if pairs {
        character_boundary(text, first.saturating_add(count))
    } else {
        start.saturating_add(count).min(units.len())
    };
    ExcelText::from_utf16_code_units(units[start..end].to_vec())
}

// --- search and replace (text_search_replace_family.rs) ----------------------------------------

fn truncated(n: f64) -> Result<f64, WorksheetErrorCode> {
    if !n.is_finite() {
        return Err(WorksheetErrorCode::Value);
    }
    Ok(n.trunc())
}

/// A one-based start or instance number: at least 1.
pub(crate) fn positive(n: f64) -> Result<usize, WorksheetErrorCode> {
    let t = truncated(n)?;
    if t < 1.0 {
        return Err(WorksheetErrorCode::Value);
    }
    if t > usize::MAX as f64 {
        return Ok(usize::MAX);
    }
    Ok(t as usize)
}

/// A count of at least 0.
pub(crate) fn nonnegative(n: f64) -> Result<usize, WorksheetErrorCode> {
    let t = truncated(n)?;
    if t < 0.0 {
        return Err(WorksheetErrorCode::Value);
    }
    if t > usize::MAX as f64 {
        return Ok(usize::MAX);
    }
    Ok(t as usize)
}

/// A UTF-16 unit with its case folded for SEARCH: the simple lower-case mapping of a unit that is
/// a whole character and maps to one character of one unit; any other unit as it is.
pub(crate) fn fold_unit(unit: u16) -> u16 {
    if unit < 0x80 {
        return if (b'A' as u16..=b'Z' as u16).contains(&unit) {
            unit + 32
        } else {
            unit
        };
    }
    let Some(ch) = char::from_u32(u32::from(unit)) else {
        return unit;
    };
    let mut lower = ch.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(single), None) if (single as u32) <= 0xFFFF => single as u32 as u16,
        _ => unit,
    }
}

fn search_units_equal(lhs: u16, rhs: u16) -> bool {
    lhs == rhs || fold_unit(lhs) == fold_unit(rhs)
}

pub(crate) fn substitute(
    text: &ExcelText,
    old_text: &ExcelText,
    new_text: &ExcelText,
    instance_num: Option<usize>,
) -> ExcelText {
    let hay = text.utf16_code_units();
    let needle = old_text.utf16_code_units();
    let replacement = new_text.utf16_code_units();
    if needle.is_empty() {
        return text.clone();
    }
    let mut out = Vec::with_capacity(hay.len());
    let mut index = 0usize;
    let mut occurrence = 0usize;
    while index < hay.len() {
        if index + needle.len() <= hay.len() && &hay[index..index + needle.len()] == needle {
            occurrence += 1;
            if instance_num.is_none() || instance_num == Some(occurrence) {
                out.extend_from_slice(replacement);
                index += needle.len();
                continue;
            }
        }
        out.push(hay[index]);
        index += 1;
    }
    ExcelText::from_utf16_code_units(out)
}

pub(crate) fn replace(
    old_text: &ExcelText,
    start_num: usize,
    num_chars: usize,
    new_text: &ExcelText,
    pairs: bool,
) -> ExcelText {
    let units = old_text.utf16_code_units();
    let first = start_num.saturating_sub(1);
    let start_index = if pairs { character_boundary(old_text, first) } else { first.min(units.len()) };
    let end_index = if pairs {
        character_boundary(old_text, first.saturating_add(num_chars))
    } else {
        start_index.saturating_add(num_chars).min(units.len())
    };
    let mut out = Vec::with_capacity(
        start_index + new_text.len_utf16_code_units() + units.len().saturating_sub(end_index),
    );
    out.extend_from_slice(&units[..start_index]);
    out.extend_from_slice(new_text.utf16_code_units());
    out.extend_from_slice(&units[end_index..]);
    ExcelText::from_utf16_code_units(out)
}

pub(crate) fn find(
    find_text: &ExcelText,
    within_text: &ExcelText,
    start_num: usize,
    pairs: bool,
) -> Result<f64, WorksheetErrorCode> {
    let needle = find_text.utf16_code_units();
    let hay = within_text.utf16_code_units();
    if needle.is_empty() {
        let length = if pairs { slice_character_count(within_text) } else { hay.len() };
        if start_num <= length + 1 {
            return Ok(start_num as f64);
        }
        return Err(WorksheetErrorCode::Value);
    }
    let mut index = if pairs { character_boundary(within_text, start_num - 1) } else { start_num - 1 };
    let mut position = start_num;
    while index < hay.len() && needle.len() <= hay.len() - index {
        if &hay[index..index + needle.len()] == needle {
            return Ok(position as f64);
        }
        index += if pairs { unit_width(hay, index) } else { 1 };
        position += 1;
    }
    Err(WorksheetErrorCode::Value)
}

/// Whether the SEARCH pattern (`*` any run, `?` one counted position, `~` escaping the next) matches a prefix
/// of `text`.
fn search_pattern_matches_prefix(pattern: &[u16], text: &[u16], pairs: bool) -> bool {
    // memo[p * (t_len + 1) + t]: 0 unknown, 1 false, 2 true.
    let width = text.len() + 1;
    let mut memo = vec![0u8; (pattern.len() + 1) * width];
    fn rec(pattern: &[u16], text: &[u16], p: usize, t: usize, memo: &mut [u8], width: usize, pairs: bool) -> bool {
        let slot = p * width + t;
        if memo[slot] != 0 {
            return memo[slot] == 2;
        }
        let result = if p == pattern.len() {
            true
        } else {
            match pattern[p] {
                42 => {
                    if p + 1 == pattern.len() {
                        true
                    } else {
                        let mut next = t;
                        loop {
                            if rec(pattern, text, p + 1, next, memo, width, pairs) { break true; }
                            if next == text.len() { break false; }
                            next += if pairs { unit_width(text, next) } else { 1 };
                        }
                    }
                }
                63 => t < text.len() && rec(pattern, text, p + 1,
                    t + if pairs { unit_width(text, t) } else { 1 }, memo, width, pairs),
                126 if p + 1 < pattern.len() => {
                    t < text.len()
                        && search_units_equal(pattern[p + 1], text[t])
                        && rec(pattern, text, p + 2, t + 1, memo, width, pairs)
                }
                ch => {
                    t < text.len()
                        && search_units_equal(ch, text[t])
                        && rec(pattern, text, p + 1, t + 1, memo, width, pairs)
                }
            }
        };
        memo[slot] = if result { 2 } else { 1 };
        result
    }
    rec(pattern, text, 0, 0, &mut memo, width, pairs)
}

pub(crate) fn search(
    find_text: &ExcelText,
    within_text: &ExcelText,
    start_num: usize,
    pairs: bool,
) -> Result<f64, WorksheetErrorCode> {
    let pattern = find_text.utf16_code_units();
    let hay = within_text.utf16_code_units();
    if pattern.is_empty() {
        if hay.is_empty() && start_num == 1 {
            return Ok(1.0);
        }
        let length = if pairs { slice_character_count(within_text) } else { hay.len() };
        if start_num <= length {
            return Ok(start_num as f64);
        }
        return Err(WorksheetErrorCode::Value);
    }
    let mut index = if pairs { character_boundary(within_text, start_num - 1) } else { start_num - 1 };
    let mut position = start_num;
    while index < hay.len() {
        if search_pattern_matches_prefix(pattern, &hay[index..], pairs) {
            return Ok(position as f64);
        }
        index += if pairs { unit_width(hay, index) } else { 1 };
        position += 1;
    }
    Err(WorksheetErrorCode::Value)
}

// --- single texts (text_scalar_misc.rs, text_unicode_fn.rs, clean_fn.rs, exact_fn.rs) ----------

/// Code page 1252's characters at 0x80..=0x9F; an undefined position maps to the C1 control of
/// the same number, as Windows' conversion does.
const CP1252_HIGH: [u16; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160,
    0x2039, 0x0152, 0x008D, 0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022,
    0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
];

/// CHAR: the character with code `n` (truncated) of code page 1252, 1 to 255.
pub(crate) fn char_from_number(n: f64) -> Result<ExcelText, WorksheetErrorCode> {
    let n = n.trunc();
    if !(1.0..=255.0).contains(&n) {
        return Err(WorksheetErrorCode::Value);
    }
    let code = n as u16;
    let unit = if (0x80..=0x9F).contains(&code) {
        CP1252_HIGH[usize::from(code - 0x80)]
    } else {
        code
    };
    Ok(ExcelText::from_utf16_code_units(vec![unit]))
}

/// CODE: the code page 1252 code of the first character, 63 (`?`) for a character outside it;
/// empty text is #VALUE!.
pub(crate) fn code_of_text(text: &ExcelText) -> Result<f64, WorksheetErrorCode> {
    let Some(&unit) = text.utf16_code_units().first() else {
        return Err(WorksheetErrorCode::Value);
    };
    if unit < 0x80 || (0xA0..=0xFF).contains(&unit) {
        return Ok(f64::from(unit));
    }
    match CP1252_HIGH.iter().position(|&mapped| mapped == unit) {
        Some(at) => Ok((0x80 + at) as f64),
        None => Ok(63.0),
    }
}

pub(crate) fn trim_ascii_spaces(text: &ExcelText) -> ExcelText {
    let mut out = Vec::new();
    let mut pending_space = false;
    let mut started = false;
    for unit in text.utf16_code_units() {
        if *unit == 32 {
            if started {
                pending_space = true;
            }
            continue;
        }
        if pending_space && !out.is_empty() {
            out.push(32);
        }
        out.push(*unit);
        started = true;
        pending_space = false;
    }
    ExcelText::from_utf16_code_units(out)
}

pub(crate) fn rept(text: &ExcelText, count: f64) -> Result<ExcelText, WorksheetErrorCode> {
    let count = count.trunc();
    if count < 0.0 {
        return Err(WorksheetErrorCode::Value);
    }
    let units = text.utf16_code_units();
    if count > EXCEL_TEXT_MAX_UTF16_CODE_UNITS as f64
        || units.len().saturating_mul(count as usize) > EXCEL_TEXT_MAX_UTF16_CODE_UNITS
    {
        return Err(WorksheetErrorCode::Value);
    }
    let count = count as usize;
    let mut out = Vec::with_capacity(units.len().saturating_mul(count));
    for _ in 0..count {
        out.extend_from_slice(units);
    }
    Ok(ExcelText::from_utf16_code_units(out))
}

pub(crate) fn unichar(n: f64) -> Result<ExcelText, WorksheetErrorCode> {
    let n = n.trunc();
    if !n.is_finite() || !(1.0..=0x10FFFF as f64).contains(&n) {
        return Err(WorksheetErrorCode::Value);
    }
    let codepoint = n as u32;
    if (0xD800..=0xDFFF).contains(&codepoint) {
        return Err(WorksheetErrorCode::NA);
    }
    let scalar = char::from_u32(codepoint).ok_or(WorksheetErrorCode::Value)?;
    Ok(text_from_str(&scalar.to_string()))
}

pub(crate) fn unicode(text: &ExcelText) -> Result<f64, WorksheetErrorCode> {
    let mut decoded = std::char::decode_utf16(text.utf16_code_units().iter().copied());
    match decoded.next() {
        Some(Ok(scalar)) => Ok(scalar as u32 as f64),
        Some(Err(_)) | None => Err(WorksheetErrorCode::Value),
    }
}

pub(crate) fn clean(text: &ExcelText) -> ExcelText {
    let filtered: Vec<u16> = text
        .utf16_code_units()
        .iter()
        .copied()
        .filter(|&u| !(u < 32 || matches!(u, 129 | 141 | 143 | 144 | 157)))
        .collect();
    ExcelText::from_utf16_code_units(filtered)
}

// --- casing (excel_casing.rs) ------------------------------------------------------------------

fn push_singleton_or_original(out: &mut String, mapped: impl Iterator<Item = char>, original: char) {
    let mut chars = mapped;
    match (chars.next(), chars.next()) {
        (Some(single), None) => out.push(single),
        _ => out.push(original),
    }
}

fn push_upper(out: &mut String, ch: char) {
    // Excel maps individual BMP code units. A supplementary letter's surrogate pair stays
    // unchanged (fn1 UPPER, U+10428), as do non-expanding ligatures and sharp s.
    if ch as u32 > 0xFFFF {
        out.push(ch);
        return;
    }
    match ch {
        'ß' => out.push('ß'),
        'ά' => out.push('Α'),
        'έ' => out.push('Ε'),
        'ή' => out.push('Η'),
        'ί' => out.push('Ι'),
        'ό' => out.push('Ο'),
        'ύ' => out.push('Υ'),
        'ώ' => out.push('Ω'),
        'ΐ' => out.push('Ϊ'),
        'ΰ' => out.push('Ϋ'),
        _ => push_singleton_or_original(out, ch.to_uppercase(), ch),
    }
}

fn adjacent_letter(chars: &[char], index: usize, offset: isize) -> bool {
    let adjacent = index as isize + offset;
    adjacent >= 0 && (adjacent as usize) < chars.len() && chars[adjacent as usize].is_alphabetic()
}

fn push_lower(out: &mut String, ch: char, is_final_sigma: bool) {
    // The delivered fn3 LOWER/PROPER readings preserve supplementary pairs.
    if ch as u32 > 0xFFFF {
        out.push(ch);
        return;
    }
    match ch {
        'İ' => out.push('i'),
        'ẞ' => out.push('ẞ'),
        'Σ' if is_final_sigma => out.push('ς'),
        _ => push_singleton_or_original(out, ch.to_lowercase(), ch),
    }
}

/// Whether a text is valid UTF-16; the casing kernels read characters and refuse other text.
pub(crate) fn is_well_formed(text: &ExcelText) -> bool {
    std::char::decode_utf16(text.utf16_code_units().iter().copied()).all(|c| c.is_ok())
}

pub(crate) fn upper(text: &ExcelText) -> ExcelText {
    let mut out = String::new();
    for ch in text.to_string_lossy().chars() {
        push_upper(&mut out, ch);
    }
    text_from_str(&out)
}

pub(crate) fn lower(text: &ExcelText) -> ExcelText {
    let chars = text.to_string_lossy().chars().collect::<Vec<_>>();
    let mut out = String::new();
    let mut previous_letter = false;
    for (index, ch) in chars.iter().copied().enumerate() {
        // The fn1 sigma controls skip preceding marks, while a following mark keeps the
        // nonfinal form even at the end: ΆΣ -> άς; ΑΣ́ -> ασ́.
        let is_final_sigma = ch == 'Σ' && previous_letter && !chars.get(index + 1)
            .is_some_and(|&c| c.is_alphabetic() || calc_value::is_combining_mark(c));
        push_lower(&mut out, ch, is_final_sigma);
        if !calc_value::is_combining_mark(ch) {
            previous_letter = ch.is_alphabetic();
        }
    }
    text_from_str(&out)
}

pub(crate) fn proper(text: &ExcelText) -> ExcelText {
    let chars = text.to_string_lossy().chars().collect::<Vec<_>>();
    let mut out = String::new();
    let mut start_of_word = true;
    for (index, ch) in chars.iter().copied().enumerate() {
        // PROPER treats a supplementary pair as a word boundary (fn3-proper-002).
        if ch as u32 <= 0xFFFF && ch.is_alphabetic() {
            if start_of_word {
                push_upper(&mut out, ch);
            } else {
                let is_final_sigma = ch == 'Σ'
                    && adjacent_letter(&chars, index, -1)
                    && !chars.get(index + 1).is_some_and(|&next|
                        next.is_alphabetic() || calc_value::is_combining_mark(next));
                push_lower(&mut out, ch, is_final_sigma);
            }
            start_of_word = false;
        } else {
            out.push(ch);
            start_of_word = true;
        }
    }
    text_from_str(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> ExcelText {
        text_from_str(s)
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn char_and_code_read_code_page_1252() {
        assert_eq!(char_from_number(128.0), Ok(t("\u{20AC}")));
        assert_eq!(char_from_number(65.9), Ok(t("A")));
        assert_eq!(char_from_number(0.0), Err(WorksheetErrorCode::Value));
        assert_eq!(code_of_text(&t("\u{20AC}")), Ok(128.0));
        assert_eq!(code_of_text(&t("\u{03E8}")), Ok(63.0));
        assert_eq!(code_of_text(&t("")), Err(WorksheetErrorCode::Value));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn search_folds_case_beyond_ascii_and_reads_wildcards() {
        assert_eq!(search(&t("é"), &t("XÉ"), 1, false), Ok(2.0));
        assert_eq!(search(&t("σ"), &t("ΑΣ"), 1, false), Ok(2.0));
        assert_eq!(search(&t("b*d"), &t("abcde"), 1, false), Ok(2.0));
        assert_eq!(search(&t("~*"), &t("a*b"), 1, false), Ok(2.0));
        assert_eq!(find(&t("B"), &t("abc"), 1, false), Err(WorksheetErrorCode::Value));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn slices_count_surrogate_pairs_once() {
        let emoji = text(vec![0xD83D, 0xDE00, 0x41]);
        assert_eq!(len_character_count(&emoji), 2);
        assert_eq!(take_left(&emoji, 1), text(vec![0xD83D, 0xDE00]));
        assert_eq!(take_right(&emoji, 1), t("A"));
        // MID counts code units (dc-fl2).
        assert_eq!(take_mid(&emoji, 2, 5, false), text(vec![0xDE00, 0x41]));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn lower_writes_a_final_sigma() {
        assert_eq!(lower(&t("ΑΣ")), t("ας"));
        assert_eq!(upper(&t("straße")), t("STRAßE"));
        assert_eq!(proper(&t("hello wORLD")), t("Hello World"));
    }
}
