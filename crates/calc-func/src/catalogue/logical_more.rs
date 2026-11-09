//! The logical functions F2 adds beside the logical family, ported from OxFunc (functions/ at
//! 6fca3518: choose_ifs_family.rs's IFS and switch_fn.rs's SWITCH).
//!
//! Both are eager here: every argument is evaluated before the call, and the kernel selects.
//!
//! IFS reads each condition as IF does (worksheet logical coercion, an error is the result) and
//! gives the value of the first TRUE condition's pair; no TRUE condition is #N/A. SWITCH compares
//! its expression with each case as the `=` operator does ([`super::comparison::order`]), except
//! that a number whose fifteen-digit reading overflows matches every number (measured for SWITCH,
//! `reading_overflow_pair`); an error in the expression is the result, the first matching case's
//! value is the result, then the default, and #N/A when there is none. An omitted selected value
//! is 0, as IF's is.

use super::comparison::{Want, order};
use super::kit::{fail, flag};
use super::FunctionId;
use crate::call::{EvalServices, FunctionFault};
use crate::kernel::{Exit, Site, Step};
use crate::value::{CalcValue, CoreRef, NumericMode, WorksheetErrorCode};
use std::cmp::Ordering;

use calc_catalogue::logical_more::{CODE, ENTRIES as DECLARATIONS, IFS, SWITCH};

/// Recorded successor cases distinguish valid scalar selectors from invalid IFS conditions:
/// only valid selectors ignore unused result shapes. Use the kernel's own logical coercion.
pub(super) fn selects_whole_result(
    policy: super::WholeResultPolicy,
    arguments: &[CalcValue],
    numeric: &NumericMode,
) -> bool {
    match policy {
        // Malformed pairs have no new capture; preserve their existing admission.
        super::WholeResultPolicy::Conditions if !arguments.len().is_multiple_of(2) => arguments
            .iter()
            .step_by(2)
            .all(|value| value.callable_value().is_none() && crate::coerce::logical(value).is_ok()),
        // Fn2 root208: once a scalar condition selects a result, later condition
        // arrays do not enlarge it. Invalid conditions still take the lifted route.
        super::WholeResultPolicy::Conditions => ifs_index(&Site::new(IFS, &EvalServices::new(numeric)), arguments).is_ok(),
        super::WholeResultPolicy::Selector => {
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
            // Fn5 selection012: a scalar case error also decides one whole
            // result before an unused array result can enlarge it.
            scalar
                || match switch_index(&Site::new(SWITCH, &EvalServices::new(numeric)), arguments) {
                    Ok(_) => true,
                    Err(Exit::Error(_)) => std::iter::once(&arguments[0])
                        .chain(arguments[1..end].iter().step_by(2))
                        .all(|value| value.callable_value().is_none()),
                    Err(_) => false,
                }
        }
        _ => false,
    }
}

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
attach_eager_family! {
    code: CODE; declarations: DECLARATIONS;
    IFS => ifs;
    SWITCH => switch;
}

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
        if reading_overflow_pair(&arguments[0], &arguments[at]) {
            return Ok(Some(at + 1));
        }
        let pair = [arguments[0].clone(), arguments[at].clone()];
        if order(site, &pair, Want::Equality)? == Ordering::Equal {
            return Ok(Some(at + 1));
        }
        at += 2;
    }
    Ok(has_default.then_some(arguments.len() - 1))
}

/// The smallest magnitude whose fifteen-digit reading, 1.79769313486232E+308, lies past
/// binary64's range: DBL_MAX less three units (ROUND(x,-293) publishes the overflow pattern
/// 0x7ff000000000000a from here up, and 0x7fefffffffffffe2 at DBL_MAX less four units).
const READING_OVERFLOW: f64 = f64::from_bits(0x7fef_ffff_ffff_fffc);

/// Excel 16.0.20527's SWITCH finds a number at or past [`READING_OVERFLOW`] in magnitude equal
/// to every number and to a blank, either sign and either side: SWITCH(x, y, 1, 0) is 1 for x
/// DBL_MAX, DBL_MAX less one, two or three units and every y tried (1, -1, 0.5, 2, 1E-300,
/// 1E+300, -DBL_MAX, a blank), and 0 at DBL_MAX less four units, 1.7E+308 and 1E+308; text and a
/// logical case do not match. Only SWITCH is measured; the = operator is not changed here.
fn reading_overflow_pair(expression: &CalcValue, case: &CalcValue) -> bool {
    let magnitude = |value: &CalcValue| match value.core_ref() {
        CoreRef::Number(n) if !value.is_excel_overflow() && n.is_finite() => Some(n.abs()),
        CoreRef::Empty | CoreRef::Missing => Some(0.0),
        _ => None,
    };
    match (magnitude(expression), magnitude(case)) {
        (Some(a), Some(b)) => a >= READING_OVERFLOW || b >= READING_OVERFLOW,
        _ => false,
    }
}
