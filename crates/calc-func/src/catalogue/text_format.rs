//! The text functions that read or write number formats, dates and other text forms, ported from
//! OxFunc (functions/ at 6fca3518): value_fn.rs, text_fn.rs, fixed_fn.rs, dollar_fn.rs,
//! number_regex_translate_family.rs, valuetotext_fn.rs, array_text_split_family.rs,
//! text_delim_family.rs, text_compat_locale_family.rs (ASC and DBCS; Excel on Windows has no
//! JIS), web_text_xml_family.rs and hyperlink_fn.rs. A call whose result is not modelled is refused as not modelled.

use super::kit::{fail, flag, grid, num, text_arg};
use crate::call::EvalServices;
use crate::ported::text_kernels as tk;
use crate::ported::text_more::{self as k, Delimited, Side};
use crate::value::{CalcArray, ExcelText};
use crate::locale::LocaleId;
use crate::format::{self, Failure};
use crate::locale::{CalcEnvironment, FormatProfile};
use crate::coerce::{self, Numeric};
use crate::kernel::{Exit, Site, Step};
use crate::value::{CalcValue, CoreRef, WorksheetErrorCode};
use crate::value_text::{self, ReadContext, Reading, Unread};

const CODE: u16 = 202;

/// Text argument `index` read as VALUE reads it under the call's locale profile
/// ([`crate::value_text`]): #VALUE! for text the grammar does not read and for a value that is
/// not text; an error is the result; refused without a profile.
pub(super) fn read_value(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<Reading> {
    match a[index].core_ref() {
        CoreRef::Text(text) => {
            let environment = site.environment().ok_or_else(|| site.not_modelled(None))?;
            let profile = environment
                .profile
                .as_ref()
                .ok_or_else(|| site.not_modelled(None))?;
            let context = ReadContext {
                profile,
                date_system: environment.date_system,
                now: environment.now,
            };
            value_text::read(text.utf16_code_units(), &context).map_err(|unread| match unread {
                Unread::Rejected => Exit::Error(WorksheetErrorCode::Value),
                Unread::NotModelled => site.not_modelled(Some(index)),
            })
        }
        CoreRef::Error(code) => fail(code),
        _ if a[index].callable_value().is_some() => Err(site.not_modelled(Some(index))),
        _ => fail(WorksheetErrorCode::Value),
    }
}

/// VALUE: a number as it is, a blank as 0, a logical #VALUE!, text read as number text, a date
/// or a time.
fn value(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    match a[0].core_ref() {
        CoreRef::Number(_) | CoreRef::Empty | CoreRef::Missing => {
            site.number(site.number_argument(0, &a[0])?)
        }
        CoreRef::Logical(_) => fail(WorksheetErrorCode::Value),
        CoreRef::Text(_) => {
            if let Ok(Numeric::Decimal(decimal)) = coerce::number(&a[0]) {
                return site.number(site.argument(0, decimal.nearest())?);
            }
            let reading = read_value(site, a, 0)?;
            site.number(reading.value())
        }
        _ => read_value(site, a, 0).and_then(|reading| site.number(reading.value())),
    }
}

/// The call's environment and its locale profile, or the refusal of a call without them.
fn profile<'a>(site: &'a Site<'_>) -> Step<(&'a CalcEnvironment, &'a FormatProfile)> {
    let environment = site.environment().ok_or_else(|| site.not_modelled(None))?;
    let profile = environment
        .profile
        .as_ref()
        .ok_or_else(|| site.not_modelled(None))?;
    Ok((environment, profile))
}

fn rendered(site: &Site<'_>, result: Result<String, Failure>) -> Step<CalcValue> {
    match result {
        Ok(text) => Ok(crate::catalogue::kit::text_value(&text)),
        Err(Failure::Value) => fail(WorksheetErrorCode::Value),
        Err(Failure::NotModelled) => Err(site.not_modelled(None)),
    }
}

/// TEXT: a number (or text that reads as one) rendered with the code, other text through the
/// code's text section.
fn text(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let (environment, profile) = profile(site)?;
    let code = text_arg(site, a, 1)?.to_string_lossy();
    let number = match a[0].core_ref() {
        CoreRef::Number(_) | CoreRef::Empty | CoreRef::Missing => {
            site.number_argument(0, &a[0])?
        }
        CoreRef::Error(code) => return fail(code),
        CoreRef::Logical(_) | CoreRef::Text(_) => {
            let as_number = if matches!(a[0].core_ref(), CoreRef::Text(_)) {
                match site.number_argument(0, &a[0]) {
                    Ok(n) => Some(n),
                    Err(Exit::Error(WorksheetErrorCode::Value)) => None,
                    Err(other) => return Err(other),
                }
            } else {
                None
            };
            match as_number {
                Some(n) => n,
                None => {
                    let shown = text_arg(site, a, 0)?.to_string_lossy();
                    return rendered(site, format::text_of_text(&shown, &code, profile));
                }
            }
        }
        _ => return Err(site.not_modelled(Some(0))),
    };
    rendered(
        site,
        format::text(profile, environment.date_system, number, &code),
    )
}

/// The places of FIXED and DOLLAR: 2 when the call leaves them out; beyond 127 is #VALUE!.
fn places(site: &Site<'_>, a: &[CalcValue]) -> Step<i32> {
    if a.len() < 2 || a[1].is_missing() {
        return Ok(2);
    }
    let places = num(site, a, 1)?.trunc();
    if places > 127.0 {
        return fail(WorksheetErrorCode::Value);
    }
    Ok(places.max(-400.0) as i32)
}

fn fixed(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let (_, profile) = profile(site)?;
    let number = num(site, a, 0)?;
    let places = places(site, a)?;
    let no_commas = a.len() > 2 && flag(site, a, 2)?;
    rendered(site, Ok(format::fixed(profile, number, places, no_commas)))
}

fn dollar(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let (_, profile) = profile(site)?;
    let number = num(site, a, 0)?;
    let places = places(site, a)?;
    rendered(site, Ok(format::dollar(profile, number, places)))
}

/// An optional argument: `None` when the call does not reach it or leaves it empty.
fn given(a: &[CalcValue], index: usize) -> bool {
    a.get(index).is_some_and(|value| !value.is_missing())
}

/// A 0-or-1 flag argument (match modes, formats): another number is #VALUE!.
fn binary_flag(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<bool> {
    if !given(a, index) {
        return Ok(false);
    }
    let n = num(site, a, index)?;
    if !n.is_finite() {
        return fail(WorksheetErrorCode::Value);
    }
    match n.trunc() {
        0.0 => Ok(false),
        1.0 => Ok(true),
        _ => fail(WorksheetErrorCode::Value),
    }
}

/// The texts a delimiter argument gives: a single text, or each cell of an array or reference.
fn delimiters(
    site: &Site<'_>,
    a: &[CalcValue],
    index: usize,
    sv: &mut EvalServices<'_>,
) -> Step<Vec<ExcelText>> {
    let g = grid(site, a, index, sv)?;
    let mut out = Vec::with_capacity(g.cells.len());
    for value in &g.cells {
        if matches!(value.core_ref(), CoreRef::Array(_) | CoreRef::Reference(_)) {
            return Err(site.not_modelled(Some(index)));
        }
        let units = site.argument(index, coerce::text(value))?;
        out.push(ExcelText::from_utf16_code_units(units.into_owned()));
    }
    Ok(out)
}

fn text_delim(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>, side: Side) -> Step<CalcValue> {
    let text = text_arg(site, a, 0)?;
    let delimiters = delimiters(site, a, 1, sv)?;
    let instance = if given(a, 2) {
        let n = num(site, a, 2)?;
        if !n.is_finite() || n.trunc().abs() > 1e9 {
            return fail(WorksheetErrorCode::Value);
        }
        n.trunc() as isize
    } else {
        1
    };
    let case_insensitive = binary_flag(site, a, 3)?;
    let match_end = binary_flag(site, a, 4)?;
    // fn1 Excel 16.0.20527: an explicitly supplied instance is validated against the empty
    // input before fallback/match_end. An empty delimiter still returns empty text.
    if given(a, 2) && text.len_utf16_code_units() == 0
        && !delimiters.iter().any(|d| d.len_utf16_code_units() == 0)
    {
        return fail(WorksheetErrorCode::Value);
    }
    match k::delimited(side, &text, &delimiters, instance, case_insensitive, match_end) {
        Delimited::Text(found) => Ok(CalcValue::text(found)),
        Delimited::BadInstance => fail(WorksheetErrorCode::Value),
        Delimited::NotFound if a.len() > 5 => Ok(if a[5].is_missing() {
            CalcValue::error(WorksheetErrorCode::NA)
        } else {
            a[5].clone()
        }),
        Delimited::NotFound => fail(WorksheetErrorCode::NA),
    }
}

/// An array result, or its single cell when it has one.
fn array_or_cell(site: &Site<'_>, rows: Vec<Vec<CalcValue>>) -> Step<CalcValue> {
    if rows.len() == 1 && rows[0].len() == 1 {
        return Ok(rows.into_iter().next().unwrap().into_iter().next().unwrap());
    }
    CalcArray::from_rows(rows)
        .map(CalcValue::array)
        .ok_or_else(|| site.not_modelled(None))
}

fn textsplit(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let text = text_arg(site, a, 0)?;
    let columns = if a[1].is_missing() {
        None
    } else {
        Some(delimiters(site, a, 1, sv)?)
    };
    let rows_by = if given(a, 2) {
        Some(delimiters(site, a, 2, sv)?)
    } else {
        None
    };
    if columns.is_none() && rows_by.is_none() {
        return fail(WorksheetErrorCode::Value);
    }
    let ignore_empty = binary_flag(site, a, 3)?;
    let case_insensitive = binary_flag(site, a, 4)?;
    let pad = match a.get(5) {
        None => CalcValue::error(WorksheetErrorCode::NA),
        Some(value) if value.is_missing() => CalcValue::error(WorksheetErrorCode::NA),
        Some(value) if matches!(value.core_ref(), CoreRef::Empty) => CalcValue::number(0.0),
        Some(value) => value.clone(),
    };
    // Empty input has no text to split. Excel's typed Value2 observation
    // w111typed-00282-textsplit-arg1-empty-text is #VALUE!, not a cell of empty text (dc-zsn.7).
    if text.len_utf16_code_units() == 0 {
        return fail(WorksheetErrorCode::Value);
    }
    let mut row_texts = match &rows_by {
        Some(d) => k::split(&text, d, ignore_empty, case_insensitive),
        None => vec![text.clone()],
    };
    if row_texts.is_empty() {
        row_texts.push(ExcelText::from_utf16_code_units(Vec::new()));
    }
    let mut rows: Vec<Vec<CalcValue>> = Vec::new();
    for row in row_texts {
        let mut cells = match &columns {
            Some(d) => k::split(&row, d, ignore_empty, case_insensitive),
            None => vec![row],
        };
        if cells.is_empty() {
            cells.push(ExcelText::from_utf16_code_units(Vec::new()));
        }
        rows.push(cells.into_iter().map(CalcValue::text).collect());
    }
    let width = rows.iter().map(Vec::len).max().unwrap_or(1);
    for row in &mut rows {
        while row.len() < width {
            row.push(pad.clone());
        }
    }
    array_or_cell(site, rows)
}

/// A value's text as VALUETOTEXT and ARRAYTOTEXT write it, concise or strict (text quoted).
fn value_text(site: &Site<'_>, index: usize, value: &CalcValue, strict: bool) -> Step<String> {
    Ok(match value.core_ref() {
        CoreRef::Number(_) => {
            String::from_utf16_lossy(&site.argument(index, coerce::text(value))?)
        }
        CoreRef::Text(t) if strict => format!("\"{}\"", t.to_string_lossy().replace('"', "\"\"")),
        CoreRef::Text(t) => t.to_string_lossy(),
        CoreRef::Logical(b) => if b { "TRUE" } else { "FALSE" }.to_owned(),
        CoreRef::Error(code) => k::error_literal(code).to_owned(),
        CoreRef::Empty | CoreRef::Missing => String::new(),
        CoreRef::Array(_) | CoreRef::Reference(_) => return Err(site.not_modelled(Some(index))),
    })
}

fn valuetotext(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let strict = binary_flag(site, a, 1)?;
    let shown = value_text(site, 0, &a[0], strict)?;
    Ok(crate::catalogue::kit::text_value(&shown))
}

fn arraytotext(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    // An array that is one error value is the result; an error inside an array is shown (Excel
    // 16.0.20430, 2026-10-25, dc-6au: `=ARRAYTOTEXT(#REF!)` #REF!, `=ARRAYTOTEXT(#DIV/0!,1)`
    // #DIV/0!, `=ARRAYTOTEXT({1,#N/A})` "1, #N/A").
    if let CoreRef::Error(code) = a[0].core_ref() {
        return fail(code);
    }
    let strict = binary_flag(site, a, 1)?;
    let g = grid(site, a, 0, sv)?;
    let mut parts = Vec::with_capacity(g.cells.len());
    for value in &g.cells {
        parts.push(value_text(site, 0, value, strict)?);
    }
    let shown = if strict {
        let rows: Vec<String> = parts.chunks(g.cols.max(1)).map(|row| row.join(",")).collect();
        format!("{{{}}}", rows.join(";"))
    } else {
        parts.join(", ")
    };
    Ok(crate::catalogue::kit::text_value(&shown))
}

/// NUMBERVALUE: the separators default to the locale's.
fn numbervalue(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let text = text_arg(site, a, 0)?;
    let separator = |index: usize, default: &str| -> Step<Option<char>> {
        let given_text;
        let text = if given(a, index) {
            given_text = text_arg(site, a, index)?.to_string_lossy();
            given_text.as_str()
        } else {
            default
        };
        match text.chars().next() {
            Some(c) => Ok(Some(c)),
            None => fail(WorksheetErrorCode::Value),
        }
    };
    let (decimal_default, group_default) = if given(a, 1) && given(a, 2) {
        (".", ",")
    } else {
        let (_, profile) = profile(site)?;
        (profile.decimal_separator, profile.thousands_separator)
    };
    let decimal = separator(1, decimal_default)?;
    let group = separator(2, group_default)?;
    out_number(site, k::numbervalue(&text, decimal, group))
}

fn out_number(site: &Site<'_>, result: Result<f64, WorksheetErrorCode>) -> Step<CalcValue> {
    match result {
        Ok(n) => site.number(n),
        Err(code) => fail(code),
    }
}

/// The case sensitivity of the REGEX functions: 0 (the default) case-sensitive, 1 not.
fn case_sensitive(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<bool> {
    Ok(!binary_flag(site, a, index)?)
}

fn regex(
    site: &Site<'_>,
    text: &ExcelText,
    pattern: &ExcelText,
    case_sensitive: bool,
    first_only: bool,
) -> Step<(Vec<char>, Vec<(usize, usize)>)> {
    if !tk::is_well_formed(text) || !tk::is_well_formed(pattern) {
        return Err(site.not_modelled(None));
    }
    k::regex_matches(
        &text.to_string_lossy(),
        &pattern.to_string_lossy(),
        case_sensitive,
        first_only,
    )
    .map_err(|_| site.not_modelled(Some(1)))
}

fn regextest(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let text = text_arg(site, a, 0)?;
    let pattern = text_arg(site, a, 1)?;
    let sensitive = case_sensitive(site, a, 2)?;
    let (_, found) = regex(site, &text, &pattern, sensitive, true)?;
    Ok(CalcValue::logical(!found.is_empty()))
}

fn regexextract(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let text = text_arg(site, a, 0)?;
    let pattern = text_arg(site, a, 1)?;
    let mode = if given(a, 2) { num(site, a, 2)?.trunc() } else { 0.0 };
    let sensitive = case_sensitive(site, a, 3)?;
    let (chars, found) = regex(site, &text, &pattern, sensitive, mode == 0.0)?;
    match (mode as i64, found.as_slice()) {
        (_, []) if mode == 0.0 || mode == 1.0 => fail(WorksheetErrorCode::NA),
        (0, [(start, end), ..]) | (1, [(start, end)]) => Ok(crate::catalogue::kit::text_value(
            &chars[*start..*end].iter().collect::<String>(),
        )),
        // Several matches as an array, and capture groups (mode 2), are not modelled.
        (0..=2, _) => Err(site.not_modelled(Some(2))),
        _ => fail(WorksheetErrorCode::Value),
    }
}

fn regexreplace(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let text = text_arg(site, a, 0)?;
    let pattern = text_arg(site, a, 1)?;
    let replacement = text_arg(site, a, 2)?.to_string_lossy();
    if replacement.contains('$') || replacement.contains('\\') {
        // Group references in the replacement are not modelled.
        return Err(site.not_modelled(Some(2)));
    }
    let occurrence = if given(a, 3) { num(site, a, 3)?.trunc() } else { 0.0 };
    let sensitive = case_sensitive(site, a, 4)?;
    let (chars, found) = regex(site, &text, &pattern, sensitive, false)?;
    let selected: Vec<(usize, usize)> = if occurrence == 0.0 {
        found
    } else if occurrence > 0.0 {
        found.get(occurrence as usize - 1).copied().into_iter().collect()
    } else {
        let back = (-occurrence) as usize;
        found
            .len()
            .checked_sub(back)
            .and_then(|i| found.get(i).copied())
            .into_iter()
            .collect()
    };
    let mut out = String::new();
    let mut cursor = 0;
    for (start, end) in selected {
        out.extend(chars[cursor..start].iter());
        out.push_str(&replacement);
        cursor = end;
    }
    out.extend(chars[cursor..].iter());
    Ok(crate::catalogue::kit::text_value(&out))
}

/// ASC and DBCS change character widths only under a double-byte language; under every other
/// profile Excel gives the text as it is (Excel 16.0.20527 under en-ZA, the catalogue's sheet
/// dczsn3_bytes: full-width and half-width forms, kana, CJK and Hangul are kept). Under the
/// ja-JP, ko-KR and zh-CN profiles they are refused (bead dc-zsn.3).
fn width_conversion(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let (_, profile) = profile(site)?;
    if matches!(
        profile.id,
        LocaleId::JaJp | LocaleId::KoKr | LocaleId::ZhCn
    ) {
        return Err(site.not_modelled(None));
    }
    Ok(CalcValue::text(text_arg(site, a, 0)?))
}

/// PHONETIC under a language with no phonetic guide is #N/A, for a text cell and a blank one
/// alike (Excel under en-ZA: PHONETIC of a blank cell, and of a cell holding x in the catalogue
/// oracle's capture). Under Japanese it is not modelled.
fn phonetic(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let (_, profile) = profile(site)?;
    if matches!(profile.id, LocaleId::JaJp) {
        return Err(site.not_modelled(None));
    }
    let _ = (a, sv);
    fail(WorksheetErrorCode::NA)
}

/// HYPERLINK: the friendly name as it is, or the link location.
fn hyperlink(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let location = text_arg(site, a, 0)?;
    match a.get(1) {
        Some(value) if !value.is_missing() => match value.core_ref() {
            CoreRef::Empty => Ok(CalcValue::number(0.0)),
            CoreRef::Array(_) | CoreRef::Reference(_) => Err(site.not_modelled(Some(1))),
            _ if value.callable_value().is_some() => Err(site.not_modelled(Some(1))),
            _ => Ok(value.clone()),
        },
        _ => Ok(CalcValue::text(location)),
    }
}

/// BAHTTEXT's words: the digits 0 to 9, the places from tens to hundred thousands, and the
/// others, in Thai.
const THAI_DIGITS: [&str; 10] = [
    "\u{0e28}\u{0e39}\u{0e19}\u{0e22}\u{0e4c}",
    "\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}",
    "\u{0e2a}\u{0e2d}\u{0e07}",
    "\u{0e2a}\u{0e32}\u{0e21}",
    "\u{0e2a}\u{0e35}\u{0e48}",
    "\u{0e2b}\u{0e49}\u{0e32}",
    "\u{0e2b}\u{0e01}",
    "\u{0e40}\u{0e08}\u{0e47}\u{0e14}",
    "\u{0e41}\u{0e1b}\u{0e14}",
    "\u{0e40}\u{0e01}\u{0e49}\u{0e32}",
];
const THAI_PLACES: [&str; 6] = [
    "",
    "\u{0e2a}\u{0e34}\u{0e1a}",
    "\u{0e23}\u{0e49}\u{0e2d}\u{0e22}",
    "\u{0e1e}\u{0e31}\u{0e19}",
    "\u{0e2b}\u{0e21}\u{0e37}\u{0e48}\u{0e19}",
    "\u{0e41}\u{0e2a}\u{0e19}",
];
const THAI_MILLION: &str = "\u{0e25}\u{0e49}\u{0e32}\u{0e19}";
const THAI_ET: &str = "\u{0e40}\u{0e2d}\u{0e47}\u{0e14}";
const THAI_YI: &str = "\u{0e22}\u{0e35}\u{0e48}";
const THAI_BAHT: &str = "\u{0e1a}\u{0e32}\u{0e17}";
const THAI_EXACT: &str = "\u{0e16}\u{0e49}\u{0e27}\u{0e19}";
const THAI_SATANG: &str = "\u{0e2a}\u{0e15}\u{0e32}\u{0e07}\u{0e04}\u{0e4c}";
const THAI_MINUS: &str = "\u{0e25}\u{0e1a}";

/// A whole number (decimal digits, no leading zero) in Thai words: groups of six digits joined by
/// the word for a million; a one in the units after any other digit is "et", a two in the tens is
/// "yi", and a one in the tens is the tens word alone.
fn thai_number(digits: &str) -> String {
    let bytes = digits.as_bytes();
    let mut out = String::new();
    let length = bytes.len();
    for (at, &byte) in bytes.iter().enumerate() {
        let digit = usize::from(byte - b'0');
        let from_right = length - 1 - at;
        let place = from_right % 6;
        if digit != 0 {
            match place {
                0 if digit == 1 && at > 0 => out.push_str(THAI_ET),
                1 if digit == 1 => {}
                1 if digit == 2 => out.push_str(THAI_YI),
                _ => out.push_str(THAI_DIGITS[digit]),
            }
            out.push_str(THAI_PLACES[place]);
        }
        if place == 0 && from_right > 0 {
            out.push_str(THAI_MILLION);
        }
    }
    out
}

/// BAHTTEXT: the number rounded to satang (two places, from its 15-digit reading, a half away
/// from zero) in Thai words. Retained BAHTTEXT readings keep the original negative sign
/// even when rounding leaves zero baht and zero satang; signed zero itself has no minus word.
fn bahttext(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let n = num(site, a, 0)?;
    let (whole, fraction) = format::digits(n, 2);
    let satang = fraction.trim_start_matches('0');
    let negative = n < 0.0;
    let mut out = String::new();
    if negative {
        out.push_str(THAI_MINUS);
    }
    if !whole.is_empty() {
        out.push_str(&thai_number(&whole));
        out.push_str(THAI_BAHT);
    }
    if satang.is_empty() {
        if whole.is_empty() {
            out.push_str(THAI_DIGITS[0]);
            out.push_str(THAI_BAHT);
        }
        out.push_str(THAI_EXACT);
    } else {
        out.push_str(&thai_number(satang));
        out.push_str(THAI_SATANG);
    }
    Ok(crate::catalogue::kit::text_value(&out))
}

/// ENCODEURL: the text's UTF-8 bytes with every byte but the ASCII letters, digits, `-`, `.` and
/// `_` written as `%` and two upper-case hex digits; a number or logical is first read as text, as
/// `&` reads it. A result longer than a cell's text is #VALUE!. Excel 16.0.20430 (dc-uar, the
/// catalogue oracle's sheet dcuar_text): `~` is encoded, `ENCODEURL("a b")` is `a%20b`,
/// `ENCODEURL(1E+20)` is `1E%2B20`, and `LEN(ENCODEURL(REPT("é",10000)))` is #VALUE!. Text that is
/// not valid UTF-16 (a lone surrogate) is refused as not modelled.
fn encodeurl(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let text = super::kit::text(site, a, 0)?;
    let mut out: Vec<u16> = Vec::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_') {
            out.push(u16::from(byte));
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push(u16::from(b'%'));
            out.push(u16::from(HEX[usize::from(byte >> 4)]));
            out.push(u16::from(HEX[usize::from(byte & 15)]));
        }
        if out.len() > crate::value::EXCEL_TEXT_MAX_UTF16_CODE_UNITS {
            return fail(WorksheetErrorCode::Value);
        }
    }
    Ok(CalcValue::text(ExcelText::from_utf16_code_units(out)))
}

/// FILTERXML (dc-uar), ported from OxFunc (web_text_xml_family.rs at 6fca3518, which used
/// sxd-document and sxd-xpath) and fitted to Excel 16.0.20527, measured through the catalogue
/// oracle's dcuar_text rows on 2026-10-29:
///
/// - XML that does not parse (trailing content, a document type declaration) and XPath that does
///   not compile are #VALUE!; a result that is not a node set (`count(//b)`, `string(//b)`,
///   `//b=1`), an empty node set, and a node that is neither an element nor an attribute
///   (`//b/text()`, `//comment()`, `/`) are #VALUE!.
/// - An element's value is its own text children, joined (`<b>x<c>y</c>z</b>` is `xz`); text of
///   white space alone is dropped, as Excel's parser drops it. A node with no value makes the
///   whole call #VALUE!.
/// - Names match by namespace prefix as the document declares them (`//p:b` finds `<p:b>` where
///   `xmlns:p` is declared), and a name without a prefix finds an element of the document's
///   default namespace; a prefix the document does not declare is #VALUE!.
/// - Each value reads as a cell entry would: a number (digits beyond the 15th significant one
///   dropped, `3.14159265358979323` is 3.14159265358979), a date, a time, a percentage, a
///   currency amount or a fraction under the call's locale profile ([`crate::value_text`]),
///   `TRUE` or `FALSE` in any case as a logical; any other text stays text (`#N/A` too).
/// - Several nodes give a column.
fn filterxml(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let value = |exit| Exit::Error(exit);
    let xml = super::kit::text(site, a, 0)?;
    let path = super::kit::text(site, a, 1)?;
    let declared = filterxml_namespaces(&xml);
    // The default namespace is left out, so that a name without a prefix finds its elements.
    let xml = filterxml_without_default_namespace(&xml);
    for prefix in filterxml_prefixes(&path) {
        if !declared.iter().any(|(declared, _)| *declared == prefix) {
            return fail(WorksheetErrorCode::Value);
        }
    }
    let package = sxd_document::parser::parse(&xml).map_err(|_| value(WorksheetErrorCode::Value))?;
    let document = package.as_document();
    let expression = sxd_xpath::Factory::new()
        .build(&path)
        .ok()
        .flatten()
        .ok_or(value(WorksheetErrorCode::Value))?;
    let mut context = sxd_xpath::Context::new();
    for (prefix, uri) in &declared {
        context.set_namespace(prefix, uri);
    }
    let result = expression
        .evaluate(&context, document.root())
        .map_err(|_| value(WorksheetErrorCode::Value))?;
    let sxd_xpath::Value::Nodeset(nodes) = result else {
        return fail(WorksheetErrorCode::Value);
    };
    let nodes = nodes.document_order();
    if nodes.is_empty() {
        return fail(WorksheetErrorCode::Value);
    }
    let mut cells = Vec::with_capacity(nodes.len());
    for node in nodes {
        let text = match node {
            sxd_xpath::nodeset::Node::Element(element) => element
                .children()
                .into_iter()
                .filter_map(|child| child.text())
                .map(|text| text.text())
                .filter(|text| !text.chars().all(char::is_whitespace))
                .collect::<String>(),
            sxd_xpath::nodeset::Node::Attribute(attribute) => attribute.value().to_owned(),
            _ => return fail(WorksheetErrorCode::Value),
        };
        if text.is_empty() {
            return fail(WorksheetErrorCode::Value);
        }
        cells.push(vec![filterxml_value(site, &text)?]);
    }
    if cells.len() == 1 {
        return Ok(cells.remove(0).remove(0));
    }
    CalcArray::from_rows(cells)
        .map(CalcValue::array)
        .ok_or_else(|| site.not_modelled(None))
}

/// The namespace prefixes the document declares (`xmlns:p="uri"`), with their URIs.
fn filterxml_namespaces(xml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find("xmlns:") {
        rest = &rest[at + 6..];
        let Some(eq) = rest.find('=') else { break };
        let prefix = rest[..eq].trim().to_owned();
        let after = rest[eq + 1..].trim_start();
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            continue;
        };
        if let Some(end) = after[1..].find(quote) {
            out.push((prefix, after[1..1 + end].to_owned()));
        }
    }
    out
}

/// The XML without its default namespace declarations (`xmlns="uri"`).
fn filterxml_without_default_namespace(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(at) = rest.find("xmlns") {
        let after = &rest[at + 5..];
        let trimmed = after.trim_start();
        if let Some(value) = trimmed.strip_prefix('=') {
            let value = value.trim_start();
            if let Some(quote) = value.chars().next().filter(|c| *c == '"' || *c == '\'')
                && let Some(end) = value[1..].find(quote)
            {
                out.push_str(&rest[..at]);
                rest = &value[1 + end + 1..];
                continue;
            }
        }
        out.push_str(&rest[..at + 5]);
        rest = after;
    }
    out.push_str(rest);
    out
}

/// The namespace prefixes an XPath expression's names use (`p` of `//p:b`), outside its string
/// literals; an axis (`child::`) is not one.
fn filterxml_prefixes(path: &str) -> Vec<String> {
    let chars: Vec<char> = path.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' || c == '"' {
            i += 1;
            while i < chars.len() && chars[i] != c {
                i += 1;
            }
            i += 1;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '-' | '.'))
            {
                i += 1;
            }
            if chars.get(i) == Some(&':') && chars.get(i + 1) != Some(&':') {
                out.push(chars[start..i].iter().collect());
            }
            if chars.get(i) == Some(&':') && chars.get(i + 1) == Some(&':') {
                i += 2;
            }
            continue;
        }
        i += 1;
    }
    out
}

/// A node's value as FILTERXML gives it, read as a cell entry would read it (see [`filterxml`]).
fn filterxml_value(site: &Site<'_>, text: &str) -> Step<CalcValue> {
    let trimmed = text.trim_matches(' ');
    if trimmed.eq_ignore_ascii_case("TRUE") || trimmed.eq_ignore_ascii_case("FALSE") {
        return Ok(CalcValue::logical(trimmed.eq_ignore_ascii_case("TRUE")));
    }
    let shown = crate::catalogue::kit::text_value(text);
    match coerce::number(&crate::catalogue::kit::text_value(&fifteen_digits(trimmed))) {
        Ok(Numeric::Number(n)) => return site.number(n),
        Ok(Numeric::Decimal(decimal)) => {
            if let Ok(n) = decimal.nearest() {
                return site.number(n);
            }
            return Ok(shown);
        }
        Err(_) => {}
    }
    let environment = site.environment().ok_or_else(|| site.not_modelled(None))?;
    let profile = environment
        .profile
        .as_ref()
        .ok_or_else(|| site.not_modelled(None))?;
    let context = ReadContext {
        profile,
        date_system: environment.date_system,
        now: environment.now,
    };
    let units: Vec<u16> = text.encode_utf16().collect();
    if units.len() > crate::value::EXCEL_TEXT_MAX_UTF16_CODE_UNITS {
        return fail(WorksheetErrorCode::Value);
    }
    match value_text::read(&units, &context) {
        Ok(reading) => site.number(reading.value()),
        Err(Unread::Rejected) => Ok(shown),
        Err(Unread::NotModelled) => Err(site.not_modelled(None)),
    }
}

/// Decimal text with the digits after its 15th significant one written as 0, as a cell entry
/// keeps 15 (`3.14159265358979323` as `3.14159265358979000`); other text unchanged.
fn fifteen_digits(text: &str) -> String {
    let mut seen = 0;
    let mut in_exponent = false;
    text.chars()
        .map(|c| {
            if c == 'e' || c == 'E' {
                in_exponent = true;
            }
            if in_exponent || !c.is_ascii_digit() {
                return c;
            }
            if seen == 0 && c == '0' {
                return c;
            }
            seen += 1;
            if seen > 15 { '0' } else { c }
        })
        .collect()
}

ported_family! {
    code: CODE;
    1 value_fn "VALUE" (1, 1) [V "text"] |s, a, sv| { value(s, a) }
    2 text_fn "TEXT" (2, 2) [V "value", V "format_text"] |s, a, sv| { text(s, a) }
    3 fixed_fn "FIXED" (1, 3) [V "number", V "decimals", V "no_commas"] |s, a, sv| { fixed(s, a) }
    4 dollar_fn "DOLLAR" (1, 2) [V "number", V "decimals"] |s, a, sv| { dollar(s, a) }
    5 numbervalue_fn "NUMBERVALUE" (1, 3) [V "text", V "decimal_separator", V "group_separator"] |s, a, sv| { numbervalue(s, a) }
    6 valuetotext_fn "VALUETOTEXT" (1, 2) [V "value", V "format"] |s, a, sv| { valuetotext(s, a) }
    7 arraytotext_fn "ARRAYTOTEXT" (1, 2) [R "array", V "format"] |s, a, sv| { arraytotext(s, a, sv) }
    8 bahttext_fn "BAHTTEXT" (1, 1) [V "number"] |s, a, sv| { bahttext(s, a) }
    9 asc_fn "ASC" (1, 1) [V "text"] |s, a, sv| { width_conversion(s, a) }
    10 dbcs_fn "DBCS" (1, 1) [V "text"] |s, a, sv| { width_conversion(s, a) }
    12 phonetic_fn "PHONETIC" (1, 1) [F "reference"] |s, a, sv| { phonetic(s, a, sv) }
    13 textbefore_fn "TEXTBEFORE" (2, 6) [V "text", R "delimiter", V "instance_num", V "match_mode", V "match_end", V "if_not_found"] |s, a, sv| { text_delim(s, a, sv, Side::Before) }
    14 textafter_fn "TEXTAFTER" (2, 6) [V "text", R "delimiter", V "instance_num", V "match_mode", V "match_end", V "if_not_found"] |s, a, sv| { text_delim(s, a, sv, Side::After) }
    15 textsplit_fn "TEXTSPLIT" (2, 6) [V "text", R "col_delimiter", R "row_delimiter", V "ignore_empty", V "match_mode", V "pad_with"] |s, a, sv| { textsplit(s, a, sv) }
    16 regextest_fn "REGEXTEST" (2, 3) [V "text", V "pattern", V "case_sensitivity"] |s, a, sv| { regextest(s, a) }
    17 regexextract_fn "REGEXEXTRACT" (2, 4) [V "text", V "pattern", V "return_mode", V "case_sensitivity"] |s, a, sv| { regexextract(s, a) }
    18 regexreplace_fn "REGEXREPLACE" (3, 5) [V "text", V "pattern", V "replacement", V "occurrence", V "case_sensitivity"] |s, a, sv| { regexreplace(s, a) }
    19 encodeurl_fn "ENCODEURL" (1, 1) [V "text"] |s, a, sv| { encodeurl(s, a) }
    20 filterxml_fn "FILTERXML" (2, 2) [V "xml", V "xpath"] |s, a, sv| { filterxml(s, a) }
    21 hyperlink_fn "HYPERLINK" (1, 2) [V "link_location", V "friendly_name"] |s, a, sv| { hyperlink(s, a) }
}
