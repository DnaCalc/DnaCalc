//! The financial family fragment ported from OxFunc (functions/ at 6fca3518):
//! financial_time_value_family.rs (PV, FV, PMT, NPER, RATE, NPV, IPMT, PPMT, ISPMT, MIRR,
//! FVSCHEDULE, PDURATION, RRI, NOMINAL, EFFECT), cumulative_finance_family.rs (CUMIPMT,
//! CUMPRINC), cashflow_rate_family.rs (IRR, XNPV, XIRR), depreciation_family.rs (SLN, SYD, DB,
//! DDB, VDB), amor_depreciation_family.rs (AMORLINC, AMORDEGRC) and dollar_fraction_family.rs
//! (DOLLARDE, DOLLARFR). The numeric kernels are OxFunc's ([`crate::ported`]); this fragment reads
//! the arguments as OxFunc's surfaces read them.
//!
//! Single-value parameters are value parameters, so a formula lifts them over an array (calc-fml).
//! An omitted argument is read as 0, as OxFunc's financial surfaces read it. NPV reads its values
//! by the aggregate rule ([`super::aggregate`]); IRR, XNPV and XIRR read theirs by OxFunc's
//! collectors. AMORLINC and AMORDEGRC count their dates in the date system of the call's
//! environment ([`crate::locale::CalcEnvironment`]); a call without one is refused as not
//! modelled.

use super::aggregate::Held;
use super::kit::{fail, flag, given, grid, num, numbers, opt, out};
use crate::call::EvalServices;
use crate::kernel::{Site, Step};
use crate::locale::DateSystem;
use crate::ported::amor_depreciation_family as amor;
use crate::ported::cashflow_rate_family as cash;
use crate::ported::cumulative_finance_family as cumulative;
use crate::ported::depreciation_family as dep;
use crate::ported::dollar_fraction_family as dollar;
use crate::ported::financial_time_value_family::{self as tv, FinancialError, PaymentTiming};
use crate::value::{CalcArray, CalcValue, CoreValue, WorksheetErrorCode};

type W = WorksheetErrorCode;

const CODE: u16 = 300;

/// The days from 1 January 1900 (serial 1 of the 1900 system, which counts a 29 February 1900)
/// to 1 January 1904 (serial 0 of the 1904 system).
pub(super) const OFFSET_1904: f64 = 1462.0;

/// A time-value kernel's result: its number, or the worksheet error its failure maps to.
fn tv_out(site: &Site<'_>, result: Result<f64, FinancialError>) -> Step<CalcValue> {
    match result {
        Ok(n) => site.number(n),
        Err(FinancialError::Num | FinancialError::NoConvergence) => fail(W::Num),
        Err(FinancialError::Div0) => fail(W::Div0),
        Err(FinancialError::Value) => fail(W::Value),
    }
}

/// The payment timing argument `index`: 0 (the default) is the end of each period, any other
/// number truncated to non-zero its beginning.
fn timing(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<PaymentTiming> {
    Ok(if opt(site, a, index, 0.0)?.trunc() == 0.0 {
        PaymentTiming::EndOfPeriod
    } else {
        PaymentTiming::BeginningOfPeriod
    })
}

/// Argument `index` as OxFunc's resolver gave it to the cash-flow collectors: a reference to one
/// cell as the cell's value, a larger reference as the array of its values, anything else as it
/// is.
fn resolved(site: &Site<'_>, a: &[CalcValue], index: usize, sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    if !matches!(a[index].core(), CoreValue::Reference(_)) {
        return Ok(a[index].clone());
    }
    let g = grid(site, a, index, sv)?;
    if g.rows == 1 && g.cols == 1 {
        return Ok(g.cells.into_iter().next().unwrap_or_else(CalcValue::empty));
    }
    let rows: Vec<Vec<CalcValue>> = g.cells.chunks(g.cols).map(<[CalcValue]>::to_vec).collect();
    CalcArray::from_rows(rows)
        .map(CalcValue::array)
        .ok_or_else(|| site.not_modelled(Some(index)))
}

/// A cash-flow collector's or kernel's failure as a worksheet error.
fn cash_err<T>(error: cash::CashflowRateEvalError) -> Step<T> {
    fail(cash::map_cashflow_rate_error_to_ws(&error))
}

/// The optional guess of IRR and XIRR: `None` when omitted, else the number OxFunc's rule reads.
fn guess(site: &Site<'_>, a: &[CalcValue], index: usize, sv: &mut EvalServices<'_>) -> Step<Option<f64>> {
    if !given(a, index) {
        return Ok(None);
    }
    let value = resolved(site, a, index, sv)?;
    match cash::scalar_number_from_eval(&value) {
        Ok(n) => Ok(Some(n)),
        Err(error) => cash_err(error),
    }
}

/// A date argument of AMORLINC or AMORDEGRC as a serial of the 1900 system, counted in the date
/// system of the call's environment.
pub(super) fn date_1900(site: &Site<'_>, a: &[CalcValue], index: usize, sv: &EvalServices<'_>) -> Step<f64> {
    let Some(environment) = sv.environment() else {
        return Err(site.not_modelled(None));
    };
    let serial = num(site, a, index)?;
    Ok(match environment.date_system {
        DateSystem::System1904 if serial >= 0.0 => serial + OFFSET_1904,
        _ => serial,
    })
}

fn amor_out(site: &Site<'_>, result: Result<f64, amor::AmorDepreciationEvalError>) -> Step<CalcValue> {
    match result {
        Ok(n) => site.number(n),
        Err(error) => fail(amor::map_amor_depreciation_error_to_ws(&error)),
    }
}

fn cumulative_out(
    site: &Site<'_>,
    result: Result<f64, cumulative::CumulativeFinanceEvalError>,
) -> Step<CalcValue> {
    match result {
        Ok(n) => site.number(n),
        Err(error) => fail(cumulative::map_cumulative_finance_error_to_ws(&error)),
    }
}

/// DOLLARDE's and DOLLARFR's argument: a logical is #VALUE! (OxFunc's surface admits numbers,
/// numeric text and blank cells only).
fn dollar_num(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<f64> {
    match a[index].core() {
        CoreValue::Logical(_) => fail(W::Value),
        _ => num(site, a, index),
    }
}

/// FVSCHEDULE's schedule: numbers, a blank cell as 0, any other held value #VALUE!; a direct
/// value through number coercion.
fn schedule(site: &Site<'_>, a: &[CalcValue], index: usize, sv: &mut EvalServices<'_>) -> Step<Vec<f64>> {
    let g = grid(site, a, index, sv)?;
    if !g.held {
        return Ok(vec![num(site, a, index)?]);
    }
    let mut rates = Vec::with_capacity(g.cells.len());
    for cell in &g.cells {
        match cell.core() {
            CoreValue::Number(n) => rates.push(*n),
            CoreValue::Empty => rates.push(0.0),
            CoreValue::Error(code) => return fail(*code),
            _ => return fail(W::Value),
        }
    }
    Ok(rates)
}

ported_family! {
    code: CODE;
    1 pv_fn "PV" (3, 5) [V "rate", V "nper", V "pmt", V "fv", V "type"] |s, a, sv| {
        let t = timing(s, a, 4)?;
        tv_out(s, tv::pv(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, opt(s, a, 3, 0.0)?, t))
    }
    2 fv_fn "FV" (3, 5) [V "rate", V "nper", V "pmt", V "pv", V "type"] |s, a, sv| {
        let t = timing(s, a, 4)?;
        tv_out(s, tv::fv(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, opt(s, a, 3, 0.0)?, t))
    }
    3 pmt_fn "PMT" (3, 5) [V "rate", V "nper", V "pv", V "fv", V "type"] |s, a, sv| {
        let t = timing(s, a, 4)?;
        tv_out(s, tv::pmt(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, opt(s, a, 3, 0.0)?, t))
    }
    4 nper_fn "NPER" (3, 5) [V "rate", V "pmt", V "pv", V "fv", V "type"] |s, a, sv| {
        let t = timing(s, a, 4)?;
        tv_out(s, tv::nper(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, opt(s, a, 3, 0.0)?, t))
    }
    5 rate_fn "RATE" (3, 6) [V "nper", V "pmt", V "pv", V "fv", V "type", V "guess"] |s, a, sv| {
        let t = timing(s, a, 4)?;
        let g = if a.len() > 5 { Some(num(s, a, 5)?) } else { None };
        tv_out(s, tv::rate(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, opt(s, a, 3, 0.0)?, t, g))
    }
    6 npv_fn "NPV" (2, 255) [V "rate", R "value1", R "value2"] |s, a, sv| {
        let rate = num(s, a, 0)?;
        let values = numbers(s, a, 1, a.len(), sv, Held::Numbers)?;
        tv_out(s, tv::npv(rate, &values))
    }
    7 ipmt_fn "IPMT" (4, 6) [V "rate", V "per", V "nper", V "pv", V "fv", V "type"] |s, a, sv| {
        let t = timing(s, a, 5)?;
        tv_out(s, tv::ipmt(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, num(s, a, 3)?, opt(s, a, 4, 0.0)?, t))
    }
    8 ppmt_fn "PPMT" (4, 6) [V "rate", V "per", V "nper", V "pv", V "fv", V "type"] |s, a, sv| {
        let t = timing(s, a, 5)?;
        tv_out(s, tv::ppmt(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, num(s, a, 3)?, opt(s, a, 4, 0.0)?, t))
    }
    9 ispmt_fn "ISPMT" (4, 4) [V "rate", V "per", V "nper", V "pv"] |s, a, sv| {
        tv_out(s, tv::ispmt(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, num(s, a, 3)?))
    }
    10 mirr_fn "MIRR" (3, 3) [R "values", V "finance_rate", V "reinvest_rate"] |s, a, sv| {
        let values = numbers(s, a, 0, 1, sv, Held::Numbers)?;
        tv_out(s, tv::mirr(&values, num(s, a, 1)?, num(s, a, 2)?))
    }
    11 fvschedule_fn "FVSCHEDULE" (2, 2) [V "principal", R "schedule"] |s, a, sv| {
        let principal = num(s, a, 0)?;
        let rates = schedule(s, a, 1, sv)?;
        tv_out(s, tv::fvschedule(principal, &rates))
    }
    12 pduration_fn "PDURATION" (3, 3) [V "rate", V "pv", V "fv"] |s, a, sv| {
        tv_out(s, tv::pduration(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?))
    }
    13 rri_fn "RRI" (3, 3) [V "nper", V "pv", V "fv"] |s, a, sv| {
        tv_out(s, tv::rri(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?))
    }
    14 nominal_fn "NOMINAL" (2, 2) [V "effect_rate", V "npery"] |s, a, sv| {
        tv_out(s, tv::nominal(num(s, a, 0)?, num(s, a, 1)?))
    }
    15 effect_fn "EFFECT" (2, 2) [V "nominal_rate", V "npery"] |s, a, sv| {
        tv_out(s, tv::effect(num(s, a, 0)?, num(s, a, 1)?))
    }
    16 cumipmt_fn "CUMIPMT" (6, 6) [V "rate", V "nper", V "pv", V "start_period", V "end_period", V "type"] |s, a, sv| {
        cumulative_out(s, cumulative::cumipmt_kernel(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, num(s, a, 3)?, num(s, a, 4)?, num(s, a, 5)?))
    }
    17 cumprinc_fn "CUMPRINC" (6, 6) [V "rate", V "nper", V "pv", V "start_period", V "end_period", V "type"] |s, a, sv| {
        cumulative_out(s, cumulative::cumprinc_kernel(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, num(s, a, 3)?, num(s, a, 4)?, num(s, a, 5)?))
    }
    18 irr_fn "IRR" (1, 2) [R "values", V "guess"] |s, a, sv| {
        let values = resolved(s, a, 0, sv)?;
        let cashflows = match cash::collect_irr_values_from_eval(&values) {
            Ok(cashflows) => cashflows,
            Err(error) => return cash_err(error),
        };
        let g = guess(s, a, 1, sv)?;
        out(s, cash::irr_kernel(&cashflows, g))
    }
    19 xnpv_fn "XNPV" (3, 3) [V "rate", R "values", R "dates"] |s, a, sv| {
        let rate = match cash::scalar_number_from_eval(&a[0]) {
            Ok(n) => n,
            Err(error) => return cash_err(error),
        };
        let values = resolved(s, a, 1, sv)?;
        let dates = resolved(s, a, 2, sv)?;
        let values = match cash::collect_numeric_vector_from_eval(&values) {
            Ok(v) => v,
            Err(error) => return cash_err(error),
        };
        let dates = match cash::collect_serial_vector_from_eval(&dates) {
            Ok(d) => d,
            Err(error) => return cash_err(error),
        };
        out(s, cash::xnpv_kernel(rate, &values, &dates))
    }
    20 xirr_fn "XIRR" (2, 3) [R "values", R "dates", V "guess"] |s, a, sv| {
        let values = resolved(s, a, 0, sv)?;
        let dates = resolved(s, a, 1, sv)?;
        let values = match cash::collect_numeric_vector_from_eval(&values) {
            Ok(v) => v,
            Err(error) => return cash_err(error),
        };
        let dates = match cash::collect_serial_vector_from_eval(&dates) {
            Ok(d) => d,
            Err(error) => return cash_err(error),
        };
        let g = guess(s, a, 2, sv)?;
        out(s, cash::xirr_kernel(&values, &dates, g))
    }
    21 sln_fn "SLN" (3, 3) [V "cost", V "salvage", V "life"] |s, a, sv| {
        out(s, dep::sln_kernel(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?))
    }
    22 syd_fn "SYD" (4, 4) [V "cost", V "salvage", V "life", V "per"] |s, a, sv| {
        out(s, dep::syd_kernel(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, num(s, a, 3)?))
    }
    23 db_fn "DB" (4, 5) [V "cost", V "salvage", V "life", V "period", V "month"] |s, a, sv| {
        let month = if given(a, 4) { num(s, a, 4)? } else { 12.0 };
        out(s, dep::db_kernel(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, num(s, a, 3)?, month))
    }
    24 ddb_fn "DDB" (4, 5) [V "cost", V "salvage", V "life", V "period", V "factor"] |s, a, sv| {
        let factor = if given(a, 4) { num(s, a, 4)? } else { 2.0 };
        out(s, dep::ddb_kernel(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, num(s, a, 3)?, factor))
    }
    25 vdb_fn "VDB" (5, 7) [V "cost", V "salvage", V "life", V "start_period", V "end_period", V "factor", V "no_switch"] |s, a, sv| {
        let factor = if given(a, 5) { num(s, a, 5)? } else { 2.0 };
        let no_switch = if given(a, 6) && !a[6].is_empty() { flag(s, a, 6)? } else { false };
        out(s, dep::vdb_kernel(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, num(s, a, 3)?, num(s, a, 4)?, factor, no_switch))
    }
    26 amorlinc_fn "AMORLINC" (6, 7) [V "cost", V "date_purchased", V "first_period", V "salvage", V "period", V "rate", V "basis"] |s, a, sv| {
        let purchased = date_1900(s, a, 1, sv)?;
        let first = date_1900(s, a, 2, sv)?;
        let basis = if given(a, 6) { num(s, a, 6)? } else { 0.0 };
        amor_out(s, amor::amorlinc_kernel(num(s, a, 0)?, purchased, first, num(s, a, 3)?, num(s, a, 4)?, num(s, a, 5)?, Some(basis)))
    }
    27 amordegrc_fn "AMORDEGRC" (6, 7) [V "cost", V "date_purchased", V "first_period", V "salvage", V "period", V "rate", V "basis"] |s, a, sv| {
        let purchased = date_1900(s, a, 1, sv)?;
        let first = date_1900(s, a, 2, sv)?;
        let basis = if given(a, 6) { num(s, a, 6)? } else { 0.0 };
        amor_out(s, amor::amordegrc_kernel(num(s, a, 0)?, purchased, first, num(s, a, 3)?, num(s, a, 4)?, num(s, a, 5)?, Some(basis)))
    }
    28 dollarde_fn "DOLLARDE" (2, 2) [V "fractional_dollar", V "fraction"] |s, a, sv| {
        if a.iter().any(CalcValue::is_missing) {
            return fail(W::NA);
        }
        out(s, dollar::dollarde_kernel(dollar_num(s, a, 0)?, dollar_num(s, a, 1)?))
    }
    29 dollarfr_fn "DOLLARFR" (2, 2) [V "decimal_dollar", V "fraction"] |s, a, sv| {
        if a.iter().any(CalcValue::is_missing) {
            return fail(W::NA);
        }
        out(s, dollar::dollarfr_kernel(dollar_num(s, a, 0)?, dollar_num(s, a, 1)?))
    }
}
