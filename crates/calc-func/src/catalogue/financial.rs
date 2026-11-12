//! The financial family fragment ported from OxFunc (functions/ at 6fca3518):
//! financial_time_value_family.rs (PV, FV, PMT, NPER, RATE, NPV, IPMT, PPMT, ISPMT, MIRR,
//! FVSCHEDULE, PDURATION, RRI, NOMINAL, EFFECT), cumulative_finance_family.rs (CUMIPMT,
//! CUMPRINC), cashflow_rate_family.rs (IRR, XNPV, XIRR), depreciation_family.rs (SLN, SYD, DB,
//! DDB, VDB), amor_depreciation_family.rs (AMORLINC, AMORDEGRC) and dollar_fraction_family.rs
//! (DOLLARDE, DOLLARFR). The numeric kernels are OxFunc's ([`crate::ported`]); this fragment reads
//! the arguments as OxFunc's surfaces read them.
//!
//! Single-value parameters are value parameters, so a formula lifts them over an array (calc-fml).
//! Most omitted arguments read as0; required CUMIPMT/CUMPRINC slots give #N/A,
//! and those entries reject logicals (unchanged R24 and fn8 live controls). NPV reads its values
//! by the aggregate rule ([`super::aggregate`]); IRR, XNPV and XIRR read theirs by OxFunc's
//! collectors. AMORLINC and AMORDEGRC count their dates in the date system of the call's
//! environment ([`crate::locale::CalcEnvironment`]); a call without one is refused as not
//! modelled.

use super::aggregate::Held;
use super::kit::{fail, flag, given, grid, num, num_with_subnormal, numbers, opt, out, out_with_subnormal, reject_logical};
use crate::call::EvalServices;
use crate::kernel::{Site, Step};
use crate::locale::DateSystem;
use crate::ported::amor_depreciation_family as amor;
use crate::ported::cashflow_rate_family as cash;
use crate::ported::cumulative_finance_family as cumulative;
use crate::ported::depreciation_family as dep;
use crate::ported::dollar_fraction_family as dollar;
use crate::ported::financial_time_value_family::{self as tv, FinancialError, PaymentTiming};
use crate::value::{CalcArray, CalcValue, CoreRef, WorksheetErrorCode};

type W = WorksheetErrorCode;

use calc_catalogue::ported::financial::{CODE, ENTRIES as DECLARATIONS};

/// The days from 1 January 1900 (serial 1 of the 1900 system, which counts a 29 February 1900)
/// to 1 January 1904 (serial 0 of the 1904 system).
pub(super) const OFFSET_1904: f64 = 1462.0;

/// Required scalar financial slots distinguish a Missing placeholder from a
/// blank cell. The latter still reaches the existing numeric coercion as zero.
pub(super) fn required_number(value: &CalcValue) -> Step<()> {
    if value.is_missing() {
        return fail(W::NA);
    }
    reject_logical(value)
}

pub(super) fn required_num(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<f64> {
    required_number(&a[index])?;
    num(site, a, index)
}

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
fn resolved(
    site: &Site<'_>,
    a: &[CalcValue],
    index: usize,
    sv: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    if !matches!(a[index].core_ref(), CoreRef::Reference(_)) {
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
fn guess(
    site: &Site<'_>,
    a: &[CalcValue],
    index: usize,
    sv: &mut EvalServices<'_>,
) -> Step<Option<f64>> {
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
pub(super) fn date_1900(
    site: &Site<'_>,
    a: &[CalcValue],
    index: usize,
    sv: &EvalServices<'_>,
) -> Step<f64> {
    let Some(environment) = sv.environment() else {
        return Err(site.not_modelled(None));
    };
    let serial = num(site, a, index)?;
    Ok(match environment.date_system {
        DateSystem::System1904 if serial >= 0.0 => serial + OFFSET_1904,
        _ => serial,
    })
}

fn amor_out(
    site: &Site<'_>,
    result: Result<f64, amor::AmorDepreciationEvalError>,
) -> Step<CalcValue> {
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
    match a[index].core_ref() {
        CoreRef::Logical(_) => fail(W::Value),
        _ => num(site, a, index),
    }
}

/// FVSCHEDULE's schedule: numbers, a blank cell as 0, any other held value #VALUE!; a direct
/// value through number coercion.
fn schedule(
    site: &Site<'_>,
    a: &[CalcValue],
    index: usize,
    sv: &mut EvalServices<'_>,
) -> Step<Vec<f64>> {
    let g = grid(site, a, index, sv)?;
    if !g.held {
        return Ok(vec![num(site, a, index)?]);
    }
    let mut rates = Vec::with_capacity(g.cells.len());
    for cell in &g.cells {
        match cell.core_ref() {
            CoreRef::Number(n) => rates.push(n),
            CoreRef::Empty => rates.push(0.0),
            CoreRef::Error(code) => return fail(code),
            _ => return fail(W::Value),
        }
    }
    Ok(rates)
}

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
ported_shared_family! {
    code: CODE;
    declarations: DECLARATIONS;
    1 pv_fn |s, a, sv| {
        let t = timing(s, a, 4)?;
        tv_out(s, tv::pv(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, opt(s, a, 3, 0.0)?, t))
    }
    2 fv_fn |s, a, sv| {
        let t = timing(s, a, 4)?;
        tv_out(s, tv::fv(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, opt(s, a, 3, 0.0)?, t))
    }
    3 pmt_fn |s, a, sv| {
        let t = timing(s, a, 4)?;
        tv_out(s, tv::pmt(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, opt(s, a, 3, 0.0)?, t))
    }
    4 nper_fn |s, a, sv| {
        let t = timing(s, a, 4)?;
        tv_out(s, tv::nper(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, opt(s, a, 3, 0.0)?, t))
    }
    5 rate_fn |s, a, sv| {
        let t = timing(s, a, 4)?;
        let g = if a.len() > 5 { Some(num(s, a, 5)?) } else { None };
        tv_out(s, tv::rate(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, opt(s, a, 3, 0.0)?, t, g))
    }
    6 npv_fn |s, a, sv| {
        let rate = num(s, a, 0)?;
        let values = numbers(s, a, 1, a.len(), sv, Held::Numbers)?;
        tv_out(s, tv::npv(rate, &values))
    }
    7 ipmt_fn |s, a, sv| {
        let t = timing(s, a, 5)?;
        tv_out(s, tv::ipmt(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, num(s, a, 3)?, opt(s, a, 4, 0.0)?, t))
    }
    8 ppmt_fn |s, a, sv| {
        let t = timing(s, a, 5)?;
        tv_out(s, tv::ppmt(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, num(s, a, 3)?, opt(s, a, 4, 0.0)?, t))
    }
    9 ispmt_fn |s, a, sv| {
        tv_out(s, tv::ispmt(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, num(s, a, 3)?))
    }
    10 mirr_fn |s, a, sv| {
        let values = numbers(s, a, 0, 1, sv, Held::Numbers)?;
        tv_out(s, tv::mirr(&values, num(s, a, 1)?, num(s, a, 2)?))
    }
    11 fvschedule_fn |s, a, sv| {
        let principal = num(s, a, 0)?;
        let rates = schedule(s, a, 1, sv)?;
        tv_out(s, tv::fvschedule(principal, &rates))
    }
    12 pduration_fn |s, a, sv| {
        tv_out(s, tv::pduration(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?))
    }
    13 rri_fn |s, a, sv| {
        tv_out(s, tv::rri(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?))
    }
    14 nominal_fn |s, a, sv| {
        tv_out(s, tv::nominal(num(s, a, 0)?, num(s, a, 1)?))
    }
    15 effect_fn |s, a, sv| {
        tv_out(s, tv::effect(num(s, a, 0)?, num(s, a, 1)?))
    }
    16 cumipmt_fn |s, a, sv| {
        cumulative_out(s, cumulative::cumipmt_kernel(required_num(s, a, 0)?, required_num(s, a, 1)?, required_num(s, a, 2)?, required_num(s, a, 3)?, required_num(s, a, 4)?, required_num(s, a, 5)?))
    }
    17 cumprinc_fn |s, a, sv| {
        cumulative_out(s, cumulative::cumprinc_kernel(required_num(s, a, 0)?, required_num(s, a, 1)?, required_num(s, a, 2)?, required_num(s, a, 3)?, required_num(s, a, 4)?, required_num(s, a, 5)?))
    }
    18 irr_fn |s, a, sv| {
        let mut values = resolved(s, a, 0, sv)?;
        // Fn7 scalar readings: held cells keep the collector's held-value rule,
        // even when the range is one cell. Direct text and logicals are ignored
        // too. Admit the guess before these scalar collections; a direct error
        // and larger arrays retain their existing validation order.
        let scalar = values.callable_value().is_none()
            && (matches!(a[0].core_ref(), CoreRef::Reference(_))
                && !matches!(values.core_ref(), CoreRef::Array(_))
                || matches!(values.core_ref(), CoreRef::Text(_) | CoreRef::Logical(_) | CoreRef::Empty));
        let mut g = None;
        if scalar {
            g = guess(s, a, 1, sv)?;
            values = CalcValue::array(CalcArray::from_scalar(values));
        }
        let cashflows = match cash::collect_irr_values_from_eval(&values) {
            Ok(cashflows) => cashflows,
            Err(error) => return cash_err(error),
        };
        if !scalar {
            g = guess(s, a, 1, sv)?;
        }
        out(s, cash::irr_kernel(&cashflows, g))
    }
    19 xnpv_fn |s, a, sv| {
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
    20 xirr_fn |s, a, sv| {
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
    21 sln_fn |s, a, sv| {
        out(s, dep::sln_kernel(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?))
    }
    22 syd_fn |s, a, sv| {
        out(s, dep::syd_kernel(num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, num(s, a, 3)?))
    }
    23 db_fn |s, a, sv| {
        // W111 independent error-order controls: earlier required arguments precede month.
        let (cost, salvage, life, period) = (num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, num(s, a, 3)?);
        let month = if a.len() > 4 { num(s, a, 4)? } else { 12.0 };
        out(s, dep::db_kernel(cost, salvage, life, period, month))
    }
    24 ddb_fn |s, a, sv| {
        let (cost, salvage, life, period) = (num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?, num(s, a, 3)?);
        let factor = if a.len() > 4 { num(s, a, 4)? } else { 2.0 };
        out(s, dep::ddb_kernel(cost, salvage, life, period, factor))
    }
    25 vdb_fn |s, a, sv| {
        // W111 typed and padding controls read the required arguments, then factor,
        // then no_switch. An explicit empty factor slot coerces to zero; only an
        // absent trailing factor takes the default of two.
        let (cost, salvage, life, start, end) = (
            num_with_subnormal(s, a, 0)?,
            num_with_subnormal(s, a, 1)?,
            num_with_subnormal(s, a, 2)?,
            num_with_subnormal(s, a, 3)?,
            num_with_subnormal(s, a, 4)?,
        );
        let factor = if a.len() > 5 { num_with_subnormal(s, a, 5)? } else { 2.0 };
        let no_switch = if given(a, 6) && !a[6].is_empty() { flag(s, a, 6)? } else { false };
        out_with_subnormal(s, dep::vdb_kernel(cost, salvage, life, start, end, factor, no_switch))
    }
    26 amorlinc_fn |s, a, sv| {
        let cost = required_num(s, a, 0)?;
        required_number(&a[1])?;
        let purchased = super::kit::checked_date_serial(date_1900(s, a, 1, sv)?)?;
        required_number(&a[2])?;
        let first = super::kit::checked_date_serial(date_1900(s, a, 2, sv)?)?;
        let (salvage, period, rate) = (required_num(s, a, 3)?, required_num(s, a, 4)?, required_num(s, a, 5)?);
        if cost <= 0.0 || salvage < 0.0 || salvage > cost || period < 0.0 || rate <= 0.0 || purchased.trunc() > first.trunc() {
            return fail(W::Num);
        }
        let basis = if given(a, 6) { super::kit::num_without_logical(s, a, 6)? } else { 0.0 };
        amor_out(s, amor::amorlinc_kernel(cost, purchased, first, salvage, period, rate, Some(basis)))
    }
    27 amordegrc_fn |s, a, sv| {
        let purchased = date_1900(s, a, 1, sv)?;
        let first = date_1900(s, a, 2, sv)?;
        let basis = if given(a, 6) { num(s, a, 6)? } else { 0.0 };
        amor_out(s, amor::amordegrc_kernel(num(s, a, 0)?, purchased, first, num(s, a, 3)?, num(s, a, 4)?, num(s, a, 5)?, Some(basis)))
    }
    28 dollarde_fn |s, a, sv| {
        if a.iter().any(CalcValue::is_missing) {
            return fail(W::NA);
        }
        out(s, dollar::dollarde_kernel(dollar_num(s, a, 0)?, dollar_num(s, a, 1)?))
    }
    29 dollarfr_fn |s, a, sv| {
        if a.iter().any(CalcValue::is_missing) {
            return fail(W::NA);
        }
        out(s, dollar::dollarfr_kernel(dollar_num(s, a, 0)?, dollar_num(s, a, 1)?))
    }
}
