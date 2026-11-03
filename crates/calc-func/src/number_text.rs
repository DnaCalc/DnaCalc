//! A number's text when the & operator reads the number as text, as Excel gives it with `.` as
//! the decimal separator and no column width; cell display, TEXT and other number formats are not
//! this conversion.
//!
//! The rule, as measured in Excel 16.0 (build 20430) under the Windows locale en-ZA, whose decimal
//! separator `.` is en-US's (en-US itself was not measured); the text holds no thousands
//! separator:
//!
//! - Zero of either sign is `0`. A subnormal number is `0` too: Excel holds none, and a cell
//!   written as one reads back as +0.
//! - The magnitude's exact value is rounded to 15 significant digits and trailing zeros are
//!   dropped.
//! - When those digits written without an exponent take at most 20 characters, the sign aside,
//!   that is the text: `1000000000000000`, `99999999999999900000`, `0.000123456789012345`,
//!   `0.000000000000000001`.
//! - Otherwise the text is `d.dddE+dd`: the 15 digits are rounded again, a tie away from zero, to
//!   15 digits when the exponent of the 15-digit value is between -98 and 98 and to 14 when it is
//!   99 or -99 or beyond, trailing zeros dropped, and the exponent written with a sign and at least
//!   two digits: `1E+20`, `1.23456789012345E-05`, `1.2345678901235E+99`, `1.7976931348623E+308`.
//!   A second rounding that carries moves the exponent and keeps the digit count
//!   (9.99999999999995E-99 gives `1E-98`).
//! - A negative number's text is `-` and its magnitude's text.
//!
//! Excel's own first rounding departs from the exact one near a tie at the 16th significant
//! digit, and every departure measured lies within a relative 2^-60 of the tie. Within a relative
//! 2^-60 of a tie (an exact tie included) the text here takes the 15 digits below the tie
//! (dc-uar): of 131 measured tie cases Excel 16.0.20527 wrote (the catalogue oracle's dch9u_functions
//! rows, built by arithmetic, and dc_q9rb's), 126 went down (`7200547821627.375` gives
//! `7200547821627.37`, `999999999999999.5` gives `999999999999999`, `1234567890123455` gives
//! `1234567890123450`), as did `4037429.935952025`, whose exact value lies 6E-21 above its tie.
//! Five went up, and no rule found tells them apart (scalings by powers of ten in binary64,
//! x87 extended and 96-bit precision, truncated or rounded, each break more readings than they
//! fix): `5758361573115.375` (`.38`), `521310271410497.5`, `-6278515587045.875`,
//! `543212677140.0625` and `5.248519552626765E-06`. Those five mismatch Excel here.
//! The W111 held-out EXACT readings 03436/03453/03454 round upward outside that band:
//! their magnitude is a relative 1.61E-18 above its decimal midpoint.
//!
//! Every step is exact decimal arithmetic on the number's exact value, so the text does not
//! depend on the numeric mode or the platform.

use crate::decimal::{ExactDecimal, Natural};

/// Significant digits of the first rounding.
const SIGNIFICANT_DIGITS: usize = 15;

/// The longest text without an exponent, the sign aside.
const WIDTH: usize = 20;

/// A number within a relative 2^-NEAR_TIE_BITS of a 16th-digit tie is near it.
const NEAR_TIE_BITS: u32 = 60;

/// The text of a finite number, as ASCII (see the module documentation). It is always `Some`;
/// the option is kept for the callers that refuse a number without a text.
pub(crate) fn text(x: f64) -> Option<String> {
    debug_assert!(x.is_finite(), "only a finite number has a text");
    if x == 0.0 || x.is_subnormal() {
        return Some("0".to_owned());
    }
    let negative = x < 0.0;
    let exact = ExactDecimal::binary(x.abs());
    let all = exact.coefficient.digits();
    // The exponent of the leading digit: the value is d1.d2d3... * 10^exponent.
    let length = i32::try_from(all.len()).expect("a digit count");
    let exponent = length - 1 + exact.exponent;
    if near_tie(&exact.coefficient, &all) {
        return Some(shaped(
            negative,
            first_rounding(all.as_bytes(), exponent, false),
        ));
    }
    let up = all
        .as_bytes()
        .get(SIGNIFICANT_DIGITS)
        .is_some_and(|&d| d >= b'5');
    Some(shaped(
        negative,
        first_rounding(all.as_bytes(), exponent, up),
    ))
}

/// Whether the exact value `coefficient` (whose decimal digits are `digits`) lies within a
/// relative 2^-NEAR_TIE_BITS of a tie at its 16th significant digit, or on it.
fn near_tie(coefficient: &Natural, digits: &str) -> bool {
    let dropped = digits.len().saturating_sub(SIGNIFICANT_DIGITS);
    if dropped == 0 {
        return false;
    }
    // The band is narrower than 2^-60 * 10^15 < 0.001 of the last kept digit, so a number near a
    // tie has dropped digits that begin 49 or 50 (a single dropped 5 is the tie itself).
    let tail = &digits.as_bytes()[SIGNIFICANT_DIGITS..];
    let lead = (tail[0], tail.get(1).copied().unwrap_or(b'0'));
    if lead != (b'4', b'9') && lead != (b'5', b'0') {
        return false;
    }
    // With the dropped part r of the m dropped digits, the distance to the tie is |r - 10^m / 2|
    // in units of the last digit, and the number is near it when
    // |2r - 10^m| * 2^NEAR_TIE_BITS < 2 * coefficient.
    let power = Natural::pow10(u32::try_from(dropped).expect("a digit count"));
    let (_, remainder) = coefficient.div_rem(&power);
    let mut twice = remainder.clone();
    twice.add(&remainder);
    let distance = if twice >= power {
        twice.subtract(&power);
        twice
    } else {
        let mut gap = power.clone();
        gap.subtract(&twice);
        gap
    };
    let scaled = distance.times(&Natural::from_u64(1u64 << (NEAR_TIE_BITS - 1)));
    scaled < *coefficient
}

/// The first SIGNIFICANT_DIGITS of `digits` (ASCII, the first not zero; the value
/// d1.d2d3... * 10^exponent), one added to the last when `up`, with trailing zeros dropped, and the
/// exponent of the result's leading digit.
fn first_rounding(digits: &[u8], exponent: i32, up: bool) -> (Vec<u8>, i32) {
    let dropped = digits.len() > SIGNIFICANT_DIGITS;
    rounded(digits, exponent, SIGNIFICANT_DIGITS, dropped && up)
}

/// The text of the rounded digits `(digits, exponent)` of a magnitude, `-` first when `negative`.
fn shaped(negative: bool, (digits, exponent): (Vec<u8>, i32)) -> String {
    let sign = if negative { "-" } else { "" };
    let fixed = without_exponent(&digits, exponent);
    if fixed.len() <= WIDTH {
        return format!("{sign}{fixed}");
    }
    let kept = if exponent.unsigned_abs() < 99 {
        SIGNIFICANT_DIGITS
    } else {
        SIGNIFICANT_DIGITS - 1
    };
    let up = digits.get(kept).is_some_and(|&d| d >= b'5');
    let (digits, exponent) = rounded(&digits, exponent, kept, up);
    let (first, rest) = digits.split_at(1);
    let mantissa = if rest.is_empty() {
        ascii(first).to_owned()
    } else {
        format!("{}.{}", ascii(first), ascii(rest))
    };
    let exponent_sign = if exponent < 0 { '-' } else { '+' };
    format!(
        "{sign}{mantissa}E{exponent_sign}{:02}",
        exponent.unsigned_abs()
    )
}

/// `digits` (ASCII, the first not zero; the value d1.d2d3... * 10^exponent) cut to `kept`
/// significant digits, one added to the last when `up`, with trailing zeros dropped, and the
/// exponent of the result's leading digit, one more than `exponent` when adding carries into a new
/// leading digit.
fn rounded(digits: &[u8], exponent: i32, kept: usize, up: bool) -> (Vec<u8>, i32) {
    let mut exponent = exponent;
    let mut result = digits[..digits.len().min(kept)].to_vec();
    if up && !increment(&mut result) {
        // Every kept digit was 9: the result is 1 followed by zeros.
        result = vec![b'1'];
        exponent += 1;
    }
    while result.len() > 1 && result.last() == Some(&b'0') {
        result.pop();
    }
    (result, exponent)
}

/// Adds one to the last digit of `digits` in place; false when every digit was 9 (and is now 0).
fn increment(digits: &mut [u8]) -> bool {
    for digit in digits.iter_mut().rev() {
        if *digit == b'9' {
            *digit = b'0';
        } else {
            *digit += 1;
            return true;
        }
    }
    false
}

/// `digits` (d1.d2d3... * 10^exponent, no trailing zero) written without an exponent.
fn without_exponent(digits: &[u8], exponent: i32) -> String {
    let count = digits.len();
    match usize::try_from(exponent) {
        Ok(whole) if count <= whole + 1 => {
            format!("{}{}", ascii(digits), "0".repeat(whole + 1 - count))
        }
        Ok(whole) => format!(
            "{}.{}",
            ascii(&digits[..=whole]),
            ascii(&digits[whole + 1..])
        ),
        Err(_) => {
            let zeros = usize::try_from(-1 - exponent).expect("a negative exponent");
            format!("0.{}{}", "0".repeat(zeros), ascii(digits))
        }
    }
}

fn ascii(digits: &[u8]) -> &str {
    std::str::from_utf8(digits).expect("ASCII digits")
}
