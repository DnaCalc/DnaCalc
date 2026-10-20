//! The arithmetic family fragment: the arithmetic operators and their kernels.
//!
//! Each operand is a scalar value (the caller dereferences a reference first) read through
//! worksheet number coercion, the left operand before the right, so the left operand's error or
//! coercion error is the result when both have one. The result follows the number publication
//! rule: an overflow is #NUM! and a zero is +0.

use super::{CatalogueEntry, Family, FunctionId, Notation, Parameter};
use crate::call::{EvalServices, FunctionFault};
use crate::coerce;
use crate::kernel::{Exit, Site, Step};
use crate::value::{CalcValue, CoreValue, WorksheetErrorCode};

const CODE: u16 = 1;

const PLUS: FunctionId = FunctionId::new(CODE, 1);
const MINUS: FunctionId = FunctionId::new(CODE, 2);
const TIMES: FunctionId = FunctionId::new(CODE, 3);
const DIVIDE: FunctionId = FunctionId::new(CODE, 4);
const POWER: FunctionId = FunctionId::new(CODE, 5);
const NEGATE: FunctionId = FunctionId::new(CODE, 6);
const UNARY_PLUS: FunctionId = FunctionId::new(CODE, 7);
const PERCENT: FunctionId = FunctionId::new(CODE, 8);

const NUMBERS: &[Parameter] = &[Parameter::value("number1"), Parameter::value("number2")];
const NUMBER: &[Parameter] = &[Parameter::value("number")];

pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[
        CatalogueEntry::operator(PLUS, "+", Notation::Infix, NUMBERS, plus),
        CatalogueEntry::operator(MINUS, "-", Notation::Infix, NUMBERS, minus),
        CatalogueEntry::operator(TIMES, "*", Notation::Infix, NUMBERS, times),
        CatalogueEntry::operator(DIVIDE, "/", Notation::Infix, NUMBERS, divide),
        CatalogueEntry::operator(POWER, "^", Notation::Infix, NUMBERS, power),
        CatalogueEntry::operator(NEGATE, "-", Notation::Prefix, NUMBER, negate),
        CatalogueEntry::operator(UNARY_PLUS, "+", Notation::Prefix, NUMBER, unary_plus),
        CatalogueEntry::operator(PERCENT, "%", Notation::Postfix, NUMBER, percent),
    ],
};

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
    binary(TIMES, arguments, services, Site::multiply)
}

fn divide(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    binary(DIVIDE, arguments, services, Site::divide)
}

fn power(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    binary(POWER, arguments, services, raise)
}

fn binary<'m>(
    id: FunctionId,
    arguments: &[CalcValue],
    services: &EvalServices<'m>,
    operation: fn(&Site<'m>, f64, f64) -> Step<f64>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(id, services);
    Site::finish(operands(&site, arguments).and_then(|(a, b)| site.number(operation(&site, a, b)?)))
}

fn operands(site: &Site<'_>, arguments: &[CalcValue]) -> Step<(f64, f64)> {
    site.scalar_operands(arguments)?;
    let a = site.number_argument(0, &arguments[0])?;
    let b = site.number_argument(1, &arguments[1])?;
    Ok((a, b))
}

/// `x ^ y` where its value is decided without a power algorithm: the domain errors (0 to a
/// negative power is #DIV/0!; 0 to the power 0 and a negative base to a non-integer power are
/// #NUM!) and the exponents whose result is exact or a single rounding (0, 1, 2 and -1, a base of
/// 0, 1 or -1). Every other power needs a pinned power implementation, which is not modelled.
fn raise(site: &Site<'_>, x: f64, y: f64) -> Step<f64> {
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
        return Err(Exit::Error(WorksheetErrorCode::Num));
    }
    if y == 0.0 || x == 1.0 {
        Ok(1.0)
    } else if y == 1.0 {
        Ok(x)
    } else if x == -1.0 {
        Ok(if y % 2.0 == 0.0 { 1.0 } else { -1.0 })
    } else if y == 2.0 {
        site.multiply(x, x)
    } else if y == -1.0 {
        site.divide(1.0, x)
    } else {
        Err(site.not_modelled(None))
    }
}

/// `-x`: the sign changes exactly, so no rounding takes place.
fn negate(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(NEGATE, services);
    Site::finish(
        site.scalar_operands(arguments)
            .and_then(|()| site.number_argument(0, &arguments[0]))
            .and_then(|x| site.number(-x)),
    )
}

/// `+x`: Excel's unary plus leaves its operand's value as it is, so text and a logical are the
/// result unchanged and an error is the result. A number is published by the number rule (a zero
/// is +0), and a blank operand is 0.
fn unary_plus(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(UNARY_PLUS, services);
    Site::finish(site.scalar_operands(arguments).and_then(|()| {
        let operand = &arguments[0];
        match operand.core() {
            CoreValue::Number(n) => site.number(site.argument(0, coerce::operand(*n))?),
            CoreValue::Empty | CoreValue::Missing => site.number(0.0),
            CoreValue::Text(_) | CoreValue::Logical(_) => Ok(operand.clone()),
            CoreValue::Error(code) => Err(Exit::Error(*code)),
            CoreValue::Array(_) | CoreValue::Reference(_) => Err(site.not_modelled(Some(0))),
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
            .and_then(|x| site.number(x)),
    )
}
