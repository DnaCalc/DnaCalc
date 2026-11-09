//! The arithmetic family fragment: the arithmetic operators and their kernels.
//!
//! Each operand is a scalar value (the caller dereferences a reference first) read through
//! worksheet number coercion, the left operand before the right, so the left operand's error or
//! coercion error is the result when both have one. The result follows the number publication
//! rule: an overflow is #NUM! and a zero is +0.
//!
//! A product or quotient below the normal range is +0 (dc-b1l, measured in Excel 16.0.20430 on
//! 2026-10-25: `=2.2250738585073E-308/2`, `=POWER(2,-1022)*0.5`, `=POWER(2,-1000)*POWER(2,-52)`,
//! `=-POWER(2,-1022)/2` and `=(POWER(2,-1022)*1.5)/2` are all 0, and
//! `=POWER(2,-1022)-POWER(2,-1022)*0.75` is 2^-1022, its product having been 0). A sum or
//! difference below it is not: `=POWER(2,-1022)*(1+POWER(2,-52))-POWER(2,-1022)` is 2^-1074 in
//! Excel. The computed-subnormal probes in dc-1943 confirm that + and - retain such values,
//! while * and / flush a subnormal result, and ^ flushes the probed subnormal results.

use super::FunctionId;
use crate::call::{EvalServices, FunctionFault};
use crate::coerce;
use crate::kernel::{Exit, Site, Step};
use crate::value::{CalcValue, CoreRef, WorksheetErrorCode};

use calc_catalogue::operators::arithmetic::{
    CODE, DIVIDE, ENTRIES as DECLARATIONS, MINUS, NEGATE, PERCENT, PLUS, POWER, TIMES, UNARY_PLUS,
};

attach_eager_family! {
    code: CODE; declarations: DECLARATIONS;
    PLUS => plus;
    MINUS => minus;
    TIMES => times;
    DIVIDE => divide;
    POWER => power;
    NEGATE => negate;
    UNARY_PLUS => unary_plus;
    PERCENT => percent;
}

fn plus(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    binary(PLUS, arguments, services, Site::add)
}

fn minus(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    binary(MINUS, arguments, services, Site::subtract)
}

fn times(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    binary(TIMES, arguments, services, |site, a, b| {
        Ok(flushed(site.multiply(a, b)?))
    })
}

fn divide(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    binary(DIVIDE, arguments, services, |site, a, b| {
        Ok(flushed(site.divide(a, b)?))
    })
}

/// A product or quotient as Excel publishes it: +0 below the normal range (see the module
/// documentation).
fn flushed(x: f64) -> f64 {
    if x.is_subnormal() { 0.0 } else { x }
}

fn power(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(POWER, services);
    Site::finish(operands(&site, arguments).and_then(|(x, y)| {
        if x.is_subnormal() {
            site.number(flushed(raise(&site, x, y)?))
        } else {
            power_value(&site, x, y)
        }
    }))
}

/// POWER's kernel distinguishes flushed integer/reciprocal results from retained fractional
/// subnormals. The operator also retains the recorded fractional results from normal inputs.
pub(super) fn power_value(site: &Site<'_>, x: f64, y: f64) -> Step<CalcValue> {
    super::kit::out_with_subnormal(site, Ok(raise(site, x, y)?))
}

fn binary<'m>(
    id: FunctionId,
    arguments: &[CalcValue],
    services: &EvalServices<'m>,
    operation: fn(&Site<'m>, f64, f64) -> Step<f64>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(id, services);
    Site::finish(operands(&site, arguments).and_then(|(a, b)| {
        let result = operation(&site, a, b)?;
        if id == PLUS || id == MINUS {
            super::kit::out_with_subnormal(&site, Ok(result))
        } else {
            site.number(result)
        }
    }))
}

fn operands(site: &Site<'_>, arguments: &[CalcValue]) -> Step<(f64, f64)> {
    site.scalar_operands(arguments)?;
    let a = super::kit::num_with_subnormal(site, arguments, 0)?;
    let b = super::kit::num_with_subnormal(site, arguments, 1)?;
    Ok((a, b))
}

/// `x ^ y`. The cases decided without a power algorithm keep their measured rule: 0 to a
/// negative power is #DIV/0! and 0 to the power 0 is #NUM!; the exponents 0, 1 and 2 and the
/// bases 0 and 1 give their exact or singly rounded result. The negative unit base retains the
/// kernel's exponent admission, with exact unit results. Every other power, a reciprocal or a
/// negative base with a fractional exponent included, is OxFunc's Excel power kernel
/// (ported/power_fn.rs, OxFunc 6fca3518): binary exponentiation with x87-staged products for an
/// integer exponent, a decimal scale for a base of 10, `exp(y ln x)` on the x87 backend for a
/// fractional one, and Excel's real-root rule for a negative base (accepted when `1/y`, read to
/// 15 significant digits, is an odd integer; #NUM! otherwise). That kernel is an arithmetic site.
pub(super) fn raise(site: &Site<'_>, x: f64, y: f64) -> Step<f64> {
    if x == 0.0 {
        return if y == 0.0 {
            Err(Exit::Error(WorksheetErrorCode::Num))
        } else if y < 0.0 {
            Err(Exit::Error(WorksheetErrorCode::Div0))
        } else {
            Ok(0.0)
        };
    }
    if x < 0.0 && y.fract() != 0.0 {
        return power_algorithm(site, x, y);
    }
    if y == 0.0 || x == 1.0 {
        Ok(1.0)
    } else if y == 1.0 {
        Ok(x)
    } else if x == -1.0 {
        // Unit results are exact, but the kernel still owns integer-width admission.
        crate::ported::power_fn::power_kernel(x, y).map_err(Exit::Error)
    } else if y == 2.0 {
        // A square below the normal range is +0, as Excel gives it (1E-160^2 in the catalogue
        // oracle's sheet f1_power), and as the power kernel flushes it.
        let square = site.multiply(x, x)?;
        Ok(if square.is_subnormal() { 0.0 } else { square })
    } else {
        power_algorithm(site, x, y)
    }
}

/// The powers that need a power algorithm: OxFunc's Excel power kernel, at one arithmetic site.
fn power_algorithm(site: &Site<'_>, x: f64, y: f64) -> Step<f64> {
    site.binary64()?;
    crate::ported::power_fn::power_kernel(x, y).map_err(Exit::Error)
}

/// `-x`: the sign changes exactly, so no rounding takes place.
fn negate(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    // Excel keeps the entire stored overflow encoding, including its sign bit.
    if arguments[0].is_excel_overflow() {
        return Ok(arguments[0].clone());
    }
    let site = Site::new(NEGATE, services);
    Site::finish(
        site.scalar_operands(arguments)
            .and_then(|()| site.number_argument(0, &arguments[0]))
            .and_then(|x| site.number(-x)),
    )
}

/// `+x`: Excel's unary plus leaves its operand's value as it is, including a computed subnormal;
/// a zero is +0 and a blank operand is 0.
fn unary_plus(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(UNARY_PLUS, services);
    Site::finish(site.scalar_operands(arguments).and_then(|()| {
        let operand = &arguments[0];
        if operand.is_excel_overflow() {
            return Ok(operand.clone());
        }
        match operand.core_ref() {
            CoreRef::Number(n) if n.is_subnormal() => Ok(CalcValue::number(n)),
            CoreRef::Number(n) => site.number(site.argument(0, coerce::operand(n))?),
            CoreRef::Empty | CoreRef::Missing => site.number(0.0),
            CoreRef::Text(_) | CoreRef::Logical(_) => Ok(operand.clone()),
            CoreRef::Error(code) => Err(Exit::Error(code)),
            CoreRef::Array(_) | CoreRef::Reference(_) => Err(site.not_modelled(Some(0))),
        }
    }))
}

/// `x%` is `x / 100`.
fn percent(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(PERCENT, services);
    Site::finish(
        site.scalar_operands(arguments)
            .and_then(|()| site.number_argument(0, &arguments[0]))
            .and_then(|x| site.divide(x, 100.0))
            .and_then(|x| site.number(flushed(x))),
    )
}
