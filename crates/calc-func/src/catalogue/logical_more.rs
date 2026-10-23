//! The logical functions F2 adds beside the logical family, ported from OxFunc (functions/ at
//! 6fca3518: choose_ifs_family.rs's IFS and switch_fn.rs's SWITCH).
//!
//! Both are eager here: every argument is evaluated before the call, and the kernel selects. An
//! argument the selection does not reach does not change the result, so only the evaluation order
//! differs from Excel's, which evaluates a branch only when it is selected.
//!
//! IFS reads each condition as IF does (worksheet logical coercion, an error is the result) and
//! gives the value of the first TRUE condition's pair; no TRUE condition is #N/A. SWITCH compares
//! its expression with each case as the `=` operator does ([`super::comparison::order`]); an
//! error in the expression is the result, the first matching case's value is the result, then the
//! default, and #N/A when there is none. An omitted selected value is 0, as IF's is.

use super::comparison::{Want, order};
use super::kit::{fail, flag};
use crate::value::{CalcValue, CoreValue, WorksheetErrorCode};
use std::cmp::Ordering;

const CODE: u16 = 201;

fn selected(value: &CalcValue) -> CalcValue {
    if value.is_missing() {
        CalcValue::number(0.0)
    } else {
        value.clone()
    }
}

ported_family! {
    code: CODE;
    1 ifs "IFS" (2, 254) [V "logical_test1", V "value_if_true1"] |s, a, sv| {
        if a.len() % 2 != 0 {
            return fail(WorksheetErrorCode::Value);
        }
        for pair in (0..a.len()).step_by(2) {
            if flag(s, a, pair)? {
                return Ok(selected(&a[pair + 1]));
            }
        }
        fail(WorksheetErrorCode::NA)
    }
    2 switch "SWITCH" (3, 254) [V "expression", V "value1", V "result1"] |s, a, sv| {
        if let CoreValue::Error(code) = a[0].core() {
            return fail(*code);
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
    }
}
