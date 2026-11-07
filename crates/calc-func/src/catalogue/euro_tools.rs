//! EUROCONVERT, the function of the Euro Currency Tools add-in (EUROTOOL.XLAM).
//!
//! Excel knows the name only while that add-in is loaded: a bare `=EUROCONVERT(...)` is #NAME? in
//! Excel's default state (dc-h9u, the dcj9k_r26_scalar oracle rows), and a workbook reaches it
//! through a library link: the file form is `[1]!EUROCONVERT(...)`, a single-cell dynamic array,
//! with `[1]` an `xlLibrary` external link to EUROTOOL.XLAM. So the entry's surface is
//! [`SURFACE`], which no bare call resolves to, and CalcFml lowers a call qualified by a link to
//! that add-in to this kernel ([`crate::Catalogue::euro_tool_call`]). DNA Calc holds the add-in's behaviour itself, as Excel does with the
//! add-in loaded; a workbook recalculated in Excel without the add-in gives #NAME? instead.
//!
//! Measured in Excel 16.0.20527 (64-bit), `Formula2` with EUROTOOL.XLAM opened read-only,
//! 2026-11-07; the workbook is `crates/calc-host/tests/fixtures/euroconvert/measured.xlsx`:
//!
//! - **Currencies.** Seventeen national units plus EUR, in any letter case and untrimmed: the
//!   eleven original members, GRD, SIT, MTL, CYP, SKK and EEK. LVL, LTL, HRK, BGN, XEU and others
//!   are #VALUE!. [`UNITS`] holds each fixed rate and its rounding places.
//! - **The calculation.** The same unit gives the number unchanged. Otherwise the amount in euro
//!   is the number itself from EUR, and otherwise `ROUND(number / rate, 19)`, which keeps 15
//!   significant digits and at most 19 places; a triangulation precision rounds that to its
//!   places (not when the source is EUR). The target amount is the euro amount times the target's
//!   rate. Full precision returns it; else it is ROUND of it to the target's places.
//! - **Overflow.** A number / rate beyond binary64 gives 0; a target amount beyond binary64 gives
//!   #NUM! at full precision and 0 otherwise.
//! - **Arguments.** Every argument is read once as the add-in reads a single value, so nothing
//!   lifts: an array, a multi-cell range and any worksheet error are #VALUE!, as is a call with
//!   fewer than three or more than five arguments. See [`amount`], [`flag`] and [`places`].

use super::kit::{R, grid};
use super::rounding::nearest_digits;
use super::{ArgumentDemand, CatalogueEntry, Family, FunctionId, Kernel, Notation, Parameter};
use crate::call::{EvalServices, FunctionFault};
use crate::kernel::{Exit, Site, Step};
use crate::value::{CalcValue, CallableArityShape, CoreRef, WorksheetErrorCode};

const CODE: u16 = 322;
const EUROCONVERT: FunctionId = FunctionId::new(CODE, 1);

/// The catalogue spelling of the add-in's function, which only a library-qualified call reaches.
pub(super) const SURFACE: &str = "_EUROTOOL.EUROCONVERT";

pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[CatalogueEntry {
        id: EUROCONVERT,
        spelling: SURFACE,
        notation: Notation::Call,
        // The add-in's own check answers a wrong count: #VALUE!, not a refused entry.
        arity: CallableArityShape::range(0, 255),
        parameters: &[
            Parameter { name: "number", class: R },
            Parameter { name: "source", class: R },
            Parameter { name: "target", class: R },
            Parameter { name: "full_precision", class: R },
            Parameter { name: "triangulation_precision", class: R },
        ],
        demand: ArgumentDemand::Eager,
        kernel: Kernel::Eager(euroconvert),
    }],
};

/// Each unit's fixed rate to the euro and the places a converted amount rounds to.
const UNITS: [(&str, f64, i32); 18] = [
    ("BEF", 40.3399, 0),
    ("LUF", 40.3399, 0),
    ("DEM", 1.95583, 2),
    ("ESP", 166.386, 0),
    ("FRF", 6.55957, 2),
    ("IEP", 0.787564, 2),
    ("ITL", 1936.27, 0),
    ("NLG", 2.20371, 2),
    ("ATS", 13.7603, 2),
    ("PTE", 200.482, 0),
    ("FIM", 5.94573, 2),
    ("GRD", 340.75, 0),
    ("SIT", 239.64, 2),
    ("MTL", 0.4293, 2),
    ("CYP", 0.585274, 2),
    ("SKK", 30.126, 2),
    ("EEK", 15.6466, 2),
    ("EUR", 1.0, 2),
];

/// The index of EUR in [`UNITS`].
const EURO: usize = 17;

/// A quotient is rounded as `ROUND(quotient,19)`: 15 significant digits, at most 19 places.
const QUOTIENT_PLACES: f64 = 19.0;

fn value_error<T>() -> Step<T> {
    Err(Exit::Error(WorksheetErrorCode::Value))
}

/// Argument `index` as one value: a single-cell reference reads its cell; an array or a range of
/// more than one cell is #VALUE!, as is an error.
fn single(
    site: &Site<'_>,
    arguments: &[CalcValue],
    index: usize,
    services: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    if matches!(arguments[index].core_ref(), CoreRef::Array(_)) {
        return value_error();
    }
    let held = grid(site, arguments, index, services)?;
    if held.rows != 1 || held.cols != 1 {
        return value_error();
    }
    let value = held.cells.into_iter().next().unwrap_or_else(CalcValue::empty);
    if matches!(value.core_ref(), CoreRef::Error(_)) {
        return value_error();
    }
    Ok(value)
}

/// A unit code: text only, matched whole in any letter case. Its index in [`UNITS`].
fn unit(value: &CalcValue) -> Step<usize> {
    let CoreRef::Text(text) = value.core_ref() else {
        return value_error();
    };
    let text = text.to_string_lossy();
    UNITS
        .iter()
        .position(|(code, ..)| code.eq_ignore_ascii_case(&text))
        .map_or_else(value_error, Ok)
}

/// Text read as the add-in reads a number; `None` when it is not one, which is #VALUE!. Measured
/// on this en-ZA machine: spaces round the text and inside parentheses go, a space may group digits
/// (`"1 000"`), a sign may lead or trail (`"1-"` is -1), parentheses negate (`"(5)"`), the currency
/// `R` may lead, an exponent is E or D with a sign or none, and `&H` and `&O` are unsigned
/// (`"&HFFFF"` is 65535). Commas, `$`, `%`, dates, `"1e"`, `"1 e 3"`, `"--1"`, `"(-5)"` and
/// `"-(5)"` are not read.
fn read_number(text: &str) -> Option<f64> {
    let text = text.trim_matches(' ');
    if let Some(body) = text.strip_prefix('&') {
        let (radix, digits) = match body.chars().next()? {
            'h' | 'H' => (16, &body[1..]),
            'o' | 'O' => (8, &body[1..]),
            _ => return None,
        };
        return u64::from_str_radix(digits, radix).ok().map(|value| value as f64);
    }
    let (negated, body) = match text.strip_prefix('(').and_then(|t| t.strip_suffix(')')) {
        Some(inner) => (true, inner.trim_matches(' ')),
        None => (false, text),
    };
    let body = body.strip_prefix('R').unwrap_or(body);
    let (lead, body) = match body.as_bytes().first()? {
        b'+' => (Some(1.0), &body[1..]),
        b'-' => (Some(-1.0), &body[1..]),
        _ => (None, body),
    };
    let (trail, body) = match body.as_bytes().last()? {
        b'+' if lead.is_none() && !negated => (Some(1.0), &body[..body.len() - 1]),
        b'-' if lead.is_none() && !negated => (Some(-1.0), &body[..body.len() - 1]),
        _ => (None, body),
    };
    if negated && (lead.is_some() || trail.is_some()) {
        return None;
    }
    let sign = lead.or(trail).unwrap_or(1.0);
    let (mantissa, exponent) = match body.find(['e', 'E', 'd', 'D']) {
        Some(at) => (&body[..at], Some(&body[at + 1..])),
        None => (body, None),
    };
    let grouped = |at: usize, byte: u8| {
        byte == b' '
            && at > 0
            && mantissa.as_bytes()[at - 1].is_ascii_digit()
            && mantissa.as_bytes().get(at + 1).is_some_and(u8::is_ascii_digit)
    };
    if !mantissa.bytes().enumerate().all(|(at, byte)| {
        byte.is_ascii_digit() || byte == b'.' || grouped(at, byte)
    }) {
        return None;
    }
    let mut decimal: String = mantissa.chars().filter(|c| *c != ' ').collect();
    if decimal.bytes().filter(|byte| *byte == b'.').count() > 1
        || !decimal.bytes().any(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    if let Some(exponent) = exponent {
        let unsigned = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if unsigned.is_empty() || !unsigned.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        decimal.push('e');
        decimal.push_str(exponent);
    }
    let value: f64 = decimal.parse().ok()?;
    Some(if negated { -value } else { sign * value })
}

/// The number to convert: a blank is 0, TRUE is -1 and FALSE 0, and text is [`read_number`]; text
/// that reads past binary64 is #NUM!.
fn amount(value: &CalcValue) -> Step<f64> {
    match value.core_ref() {
        CoreRef::Number(number) => Ok(number),
        CoreRef::Logical(logical) => Ok(if logical { -1.0 } else { 0.0 }),
        CoreRef::Empty | CoreRef::Missing => Ok(0.0),
        CoreRef::Text(text) => match read_number(&text.to_string_lossy()) {
            Some(number) if number.is_finite() => Ok(number),
            Some(_) => Err(Exit::Error(WorksheetErrorCode::Num)),
            None => value_error(),
        },
        _ => value_error(),
    }
}

/// full_precision: absent, blank and zero are FALSE; any other number is TRUE, and text is
/// `TRUE` or `FALSE` in any letter case or a number.
fn flag(value: &CalcValue) -> Step<bool> {
    match value.core_ref() {
        CoreRef::Logical(logical) => Ok(logical),
        CoreRef::Number(number) => Ok(number != 0.0),
        CoreRef::Empty | CoreRef::Missing => Ok(false),
        CoreRef::Text(text) => {
            let text = text.to_string_lossy();
            if text.eq_ignore_ascii_case("TRUE") {
                Ok(true)
            } else if text.eq_ignore_ascii_case("FALSE") {
                Ok(false)
            } else {
                match read_number(&text) {
                    Some(number) => Ok(number != 0.0),
                    None => value_error(),
                }
            }
        }
        _ => value_error(),
    }
}

/// triangulation_precision, when given: a number or numeric text, rounded to an integer with
/// halves to even; below 3 is #VALUE!, and past 16 bits #NUM!. A logical or a blank cell is #VALUE!.
fn places(value: &CalcValue) -> Step<Option<i32>> {
    let number = match value.core_ref() {
        CoreRef::Missing => return Ok(None),
        CoreRef::Number(number) => number,
        CoreRef::Text(text) => match read_number(&text.to_string_lossy()) {
            Some(number) => number,
            None => return value_error(),
        },
        _ => return value_error(),
    };
    let nearest = number.round_ties_even();
    if !(-32768.0..=32767.0).contains(&nearest) {
        return Err(Exit::Error(WorksheetErrorCode::Num));
    }
    if nearest < 3.0 {
        return value_error();
    }
    Ok(Some(nearest as i32))
}

fn convert(
    site: &Site<'_>,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    if !(3..=5).contains(&arguments.len()) {
        return value_error();
    }
    let number = single(site, arguments, 0, services)?;
    let source = single(site, arguments, 1, services)?;
    let target = single(site, arguments, 2, services)?;
    let full = match arguments.get(3).filter(|value| !value.is_missing()) {
        Some(_) => flag(&single(site, arguments, 3, services)?)?,
        None => false,
    };
    let triangulation = match arguments.get(4).filter(|value| !value.is_missing()) {
        Some(_) => places(&single(site, arguments, 4, services)?)?,
        None => None,
    };
    let source_unit = unit(&source)?;
    let target_unit = unit(&target)?;
    let (_, source_rate, _) = UNITS[source_unit];
    let (_, target_rate, target_places) = UNITS[target_unit];
    let (from_euro, to_euro) = (source_unit == EURO, target_unit == EURO);
    let number = amount(&number)?;
    site.binary64()?;
    if source_unit == target_unit {
        return site.number(number);
    }
    let mut euro = number;
    if !from_euro {
        let quotient = number / source_rate;
        if !quotient.is_finite() {
            return Ok(CalcValue::number(0.0));
        }
        euro = nearest_digits(site, quotient, QUOTIENT_PLACES)?;
        if let Some(places) = triangulation {
            euro = nearest_digits(site, euro, f64::from(places))?;
        }
    }
    let converted = if to_euro { euro } else { euro * target_rate };
    if full {
        return site.number(converted);
    }
    if !converted.is_finite() {
        return Ok(CalcValue::number(0.0));
    }
    let rounded = nearest_digits(site, converted, f64::from(target_places))?;
    site.number(rounded)
}

fn euroconvert(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(EUROCONVERT, services);
    Site::finish(convert(&site, arguments, services))
}
