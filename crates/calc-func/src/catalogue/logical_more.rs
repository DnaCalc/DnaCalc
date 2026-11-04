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
use crate::value::{CalcValue, CallableArityShape, CoreRef, NumericMode, WorksheetErrorCode};
use std::cmp::Ordering;

const CODE: u16 = 201;
const IFS: FunctionId = FunctionId::new(CODE, 1);
const SWITCH: FunctionId = FunctionId::new(CODE, 2);

/// Recorded successor cases distinguish valid scalar selectors from invalid IFS conditions:
/// only valid selectors ignore unused result shapes. Use the kernel's own logical coercion.
pub(super) fn selects_whole_result(
    id: FunctionId,
    arguments: &[CalcValue],
    numeric: &NumericMode,
) -> bool {
    match id {
        // Malformed pairs have no new capture; preserve their existing admission.
        IFS if !arguments.len().is_multiple_of(2) => arguments
            .iter()
            .step_by(2)
            .all(|value| value.callable_value().is_none() && crate::coerce::logical(value).is_ok()),
        // Fn2 root208: once a scalar condition selects a result, later condition
        // arrays do not enlarge it. Invalid conditions still take the lifted route.
        IFS => ifs_index(&Site::new(IFS, &EvalServices::new(numeric)), arguments).is_ok(),
        SWITCH => {
            // Fn2 root022: a formed scalar expression error is one result, even
            // when an unused case result is an array. Callable cores stay distinct.
            if arguments[0].callable_value().is_none()
                && matches!(arguments[0].core_ref(), CoreRef::Error(_))
            {
                return true;
            }
            let end = arguments.len() - usize::from(arguments.len().is_multiple_of(2));
            let scalar = std::iter::once(&arguments[0])
                .chain(arguments[1..end].iter().step_by(2))
                .all(|value| {
                    value.callable_value().is_none()
                        && !matches!(
                            value.core_ref(),
                            CoreRef::Array(_) | CoreRef::Reference(_) | CoreRef::Error(_)
                        )
                });
            // A matching scalar prefix never reads a later array case. Reuse SWITCH's
            // selector and comparison rules without invoking a kernel or evaluating args.
            scalar
                || switch_index(&Site::new(SWITCH, &EvalServices::new(numeric)), arguments).is_ok()
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

/// A selected result may be an array. The caller owns lifting array conditions/cases;
/// scalar selectors return their chosen result whole, without inspecting unused results.
/// A callable keeps its identity here; worksheet publication owns its visible core.
fn selection(
    id: FunctionId,
    arguments: &[CalcValue],
    services: &EvalServices<'_>,
    select: fn(&Site<'_>, &[CalcValue]) -> Step<CalcValue>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(id, services);
    Site::finish(site.binary64().and_then(|()| select(&site, arguments)))
}

fn ifs(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    selection(IFS, arguments, services, |s, a| {
        if !a.len().is_multiple_of(2) {
            return fail(WorksheetErrorCode::Value);
        }
        match ifs_index(s, a)? {
            Some(index) => Ok(selected(&a[index])),
            None => fail(WorksheetErrorCode::NA),
        }
    })
}

/// The first true IFS condition. Shape admission and selection share logical coercion;
/// the kernel validates pair structure before using the selected result index.
fn ifs_index(site: &Site<'_>, arguments: &[CalcValue]) -> Step<Option<usize>> {
    for pair in (0..arguments.len()).step_by(2) {
        if flag(site, arguments, pair)? {
            return Ok(Some(pair + 1));
        }
    }
    Ok(None)
}

fn switch(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    selection(SWITCH, arguments, services, |s, a| {
        match switch_index(s, a)? {
            Some(index) => Ok(selected(&a[index])),
            None => fail(WorksheetErrorCode::NA),
        }
    })
}

/// The selected SWITCH argument, or no match without a default. Both shape admission
/// and the kernel use this one selector; comparisons read no host or random services.
fn switch_index(site: &Site<'_>, arguments: &[CalcValue]) -> Step<Option<usize>> {
    if let CoreRef::Error(code) = arguments[0].core_ref() {
        return fail(code);
    }
    let has_default = arguments.len().is_multiple_of(2);
    let cases = arguments.len() - usize::from(has_default);
    let mut at = 1;
    while at + 1 < cases {
        let pair = [arguments[0].clone(), arguments[at].clone()];
        if order(site, &pair, Want::Equality)? == Ordering::Equal {
            return Ok(Some(at + 1));
        }
        at += 2;
    }
    Ok(has_default.then_some(arguments.len() - 1))
}
