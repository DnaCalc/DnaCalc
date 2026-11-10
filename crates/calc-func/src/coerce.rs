//! Worksheet coercion: how an operand or a direct function argument is read as a number, a
//! logical or text. A value that a reference or an array supplies to an aggregate is not coerced
//! here; the kernel reading it decides (SUM skips text and logicals there, AVERAGEA reads them as
//! numbers).
//!
//! Empty (a blank cell), Missing (an omitted argument) and empty text stay distinct: the first two
//! read as 0, FALSE and empty text, while empty text read as a number or a logical is #VALUE!.
//!
//! Text reads as a number only in the plain decimal form: optional spaces, an optional sign,
//! digits with at most one '.', an optional exponent and optional spaces. Digits beyond the
//! first 15 significant digits are cut, using the same decimal converter as VALUE. Plain text
//! beyond 255 UTF-16 units is #VALUE! before trimming. Plain decimal text whose scale is outside
//! -308..308 ([`Decimal::nearest`]) is #VALUE!, not refused, and so is text with no digit at all. Text of printable ASCII holding
//! a word outside the list that [`number_words`] measured in Excel 16.0 (build 20430) under the
//! Windows locale en-ZA, such as the label 2016-17 Winter, is #VALUE! too: the rule takes those to
//! be the only words Excel reads inside number text, and other locales were not measured. Any
//! other text holding a digit (a date, a time, a fraction, a currency amount, grouped digits, a
//! percentage) is refused as not modelled, because Excel may read it as a number through its
//! locale's number-text rules (dc-uar); the same holds for a logical's text forms, which are the English TRUE and FALSE. Coercion only
//! reads the decimal ([`Numeric::Decimal`]): finding its nearest binary64 rounds, so the kernel
//! does that at an arithmetic site, under the call's numeric mode, and the scale's #VALUE! comes
//! from there.
//!
//! A number read as text takes its text from [`number_text`]. A subnormal number reads as text as
//! 0, since Excel holds none and reads a cell written as one back as +0; read as a number
//! ([`operand`]) a subnormal is refused as not modelled, since Excel's arithmetic on one is not
//! (dc-j9k owns subnormal numbers and makes the two readings agree). Near a tie at the 16th
//! significant digit, [`number_text`] produces text under its documented rule, with measured
//! mismatches still open under dc-b1l.25; these are calculated mismatches, not typed refusals.

use crate::number_text;
use crate::number_words;
use crate::value::{CalcValue, CoreRef, ExcelText, WorksheetErrorCode};
use std::borrow::Cow;

/// Why a coercion produced no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    /// The result is this worksheet error, which the call returns.
    Error(WorksheetErrorCode),
    /// Excel's result for this value is not modelled, and the call is refused.
    NotModelled,
}

const TRUE_TEXT: [u16; 4] = [b'T' as u16, b'R' as u16, b'U' as u16, b'E' as u16];
const FALSE_TEXT: [u16; 5] = [
    b'F' as u16,
    b'A' as u16,
    b'L' as u16,
    b'S' as u16,
    b'E' as u16,
];

/// A value read as a number, before any rounding.
pub(crate) enum Numeric {
    /// A number the value gives exactly.
    Number(f64),
    /// Text in the plain decimal form. Its value is the nearest binary64, which only rounding
    /// finds, so a kernel converts it at an arithmetic site under the call's numeric mode.
    Decimal(Decimal),
}

/// Text in the plain decimal form (see the module documentation), with its spaces trimmed.
pub(crate) struct Decimal(String);

impl Decimal {
    /// The nearest binary64, ties to even, of text that Excel reads as a number; a zero ("-0"
    /// included) and a number in binary64's subnormal range read as +0.
    ///
    /// Excel reads the text only when its scale ([`Decimal::scale`]) is from -308 to 308, and
    /// gives #VALUE! otherwise. The oracle rows of cfunc_m1_2__mixed_operands measure both ends:
    /// "9.99999999999999e307", "0.01e309", "1000e-310", "0e308", "0e-308", "0.0e309" and
    /// "1e-309" read (the last as 0, also when multiplied by 1E+300), while "1e308", "-1e308",
    /// "0e309", "0e-309", "0.0e-308", "9e-310", "0.01e-308" and "1e-400" are #VALUE!. The rule is
    /// applied unmeasured to the shapes no row holds: a whole part with leading zeros ("001e308"),
    /// a whole part of several digits at the upper end ("10e307"; "1000e-310" measures one at the
    /// lower end), a zero mantissa with no digit after its point ("0.e309"), and a written
    /// exponent beyond the cap of [`Decimal::scale`].
    pub(crate) fn nearest(&self) -> Result<f64, Stop> {
        let position = self.scale();
        if !readable_scale(position) {
            return Err(Stop::Error(WorksheetErrorCode::Value));
        }
        // Plain spellings need no currency, date or grouping profile. Fn11's
        // VALUE/DELTA/PMT pairs retain the first 15 digits, including long text.
        let unsigned = self.0.trim_start_matches(['+', '-']);
        let mantissa = unsigned.split(['e', 'E']).next().expect("a mantissa");
        let digits: String = mantissa.chars().filter(|c| c.is_ascii_digit()).collect();
        let significant = digits.trim_start_matches('0').len() as i64;
        let magnitude = crate::value_text::decimal(&digits, position - significant)
            .ok_or(Stop::Error(WorksheetErrorCode::Value))?;
        let n = if self.0.starts_with('-') {
            -magnitude
        } else {
            magnitude
        };
        if n == 0.0 || n.is_subnormal() {
            Ok(0.0)
        } else {
            operand(n)
        }
    }

    /// The power of ten k for which the text's value is 0.d1d2... times 10^k: its written exponent
    /// plus the digits of its whole part from the first that is not 0 (unmeasured with leading
    /// zeros, "001e308" having scale 309), or less the zeros that follow the point before the
    /// first digit that is not 0. A zero mantissa counts every digit after its point as such a
    /// zero ("0.0e309" has scale 308, measured; "0.e309" has 309, unmeasured). A written exponent
    /// beyond a million in size is taken as a million, so that one of 19 digits or more cannot
    /// overflow i64; no mantissa of a cell's text brings that back into range. No row measures
    /// such an exponent.
    fn scale(&self) -> i64 {
        let unsigned = self.0.trim_start_matches(['+', '-']);
        let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
            Some(at) => (&unsigned[..at], &unsigned[at + 1..]),
            None => (unsigned, ""),
        };
        let (negative, digits) = match exponent.as_bytes().first() {
            Some(b'-') => (true, &exponent[1..]),
            Some(b'+') => (false, &exponent[1..]),
            _ => (false, exponent),
        };
        let size = digits.bytes().fold(0_i64, |size, digit| {
            (size * 10 + i64::from(digit - b'0')).min(1_000_000)
        });
        let written = if negative { -size } else { size };
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        let count = |digits: &str| i64::try_from(digits.len()).unwrap_or(i64::MAX);
        match whole.find(|c| c != '0') {
            Some(first) => written + count(&whole[first..]),
            None => match fraction.find(|c| c != '0') {
                Some(zeros) => written - count(&fraction[..zeros]),
                None => written - count(fraction),
            },
        }
    }
}

/// Whether Excel reads a decimal whose [`Decimal::scale`] is `position`: from -308 to 308.
fn readable_scale(position: i64) -> bool {
    (-308..=308).contains(&position)
}

/// Whether Excel accepts the digits of a number literal (digits, an optional point and an optional
/// exponent, no sign) by their scale. A formula holding a literal below this range (`=1E-310`,
/// `=1E-400`) is not accepted by Excel at all: entering it is an error (read through COM on
/// Excel 16.0.20527, 2026-11-09). One in range whose value lies below the normal range is
/// accepted and read as 0 (`=1E-308`, `=2.2250738585072014E-308` and `=1E-309` all become `=0`).
pub(crate) fn literal_scale_is_readable(text: &str) -> bool {
    readable_scale(Decimal(text.to_owned()).scale())
}

/// The value as a number: a number as it is, TRUE as 1 and FALSE as 0, Empty and Missing as 0,
/// text through [`text_as_decimal`], an error as itself. An array (whose operator lifts over its
/// elements) and a reference are not read here.
pub(crate) fn number(value: &CalcValue) -> Result<Numeric, Stop> {
    if value.is_excel_overflow() {
        return Err(Stop::Error(WorksheetErrorCode::Num));
    }
    match value.core_ref() {
        CoreRef::Number(n) => operand(n).map(Numeric::Number),
        CoreRef::Logical(b) => Ok(Numeric::Number(if b { 1.0 } else { 0.0 })),
        CoreRef::Text(text) => text_as_decimal(text).map(Numeric::Decimal),
        CoreRef::Empty | CoreRef::Missing => Ok(Numeric::Number(0.0)),
        CoreRef::Error(code) => Err(Stop::Error(code)),
        CoreRef::Array(_) | CoreRef::Reference(_) => Err(Stop::NotModelled),
    }
}

/// A number as an operand. A non-finite number is not a worksheet value, and Excel's handling of
/// a subnormal one is not modelled; both are refused.
pub(crate) fn operand(n: f64) -> Result<f64, Stop> {
    if n.is_finite() && !n.is_subnormal() {
        Ok(n)
    } else {
        Err(Stop::NotModelled)
    }
}

/// The value as a logical: a number is TRUE unless it is 0, a logical as it is, Empty and
/// Missing as FALSE, the text TRUE or FALSE in any ASCII case as that logical, an error as
/// itself. Other text is #VALUE!, digit text included: Excel reads no number from text in a
/// logical position (NOT("1") and IF("1",1,2) are #VALUE! in the captures of dc_fwpz and
/// cfunc_m1_2, and IF over the date label 2020/01/02 in dc_e00t's; OxFunc's
/// `parse_excel_logical_text` records the same rule).
pub(crate) fn logical(value: &CalcValue) -> Result<bool, Stop> {
    match value.core_ref() {
        CoreRef::Number(n) => operand(n).map(|n| n != 0.0),
        CoreRef::Logical(b) => Ok(b),
        CoreRef::Text(text) => match spelled_logical(text) {
            Some(logical) => Ok(logical),
            None => Err(Stop::Error(WorksheetErrorCode::Value)),
        },
        CoreRef::Empty | CoreRef::Missing => Ok(false),
        CoreRef::Error(code) => Err(Stop::Error(code)),
        CoreRef::Array(_) | CoreRef::Reference(_) => Err(Stop::NotModelled),
    }
}

/// The logical that text spells: TRUE or FALSE in any ASCII case, with nothing around it. Read
/// as a logical ([`logical`]) other text is #VALUE! or refused; AND, OR and XOR skip it when it is
/// given directly.
pub(crate) fn spelled_logical(text: &ExcelText) -> Option<bool> {
    let units = text.utf16_code_units();
    if eq_ignore_ascii_case(units, &TRUE_TEXT) {
        Some(true)
    } else if eq_ignore_ascii_case(units, &FALSE_TEXT) {
        Some(false)
    } else {
        None
    }
}

/// The value as text to concatenate, in UTF-16 code units: text as it is, a number as its text
/// ([`number_text`], Excel's text with `.` as the decimal separator and no column width), TRUE
/// and FALSE as those words, Empty and Missing as empty text, an error as itself. Every finite
/// number has text under that model, including its documented near-tie mismatches. A number
/// qualified as Excel's exponent-carry rounding result uses its measured General
/// rendering. Other nonfinite numbers are refused as not modelled.
pub(crate) fn text(value: &CalcValue) -> Result<Cow<'_, [u16]>, Stop> {
    match value.core_ref() {
        CoreRef::Text(text) => Ok(Cow::Borrowed(text.utf16_code_units())),
        CoreRef::Number(n) if value.is_excel_overflow() => {
            let profile = crate::locale::format_profile(crate::locale::LocaleId::EnZa);
            Ok(Cow::Owned(
                crate::format::general(&profile, n).encode_utf16().collect(),
            ))
        }
        CoreRef::Number(n) if n.is_finite() => number_text::text(n)
            .map(|text| Cow::Owned(text.encode_utf16().collect()))
            .ok_or(Stop::NotModelled),
        CoreRef::Logical(true) => Ok(Cow::Borrowed(&TRUE_TEXT)),
        CoreRef::Logical(false) => Ok(Cow::Borrowed(&FALSE_TEXT)),
        CoreRef::Empty | CoreRef::Missing => Ok(Cow::Borrowed(&[])),
        CoreRef::Error(code) => Err(Stop::Error(code)),
        CoreRef::Number(_) | CoreRef::Array(_) | CoreRef::Reference(_) => Err(Stop::NotModelled),
    }
}

/// Text that reads as a number, as the decimal it holds; see the module documentation for the
/// form read. Text with no digit, or holding a word [`number_words`] rules out, is #VALUE!; other
/// text outside that form is not modelled.
fn text_as_decimal(text: &ExcelText) -> Result<Decimal, Stop> {
    let units = text.utf16_code_units();
    if !holds_digit(text) || number_words::cannot_be_number(units) {
        return Err(Stop::Error(WorksheetErrorCode::Value));
    }
    let start = units.iter().position(|&u| u != u16::from(b' '));
    let end = units.iter().rposition(|&u| u != u16::from(b' '));
    let trimmed = match (start, end) {
        (Some(start), Some(end)) => &units[start..=end],
        _ => &[][..],
    };
    let mut decimal = String::with_capacity(trimmed.len());
    for &unit in trimmed {
        match u8::try_from(unit) {
            Ok(byte) if byte.is_ascii() => decimal.push(char::from(byte)),
            _ => return Err(Stop::NotModelled),
        }
    }
    if !decimal_form(decimal.as_bytes()) {
        return Err(Stop::NotModelled);
    }
    if units.len() > crate::value_text::MAX_READER_TEXT {
        return Err(Stop::Error(WorksheetErrorCode::Value));
    }
    Ok(Decimal(decimal))
}

/// Whether `text` is in the plain decimal form
/// `[+|-] (digits [. [digits]] | . digits) [(e|E) [+|-] digits]`.
fn decimal_form(text: &[u8]) -> bool {
    let mut at = 0;
    if matches!(text.first(), Some(b'+' | b'-')) {
        at += 1;
    }
    let mut digits = 0;
    let mut points = 0;
    while let Some(&byte) = text.get(at) {
        match byte {
            b'0'..=b'9' => digits += 1,
            b'.' => points += 1,
            _ => break,
        }
        at += 1;
    }
    if digits == 0 || points > 1 {
        return false;
    }
    if matches!(text.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(text.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let exponent_start = at;
        while text.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at == exponent_start {
            return false;
        }
    }
    if at != text.len() {
        return false;
    }
    true
}

/// Whether the text holds a character that some script writes as a digit.
fn holds_digit(text: &ExcelText) -> bool {
    char::decode_utf16(text.utf16_code_units().iter().copied())
        .any(|c| c.is_ok_and(char::is_numeric))
}

fn eq_ignore_ascii_case(units: &[u16], upper: &[u16]) -> bool {
    units.len() == upper.len()
        && units.iter().zip(upper).all(|(&unit, &expected)| {
            u8::try_from(unit).is_ok_and(|byte| u16::from(byte.to_ascii_uppercase()) == expected)
        })
}

/// The cap on a written exponent's size in [`Decimal::scale`], which no oracle row reaches.
#[cfg(test)]
mod tests {
    use super::*;

    /// The text read as a number, converted as a kernel converts it.
    fn read(text: &str) -> Result<f64, Stop> {
        match number(&CalcValue::text(ExcelText::from_interop_assignment(text)))? {
            Numeric::Number(n) => Ok(n),
            Numeric::Decimal(decimal) => decimal.nearest(),
        }
    }

    /// An exponent of 19 digits or more is beyond i64 (and 2^64 wraps to 0 where overflow is not
    /// checked); with the cap each is #VALUE!, as any scale beyond -308..308 is.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn an_exponent_beyond_i64_is_value() {
        for text in [
            "1e9999999999999999999",
            "1e-9999999999999999999",
            "1e18446744073709551616",
            "0.0e18446744073709551616",
        ] {
            assert_eq!(
                read(text),
                Err(Stop::Error(WorksheetErrorCode::Value)),
                "{text}"
            );
        }
    }
}
