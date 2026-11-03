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
//! each number rounded to 15 significant digits ([`ExactDecimal::fifteen`]), compute on that
//! decimal exactly, and convert the exact result once to the nearest binary64 at an arithmetic
//! site. So ROUND(2.675,2) is 2.68, as the typed 2.675 reads, although the binary64 of 2.675 lies
//! below the half, and TRUNC(2.9999999999999996) is 3 (dc-bd5). MOD and QUOTIENT divide in
//! binary64 (QUOTIENT(0.3,0.1) is 2).
//!
//! CEILING and FLOOR compute in binary64: the quotient number / significance, rounded at an
//! arithmetic site, is taken to the integer above or below it, and the significance times that
//! integer is rounded at another. So CEILING(0.3,0.1) is 0.30000000000000004 and FLOOR(0.3,0.1)
//! is 0.2, the binary64 quotient being 2.9999999999999996.
//!
//! MROUND counts its multiples from the binary64 quotient number / multiple, rounded to the
//! nearest integer, a half away from zero; the count times the multiple's exact decimal reading is
//! computed exactly and converted once. Its page's Known Limitations example gives MROUND(6.05,0.1)
//! as 6.0 and MROUND(7.05,0.1) as 7.1. Counting from the binary64 quotients, 60.49999999999999 and
//! 70.5, agrees with both outcomes; counting on the decimal readings gives the half 60.5 and so
//! 6.1 for the first.
//!
//! No page settles which of these rules Excel's division follows for each function; the owner's
//! Excel capture of the oracle rows that tell them apart (CEILING(0.3,0.1), FLOOR(0.3,0.1),
//! MOD(0.3,0.1) and QUOTIENT(0.3,0.1) among them) decides.
//!
//! Nothing passes through an integer type that could overflow: inputs near 2^53 or at the largest
//! binary64 are read and computed exactly, or, in CEILING and FLOOR, as binary64.
//!
//! A result follows the number publication rule: one beyond binary64's range is #NUM!, a zero is
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
        arity: CallableArityShape::range(min, parameters.len()),
        parameters,
        demand: ArgumentDemand::Eager,
        kernel: Kernel::Eager(kernel),
    }
}

pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[
        call(ABS, "ABS", 1, NUMBER, abs),
        call(SIGN, "SIGN", 1, NUMBER, sign),
        call(INT, "INT", 1, NUMBER, int),
        call(TRUNC, "TRUNC", 1, DIGITS, trunc),
        call(ROUND, "ROUND", 2, DIGITS, round),
        call(ROUNDUP, "ROUNDUP", 2, DIGITS, roundup),
        call(ROUNDDOWN, "ROUNDDOWN", 2, DIGITS, rounddown),
        call(MROUND, "MROUND", 2, MULTIPLE, mround),
        call(CEILING, "CEILING", 2, SIGNIFICANCE, ceiling),
        call(FLOOR, "FLOOR", 2, SIGNIFICANCE, floor),
        call(MOD, "MOD", 2, DIVISOR, modulo),
        call(QUOTIENT, "QUOTIENT", 2, FRACTION, quotient),
        call(EVEN, "EVEN", 1, NUMBER, even),
        call(ODD, "ODD", 1, NUMBER, odd),
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
    let flush_input = matches!(id, TRUNC | ROUND | ROUNDUP | ROUNDDOWN | MOD);
    Site::finish(numbers(&site, arguments, flush_input).and_then(|read| site.number(compute(&site, &read)?)))
}

/// The arguments as numbers: an array argument is refused first, then each argument is read in
/// order, so the first one's error is the result.
fn numbers(site: &Site<'_>, arguments: &[CalcValue], flush_input: bool) -> Step<Vec<f64>> {
    site.scalar_operands(arguments)?;
    arguments
        .iter()
        .enumerate()
        .map(|(index, argument)| {
            // W111 typed MROUND/QUOTIENT readings: logicals are #VALUE!, an explicit
            // omission #N/A. A referenced blank remains Empty and coerces to zero.
            if matches!(site.id(), MROUND | QUOTIENT) {
                match argument.core_ref() {
                    CoreRef::Logical(_) => return Err(Exit::Error(WorksheetErrorCode::Value)),
                    CoreRef::Missing => return Err(Exit::Error(WorksheetErrorCode::NA)),
                    _ => {}
                }
            }
            if flush_input && argument.as_number().is_some_and(f64::is_subnormal) {
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
        to_digits(
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
        to_digits(site, n[0], n[1], Direction::Nearest)
    })
}

/// ROUNDUP: as ROUND, away from zero whenever a dropped digit is not zero.
fn roundup(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    numeric(ROUNDUP, arguments, services, |site, n| {
        to_digits(site, n[0], n[1], Direction::AwayFromZero)
    })
}

/// ROUNDDOWN: as ROUND, toward zero.
fn rounddown(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    numeric(ROUNDDOWN, arguments, services, |site, n| {
        to_digits(site, n[0], n[1], Direction::TowardZero)
    })
}

/// Beyond this many places either way no number's reading has a digit (a reading's digits lie
/// between 10^-1074 and 10^308), so clamping num_digits here changes no result.
const PLACES: f64 = 2000.0;

/// `number`, read as an exact decimal, rounded in `direction` to `places` decimal places, of
/// which only the integer part counts. A number with no digit beyond that position is the result
/// unchanged, with no rounding; otherwise the rounded decimal is converted at an arithmetic site.
fn to_digits(site: &Site<'_>, number: f64, places: f64, direction: Direction) -> Step<f64> {
    if number == 0.0 {
        return Ok(0.0);
    }
    // ROUND with num_digits of 2147483647 or more is 0, where 2147483646 leaves 1.5 as it is;
    // ROUNDUP, ROUNDDOWN and TRUNC leave it (dc-bd5: Excel 16.0.20527, ROUND(1.5,1E+300) and
    // ROUND(1.5,2147483647) are 0, ROUNDUP(1.5,1E+300) and TRUNC(1.5,1E+300) are 1.5).
    if direction == Direction::Nearest && places >= 2_147_483_647.0 {
        return Ok(0.0);
    }
    // An integer within +/-2000 after the clamp, so the conversion to i32 is exact.
    let places = decimal::truncate(places).clamp(-PLACES, PLACES) as i32;
    let (reading, changed) = ExactDecimal::fifteen(number);
    let result = match reading.round_at(-places, direction) {
        None if changed => site.nearest(&reading)?,
        None => return Ok(number),
        Some(rounded) => site.nearest(&rounded)?,
    };
    // A result below the normal range is 0, as Excel gives it (dc-bd5:
    // ROUNDDOWN(2.5E-308,308) is 0 in Excel 16.0.20430).
    Ok(if result.is_subnormal() { 0.0 } else { result })
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
/// count of multiples is their binary64 quotient, a division at an arithmetic site, rounded to the
/// nearest integer, a half away from zero; the result is the count times the multiple, a binary64
/// product (dc-j9k). So MROUND(6.05,0.1) is 6, its quotient being 60.49999999999999, and
/// MROUND(1.3,0.2) is 1.4000000000000001 and MROUND(7.05,0.1) 7.1000000000000005, as Excel gives
/// them (the catalogue oracle's sheet dcj9k_math: 125 of 125 readings). A quotient beyond
/// binary64's range is #NUM!.
fn nearest_multiple(site: &Site<'_>, number: f64, multiple: f64) -> Step<f64> {
    // Positive, since the signs agree.
    let quotient = site.divide(number, multiple)?;
    if quotient.is_infinite() {
        return Err(Exit::Error(WorksheetErrorCode::Num));
    }
    let count = (quotient + 0.5).floor();
    let count = if count - quotient > 0.5 { count - 1.0 } else { count };
    site.multiply(count, multiple)
}

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
/// (Excel: MOD(1125899999999,1) is 0 and MOD(1125900000000,1) is #NUM!).
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
        // The recorded banks distinguish binary-power and general divisors
        // when reduction produces a subnormal intermediate.
        let binary_divisor = divisor.abs().to_bits() & 0x000f_ffff_ffff_ffff == 0;
        if remainder.is_subnormal() && !binary_divisor {
            return Err(Exit::Error(WorksheetErrorCode::Num));
        }
        let result = if remainder != 0.0 && (remainder < 0.0) != (divisor < 0.0) {
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
