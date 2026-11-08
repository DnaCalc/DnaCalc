//! The rounding and integer family fragment: ABS, SIGN, INT, TRUNC, ROUND, ROUNDUP, ROUNDDOWN,
//! MROUND, CEILING, FLOOR, MOD, QUOTIENT, EVEN and ODD, and their kernels.
//!
//! Every parameter takes a single value: the caller dereferences a reference first, and an array
//! argument, over which Excel lifts the function, is refused as not modelled before any argument
//! is read. The arguments are read left to right through worksheet number coercion, so the first
//! argument's error, or its coercion error, is the result before any domain rule applies.
//!
//! ABS, SIGN, INT, EVEN and ODD act on the binary64 itself: its absolute value, sign and integer
//! parts are exact. TRUNC, ROUND, ROUNDUP and ROUNDDOWN round to a decimal position; they read
//! each number to 15 significant digits ([`ExactDecimal::fifteen_x87`] where the scaling is exact
//! on the x87 unit, [`ExactDecimal::fifteen`] elsewhere), compute on that decimal exactly, and
//! convert the exact result once to the nearest binary64 at an arithmetic site. So
//! ROUND(2.675,2) is 2.68, as the typed 2.675 reads, although the binary64 of 2.675 lies below
//! the half, and TRUNC(2.9999999999999996) is 3 (dc-bd5). ROUND counts num_digits as a 32-bit
//! integer and the directed functions as a 16-bit one (`nearest_digits`, `directed_places`);
//! ROUNDUP adds its unit in binary64 (`directed`). MOD and QUOTIENT divide in binary64
//! (QUOTIENT(0.3,0.1) is 2).
//!
//! CEILING and FLOOR compute in binary64: the quotient number / significance, rounded at an
//! arithmetic site, is taken to the integer above or below it, and the significance times that
//! integer is rounded at another. So CEILING(0.3,0.1) is 0.30000000000000004 and FLOOR(0.3,0.1)
//! is 0.2, the binary64 quotient being 2.9999999999999996.
//!
//! MROUND counts its multiples from the binary64 quotient number / multiple, rounded at an
//! arithmetic site, to the integer below it, plus one when its fraction is at least a half less
//! 5E-15 (`nearest_multiple`); the count times the multiple is rounded at another site. Its
//! page's Known Limitations example gives MROUND(6.05,0.1) as 6.0 and MROUND(7.05,0.1) as 7.1.
//! Counting from the binary64 quotients, 60.49999999999999 and 70.5, agrees with both outcomes;
//! counting on the decimal readings gives the half 60.5 and so 6.1 for the first.
//!
//! No page settles which of these rules Excel's division follows for each function; the owner's
//! Excel capture of the oracle rows that tell them apart (CEILING(0.3,0.1), FLOOR(0.3,0.1),
//! MOD(0.3,0.1) and QUOTIENT(0.3,0.1) among them) decides.
//!
//! Nothing passes through an integer type that could overflow: inputs near 2^53 or at the largest
//! binary64 are read and computed exactly, or, in CEILING and FLOOR, as binary64.
//!
//! The directed functions retain an initial reading below MIN_NORMAL as described in
//! `directed_initial_underflow`; other results follow the number publication rule: one beyond
//! binary64's range is #NUM!, a zero is
//! +0 whatever its sign, and a subnormal one is refused, as is a non-zero exact result too small
//! to convert to anything but a zero. Under NumericMode::Exact a call that reaches an arithmetic
//! site (the conversion of number text or of an exact result, the division of MROUND, CEILING and
//! FLOOR, or ODD's and EVEN's step to the next integer) is refused with ExactUnavailable; one that
//! reaches none computes.

use super::{ArgumentDemand, CatalogueEntry, Family, FunctionId, Kernel, Notation, Parameter};
use crate::call::{EagerKernel, EvalServices, FunctionFault};
use crate::decimal::{self, Direction, ExactDecimal};
use crate::kernel::{Exit, Site, Step};
use crate::value::{CalcValue, CallableArityShape, CoreRef, WorksheetErrorCode};

const CODE: u16 = 6;

const ABS: FunctionId = FunctionId::new(CODE, 1);
const SIGN: FunctionId = FunctionId::new(CODE, 2);
const INT: FunctionId = FunctionId::new(CODE, 3);
const TRUNC: FunctionId = FunctionId::new(CODE, 4);
const ROUND: FunctionId = FunctionId::new(CODE, 5);
const ROUNDUP: FunctionId = FunctionId::new(CODE, 6);
const ROUNDDOWN: FunctionId = FunctionId::new(CODE, 7);
const MROUND: FunctionId = FunctionId::new(CODE, 8);
const CEILING: FunctionId = FunctionId::new(CODE, 9);
const FLOOR: FunctionId = FunctionId::new(CODE, 10);
const MOD: FunctionId = FunctionId::new(CODE, 11);
const QUOTIENT: FunctionId = FunctionId::new(CODE, 12);
const EVEN: FunctionId = FunctionId::new(CODE, 13);
const ODD: FunctionId = FunctionId::new(CODE, 14);
const ECMA_CEILING: FunctionId = FunctionId::new(CODE, 15);

const NUMBER: &[Parameter] = &[Parameter::value("number")];
const DIGITS: &[Parameter] = &[Parameter::value("number"), Parameter::value("num_digits")];
const MULTIPLE: &[Parameter] = &[Parameter::value("number"), Parameter::value("multiple")];
const SIGNIFICANCE: &[Parameter] = &[Parameter::value("number"), Parameter::value("significance")];
const DIVISOR: &[Parameter] = &[Parameter::value("number"), Parameter::value("divisor")];
const FRACTION: &[Parameter] = &[
    Parameter::value("numerator"),
    Parameter::value("denominator"),
];

/// An eager named call that takes from `min` to as many arguments as it names parameters.
const fn call(
    id: FunctionId,
    spelling: &'static str,
    min: usize,
    parameters: &'static [Parameter],
    kernel: EagerKernel,
) -> CatalogueEntry {
    CatalogueEntry {
        id,
        spelling,
        notation: Notation::Call,
        kind: super::EntryKind::Worksheet,
        availability: super::Availability::Unspecified,
        arity: CallableArityShape::range(min, parameters.len()),
        groups: None,
        parameters,
        result: super::ResultForm::Value,
        demand: ArgumentDemand::Eager,
        traits: super::EntryTraits::NONE,
        kernel: Kernel::Eager(kernel),
    }
}

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[
        call(ABS, "ABS", 1, NUMBER, abs).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
        call(SIGN, "SIGN", 1, NUMBER, sign).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
        call(INT, "INT", 1, NUMBER, int).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
        call(TRUNC, "TRUNC", 1, DIGITS, trunc).with_three_d(super::ThreeD::new(None, None, Some(&[]))).with_traits(super::EntryTraits::NONE.flush_subnormal_inputs().retain_overflow_number().retain_subnormal_output()),
        call(ROUND, "ROUND", 2, DIGITS, round).with_three_d(super::ThreeD::new(None, None, Some(&[]))).with_traits(super::EntryTraits::NONE.flush_subnormal_inputs().retain_overflow_number()),
        // W111 directed-rounding padding and multi-cell reference controls.
        call(ROUNDUP, "ROUNDUP", 2, DIGITS, roundup).with_three_d(super::ThreeD::new(None, None, Some(&[]))).with_traits(super::EntryTraits::NONE.padding_precedes_call().flush_subnormal_inputs().retain_subnormal_output()),
        call(ROUNDDOWN, "ROUNDDOWN", 2, DIGITS, rounddown).with_three_d(super::ThreeD::new(None, None, Some(&[]))).with_traits(super::EntryTraits::NONE.padding_precedes_call().flush_subnormal_inputs().retain_overflow_number().retain_subnormal_output()),
        call(MROUND, "MROUND", 2, MULTIPLE, mround).with_three_d(super::ThreeD::new(None, Some(&[]), None)).with_traits(super::EntryTraits::NONE.reject_multi_cell_reference().strict_number_arguments()),
        call(CEILING, "CEILING", 2, SIGNIFICANCE, ceiling).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
        call(FLOOR, "FLOOR", 2, SIGNIFICANCE, floor).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
        call(MOD, "MOD", 2, DIVISOR, modulo).with_three_d(super::ThreeD::new(None, None, Some(&[]))).with_traits(super::EntryTraits::NONE.flush_subnormal_inputs()),
        call(QUOTIENT, "QUOTIENT", 2, FRACTION, quotient).with_three_d(super::ThreeD::new(None, Some(&[]), None)).with_traits(super::EntryTraits::NONE.reject_multi_cell_reference().strict_number_arguments()),
        call(EVEN, "EVEN", 1, NUMBER, even).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
        call(ODD, "ODD", 1, NUMBER, odd).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
        call(ECMA_CEILING, "ECMA.CEILING", 2, SIGNIFICANCE, ecma_ceiling),
    ],
};

/// Runs `compute` on the call's arguments, read as numbers in order, and publishes its number.
fn numeric(
    id: FunctionId,
    arguments: &[CalcValue],
    services: &EvalServices<'_>,
    compute: fn(&Site<'_>, &[f64]) -> Step<f64>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(id, services);
    let traits = super::catalogue().resolve(id).expect("declared rounding entry").traits();
    Site::finish(numbers(&site, arguments, traits).and_then(|read| {
        let result = compute(&site, &read)?;
        // Only `stored` makes a NaN here: Excel's own bit pattern for a result past DBL_MAX,
        // which the worksheet holds as a number.
        if result.is_nan() && traits.retain_overflow_number {
            Ok(CalcValue::excel_overflow_number(result).expect("the stored rounding result"))
        } else if result.is_subnormal() && traits.retain_subnormal_output {
            // Only the measured initial-reading return below can reach this publication.
            super::kit::out_with_subnormal(&site, Ok(result))
        } else {
            site.number(result)
        }
    }))
}

/// The arguments as numbers: an array argument is refused first, then each argument is read in
/// order, so the first one's error is the result.
fn numbers(site: &Site<'_>, arguments: &[CalcValue], traits: super::EntryTraits) -> Step<Vec<f64>> {
    site.scalar_operands(arguments)?;
    arguments
        .iter()
        .enumerate()
        .map(|(index, argument)| {
            // W111 typed MROUND/QUOTIENT readings: logicals are #VALUE!, an explicit
            // omission #N/A. A referenced blank remains Empty and coerces to zero.
            if traits.strict_number_arguments {
                match argument.core_ref() {
                    CoreRef::Logical(_) => return Err(Exit::Error(WorksheetErrorCode::Value)),
                    CoreRef::Missing => return Err(Exit::Error(WorksheetErrorCode::NA)),
                    _ => {}
                }
            }
            if traits.flush_subnormal_inputs && argument.as_number().is_some_and(f64::is_subnormal) {
                Ok(0.0)
            } else {
                site.number_argument(index, argument)
            }
        })
        .collect()
}

/// ABS: the number without its sign.
fn abs(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    if arguments[0].is_excel_overflow() {
        let magnitude = arguments[0].as_number().expect("qualified overflow number").abs();
        return Ok(CalcValue::excel_overflow_number(magnitude).expect("sign removal retains overflow encoding"));
    }
    numeric(ABS, arguments, services, |_, n| Ok(n[0].abs()))
}

/// SIGN: 1 for a positive number, -1 for a negative one, 0 for zero.
fn sign(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    numeric(SIGN, arguments, services, |_, n| {
        Ok(if n[0] > 0.0 {
            1.0
        } else if n[0] < 0.0 {
            -1.0
        } else {
            0.0
        })
    })
}

/// INT: the largest integer not above the number read to 15 significant digits
/// ([`ExactDecimal::fifteen`]) when it is below 1E15 in magnitude, and not above its binary64
/// otherwise (dc-bd5, Excel 16.0.20527 through COM on 2026-10-30, 109 readings:
/// INT(2.9999999999999996) and INT(2.9999999999999982) are 3, INT(123456789012345.67) is
/// 123456789012346, INT(-3.0000000000000004) is -3, INT(999999999999999.5) is 1E15, while
/// INT(4503599627370495.5) is 4503599627370495).
fn int(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    numeric(INT, arguments, services, |site, n| {
        let x = n[0];
        if x.abs() >= 1e15 {
            return Ok(decimal::floor(x));
        }
        match ExactDecimal::fifteen(x) {
            (reading, true) => match reading.round_at(0, Direction::TowardZero) {
                Some(whole) => {
                    let whole = site.nearest(&whole)?;
                    let read = site.nearest(&reading)?;
                    // Toward zero, one less for a negative reading with a fraction.
                    Ok(if read < whole { whole - 1.0 } else { whole })
                }
                None => site.nearest(&reading),
            },
            (_, false) => Ok(decimal::floor(x)),
        }
    })
}

/// TRUNC: ROUNDDOWN with num_digits 0 when it is omitted.
fn trunc(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    numeric(TRUNC, arguments, services, |site, n| {
        directed(
            site,
            n[0],
            n.get(1).copied().unwrap_or(0.0),
            Direction::TowardZero,
        )
    })
}

/// ROUND: to num_digits decimal places (left of the point when negative), a half away from zero.
fn round(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    numeric(ROUND, arguments, services, |site, n| {
        nearest_digits(site, n[0], n[1])
    })
}

/// ROUNDUP: as ROUND, away from zero whenever a dropped digit is not zero.
fn roundup(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    numeric(ROUNDUP, arguments, services, |site, n| {
        directed(site, n[0], n[1], Direction::AwayFromZero)
    })
}

/// ROUNDDOWN: as ROUND, toward zero.
fn rounddown(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    numeric(ROUNDDOWN, arguments, services, |site, n| {
        directed(site, n[0], n[1], Direction::TowardZero)
    })
}

/// Beyond this many places either way no number's reading has a digit (a reading's digits lie
/// between 10^-1074 and 10^308), so clamping a position here changes no result.
const PLACES: i32 = 2000;

/// The fifteen significant digits the rounding functions round: the x87 reading where it is
/// known ([`ExactDecimal::fifteen_x87`]), and otherwise the number read to fifteen significant
/// digits, a half away from zero ([`ExactDecimal::fifteen`]).
fn reading(number: f64, ties_up: bool) -> ExactDecimal {
    ExactDecimal::fifteen_x87(number, ties_up).unwrap_or_else(|| ExactDecimal::fifteen(number).0)
}

/// A result below the normal range is 0, as Excel gives it (dc-bd5:
/// ROUNDDOWN(2.5E-308,308) is 0 in Excel 16.0.20430).
fn flush(result: f64) -> f64 {
    if result.is_subnormal() { 0.0 } else { result }
}

/// ROUND's kernel. num_digits counts as a 32-bit integer, toward zero and limited to
/// +/-2147483647, and ROUND keeps `digits + e + 1` significant digits of the number, e being its
/// decimal exponent, the sum taken in 32-bit two's complement: fifteen or more keep the whole
/// reading, fewer than none give 0, and otherwise the reading is rounded at 10^-digits. So
/// ROUND(1.5,2147483647) is 0 (the sum wraps negative) where 2147483646 leaves 1.5, and
/// ROUND(-5.480222088275654E-221,-2147483480.2228494) is -5.48022208827565E-221 (the sum wraps
/// positive), as Excel 16.0.20430 gives them (the OxFunc-Evidence bank's integer-threshold,
/// hyperbolic and count-refinement runs).
pub(super) fn nearest_digits(site: &Site<'_>, number: f64, places: f64) -> Step<f64> {
    nearest_digits_with(site, number, places, stored)
}

/// EUROTOOL publishes its decimal reading directly. The captured add-in
/// conversion of7840 ESP to NLG retains the upper double, while worksheet
/// ROUND(7840/166.386,19) publishes the lower radix16-scaled double.
pub(super) fn nearest_digits_exact(site: &Site<'_>, number: f64, places: f64) -> Step<f64> {
    nearest_digits_with(site, number, places, stored_nearest)
}

fn nearest_digits_with(
    site: &Site<'_>, number: f64, places: f64,
    publish: fn(&Site<'_>, &ExactDecimal) -> Step<f64>,
) -> Step<f64> {
    if number == 0.0 {
        return Ok(0.0);
    }
    let limit = f64::from(i32::MAX);
    // An integer within +/-2147483647, so the conversion is exact.
    let digits = decimal::truncate(places).clamp(-limit, limit) as i32;
    let kept = digits
        .wrapping_add(ExactDecimal::binary(number).magnitude())
        .wrapping_add(1);
    if kept < 0 {
        return Ok(0.0);
    }
    let reading = reading(number, false);
    // Without a wrap, kept from 0 to 14 bounds digits to within 340 of 0.
    match reading.round_at(-digits.clamp(-PLACES, PLACES), Direction::Nearest) {
        Some(rounded) if kept < 15 => Ok(flush(publish(site, &rounded)?)),
        _ => whole_reading_with(site, number, &reading, publish),
    }
}

/// The reading itself as the result: the number unchanged, with no rounding, when the reading
/// is its exact value, and otherwise the reading converted at an arithmetic site.
fn whole_reading(site: &Site<'_>, number: f64, reading: &ExactDecimal) -> Step<f64> {
    whole_reading_with(site, number, reading, stored)
}

fn whole_reading_with(
    site: &Site<'_>, number: f64, reading: &ExactDecimal,
    publish: fn(&Site<'_>, &ExactDecimal) -> Step<f64>,
) -> Step<f64> {
    if reading.is_value_of(number) {
        Ok(number)
    } else {
        Ok(flush(publish(site, reading)?))
    }
}

/// An exact decimal result as Excel stores it in binary64: the nearest binary64, except that a
/// result of 2^1024 or more keeps its 53-bit significand and carries its exponent into the
/// exponent field without a check, so the field reads 2047 and the bits are a NaN. So
/// ROUND(1.7976931348623157E+308,-308), 2E+308, is 0x7ff1ccf385ebc8a0 and the reading of
/// DBL_MAX, 1.79769313486232E+308, is 0x7ff000000000000a, as Excel 16.0.20430 and 16.0.20527 give
/// them (627 of 627 such bank rows). Excel shows such a value as 2E+308, ISNUMBER is TRUE, and
/// arithmetic on it is #NUM!. A decimal of 2^1025 or more, which no reading reaches, is #NUM!.
fn stored(site: &Site<'_>, decimal: &ExactDecimal) -> Step<f64> {
    if site.nearest(decimal)?.is_normal() {
        site.nearest(&decimal.scaled_pc64())
    } else {
        stored_nearest(site, decimal)
    }
}

fn stored_nearest(site: &Site<'_>, decimal: &ExactDecimal) -> Step<f64> {
    let nearest = site.nearest(decimal)?;
    if nearest.is_finite() {
        return Ok(nearest);
    }
    // Half the decimal, exactly: the same significand one binade lower.
    let mut coefficient = decimal.coefficient.clone();
    coefficient.add(&decimal.coefficient.times(&decimal::Natural::from_u64(4)));
    let half = site.nearest(&ExactDecimal {
        negative: decimal.negative,
        coefficient,
        exponent: decimal.exponent - 1,
    })?;
    if half.is_finite() {
        Ok(f64::from_bits(half.to_bits() + (1 << 52)))
    } else {
        Err(Exit::Error(WorksheetErrorCode::Num))
    }
}

/// How ROUNDUP, ROUNDDOWN and TRUNC count num_digits: the 16 low bits of the integer part of its
/// magnitude, with its sign. A magnitude of 2^32 or more, or one whose 16 low bits reach 32768,
/// counts as 100 places either way and is out of range (false in the second place). So 32768 to
/// 65535 places count as 100, 65537 as 1, and -2147483748 as -100, as Excel 16.0.20430 gives
/// them (the OxFunc-Evidence bank's count-refinement and held-out runs).
fn directed_places(places: f64) -> (i32, bool) {
    let magnitude = places.abs();
    let low = if magnitude >= 4_294_967_296.0 {
        None
    } else {
        // Below 2^32, so the conversion is exact; the mask keeps 16 bits.
        let low = (decimal::truncate(magnitude) as u64) & 0xFFFF;
        (low < 0x8000).then(|| i32::try_from(low).expect("15 bits"))
    };
    let (count, in_range) = match low {
        Some(low) => (low, true),
        None => (100, false),
    };
    (if places < 0.0 { -count } else { count }, in_range)
}

/// ROUNDUP's, ROUNDDOWN's and TRUNC's kernel: the reading truncated at 10^-places, and for
/// ROUNDUP, when a digit was dropped, that truncation plus one unit of the place, the two
/// converted to binary64 and added at an arithmetic site (so ROUNDUP(-1.125,2) is
/// -1.1300000000000001 and ROUNDUP(3.535,2) 3.5399999999999996, as Excel gives them). With
/// num_digits out of range ROUNDUP adds no unit for a positive count and is #NUM! for a negative
/// one whenever a digit is dropped.
fn directed(site: &Site<'_>, number: f64, places: f64, direction: Direction) -> Step<f64> {
    if number == 0.0 {
        return Ok(0.0);
    }
    let (places, in_range) = directed_places(places);
    let position = (-places).clamp(-PLACES, PLACES);
    let reading = reading(number, true);
    if let Some(initial) = directed_initial_underflow(site, number, &reading)? {
        return Ok(initial);
    }
    let Some(down) = reading.round_at(position, Direction::TowardZero) else {
        let whole = whole_reading(site, number, &reading)?;
        // ROUNDUP gives #NUM! where ROUNDDOWN and TRUNC keep the bits past DBL_MAX (Excel
        // 16.0.20430: 102 of 102 bank rows, ROUNDUP(1.7976931348623153E+308,0) among them).
        return if whole.is_nan() && direction == Direction::AwayFromZero {
            Err(Exit::Error(WorksheetErrorCode::Num))
        } else {
            Ok(whole)
        };
    };
    let lower = stored(site, &down)?;
    let dropped = direction == Direction::AwayFromZero
        && reading
            .round_at(position, Direction::AwayFromZero)
            .is_some_and(|up| up.coefficient != down.coefficient);
    if !dropped {
        return Ok(flush(lower));
    }
    if !in_range {
        return if places > 0 {
            Ok(flush(lower))
        } else {
            Err(Exit::Error(WorksheetErrorCode::Num))
        };
    }
    if lower.is_nan() {
        // Past DBL_MAX, adding the unit is an overflow: #NUM!, as Excel gives it.
        return Err(Exit::Error(WorksheetErrorCode::Num));
    }
    // The power-of-ten unit is published from PC64 before the addition.
    // All616 exponent controls (-307..308) agree, including the double-rounding
    // discriminator at10^126. General coefficient conversion is a separate step.
    let unit = site.nearest(&ExactDecimal::unit(position).rounded_pc64())?;
    // A unit below the normal range is not added, whichever the sign, and a positive number whose
    // reading lies less than 2^-1026 above the truncation, both as binary64, stays at it (Excel
    // 16.0.20527: ROUNDUP(1.0001086461844975E-307,307) is 1E-307, while
    // ROUNDUP(-1.0001086461844975E-307,307) is -2E-307). The negative rule has no such distance.
    if unit.is_subnormal() || (number > 0.0 && site.nearest(&reading)? - lower < SMALLEST_DISTANCE)
    {
        return Ok(flush(lower));
    }
    let magnitude = site.add(lower.abs(), unit)?;
    Ok(flush(if number < 0.0 { -magnitude } else { magnitude }))
}

/// A normal input whose initial15-digit reading falls below MIN_NORMAL returns before
/// digit rounding in the directed functions. Excel20430 and20527 retain its sign and the
/// normalized significand with exponent field zero, without shifting for a subnormal.
/// Doubling the exact decimal gives a normal value; decrementing its encoded exponent
/// reproduces that observed packing. This is an output model, not an internal-code claim.
/// The other directed paths still flush final subnormal results; ROUND keeps its own policy.
fn directed_initial_underflow(
    site: &Site<'_>,
    number: f64,
    reading: &ExactDecimal,
) -> Step<Option<f64>> {
    // Only the lowest normal binade can have a15-digit reading below MIN_NORMAL.
    // Keep arithmetic-free exact calls on their existing path, including under Exact mode.
    if number.abs() >= 2.0 * f64::MIN_POSITIVE || reading.is_value_of(number) {
        return Ok(None);
    }
    if !site.nearest(reading)?.is_subnormal() {
        return Ok(None);
    }
    let mut twice = reading.clone();
    twice.coefficient.add(&reading.coefficient);
    let normal = site.nearest(&twice)?;
    debug_assert!(normal.is_normal());
    Ok(Some(f64::from_bits(normal.to_bits() - (1 << 52))))
}

/// How CEILING and FLOOR choose a multiple of the significance.
#[derive(Clone, Copy)]
enum Toward {
    /// The next multiple above the quotient: CEILING.
    Up,
    /// The next multiple below the quotient: FLOOR.
    Down,
}

/// MROUND: the multiple of `multiple` nearest the number, a half away from zero, counted from the
/// binary64 quotient. Arguments of different signs are #NUM!; a zero multiple gives 0.
fn mround(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    numeric(MROUND, arguments, services, |site, n| {
        let (number, multiple) = (n[0], n[1]);
        if multiple == 0.0 || number == 0.0 {
            Ok(0.0)
        } else if (number < 0.0) != (multiple < 0.0) {
            Err(Exit::Error(WorksheetErrorCode::Num))
        } else {
            nearest_multiple(site, number, multiple)
        }
    })
}

/// The multiple MROUND chooses for `number` and `multiple`, both non-zero and of one sign: the
/// count of multiples is the integer part of their quotient, a division at an arithmetic site,
/// plus one when the quotient's fraction is a half or within [`MROUND_HALF_TOLERANCE`] below it;
/// the result is the count times the multiple, a product at an arithmetic site (dc-j9k). So
/// MROUND(6.05,0.1) is 6, its quotient 60.49999999999999 lying 7.1E-15 below the half, while
/// MROUND(23.499999999999996,1) is 24 and MROUND(-0.04999999999999977,-0.1) is -0.1, as Excel
/// 16.0.20430 gives them (the OxFunc-Evidence bank's half-boundary and fraction-cutoff runs). A
/// quotient beyond binary64's range is #NUM!.
fn nearest_multiple(site: &Site<'_>, number: f64, multiple: f64) -> Step<f64> {
    // Positive, since the signs agree.
    let quotient = site.divide(number, multiple)?;
    if quotient.is_infinite() {
        return Err(Exit::Error(WorksheetErrorCode::Num));
    }
    let whole = decimal::floor(quotient);
    // Exact: the fraction of a binary64, and its distance below a half when it is near one.
    let below_half = 0.5 - (quotient - whole);
    let count = if below_half <= MROUND_HALF_TOLERANCE {
        whole + 1.0
    } else {
        whole
    };
    site.multiply(count, multiple)
}

/// How far below a half MROUND's quotient fraction still counts up. The recorded bank pins it
/// within one binary64 step at 0.5: for quotients below 1, fractions 0.5 - 4.996E-15 count up
/// and 0.5 - 5.0515E-15 count down. 5E-15 is the decimal in that interval, and with it every
/// recorded MROUND reading matches.
const MROUND_HALF_TOLERANCE: f64 = 5e-15;

/// CEILING: the number rounded up to a multiple of the significance; for a negative number and a
/// positive significance up is toward zero, and for both negative it is away from zero. A
/// positive number with a negative significance is #NUM!; a zero significance gives 0.
fn ceiling(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    ceiling_for(CEILING, arguments, services)
}

/// fn1 Excel 16.0.20527: this spelling uses CEILING's sign and zero rules.
fn ecma_ceiling(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    ceiling_for(ECMA_CEILING, arguments, services)
}

fn ceiling_for(
    id: FunctionId,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    numeric(id, arguments, services, |site, n| {
        let (number, significance) = (n[0], n[1]);
        if significance == 0.0 || number == 0.0 {
            Ok(0.0)
        } else if number > 0.0 && significance < 0.0 {
            Err(Exit::Error(WorksheetErrorCode::Num))
        } else {
            to_multiple(site, number, significance, Toward::Up)
        }
    })
}

/// FLOOR: the number rounded down to a multiple of the significance; for a negative number and a
/// positive significance down is away from zero, and for both negative it is toward zero. A zero
/// significance is #DIV/0! for a nonzero number; zero always gives positive zero. A positive
/// number with a negative significance gives #NUM!.
fn floor(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    numeric(FLOOR, arguments, services, |site, n| {
        let (number, significance) = (n[0], n[1]);
        if number == 0.0 {
            Ok(0.0)
        } else if significance == 0.0 {
            Err(Exit::Error(WorksheetErrorCode::Div0))
        } else if number > 0.0 && significance < 0.0 {
            Err(Exit::Error(WorksheetErrorCode::Num))
        } else {
            to_multiple(site, number, significance, Toward::Down)
        }
    })
}

/// The multiple of `significance` that `toward` chooses for `number`, both non-zero, in binary64:
/// significance * ceil(number / significance) for CEILING and significance *
/// floor(number / significance) for FLOOR, whatever the signs. The division and the product are
/// arithmetic sites, each rounding once; the integer above or below a binary64 is exact. The
/// quotient is first rounded to 15 significant digits, so one that rounds to just below or above
/// an integer counts as that integer (0.3 / 0.1 is 2.9999999999999996, counted as 3), and a
/// quotient beyond binary64's range gives an infinite product, which the number rule publishes
/// as #NUM!.
fn to_multiple(site: &Site<'_>, number: f64, significance: f64, toward: Toward) -> Step<f64> {
    let quotient = site.divide(number, significance)?;
    // Counted from the quotient rounded to 15 significant digits, as Excel counts (dc-j9k; see
    // ceiling_floor_family::excel_quotient).
    let quotient = if quotient.is_finite() && quotient != 0.0 {
        crate::ported::ceiling_floor_family::excel_quotient(number, significance)
    } else {
        quotient
    };
    let count = match toward {
        Toward::Up => decimal::ceil(quotient),
        Toward::Down => decimal::floor(quotient),
    };
    site.multiply(significance, count)
}

/// MOD: number - divisor * INT(number / divisor), with the sign of the divisor, as Excel computes
/// it (dc-j9k): the exact binary64 remainder of the number by the divisor (`%`, which rounds
/// nothing), plus the divisor once when the remainder's sign differs from the divisor's, an
/// addition at an arithmetic site. So MOD(10,0.1) is 0.09999999999999945 and MOD(-5.3,1) is
/// 0.7000000000000002, as Excel gives them (the catalogue oracle's sheet dcj9k_math: 124 of 124
/// readings). A zero divisor is #DIV/0!, and a quotient of magnitude 1.1259E+12 or more is #NUM!
/// (Excel: MOD(1125899999999,1) is 0 and MOD(1125900000000,1) is #NUM!). A remainder below the
/// normal range that needs the divisor added follows `tiny_remainder`.
fn modulo(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    numeric(MOD, arguments, services, |site, n| {
        let (number, divisor) = (n[0], n[1]);
        if divisor == 0.0 {
            return Err(Exit::Error(WorksheetErrorCode::Div0));
        }
        if number == 0.0 {
            return Ok(0.0);
        }
        let quotient = site.divide(number, divisor)?;
        if !(quotient.abs() < MOD_QUOTIENT_LIMIT) {
            return Err(Exit::Error(WorksheetErrorCode::Num));
        }
        let remainder = number % divisor;
        let result = if remainder != 0.0 && (remainder < 0.0) != (divisor < 0.0) {
            if remainder.is_subnormal()
                && let Some(result) = tiny_remainder(site, remainder, divisor, quotient)?
            {
                return Ok(result);
            }
            let adjusted = site.add(remainder, divisor)?;
            // A remainder the divisor absorbs is 0 (dc-bd5: MOD(-5,9.99999999999999E+307) is 0
            // in Excel 16.0.20430).
            if adjusted == divisor { 0.0 } else { adjusted }
        } else {
            remainder
        };
        // The recorded tiny MOD banks return #NUM! when the final remainder is subnormal.
        if result.is_subnormal() {
            Err(Exit::Error(WorksheetErrorCode::Num))
        } else {
            Ok(result)
        }
    })
}

/// A subnormal `remainder` whose sign differs from the `divisor`'s. A divisor whose Veltkamp low
/// part (the bits a split at 2^27 + 1 leaves below its high half) is itself subnormal gives #NUM!,
/// as does a quotient of 2^26 or more; the split's two parts are what a Dekker product of the
/// count and the divisor multiplies, and these are the cases where that product leaves the normal
/// range. Otherwise a remainder below 2^-1026 is added to the divisor's magnitude instead of taken
/// from it, and a larger one (`None`) takes the ordinary step. So MOD(-1.0000000000000001E-296,
/// 1E-296) is 1.0000000000000001E-296 and the same at 1E-300 is #NUM!, as Excel 16.0.20527 gives
/// them. This rule matches all 152,395 bank rows with normal operands and 1,128 new readings.
fn tiny_remainder(
    site: &Site<'_>,
    remainder: f64,
    divisor: f64,
    quotient: f64,
) -> Step<Option<f64>> {
    let magnitude = divisor.abs();
    let scaled = VELTKAMP_SPLIT * magnitude;
    let low = magnitude - (scaled - (scaled - magnitude));
    if low.is_subnormal() || quotient.abs() >= MOD_DEKKER_COUNT {
        return Err(Exit::Error(WorksheetErrorCode::Num));
    }
    if remainder.abs() >= SMALLEST_DISTANCE {
        return Ok(None);
    }
    let sum = site.add(magnitude, remainder.abs())?;
    Ok(Some(if divisor < 0.0 { -sum } else { sum }))
}

/// 2^27 + 1, Veltkamp's multiplier for splitting a binary64 into two 26-bit halves.
const VELTKAMP_SPLIT: f64 = 134_217_729.0;

/// 2^26: from here a subnormal remainder gives #NUM!.
const MOD_DEKKER_COUNT: f64 = 67_108_864.0;

/// 2^-1026: ROUNDUP's smallest distance above the truncation, and MOD's smallest subnormal
/// remainder taken from the divisor rather than added to it.
const SMALLEST_DISTANCE: f64 = f64::MIN_POSITIVE / 16.0;

/// The magnitude of number/divisor at and above which Excel's MOD is #NUM!, measured by bisection
/// (dc-j9k).
const MOD_QUOTIENT_LIMIT: f64 = 1.1259e12;

/// QUOTIENT: the integer part of the binary64 quotient numerator / denominator, toward zero. A
/// zero denominator is #DIV/0!.
fn quotient(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    numeric(QUOTIENT, arguments, services, |site, n| {
        let (numerator, denominator) = (n[0], n[1]);
        if denominator == 0.0 {
            return Err(Exit::Error(WorksheetErrorCode::Div0));
        }
        if numerator == 0.0 {
            return Ok(0.0);
        }
        // The binary64 quotient, toward zero (dc-bd5: QUOTIENT(0.3,0.1) is 2 in Excel
        // 16.0.20430, the quotient being 2.9999999999999996).
        let quotient = site.divide(numerator, denominator)?;
        Ok(decimal::truncate(quotient))
    })
}

/// EVEN: rounded away from zero to an even integer; an even integer is unchanged.
fn even(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    numeric(EVEN, arguments, services, |site, n| {
        to_parity(site, n[0], false)
    })
}

/// ODD: rounded away from zero to an odd integer; an odd integer is unchanged, and 0 gives 1.
fn odd(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    numeric(ODD, arguments, services, |site, n| {
        to_parity(site, n[0], true)
    })
}

/// The integer of the wanted parity at or beyond |number|, with the number's sign. The integer
/// above |number| is exact; stepping on to the next integer is an addition at an arithmetic site,
/// which from 2^53 on, where every binary64 is even, rounds the odd integer to a binary64.
fn to_parity(site: &Site<'_>, number: f64, odd: bool) -> Step<f64> {
    if number == 0.0 {
        return Ok(if odd { 1.0 } else { 0.0 });
    }
    let magnitude = decimal::ceil(number.abs());
    // Halving an integer of at least 1 is exact.
    let half = magnitude * 0.5;
    let is_even = decimal::truncate(half) == half;
    let rounded = if is_even == odd {
        site.add(magnitude, 1.0)?
    } else {
        magnitude
    };
    Ok(if number < 0.0 { -rounded } else { rounded })
}
