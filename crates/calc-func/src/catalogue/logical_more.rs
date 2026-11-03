//! The logical functions F2 adds beside the logical family, ported from OxFunc (functions/ at
//! 6fca3518: choose_ifs_family.rs's IFS and switch_fn.rs's SWITCH).
//!
//! Both are eager here: every argument is evaluated before the call, and the kernel selects.
//!
//! IFS reads each condition as IF does (worksheet logical coercion, an error is the result) and
//! gives the value of the first TRUE condition's pair; no TRUE condition is #N/A. SWITCH compares
//! its expression with each case as the `=` operator does ([`super::comparison::order`]); an
//! error in the expression is the result, the first matching case's value is the result, then the
//! default, and #N/A when there is none. An omitted selected value is 0, as IF's is.

use super::comparison::{Want, order};
use super::kit::{fail, flag};
use super::{ArgumentDemand, CatalogueEntry, Family, FunctionId, Kernel, Notation, Parameter};
use crate::call::{EvalServices, FunctionFault};
use crate::kernel::{Site, Step};
use crate::value::{CalcValue, CallableArityShape, CoreRef, WorksheetErrorCode};
use std::cmp::Ordering;

const CODE: u16 = 201;
const IFS: FunctionId = FunctionId::new(CODE, 1);
const SWITCH: FunctionId = FunctionId::new(CODE, 2);

/// Recorded successor cases distinguish valid scalar selectors from invalid IFS conditions:
/// only valid selectors ignore unused result shapes. Use the kernel's own logical coercion.
pub(super) fn selects_whole_result(id: FunctionId, arguments: &[CalcValue]) -> bool {
    match id {
        IFS => arguments
            .iter()
            .step_by(2)
            .all(|value| value.callable_value().is_none() && crate::coerce::logical(value).is_ok()),
        SWITCH => {
            let end = arguments.len() - usize::from(arguments.len().is_multiple_of(2));
            std::iter::once(&arguments[0])
                .chain(arguments[1..end].iter().step_by(2))
                .all(|value| {
                    value.callable_value().is_none()
                        && !matches!(
                            value.core_ref(),
                            CoreRef::Array(_) | CoreRef::Reference(_) | CoreRef::Error(_)
                        )
                })
        }
        _ => false,
    }
}

pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[
        CatalogueEntry {
            id: IFS,
            spelling: "IFS",
            notation: Notation::Call,
            arity: CallableArityShape::range(2, 254),
            parameters: &[
                Parameter::value("logical_test1"),
                Parameter::value("value_if_true1"),
            ],
            demand: ArgumentDemand::Eager,
            kernel: Kernel::Eager(ifs),
        },
        CatalogueEntry {
            id: SWITCH,
            spelling: "SWITCH",
            notation: Notation::Call,
            arity: CallableArityShape::range(3, 254),
            parameters: &[
                Parameter::value("expression"),
                Parameter::value("value1"),
                Parameter::value("result1"),
            ],
            demand: ArgumentDemand::Eager,
            kernel: Kernel::Eager(switch),
        },
    ],
};

fn selected(value: &CalcValue) -> CalcValue {
    if value.is_missing() {
        CalcValue::number(0.0)
    } else {
        value.clone()
    }
}

fn contains_callable(value: &CalcValue) -> bool {
    value.callable_value().is_some()
        || value
            .as_array()
            .is_some_and(|array| array.iter_row_major().any(contains_callable))
}

/// A selected result may be an array. The caller owns lifting array conditions/cases;
/// scalar selectors return their chosen result whole, without inspecting unused results.
fn selection(
    id: FunctionId,
    arguments: &[CalcValue],
    services: &EvalServices<'_>,
    select: fn(&Site<'_>, &[CalcValue]) -> Step<CalcValue>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(id, services);
    Site::finish(site.binary64().and_then(|()| {
        let value = select(&site, arguments)?;
        if contains_callable(&value) {
            Err(site.not_modelled(None))
        } else {
            Ok(value)
        }
    }))
}

fn ifs(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    selection(IFS, arguments, services, |s, a| {
        if !a.len().is_multiple_of(2) {
            return fail(WorksheetErrorCode::Value);
        }
        for pair in (0..a.len()).step_by(2) {
            if flag(s, a, pair)? {
                return Ok(selected(&a[pair + 1]));
            }
        }
        fail(WorksheetErrorCode::NA)
    })
}

fn switch(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    selection(SWITCH, arguments, services, |s, a| {
        if let CoreRef::Error(code) = a[0].core_ref() {
            return fail(code);
        }
        let has_default = a.len() % 2 == 0;
        let cases = if has_default { a.len() - 1 } else { a.len() };
        let mut at = 1;
        while at + 1 < cases {
            let pair = [a[0].clone(), a[at].clone()];
            if order(s, &pair, Want::Equality)? == Ordering::Equal {
                return Ok(selected(&a[at + 1]));
            }
            at += 2;
        }
        if has_default {
            return Ok(selected(&a[a.len() - 1]));
        }
        fail(WorksheetErrorCode::NA)
    })
}
