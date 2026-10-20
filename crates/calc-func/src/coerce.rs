//! Worksheet coercion: how an operand or a direct function argument is read as a number, a
//! logical or text. A value that a reference or an array supplies to an aggregate is not coerced
//! here; the kernel reading it decides (SUM skips text and logicals there).
//!
//! Empty (a blank cell), Missing (an omitted argument) and empty text stay distinct: the first two
//! read as 0, FALSE and empty text, while empty text read as a number or a logical is #VALUE!.
//!
//! Text reads as a number only in the plain decimal form: optional spaces, an optional sign,
//! digits with at most one '.', an optional exponent and optional spaces, with at most 15
//! significant digits. Text with no digit at all is #VALUE!. Any other text holding a digit (a
//! date, a time, a fraction, a currency amount, grouped digits, a percentage, a 16th significant
//! digit) is refused as not modelled, because Excel may read it as a number through its locale's
//! number-text rules, which no service supplies yet; the same holds for a logical's text forms,
//! which are the English TRUE and FALSE. Coercion only reads the decimal ([`Numeric::Decimal`]):
//! finding its nearest binary64 rounds, so the kernel does that at an arithmetic site, under the
//! call's numeric mode.

use crate::value::{CalcValue, CoreValue, ExcelText, WorksheetErrorCode};

/// Why a coercion produced no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    /// The result is this worksheet error, which the call returns.
    Error(WorksheetErrorCode),
    /// Excel's result for this value is not modelled, and the call is refused.
    NotModelled,
}

/// Significant digits Excel keeps when it reads a number.
const SIGNIFICANT_DIGITS: usize = 15;

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
    /// The nearest binary64, ties to even. A zero ("-0" included) and a value below the smallest
    /// subnormal read as +0; a value beyond binary64's range or in its subnormal range is refused
    /// as not modelled.
    pub(crate) fn nearest(&self) -> Result<f64, Stop> {
        let n: f64 = self.0.parse().map_err(|_| Stop::NotModelled)?;
        if n == 0.0 { Ok(0.0) } else { operand(n) }
    }
}

/// The value as a number: a number as it is, TRUE as 1 and FALSE as 0, Empty and Missing as 0,
/// text through [`number_text`], an error as itself. An array (whose operator lifts over its
/// elements) and a reference are not read here.
pub(crate) fn number(value: &CalcValue) -> Result<Numeric, Stop> {
    match value.core() {
        CoreValue::Number(n) => operand(*n).map(Numeric::Number),
        CoreValue::Logical(b) => Ok(Numeric::Number(if *b { 1.0 } else { 0.0 })),
        CoreValue::Text(text) => number_text(text).map(Numeric::Decimal),
        CoreValue::Empty | CoreValue::Missing => Ok(Numeric::Number(0.0)),
        CoreValue::Error(code) => Err(Stop::Error(*code)),
        CoreValue::Array(_) | CoreValue::Reference(_) => Err(Stop::NotModelled),
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
/// itself. Other text is #VALUE! unless it holds a digit, which is refused as not modelled.
pub(crate) fn logical(value: &CalcValue) -> Result<bool, Stop> {
    match value.core() {
        CoreValue::Number(n) => operand(*n).map(|n| n != 0.0),
        CoreValue::Logical(b) => Ok(*b),
        CoreValue::Text(text) => {
            let units = text.utf16_code_units();
            if eq_ignore_ascii_case(units, &TRUE_TEXT) {
                Ok(true)
            } else if eq_ignore_ascii_case(units, &FALSE_TEXT) {
                Ok(false)
            } else if holds_digit(text) {
                Err(Stop::NotModelled)
            } else {
                Err(Stop::Error(WorksheetErrorCode::Value))
            }
        }
        CoreValue::Empty | CoreValue::Missing => Ok(false),
        CoreValue::Error(code) => Err(Stop::Error(*code)),
        CoreValue::Array(_) | CoreValue::Reference(_) => Err(Stop::NotModelled),
    }
}

/// The value as text to concatenate, in UTF-16 code units: text as it is, TRUE and FALSE as
/// those words, Empty and Missing as empty text, an error as itself. A number's text is the
/// General number format's, which no service supplies yet, so a number is refused as not
/// modelled.
pub(crate) fn text(value: &CalcValue) -> Result<&[u16], Stop> {
    match value.core() {
        CoreValue::Text(text) => Ok(text.utf16_code_units()),
        CoreValue::Logical(true) => Ok(&TRUE_TEXT),
        CoreValue::Logical(false) => Ok(&FALSE_TEXT),
        CoreValue::Empty | CoreValue::Missing => Ok(&[]),
        CoreValue::Error(code) => Err(Stop::Error(*code)),
        CoreValue::Number(_) | CoreValue::Array(_) | CoreValue::Reference(_) => {
            Err(Stop::NotModelled)
        }
    }
}

/// Text that reads as a number, as the decimal it holds; see the module documentation for the
/// form read.
fn number_text(text: &ExcelText) -> Result<Decimal, Stop> {
    if !holds_digit(text) {
        return Err(Stop::Error(WorksheetErrorCode::Value));
    }
    let units = text.utf16_code_units();
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
    let Some(significant) = decimal_form(decimal.as_bytes()) else {
        return Err(Stop::NotModelled);
    };
    if significant > SIGNIFICANT_DIGITS {
        return Err(Stop::NotModelled);
    }
    Ok(Decimal(decimal))
}

/// The number of significant digits of `text` when it is in the plain decimal form
/// `[+|-] (digits [. [digits]] | . digits) [(e|E) [+|-] digits]`, or `None`.
fn decimal_form(text: &[u8]) -> Option<usize> {
    let mut at = 0;
    if matches!(text.first(), Some(b'+' | b'-')) {
        at += 1;
    }
    let mantissa_start = at;
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
        return None;
    }
    let mantissa = &text[mantissa_start..at];
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
            return None;
        }
    }
    if at != text.len() {
        return None;
    }
    let significant: Vec<u8> = mantissa
        .iter()
        .copied()
        .filter(u8::is_ascii_digit)
        .collect();
    let first = significant.iter().position(|&d| d != b'0');
    let last = significant.iter().rposition(|&d| d != b'0');
    Some(match (first, last) {
        (Some(first), Some(last)) => last - first + 1,
        _ => 0,
    })
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
