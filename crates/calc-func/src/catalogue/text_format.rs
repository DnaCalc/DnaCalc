//! The text functions that read or write number formats, dates and other text forms, ported from
//! OxFunc (functions/ at 6fca3518): value_fn.rs, text_fn.rs, fixed_fn.rs, dollar_fn.rs,
//! number_regex_translate_family.rs, valuetotext_fn.rs, array_text_split_family.rs,
//! text_delim_family.rs, text_compat_locale_family.rs (ASC and DBCS; JIS is held as absent from Excel on
//! Windows), web_text_xml_family.rs and hyperlink_fn.rs. A call whose result is not modelled is refused as not modelled.

use super::kit::{fail, flag, grid, num, scalar_error, text_arg};
use crate::call::EvalServices;
use crate::coerce::{self, Numeric, Stop};
use crate::format::{self, Failure};
use crate::kernel::{Exit, Site, Step};
use crate::locale::LocaleId;
use crate::locale::{CalcEnvironment, FormatProfile};
use crate::ported::text_kernels as tk;
use crate::ported::text_more::{self as k, Delimited, Side};
use crate::value::{ArrayShape, CalcArray, ExcelText};
use crate::value::{CalcValue, CoreRef, WorksheetErrorCode};
use crate::value_text::{self, MAX_READER_TEXT, ReadContext, Reading, Unread};

use calc_catalogue::ported::text_format::{CODE, ENTRIES as DECLARATIONS};

/// Text argument `index` read as VALUE reads it under the call's locale profile
/// ([`crate::value_text`]): #VALUE! for text the grammar does not read and for a value that is
/// not text; an error is the result; refused without a profile.
pub(super) fn read_value(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<Reading> {
    match a[index].core_ref() {
        CoreRef::Text(text) => {
            // Empty text and ASCII words without digits are not number, date, or time
            // strings, even without a locale profile. A blank cell is distinct.
            if text.utf16_code_units().iter().all(|&unit| {
                unit == u16::from(b' ')
                    || (u16::from(b'A')..=u16::from(b'Z')).contains(&unit)
                    || (u16::from(b'a')..=u16::from(b'z')).contains(&unit)
            }) {
                return fail(WorksheetErrorCode::Value);
            }
            // With the captured dot/NBSP separators, the shared decimal grammar already
            // identifies numeric-only text. DATEVALUE and TIMEVALUE reject that reading,
            // without needing the unrecorded date ordering or currency symbol.
            if site.environment().is_none_or(|env| env.profile.is_none())
                && site.number_separators() == Some((".", "\u{a0}"))
                && let Ok(Numeric::Decimal(decimal)) = coerce::number(&a[index])
            {
                return Ok(Reading::Number(site.argument(index, decimal.nearest())?));
            }
            if site.environment().is_none_or(|env| env.profile.is_none())
                && site.number_separators().is_some_and(|(decimal, group)| {
                    value_text::rejected_comma_number(text.utf16_code_units(), decimal, group)
                        || value_text::rejected_comparison_number(text.utf16_code_units(), decimal, group)
                })
            {
                return fail(WorksheetErrorCode::Value);
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
        CoreRef::Text(text) => {
            if text.len_utf16_code_units() > MAX_READER_TEXT {
                return fail(WorksheetErrorCode::Value);
            }
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
        Ok(text) => {
            // Excel TEXT, FIXED, and DOLLAR reject formatted outputs exceeding
            // 255 UTF-16 code units with `#VALUE!` (dc-ztvv.26.14 / cb28 S1f/S2d/S3c).
            if text.encode_utf16().count() > 255 {
                return fail(WorksheetErrorCode::Value);
            }
            Ok(crate::catalogue::kit::text_value(&text))
        }
        Err(Failure::Value) => fail(WorksheetErrorCode::Value),
        Err(Failure::NotModelled) => Err(site.not_modelled(None)),
    }
}

/// TEXT: a number (or text that reads as one) rendered with the code, other text through the
/// code's text section.
fn text(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    // Excel propagates argument errors before trying to interpret a format. A logical
    // format is invalid even when the workbook's locale is unavailable.
    for value in a {
        if let CoreRef::Error(code) = value.core_ref() {
            return fail(code);
        }
    }
    if matches!(a[1].core_ref(), CoreRef::Logical(_)) {
        return fail(WorksheetErrorCode::Value);
    }
    // Excel 16.0.20527: a numeric format argument has no text section, so a
    // logical value is returned as text without consulting the host locale.
    if let (CoreRef::Logical(value), CoreRef::Number(code)) =
        (a[0].core_ref(), a[1].core_ref())
    {
        if code.is_finite() {
            return Ok(crate::catalogue::kit::text_value(if value {
                "TRUE"
            } else {
                "FALSE"
            }));
        }
    }
    if let (CoreRef::Text(text), CoreRef::Number(code)) = (a[0].core_ref(), a[1].core_ref()) {
        // Numeric format arguments contain no text section. When the shared
        // coercion already decides that this text cannot be a number, Excel
        // returns the original text without needing a format profile.
        if code.is_finite()
            && (matches!(
                coerce::number(&a[0]),
                Err(Stop::Error(WorksheetErrorCode::Value))
            ) || (site.environment().is_some_and(|env| env.profile.is_none())
                && site.number_separators().is_some_and(|(decimal, group)| {
                    value_text::rejected_comma_number(text.utf16_code_units(), decimal, group)
                })))
        {
            return Ok(a[0].clone());
        }
    }
    if a[0].is_excel_overflow() {
        let code = text_arg(site, a, 1)?.to_string_lossy();
        let mut fallback = crate::locale::format_profile(LocaleId::EnUs);
        if site
            .environment()
            .and_then(|e| e.profile.as_ref())
            .is_none()
            && let Some((decimal, group)) = site.number_separators()
            && let Some(custom) = fallback.with_separators(decimal, group)
        {
            fallback = custom;
        }
        let profile = site
            .environment()
            .and_then(|e| e.profile.as_ref())
            .unwrap_or(&fallback);
        return rendered(
            site,
            format::text_overflow(
                profile,
                a[0].as_number().expect("qualified overflow number"),
                &code,
            ),
        );
    }
    // Excel 20527 under en-ZA ("." decimal; Application.ThousandsSeparator U+00A0,
    // International U+0020) refuses a code that reads as plain exponent number text,
    // such as 1e2 or 1e+100, with or without the full locale profile.
    if matches!(
        a[0].core_ref(),
        CoreRef::Number(_) | CoreRef::Empty | CoreRef::Missing
    ) || matches!(coerce::number(&a[0]), Ok(Numeric::Decimal(_)))
    {
        if let CoreRef::Text(code) = a[1].core_ref() {
            if site
                .number_separators()
                .is_some_and(|(decimal, group)| value_text::measured_dot_space_separators(decimal, group))
                && format::invalid_plain_exponent_code(&code.to_string_lossy())
            {
                site.number_argument(0, &a[0])?;
                return fail(WorksheetErrorCode::Value);
            }
        }
    }
    // These codes read only their supplied decimal separator, when present;
    // the same renderer handles numbers and plain decimal text read by coercion.
    if site
        .environment()
        .and_then(|e| e.profile.as_ref())
        .is_none()
        && (matches!(
            a[0].core_ref(),
            CoreRef::Number(_) | CoreRef::Empty | CoreRef::Missing
        ) || matches!(coerce::number(&a[0]), Ok(Numeric::Decimal(_))))
        && matches!(
            a[1].core_ref(),
            CoreRef::Number(_) | CoreRef::Text(_) | CoreRef::Empty
        )
    {
        let code = text_arg(site, a, 1)?.to_string_lossy();
        let known_decimal = site.number_separators().is_some_and(|(decimal, group)| {
            decimal == "." && group != "." && format::plain_decimal_code(&code)
        });
        let known_scientific_decimal = site.number_separators().is_some_and(|(decimal, group)| {
            decimal == "." && group != "." && format::literal_scientific_decimal_code(&code)
        });
        let known_alpha_literal = site.number_separators().is_some_and(|(decimal, group)| {
            format::plain_letter_literal_code(&code, decimal, group)
        });
        if matches!(a[0].core_ref(), CoreRef::Number(number) if number < 0.0)
            && site.number_separators().is_some_and(|(decimal, group)| {
                format::plain_datetime_code(&code, decimal, group)
            })
        {
            site.number_argument(0, &a[0])?;
            return fail(WorksheetErrorCode::Value);
        }
        let known_literal = !format::integer_code(&code)
            && site.number_separators().is_some_and(|(decimal, group)| {
                format::literal_integer_code(&code, decimal, group)
            });
        if code.is_empty()
            || format::integer_code(&code)
            || known_decimal
            || known_literal
            || known_scientific_decimal
            || known_alpha_literal
            || format::literal_scientific_code(&code)
        {
            let number = site.number_argument(0, &a[0])?;
            let system = site
                .environment()
                .map_or(crate::locale::DateSystem::System1900, |e| e.date_system);
            let mut format_profile = crate::locale::format_profile(LocaleId::EnZa);
            if known_decimal || known_literal || known_scientific_decimal || known_alpha_literal {
                let (decimal, group) = site.number_separators().expect("known decimal separators");
                format_profile = format_profile
                    .with_separators(decimal, group)
                    .ok_or_else(|| site.not_modelled(None))?;
            }
            return rendered(site, format::text(&format_profile, system, number, &code));
        }
    }
    if let (Some(environment), CoreRef::Number(_), CoreRef::Text(code)) =
        (site.environment(), a[0].core_ref(), a[1].core_ref())
    {
        if environment.profile.is_none() {
            let number = site.number_argument(0, &a[0])?;
            return rendered(
                site,
                format::text_declared_date_locale(
                    environment.date_system,
                    number,
                    &code.to_string_lossy(),
                ),
            );
        }
    }
    let (environment, profile) = profile(site)?;
    let code = text_arg(site, a, 1)?.to_string_lossy();
    let number = match a[0].core_ref() {
        CoreRef::Number(_) | CoreRef::Empty | CoreRef::Missing => site.number_argument(0, &a[0])?,
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

/// The places of FIXED and DOLLAR: 2 when the call leaves them out.
fn places(site: &Site<'_>, a: &[CalcValue]) -> Step<f64> {
    if a.len() < 2 || a[1].is_missing() {
        return Ok(2.0);
    }
    let number = num(site, a, 1)?;
    // Excel 20527 uses the shared tolerant integer boundary on the magnitude.
    Ok(crate::ported::text_kernels::date_integer_floor(number.abs()).copysign(number))
}

fn checked_places(places: f64) -> Step<i32> {
    if places > 127.0 {
        return fail(WorksheetErrorCode::Value);
    }
    Ok(places.max(-400.0) as i32)
}

fn fixed(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let number = num(site, a, 0)?;
    let places = places(site, a)?;
    let no_commas = a.len() > 2 && flag(site, a, 2)?;
    let places = checked_places(places)?;
    let profile = site
        .environment()
        .and_then(|environment| environment.profile.as_ref());
    rendered(
        site,
        format::fixed_with_separators(profile, site.number_separators(), number, places, no_commas),
    )
}

fn dollar(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let number = num(site, a, 0)?;
    // W111 locale-profile 00193: unlike FIXED, an explicit placeholder means zero places.
    let places = if a.get(1).is_some_and(CalcValue::is_missing) {
        0.0
    } else {
        places(site, a)?
    };
    let mut negative_places = places.is_sign_negative();
    // Excel 20527 en-ZA (dc-7ofm locale answers): from -2147483647 places down, a negative
    // value whose 15-digit magnitude is below 0.01 keeps its minus ("-R0"); a larger one, as
    // for any other negative places, shows "R0".
    if places <= -2_147_483_647.0 && number < 0.0 {
        let fifteen: f64 = format!("{:.14e}", number.abs()).parse().unwrap_or(0.0);
        negative_places = fifteen >= 0.01;
    }
    let places = checked_places(places)?;
    let (_, profile) = profile(site)?;
    rendered(site, format::dollar(profile, number, places, negative_places))
}

/// An optional argument: `None` when the call does not reach it or leaves it empty.
fn given(a: &[CalcValue], index: usize) -> bool {
    a.get(index).is_some_and(|value| !value.is_missing())
}

/// A 0-or-1 flag argument (match modes, formats): another number is #VALUE!.
fn binary_flag(site: &Site<'_>, a: &[CalcValue], index: usize, tolerant: bool) -> Step<bool> {
    if !given(a, index) {
        return Ok(false);
    }
    let n = num(site, a, index)?;
    if !n.is_finite() {
        return fail(WorksheetErrorCode::Value);
    }
    match if tolerant { tk::date_integer_floor(n) } else { n.trunc() } {
        0.0 => Ok(false),
        1.0 => Ok(true),
        _ => fail(WorksheetErrorCode::Value),
    }
}

/// A logical flag read as Excel reads a logical argument: any non-zero number is TRUE (TEXTSPLIT's
/// ignore_empty and match_mode, fuzz sweep dc-j9k.192-.197: -2, -1, 332 and 2.0000000000000004 all
/// split rather than #VALUE!).
fn logical_flag(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<bool> {
    if !given(a, index) {
        return Ok(false);
    }
    let n = num(site, a, index)?;
    if !n.is_finite() {
        return fail(WorksheetErrorCode::Value);
    }
    Ok(n != 0.0)
}

/// The texts a delimiter argument gives: a single text, or each cell of an array or reference.
fn delimiters(
    site: &Site<'_>,
    a: &[CalcValue],
    index: usize,
    sv: &mut EvalServices<'_>,
    split_array_blanks: bool,
) -> Step<Vec<ExcelText>> {
    let g = grid(site, a, index, sv)?;
    let mut out = Vec::with_capacity(g.cells.len());
    for value in &g.cells {
        if matches!(value.core_ref(), CoreRef::Array(_) | CoreRef::Reference(_)) {
            return Err(site.not_modelled(Some(index)));
        }
        // An empty element of an array expression arrives at TEXTSPLIT as zero;
        // an empty cell of a referenced range remains an empty delimiter.
        if split_array_blanks && matches!(a[index].core_ref(), CoreRef::Array(_))
            && matches!(value.core_ref(), CoreRef::Empty)
        {
            out.push(ExcelText::from_interop_assignment("0"));
            continue;
        }
        let units = site.argument(index, coerce::text(value))?;
        out.push(ExcelText::from_utf16_code_units(units.into_owned()));
    }
    Ok(out)
}

fn text_delim(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
    side: Side,
) -> Step<CalcValue> {
    let scalar_delimiter = match a[1].core_ref() {
        CoreRef::Array(_) => false,
        CoreRef::Reference(reference) => {
            matches!(reference.areas(), [area] if area.cell_count() == 1)
        }
        _ => true,
    };
    // Excel propagates errors in the searched text, delimiter and options before
    // validating their non-error values. The if-not-found argument remains lazy.
    if scalar_delimiter {
        if let Some(code) = a[0].as_error() {
            return fail(code);
        }
        // Delimiters can be one-cell references. Read that cell before looking
        // at later options so its error keeps its original argument position.
        let delimiter_error = if matches!(a[1].core_ref(), CoreRef::Reference(_)) {
            grid(site, a, 1, sv)?.cells.first().and_then(CalcValue::as_error)
        } else {
            a[1].as_error()
        };
        if let Some(code) = delimiter_error.or_else(|| a.iter().skip(2).take(3).find_map(CalcValue::as_error)) {
            return fail(code);
        }
    }
    let text = text_arg(site, a, 0)?;
    let delimiters = delimiters(site, a, 1, sv, false)?;
    let single_delimiter = scalar_delimiter && delimiters.len() == 1;
    let instance = if given(a, 2) {
        let n = num(site, a, 2)?;
        let empty_single = single_delimiter && delimiters[0].len_utf16_code_units() == 0;
        if !n.is_finite() || (!empty_single && n.trunc().abs() > 1e9) {
            return fail(WorksheetErrorCode::Value);
        }
        // For a single delimiter Excel applies the same tolerant floor used by
        // date and count arguments, including for negative fractions.
        if single_delimiter {
            tk::date_integer_floor(n) as isize
        } else {
            n.trunc() as isize
        }
    } else {
        1
    };
    let case_insensitive = binary_flag(site, a, 3, false)?;
    let match_end = binary_flag(site, a, 4, false)?;
    // An explicitly supplied instance counts possible start positions, rather
    // than just text units. An omitted instance and an empty delimiter have
    // different measured rules.
    if given(a, 2) && delimiters.len() == 1 {
        // A one-cell array delimiter validates like a scalar one (dc-j9k.159, .160: Excel 20527
        // #VALUE!, where we answered #N/A for a delimiter longer than the text).
        let text_len = text.len_utf16_code_units();
        let delimiter_len = delimiters[0].len_utf16_code_units();
        if delimiter_len != 0
            && (delimiter_len > text_len
                || instance.unsigned_abs() > text_len - delimiter_len + 1)
        {
            return fail(WorksheetErrorCode::Value);
        }
    } else if given(a, 2) && delimiters.iter().all(|d| d.len_utf16_code_units() > text.len_utf16_code_units()) {
        // Several delimiters, every one longer than the text: #VALUE! with an explicit instance
        // (dc-j9k.173, .174: Excel 20527 fuzz sweep). The instance-count limit for several
        // delimiters is not measured.
        return fail(WorksheetErrorCode::Value);
    }
    // fn1 Excel 16.0.20527: an explicitly supplied instance is validated against the empty
    // input before fallback/match_end. An empty delimiter still returns empty text.
    if given(a, 2)
        && text.len_utf16_code_units() == 0
        && !delimiters.iter().any(|d| d.len_utf16_code_units() == 0)
    {
        return fail(WorksheetErrorCode::Value);
    }
    match k::delimited(
        side,
        &text,
        &delimiters,
        instance,
        case_insensitive,
        match_end,
        single_delimiter,
    ) {
        Delimited::Text(found) => Ok(CalcValue::text(found)),
        Delimited::BadInstance => fail(WorksheetErrorCode::Value),
        Delimited::NotFound if a.len() > 5 => {
            if a[5].is_missing() {
                fail(WorksheetErrorCode::NA)
            } else if single_delimiter {
                let units = site.argument(5, coerce::text(&a[5]))?;
                Ok(CalcValue::text(ExcelText::from_utf16_code_units(units.into_owned())))
            } else {
                Ok(a[5].clone())
            }
        }
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
        Some(delimiters(site, a, 1, sv, true)?)
    };
    let rows_by = if given(a, 2) {
        Some(delimiters(site, a, 2, sv, true)?)
    } else {
        None
    };
    if columns.is_none() && rows_by.is_none() {
        return fail(WorksheetErrorCode::Value);
    }
    let ignore_empty = logical_flag(site, a, 3)?;
    // match_mode truncates and must then be 0 or 1 (Excel 20527, dc-j9k.546, .547: 2, -1, 3, 255, 1e10
    // and 1e100 are #VALUE!; 0.5, -0.9999 and 1.9 are accepted). ignore_empty takes any number.
    let case_insensitive = binary_flag(site, a, 4, false)?;
    if columns.iter().chain(rows_by.iter()).flatten().any(|d| d.len_utf16_code_units() == 0) {
        return fail(WorksheetErrorCode::Value);
    }
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
    let mut rows: Vec<Vec<CalcValue>> = k::split_grid(&text, columns.as_deref(), rows_by.as_deref(), ignore_empty, case_insensitive)
        .into_iter()
        .map(|row| row.into_iter().map(CalcValue::text).collect())
        .collect();
    if rows.is_empty() {
        return fail(WorksheetErrorCode::Calc);
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
        CoreRef::Number(_) => String::from_utf16_lossy(&site.argument(index, coerce::text(value))?),
        CoreRef::Text(t) if strict => format!("\"{}\"", t.to_string_lossy().replace('"', "\"\"")),
        CoreRef::Text(t) => t.to_string_lossy(),
        CoreRef::Logical(b) => if b { "TRUE" } else { "FALSE" }.to_owned(),
        CoreRef::Error(code) => k::error_literal(code).to_owned(),
        CoreRef::Empty | CoreRef::Missing => String::new(),
        CoreRef::Array(_) | CoreRef::Reference(_) => return Err(site.not_modelled(Some(index))),
    })
}

fn valuetotext(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let strict = binary_flag(site, a, 1, true)?;
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
    // An array of references (OFFSET or INDIRECT lifted over an array, dc-b1l) is #CALC! once the
    // format reads as a number, whatever number (Excel 16.0.20527 through Touchstone sheet cases,
    // 2026-11-17, family reference-arrays: `=ARRAYTOTEXT(OFFSET(A1,{0;1},0),5)` and `,-1)` #CALC!,
    // `,"x")` #VALUE!, `,NA())` #N/A, `=ARRAYTOTEXT(INDIRECT({"A1";"A1,B1"}))` #CALC!, while
    // `=ARRAYTOTEXT({1;2},5)` is #VALUE!).
    if a[0].as_array().is_some_and(|array| {
        array
            .iter_row_major()
            .any(|cell| cell.as_reference().is_some())
    }) {
        if given(a, 1) {
            num(site, a, 1)?;
        }
        return fail(WorksheetErrorCode::Calc);
    }
    let g = grid(site, a, 0, sv)?;
    // A one-cell reference is read as the cell's value: an error there is the result whatever the
    // format, and a blank cell is #VALUE! (Excel 16.0.20527, Touchstone 2026-11-25).
    if a[0].as_reference().is_some() && g.cells.len() == 1 {
        match g.cells[0].core_ref() {
            CoreRef::Error(code) => return fail(code),
            CoreRef::Empty | CoreRef::Missing => return fail(WorksheetErrorCode::Value),
            _ => {}
        }
    }
    let strict = binary_flag(site, a, 1, true)?;
    let mut parts = Vec::with_capacity(g.cells.len());
    for value in &g.cells {
        parts.push(value_text(site, 0, value, strict)?);
    }
    let shown = if strict {
        let rows: Vec<String> = parts
            .chunks(g.cols.max(1))
            .map(|row| row.join(","))
            .collect();
        format!("{{{}}}", rows.join(";"))
    } else {
        parts.join(", ")
    };
    Ok(crate::catalogue::kit::text_value(&shown))
}

/// NUMBERVALUE: the separators default to the locale's.
fn numbervalue(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let text = text_arg(site, a, 0)?;
    if text.len_utf16_code_units() > MAX_READER_TEXT {
        return fail(WorksheetErrorCode::Value);
    }
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
        site.number_separators()
            .ok_or_else(|| site.not_modelled(None))?
    };
    let decimal = separator(1, decimal_default)?;
    let group = separator(2, group_default)?;
    // This is a final argument error; the host VALUE grammar cannot recover it.
    if decimal.is_some() && decimal == group {
        return fail(WorksheetErrorCode::Value);
    }
    // Excel 20527 en-ZA (dc-7ofm locale answers): whatever separators the call names, a
    // measured non-ASCII space other than the named group separator is #VALUE!, and text the
    // separators cannot read falls back to the host's VALUE grammar (R12 is 12, (12) is -12).
    // With the separators "." "," " " and U+00A0 the host reads the text as given. With any
    // other separator (a letter or a digit) it reads the text with the group separator removed
    // before the first decimal separator and each decimal separator as the host's (dc-iuwct.35,
    // fresh answers: 19 of 21 then 11 of 11 frozen predictions; ("%1e3", ".", "e") is 0.13 as
    // "%13", ("1e-3", ".", "e") the date "1-3", ("1e-3", "e", ",") #VALUE! as "1.-3",
    // ("1e3,0", "e", ",") #VALUE! as "1.3,0", ("12.34", 25569) #VALUE! as "1..34").
    let host_reader = site
        .environment()
        .and_then(|environment| environment.profile.as_ref())
        .filter(|profile| value_text::measured_host_reader(profile));
    let measured_separators = [decimal, group]
        .iter()
        .all(|c| matches!(c, Some('.' | ',' | ' ' | '\u{a0}')));
    let host_profile = host_reader.filter(|_| measured_separators);
    if host_profile.is_some_and(|profile| {
        text.to_string_lossy()
            .chars()
            .any(|ch| Some(ch) != group && value_text::measured_space(ch, profile))
    }) {
        return fail(WorksheetErrorCode::Value);
    }
    let result = match k::numbervalue(&text, decimal, group) {
        // A numeric range error is final, including a zero coefficient. The host
        // grammar fallback must not admit the same rejected exponent as VALUE.
        Err(k::NumberValueError::Range) => return fail(WorksheetErrorCode::Value),
        result => result.map_err(|_| WorksheetErrorCode::Value),
    };
    let Some(profile) = host_reader else {
        return out_number(site, result);
    };
    if result.is_ok() || !matches!(a[0].core_ref(), CoreRef::Text(_)) {
        return out_number(site, result);
    }
    let host_decimal = profile.decimal_separator.chars().next().unwrap_or('.');
    // The measured host admits the same currency, digit and date grammar as
    // VALUE after ASCII spaces and trailing percents are removed. Keep the
    // existing numeric parser and its percent arithmetic for its prior forms.
    let mut after_decimal = false;
    let compact: String = text
        .to_string_lossy()
        .chars()
        .filter(|&ch| ch != ' ')
        .filter_map(|ch| {
            if measured_separators {
                Some(ch)
            } else if Some(ch) == decimal {
                after_decimal = true;
                Some(host_decimal)
            } else if Some(ch) == group && !after_decimal {
                None
            } else {
                Some(ch)
            }
        })
        .collect();
    // Live Excel 16.0.20527 en-ZA (dc-iuwct.44, 17/17 frozen predictions): NUMBERVALUE strips
    // ASCII spaces and trailing percents before falling back to the VALUE grammar, including
    // slash dates ("1/3" -> 46025, "1 1/2" -> "11/2" = 46328, "2024/01/05" -> 45296,
    // "1e/3" with group "e" -> 46025) and colon times ("12:30" -> 0.520833..., "12:30%" ->
    // 0.00520833...); space-separated date+time or AM/PM ("1-3 12:00", "12:30 pm") become
    // #VALUE! because spaces were already removed.
    let base = compact.trim_end_matches('%');
    let percent_count = (compact.len() - base.len()) as u32;
    let value = super::kit::text_value(base);
    match read_value(site, &[value], 0) {
        Ok(reading) => site.number(k::numbervalue_percent(reading.value(), percent_count)),
        Err(Exit::Error(WorksheetErrorCode::Value)) => out_number(site, result),
        Err(other) => Err(other),
    }
}

fn out_number(site: &Site<'_>, result: Result<f64, WorksheetErrorCode>) -> Step<CalcValue> {
    match result {
        Ok(n) => site.number(n),
        Err(code) => fail(code),
    }
}

/// The case sensitivity of the REGEX functions: 0 (the default) case-sensitive, 1 not.
fn case_sensitive(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<bool> {
    Ok(!binary_flag(site, a, index, false)?)
}

fn regex(
    site: &Site<'_>,
    text: &ExcelText,
    pattern: &ExcelText,
    case_sensitive: bool,
    first_only: bool,
) -> Step<k::RegexMatches> {
    if !tk::is_well_formed(text) || !tk::is_well_formed(pattern) {
        return Err(site.not_modelled(None));
    }
    k::regex_matches(
        &text.to_string_lossy(),
        &pattern.to_string_lossy(),
        case_sensitive,
        first_only,
    )
    .map_err(|error| match error {
        k::PatternError::Invalid => Exit::Error(WorksheetErrorCode::Value),
        k::PatternError::Unsupported => site.not_modelled(Some(1)),
    })
}

fn regextest(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let text = text_arg(site, a, 0)?;
    let pattern = text_arg(site, a, 1)?;
    let sensitive = case_sensitive(site, a, 2)?;
    let k::RegexMatches { found, .. } = regex(site, &text, &pattern, sensitive, true)?;
    Ok(CalcValue::logical(!found.is_empty()))
}

fn regex_text(chars: &[char], (start, end): (usize, usize)) -> CalcValue {
    crate::catalogue::kit::text_value(&chars[start..end].iter().collect::<String>())
}

fn regex_row(mut cells: Vec<CalcValue>) -> CalcValue {
    if cells.len() == 1 {
        return cells.pop().expect("one match");
    }
    CalcValue::array(
        CalcArray::new(
            ArrayShape {
                rows: 1,
                cols: cells.len(),
            },
            cells,
        )
        .expect("a nonempty row of matches"),
    )
}

fn regexextract(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let text = text_arg(site, a, 0)?;
    let pattern = text_arg(site, a, 1)?;
    let mode = if given(a, 2) {
        num(site, a, 2)?.trunc()
    } else {
        0.0
    };
    let sensitive = case_sensitive(site, a, 3)?;
    let k::RegexMatches {
        chars,
        found,
        group_count,
    } = regex(site, &text, &pattern, sensitive, mode == 0.0 || mode == 2.0)?;
    match (mode as i64, found.as_slice()) {
        (0 | 1, []) => fail(WorksheetErrorCode::NA),
        (0, [first, ..]) => Ok(regex_text(&chars, first.whole)),
        // Fn5 regex003..005 settle the horizontal shape. Lifted fn2 roots078/079
        // retain each call's first cell through the existing evaluator.
        (1, matches) => Ok(regex_row(
            matches
                .iter()
                .map(|m| regex_text(&chars, m.whole))
                .collect(),
        )),
        // A group-free pattern is Value even without a match (fn5 regex001/006).
        (2, _) if group_count == 0 => fail(WorksheetErrorCode::Value),
        (2, [first, ..]) => Ok(regex_row(
            first
                .groups
                .iter()
                .map(|group| {
                    group.map_or_else(
                        || CalcValue::error(WorksheetErrorCode::NA),
                        |range| regex_text(&chars, range),
                    )
                })
                .collect(),
        )),
        (2, []) => fail(WorksheetErrorCode::NA),
        _ => fail(WorksheetErrorCode::Value),
    }
}

enum RegexReplacement<'a> {
    Literal(&'a str),
    Group(usize),
}

fn regex_replacement(replacement: &str) -> Option<Vec<RegexReplacement<'_>>> {
    if replacement.contains('\\') {
        return None;
    }
    let mut rest = replacement;
    let mut parts = Vec::new();
    while let Some((literal, after_dollar)) = rest.split_once('$') {
        if !literal.is_empty() {
            parts.push(RegexReplacement::Literal(literal));
        }
        let digits = after_dollar.bytes().take_while(u8::is_ascii_digit).count();
        let group = after_dollar[..digits]
            .parse::<usize>()
            .ok()?
            .checked_sub(1)?;
        parts.push(RegexReplacement::Group(group));
        rest = &after_dollar[digits..];
    }
    if !rest.is_empty() {
        parts.push(RegexReplacement::Literal(rest));
    }
    Some(parts)
}

fn regexreplace(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let text = text_arg(site, a, 0)?;
    let pattern = text_arg(site, a, 1)?;
    let replacement = text_arg(site, a, 2)?.to_string_lossy();
    // The captured replacement syntax is a dollar followed by a numbered group.
    let parts = regex_replacement(&replacement).ok_or_else(|| site.not_modelled(Some(2)))?;
    let occurrence = if given(a, 3) {
        num(site, a, 3)?.trunc()
    } else {
        0.0
    };
    let sensitive = case_sensitive(site, a, 4)?;
    let k::RegexMatches {
        chars,
        found,
        group_count,
    } = if pattern.utf16_code_units().is_empty() {
        if !tk::is_well_formed(&text) {
            return Err(site.not_modelled(None));
        }
        let chars: Vec<char> = text.to_string_lossy().chars().collect();
        let limit = (occurrence > 0.0).then_some(occurrence as usize);
        let found = empty_regex_replace_matches(&chars, text.utf16_code_units().len(), limit)?
            .into_iter()
            .map(|whole| k::RegexMatch {
                whole,
                groups: Vec::new(),
            })
            .collect();
        k::RegexMatches {
            chars,
            found,
            group_count: 0,
        }
    } else {
        regex(site, &text, &pattern, sensitive, false)?
    };
    if parts
        .iter()
        .any(|part| matches!(part, RegexReplacement::Group(group) if *group >= group_count))
    {
        return Err(site.not_modelled(Some(2)));
    }

    let selected = if occurrence == 0.0 {
        found.as_slice()
    } else {
        let index = if occurrence > 0.0 {
            Some(occurrence as usize - 1)
        } else {
            found.len().checked_sub((-occurrence) as usize)
        };
        index
            .and_then(|i| found.get(i))
            .map(std::slice::from_ref)
            .unwrap_or_default()
    };
    let mut out = String::new();
    let mut cursor = 0;
    for found in selected {
        let (start, end) = found.whole;
        out.extend(chars[cursor..start].iter());
        for part in &parts {
            match part {
                RegexReplacement::Literal(literal) => out.push_str(literal),
                RegexReplacement::Group(group) => {
                    let Some((start, end)) = found.groups[*group] else {
                        return Err(site.not_modelled(Some(2)));
                    };
                    out.extend(chars[start..end].iter());
                }
            }
        }
        cursor = end;
    }
    out.extend(chars[cursor..].iter());
    Ok(crate::catalogue::kit::text_value(&out))
}

fn empty_regex_replace_matches(
    chars: &[char],
    utf16_len: usize,
    limit: Option<usize>,
) -> Step<Vec<(usize, usize)>> {
    let skip_leading_lf = utf16_len == 13 && chars.first() == Some(&'\n');
    let mut matches = vec![(0, 0)];
    for (index, &ch) in chars.iter().enumerate() {
        if limit == Some(matches.len()) {
            break;
        }
        // Excel's next UTF-16 offset lands inside the surrogate pair. A positive
        // occurrence can finish before the scan reaches that offset.
        if ch.len_utf16() == 2 {
            return fail(WorksheetErrorCode::Value);
        }
        let boundary = index + 1;
        // Excel visits the boundary inside CRLF, skips the one after LF in the
        // interior of the text, and visits the end of the text even after CRLF.
        let after_internal_crlf =
            ch == '\n' && index > 0 && chars[index - 1] == '\r' && boundary < chars.len();
        if !after_internal_crlf && !(skip_leading_lf && index == 0) {
            matches.push((boundary, boundary));
        }
    }
    Ok(matches)
}

/// ASC and DBCS change character widths only under a double-byte language; under every other
/// profile Excel gives the text as it is (Excel 16.0.20527 under en-ZA, the catalogue's sheet
/// dczsn3_bytes: full-width and half-width forms, kana, CJK and Hangul are kept). Under the
/// ja-JP, ko-KR and zh-CN profiles they are refused (bead dc-zsn.3).
fn width_conversion(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    // A formed error is independent of the language needed for width conversion.
    scalar_error(&a[0])?;
    // No editing language can change the width of a string with no characters.
    // Retained Excel captures independently confirm ASC("") and DBCS("").
    if matches!(a[0].core_ref(), CoreRef::Text(text) if text.utf16_code_units().is_empty()) {
        return Ok(a[0].clone());
    }
    // A blank cell is read as empty text by both width conversions.
    if matches!(a[0].core_ref(), CoreRef::Empty) {
        return Ok(crate::catalogue::kit::text_value(""));
    }
    let (_, profile) = profile(site)?;
    if matches!(profile.id, LocaleId::JaJp | LocaleId::KoKr | LocaleId::ZhCn) {
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
/// - Invalid XML or XPath, a result that is not a node set, and an empty node set give #VALUE!.
/// - An element's value is its own text children, joined (`<b>x<c>y</c>z</b>` is `xz`).
///   Ordinary whitespace-only text is discarded before XPath evaluation. CDATA and inherited
///   `xml:space="preserve"` retain whitespace. Nodes without a value and nodes other than
///   elements or attributes give #VALUE! in their own spill cell (Excel 16.0.20625, dc-j9k.172).
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
    // sxd parses CDATA and the pieces of character data around references as Text.
    // Mark their origins until the pieces have been joined into XPath text nodes.
    let mut marker = String::from("\u{e000}\u{e000}");
    while xml.contains(&marker) {
        marker.push('\u{e000}');
    }
    let cdata_marker = format!("{marker}C");
    let reference_marker = format!("{marker}R");
    let xml = filterxml_mark_text_sources(&xml, &cdata_marker, &reference_marker);
    let package =
        sxd_document::parser::parse(&xml).map_err(|_| value(WorksheetErrorCode::Value))?;
    let document = package.as_document();
    let reference_whitespace =
        filterxml_normalize_text(document, &cdata_marker, &reference_marker);
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
        use sxd_xpath::nodeset::Node;
        let text = match node {
            Node::Element(element) if !reference_whitespace.contains(&element) => Some(
                element
                    .children()
                    .into_iter()
                    .filter_map(|child| child.text())
                    .map(|text| text.text())
                    .collect::<String>(),
            ),
            Node::Attribute(attribute) if !filterxml_xml_whitespace(attribute.value()) => {
                Some(attribute.value().to_owned())
            }
            _ => None,
        };
        let cell = match text.as_deref().filter(|text| !text.is_empty()) {
            Some(text) => match filterxml_value(site, text) {
                Ok(cell) => cell,
                Err(Exit::Error(code)) => CalcValue::error(code),
                Err(fault) => return Err(fault),
            },
            None => CalcValue::error(WorksheetErrorCode::Value),
        };
        cells.push(vec![cell]);
    }
    if cells.len() == 1 {
        return Ok(cells.remove(0).remove(0));
    }
    CalcArray::from_rows(cells)
        .map(CalcValue::array)
        .ok_or_else(|| site.not_modelled(None))
}

fn filterxml_xml_whitespace(text: &str) -> bool {
    text.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
}

fn filterxml_mark_text_sources(xml: &str, cdata_marker: &str, reference_marker: &str) -> String {
    let mut output = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        let (text, markup) = rest.split_at(start);
        if text.contains('&') {
            output.push_str(reference_marker);
        }
        output.push_str(text);
        let special = [("<![CDATA[", "]]>"), ("<!--", "-->"), ("<?", "?>")]
            .into_iter()
            .find_map(|(open, close)| markup.strip_prefix(open).map(|body| (open, body, close)));
        let end = if let Some((open, body, close)) = special {
            body.find(close).map(|end| open.len() + end + close.len())
        } else {
            let mut quote = None;
            markup.bytes().enumerate().find_map(|(at, byte)| {
                match (quote, byte) {
                    (Some(expected), byte) if expected == byte => quote = None,
                    (None, b'\'' | b'"') => quote = Some(byte),
                    (None, b'>') => return Some(at + 1),
                    _ => {}
                }
                None
            })
        };
        let Some(end) = end else {
            output.push_str(markup);
            return output;
        };
        let (tag, tail) = markup.split_at(end);
        if let Some(body) = tag.strip_prefix("<![CDATA[") {
            output.push_str("<![CDATA[");
            output.push_str(cdata_marker);
            output.push_str(body);
        } else {
            output.push_str(tag);
        }
        rest = tail;
    }
    output.push_str(rest);
    output
}

fn filterxml_normalize_text<'d>(
    document: sxd_document::dom::Document<'d>,
    cdata_marker: &str,
    reference_marker: &str,
) -> std::collections::HashSet<sxd_document::dom::Element<'d>> {
    use sxd_document::dom::ChildOfElement;
    let space = ("http://www.w3.org/XML/1998/namespace", "space");
    let mut reference_whitespace = std::collections::HashSet::new();
    let mut elements: Vec<_> = document
        .root()
        .children()
        .into_iter()
        .filter_map(|child| child.element().map(|element| (element, false)))
        .collect();
    while let Some((element, inherited)) = elements.pop() {
        let preserve = match element.attribute_value(space) {
            Some("preserve") => true,
            Some("default") => false,
            _ => inherited,
        };
        let mut has_element_text = false;
        let mut has_reference_whitespace = false;
        let mut children = element.children().into_iter().peekable();
        while let Some(child) = children.next() {
            match child {
                ChildOfElement::Element(child) => elements.push((child, preserve)),
                ChildOfElement::Text(text) => {
                    if let Some(cdata) = text.text().strip_prefix(cdata_marker) {
                        text.set_text(cdata);
                        has_element_text |= !cdata.is_empty();
                        continue;
                    }
                    let referenced = text.text().strip_prefix(reference_marker);
                    let mut combined = referenced.unwrap_or_else(|| text.text()).to_owned();
                    while let Some(ChildOfElement::Text(next)) = children.peek()
                        && !next.text().starts_with(cdata_marker)
                    {
                        combined.push_str(next.text());
                        next.remove_from_parent();
                        children.next();
                    }
                    if preserve || !filterxml_xml_whitespace(&combined) {
                        text.set_text(&combined);
                        has_element_text |= !combined.is_empty();
                    } else if referenced.is_some() {
                        // A whitespace character reference is visible to XPath even when
                        // the element has no value to put in a worksheet cell.
                        text.set_text(&combined);
                        has_reference_whitespace = true;
                    } else {
                        text.remove_from_parent();
                    }
                }
                _ => {}
            }
        }
        if has_reference_whitespace && !has_element_text {
            reference_whitespace.insert(element);
        }
    }
    reference_whitespace
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

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
ported_shared_family! {
    code: CODE;
    declarations: DECLARATIONS;
    1 value_fn |s, a, sv| { value(s, a) }
    2 text_fn |s, a, sv| { text(s, a) }
    3 fixed_fn |s, a, sv| { fixed(s, a) }
    4 dollar_fn |s, a, sv| { dollar(s, a) }
    5 numbervalue_fn |s, a, sv| { numbervalue(s, a) }
    6 valuetotext_fn |s, a, sv| { valuetotext(s, a) }
    7 arraytotext_fn |s, a, sv| { arraytotext(s, a, sv) }
    8 bahttext_fn |s, a, sv| { bahttext(s, a) }
    9 asc_fn |s, a, sv| { width_conversion(s, a) }
    10 dbcs_fn |s, a, sv| { width_conversion(s, a) }
    // JIS: Excel 16.0.20527 for Windows answers #NAME? to every call (the fn1-jis bank rows,
    // text, number, logical and empty arguments alike), so the name is held as absent. JIS is not in the dispatch table (Excel holds no such name), and
    // its member number sits apart from the table's so that no neighbouring ID resolves to an
    // entry the table does not admit.
    100 jis_fn |s, a, sv| { fail(WorksheetErrorCode::Name) }
    // PHONETIC's reference takes only an operand that may give one (dc-mx3x, Excel 16.0.20527,
    // 2026-11-07, Formula2 and Formula alike): `=PHONETIC(2)`, `=PHONETIC({1,2})` and
    // `=PHONETIC(1+1)` are refused, `=PHONETIC(B1)` and `=PHONETIC(INDEX(B1:B3,0))` enter.
    12 phonetic_fn |s, a, sv| { phonetic(s, a, sv) }
    13 textbefore_fn |s, a, sv| { text_delim(s, a, sv, Side::Before) }
    14 textafter_fn |s, a, sv| { text_delim(s, a, sv, Side::After) }
    15 textsplit_fn |s, a, sv| { textsplit(s, a, sv) }
    16 regextest_fn |s, a, sv| { regextest(s, a) }
    17 regexextract_fn |s, a, sv| { regexextract(s, a) }
    18 regexreplace_fn |s, a, sv| { regexreplace(s, a) }
    19 encodeurl_fn |s, a, sv| { encodeurl(s, a) }
    20 filterxml_fn |s, a, sv| { filterxml(s, a) }
    21 hyperlink_fn |s, a, sv| { hyperlink(s, a) }
}
