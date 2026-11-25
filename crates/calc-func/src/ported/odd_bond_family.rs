//! Odd-first and odd-last period bond kernels (ODDFPRICE, ODDFYIELD, ODDLPRICE, ODDLYIELD).
//!
//! Parts of this file are a translation, changed, of ExcelFinancialFunctions by Luca Bolognese
//! (<https://github.com/fsprojects/ExcelFinancialFunctions>, `oddbonds.fs` and `daycountbasis.fs`;
//! Copyright 2010-2013, Apache License 2.0): `oddfprice_kernel` (`oddFPrice`) and the helpers named
//! after their F# counterparts in their doc comments (`changeMonth`, `findCouponDates`,
//! `numberOfCoupons`, `CoupDays`, `coupNumber`, `aggrBetween`). They were rewritten in Rust and
//! changed where Excel measurement required it. The licence text is in
//! `deploy/notices/ported.txt`. The rest of the file is our own work.

use crate::locale::{
    DateSystem as WorkbookDateSystem, excel_serial_from_ymd, ymd_from_excel_serial,
};
use crate::ported::excel_numeric::{excel_pow_chain, excel_x87_div};
use crate::value::CalcValue;
use crate::value::WorksheetErrorCode;

#[derive(Debug, Clone, PartialEq)]
pub enum OddBondEvalError {
    ArityMismatch {
        expected_min: usize,
        expected_max: usize,
        actual: usize,
    },
    Domain(WorksheetErrorCode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DayCountBasis {
    Us30_360,
    ActualActual,
    Actual360,
    Actual365,
    European30_360,
}

fn parse_basis(value: f64) -> Result<DayCountBasis, OddBondEvalError> {
    if !value.is_finite() {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    match value.trunc() as i64 {
        0 => Ok(DayCountBasis::Us30_360),
        1 => Ok(DayCountBasis::ActualActual),
        2 => Ok(DayCountBasis::Actual360),
        3 => Ok(DayCountBasis::Actual365),
        4 => Ok(DayCountBasis::European30_360),
        _ => Err(OddBondEvalError::Domain(WorksheetErrorCode::Num)),
    }
}

pub(crate) fn parse_frequency(value: f64) -> Result<i64, OddBondEvalError> {
    if !value.is_finite() {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    match value.trunc() as i64 {
        1 | 2 | 4 => Ok(value.trunc() as i64),
        _ => Err(OddBondEvalError::Domain(WorksheetErrorCode::Num)),
    }
}

fn max_excel_serial() -> i64 {
    excel_serial_from_ymd(WorkbookDateSystem::System1900, 9999, 12, 31).unwrap() as i64
}

fn parse_date_serial(value: f64) -> Result<i64, OddBondEvalError> {
    if !value.is_finite() {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Value));
    }
    let serial = value.trunc() as i64;
    if serial < 1 || serial > max_excel_serial() {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    ymd_from_excel_serial(WorkbookDateSystem::System1900, serial as f64)
        .ok_or(OddBondEvalError::Domain(WorksheetErrorCode::Value))?;
    Ok(serial)
}

fn days_in_year(year: i64) -> f64 {
    if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
        366.0
    } else {
        365.0
    }
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if days_in_year(year) == 366.0 {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

fn add_months_clamped(serial: i64, months: i64) -> Option<i64> {
    let (year, month, day) = ymd_from_excel_serial(WorkbookDateSystem::System1900, serial as f64)?;
    let month_index = year
        .checked_mul(12)?
        .checked_add(month - 1)?
        .checked_add(months)?;
    let target_year = month_index.div_euclid(12);
    let target_month = month_index.rem_euclid(12) + 1;
    let source_is_month_end = day == days_in_month(year, month);
    let target_day = if source_is_month_end {
        days_in_month(target_year, target_month)
    } else {
        day.min(days_in_month(target_year, target_month))
    };
    excel_serial_from_ymd(
        WorkbookDateSystem::System1900,
        target_year,
        target_month,
        target_day,
    )
    .map(|v| v as i64)
}

fn days360_us(start: i64, end: i64) -> Result<f64, OddBondEvalError> {
    crate::ported::day_count_common::us_30_360(start, end).map_err(OddBondEvalError::Domain)
}
fn days360_us_both(start: i64, end: i64) -> Result<f64, OddBondEvalError> {
    let (sy, sm, mut sd) = ymd_from_excel_serial(WorkbookDateSystem::System1900, start as f64)
        .ok_or(OddBondEvalError::Domain(WorksheetErrorCode::Value))?;
    let (ey, em, mut ed) = ymd_from_excel_serial(WorkbookDateSystem::System1900, end as f64)
        .ok_or(OddBondEvalError::Domain(WorksheetErrorCode::Value))?;

    let start_last_feb = sm == 2 && sd == days_in_month(sy, sm);
    let end_last_feb = em == 2 && ed == days_in_month(ey, em);
    if end_last_feb {
        ed = 30;
    }
    if ed == 31 {
        ed = 30;
    }
    if sd == 31 {
        sd = 30;
    }
    if start_last_feb {
        sd = 30;
    }

    Ok(((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64)
}

fn days360_eu(start: i64, end: i64) -> Result<f64, OddBondEvalError> {
    let (sy, sm, mut sd) = ymd_from_excel_serial(WorkbookDateSystem::System1900, start as f64)
        .ok_or(OddBondEvalError::Domain(WorksheetErrorCode::Value))?;
    let (ey, em, mut ed) = ymd_from_excel_serial(WorkbookDateSystem::System1900, end as f64)
        .ok_or(OddBondEvalError::Domain(WorksheetErrorCode::Value))?;
    if sd == 31 {
        sd = 30;
    }
    if ed == 31 {
        ed = 30;
    }
    Ok(((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64)
}

fn days360_us_accrued(start: i64, end: i64) -> Result<f64, OddBondEvalError> {
    let (sy, sm, mut sd) = ymd(start)?;
    let (mut ey, mut em, mut ed) = ymd(end)?;
    let feb_end = |y: i64, m: i64| if y == 1900 { 29 } else { days_in_month(y, m) };
    let start_last_feb = sm == 2 && sd == feb_end(sy, sm);
    let end_last_feb = em == 2 && ed == feb_end(ey, em);
    if ed == 31 {
        if sd < 30 {
            (ey, em) = if em == 12 { (ey + 1, 1) } else { (ey, em + 1) };
            ed = 1;
        } else {
            ed = 30;
        }
    }
    if sd == 31 || start_last_feb {
        sd = 30;
    }
    if start_last_feb && end_last_feb {
        ed = 30;
    }
    Ok(((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64)
}

fn day_count_accrued(start: i64, end: i64, basis: DayCountBasis) -> Result<f64, OddBondEvalError> {
    match basis {
        DayCountBasis::Us30_360 => days360_us_accrued(start, end),
        DayCountBasis::ActualActual | DayCountBasis::Actual360 | DayCountBasis::Actual365 => {
            Ok((end - start) as f64)
        }
        DayCountBasis::European30_360 => days360_eu(start, end),
    }
}

fn day_count(start: i64, end: i64, basis: DayCountBasis) -> Result<f64, OddBondEvalError> {
    match basis {
        DayCountBasis::Us30_360 => days360_us(start, end),
        DayCountBasis::ActualActual | DayCountBasis::Actual360 | DayCountBasis::Actual365 => {
            Ok((end - start) as f64)
        }
        DayCountBasis::European30_360 => days360_eu(start, end),
    }
}
fn day_count_non_negative(
    start: i64,
    end: i64,
    basis: DayCountBasis,
) -> Result<f64, OddBondEvalError> {
    Ok(day_count(start, end, basis)?.max(0.0))
}
fn day_count_non_negative_with_us_hack(
    start: i64,
    end: i64,
    basis: DayCountBasis,
) -> Result<f64, OddBondEvalError> {
    let result = match basis {
        DayCountBasis::Us30_360 => days360_us_both(start, end)?,
        _ => day_count(start, end, basis)?,
    };
    Ok(result.max(0.0))
}

fn ymd(serial: i64) -> Result<(i64, i64, i64), OddBondEvalError> {
    ymd_from_excel_serial(WorkbookDateSystem::System1900, serial as f64)
        .ok_or(OddBondEvalError::Domain(WorksheetErrorCode::Num))
}

fn last_day_of_month(year: i64, month: i64, day: i64) -> bool {
    day == days_in_month(year, month)
}

/// F# `changeMonth` (daycountbasis.fs): shift the month and either clamp the day
/// to the target month length (.NET `DateTime.AddMonths` semantics — no
/// end-of-month snapping) or force the last day of the target month when
/// `return_last_day` is set. `basis` is intentionally not consulted (the F#
/// `isLastDay` it computes is dead code).
fn change_month(
    serial: i64,
    num_months: i64,
    return_last_day: bool,
) -> Result<i64, OddBondEvalError> {
    let (year, month, day) = ymd(serial)?;
    let month_index = year
        .checked_mul(12)
        .and_then(|v| v.checked_add(month - 1))
        .and_then(|v| v.checked_add(num_months))
        .ok_or(OddBondEvalError::Domain(WorksheetErrorCode::Num))?;
    let target_year = month_index.div_euclid(12);
    let target_month = month_index.rem_euclid(12) + 1;
    let target_day = if return_last_day {
        days_in_month(target_year, target_month)
    } else {
        day.min(days_in_month(target_year, target_month))
    };
    excel_serial_from_ymd(
        WorkbookDateSystem::System1900,
        target_year,
        target_month,
        target_day,
    )
    .map(|v| v as i64)
    .ok_or(OddBondEvalError::Domain(WorksheetErrorCode::Num))
}

/// F# `datesAggregate1`/`findPcdNcd`: walk `start` toward `end` by `num_months`
/// steps, returning the bounding pair `(front, trailing)` where `front` is the
/// first position at/past `end` and `trailing` the one before it.
fn find_pcd_ncd(
    start: i64,
    end: i64,
    num_months: i64,
    return_last_month: bool,
) -> Result<(i64, i64), OddBondEvalError> {
    let mut front = start;
    let mut trailing = end;
    loop {
        let stop = if num_months > 0 {
            front >= end
        } else {
            front <= end
        };
        if stop {
            return Ok((front, trailing));
        }
        trailing = front;
        front = change_month(front, num_months, return_last_month)?;
    }
}

/// F# `findCouponDates`: the (previous, next) quasi-coupon dates bounding
/// `settlement`, anchored on `maturity` and stepping back one period at a time.
fn find_coupon_dates(settl: i64, mat: i64, frequency: i64) -> Result<(i64, i64), OddBondEvalError> {
    let (my, mm, md) = ymd(mat)?;
    let end_month = last_day_of_month(my, mm, md);
    let num_months = -(12 / frequency);
    find_pcd_ncd(mat, settl, num_months, end_month)
}

fn previous_coupon_date(settl: i64, mat: i64, frequency: i64) -> Result<i64, OddBondEvalError> {
    Ok(find_coupon_dates(settl, mat, frequency)?.0)
}

fn next_coupon_date(settl: i64, mat: i64, frequency: i64) -> Result<i64, OddBondEvalError> {
    Ok(find_coupon_dates(settl, mat, frequency)?.1)
}

/// F# `numberOfCoupons` (`CoupNum`): whole months from settlement's previous
/// coupon date to maturity, scaled to coupon periods.
fn number_of_coupons(settl: i64, mat: i64, frequency: i64) -> Result<f64, OddBondEvalError> {
    let pcd = previous_coupon_date(settl, mat, frequency)?;
    let (my, mm, _) = ymd(mat)?;
    let (pcy, pcm, _) = ymd(pcd)?;
    let months = ((my - pcy) * 12 + (mm - pcm)) as f64;
    Ok(months * frequency as f64 / 12.0)
}

/// F# `CoupDays`: the normal length of the coupon period that contains
/// settlement. 30/360 and Actual/360 use `360/freq`, Actual/365 `365/freq`, and
/// Actual/Actual the actual days between the bounding coupon dates.
fn coup_days(
    settl: i64,
    mat: i64,
    frequency: i64,
    basis: DayCountBasis,
) -> Result<f64, OddBondEvalError> {
    match basis {
        DayCountBasis::ActualActual => {
            let (pcd, ncd) = find_coupon_dates(settl, mat, frequency)?;
            Ok((ncd - pcd) as f64)
        }
        DayCountBasis::Actual365 => Ok(365.0 / frequency as f64),
        _ => Ok(360.0 / frequency as f64),
    }
}

/// F# `coupNumber` with `isWholeNumber=true`: the count of whole quasi-coupon
/// periods between settlement and the first coupon (used as `Nq`).
fn whole_coupon_count(
    mat: i64,
    settl: i64,
    months_per_coupon: i64,
) -> Result<f64, OddBondEvalError> {
    let (my, mm, md) = ymd(mat)?;
    let (sy, sm, sd) = ymd(settl)?;
    let end_of_month_temp = last_day_of_month(my, mm, md);
    let end_of_month = if !end_of_month_temp && mm != 2 && md > 28 && md < days_in_month(my, mm) {
        last_day_of_month(sy, sm, sd)
    } else {
        end_of_month_temp
    };
    let start_date = change_month(settl, 0, end_of_month)?;
    let mut coupons = if settl < start_date { 1.0 } else { 0.0 };
    let mut front = change_month(start_date, months_per_coupon, end_of_month)?;
    while front < mat {
        front = change_month(front, months_per_coupon, end_of_month)?;
        coupons += 1.0;
    }
    Ok(coupons)
}

/// F# `aggrBetween` index sequence: ascending when `start <= end`, otherwise
/// descending. The fold order is observable in floating-point accumulation.
fn aggr_indices(start: i64, end: i64) -> Vec<i64> {
    if start <= end {
        (start..=end).collect()
    } else {
        (end..=start).rev().collect()
    }
}

fn validate_positive_inputs(values: &[f64]) -> Result<(), OddBondEvalError> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(OddBondEvalError::Domain(WorksheetErrorCode::Value))
    }
}

/// Excel's bond discount factor uses the C-runtime `pow` integer special case: binary
/// exponentiation (`binexp_positive`) for non-negative integer exponents, `excel_pow_chain`
/// otherwise. Identical to `bond_core_family::excel_bond_pow`.
fn excel_bond_pow(base: f64, exp: f64) -> f64 {
    if exp.fract() == 0.0 && exp.abs() < u32::MAX as f64 {
        let p = crate::ported::power_fn::binexp_positive(base, exp.abs() as u64);
        return if exp >= 0.0 { p } else { excel_x87_div(1.0, p) };
    }
    if exp < 0.0 {
        excel_x87_div(1.0, excel_pow_chain(base, -exp))
    } else {
        excel_pow_chain(base, exp)
    }
}

#[allow(clippy::too_many_arguments)]
fn oddfprice_inner(
    settlement: i64,
    maturity: i64,
    issue: i64,
    first_coupon: i64,
    rate: f64,
    yld: f64,
    redemption: f64,
    frequency: i64,
    basis: DayCountBasis,
) -> Result<f64, OddBondEvalError> {
    // Faithful port of ExcelFinancialFunctions `oddFPrice` (oddbonds.fs), which is
    // bit-exact with live Excel across every day-count basis (verified by the G6
    // three-way ledger). Two branches: a short odd first coupon (DFC < E) and a
    // long one spanning multiple quasi-coupon periods (DFC >= E). The long branch
    // sums per-period day-count fractions (`dci/nl`) — this is exactly what the
    // earlier single-period-length closed form got materially wrong for the
    // actual-day bases (where quasi periods have unequal actual lengths).
    let m = frequency as f64;
    let months_per_coupon = 12 / frequency;
    let e = coup_days(settlement, first_coupon, frequency, basis)?;
    if e <= 0.0 {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    let dfc = day_count_non_negative(issue, first_coupon, basis)?;
    let x = yld / m + 1.0;
    if !(x > 0.0) || !x.is_finite() {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }

    let price = if dfc < e {
        // Short odd first coupon.
        let n = number_of_coupons(settlement, maturity, frequency)?;
        let dsc = day_count_non_negative(settlement, first_coupon, basis)?;
        let a = day_count_non_negative(issue, settlement, basis)?;
        let y = dsc / e;
        let p1 = excel_bond_pow(x, n - 1.0 + y);
        let p2 = excel_bond_pow(x, y);
        if !p1.is_finite() || !p2.is_finite() {
            return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
        }
        let term1 = excel_x87_div(redemption, p1);
        let term2 = 100.0 * rate / m * dfc / e / p2;
        let mut term3 = 0.0;
        for index in aggr_indices(2, n as i64) {
            let pk = excel_bond_pow(x, index as f64 - 1.0 + y);
            if !pk.is_finite() {
                return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
            }
            term3 += 100.0 * rate / m / pk;
        }
        let term4 = a / e * (rate / m) * 100.0;
        term1 + term2 + term3 - term4
    } else {
        // Long odd first coupon: accumulate day-count fractions over each quasi
        // period (walking back from first_coupon), then discount.
        let nc = number_of_coupons(issue, first_coupon, frequency)? as i64;
        let mut late_coupon = first_coupon;
        let mut dcnl = 0.0;
        let mut anl = 0.0;
        for index in aggr_indices(nc, 1) {
            let early_coupon = change_month(late_coupon, -months_per_coupon, false)?;
            let nl = if basis == DayCountBasis::ActualActual {
                day_count_non_negative(early_coupon, late_coupon, basis)?
            } else {
                e
            };
            if nl <= 0.0 {
                return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
            }
            let dci = if index > 1 {
                nl
            } else {
                day_count_non_negative(issue, late_coupon, basis)?
            };
            let start_date = issue.max(early_coupon);
            let end_date = settlement.min(late_coupon);
            let a = day_count_non_negative(start_date, end_date, basis)?;
            late_coupon = early_coupon;
            dcnl += dci / nl;
            anl += a / nl;
        }
        let dsc = match basis {
            DayCountBasis::Actual360 | DayCountBasis::Actual365 => {
                let date = next_coupon_date(settlement, first_coupon, frequency)?;
                day_count_non_negative(settlement, date, basis)?
            }
            _ => {
                let date = previous_coupon_date(settlement, first_coupon, frequency)?;
                let a = day_count(date, settlement, basis)?;
                e - a
            }
        };
        let nq = whole_coupon_count(first_coupon, settlement, months_per_coupon)?;
        let n = number_of_coupons(first_coupon, maturity, frequency)?;
        let y = dsc / e;
        let p1 = excel_bond_pow(x, y + nq + n);
        let p2 = excel_bond_pow(x, nq + y);
        if !p1.is_finite() || !p2.is_finite() {
            return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
        }
        let term1 = excel_x87_div(redemption, p1);
        let term2 = 100.0 * rate / m * dcnl / p2;
        let mut term3 = 0.0;
        for index in aggr_indices(1, n as i64) {
            let pk = excel_bond_pow(x, index as f64 + nq + y);
            if !pk.is_finite() {
                return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
            }
            term3 += 100.0 * rate / m / pk;
        }
        let term4 = 100.0 * rate / m * anl;
        term1 + term2 + term3 - term4
    };

    if price.is_finite() {
        Ok(price)
    } else {
        Err(OddBondEvalError::Domain(WorksheetErrorCode::Num))
    }
}

pub fn oddfprice_kernel(
    settlement: f64,
    maturity: f64,
    issue: f64,
    first_coupon: f64,
    rate: f64,
    yld: f64,
    redemption: f64,
    frequency: f64,
    basis: Option<f64>,
) -> Result<f64, OddBondEvalError> {
    validate_positive_inputs(&[
        settlement,
        maturity,
        issue,
        first_coupon,
        rate,
        yld,
        redemption,
        frequency,
    ])?;
    if rate < 0.0 || yld < 0.0 || redemption <= 0.0 {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    let settlement = parse_date_serial(settlement)?;
    let maturity = parse_date_serial(maturity)?;
    let issue = parse_date_serial(issue)?;
    let first_coupon = parse_date_serial(first_coupon)?;
    let frequency_arg = frequency;
    let frequency = parse_frequency(frequency)?;
    let basis_arg = basis;
    let basis = parse_basis(basis.unwrap_or(0.0))?;
    if !(issue < settlement && settlement < first_coupon && first_coupon < maturity) {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    // Live Excel (dc-j9k.291): the first coupon sits on the coupon schedule back from the maturity
    // (the rule ODDFYIELD already measured), else #NUM!.
    let previous = crate::ported::coupon_family::couppcd_kernel(
        first_coupon as f64,
        maturity as f64,
        frequency_arg,
        basis_arg,
    )
    .map_err(OddBondEvalError::Domain)?;
    if previous != first_coupon as f64 {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    oddfprice_inner(
        settlement,
        maturity,
        issue,
        first_coupon,
        rate,
        yld,
        redemption,
        frequency,
        basis,
    )
}

pub fn oddlprice_kernel(
    settlement: f64,
    maturity: f64,
    last_interest: f64,
    rate: f64,
    yld: f64,
    redemption: f64,
    frequency: f64,
    basis: Option<f64>,
) -> Result<f64, OddBondEvalError> {
    validate_positive_inputs(&[
        settlement,
        maturity,
        last_interest,
        rate,
        yld,
        redemption,
        frequency,
    ])?;
    if rate < 0.0 || yld < 0.0 || redemption <= 0.0 {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    let settlement = parse_date_serial(settlement)?;
    let maturity = parse_date_serial(maturity)?;
    let last_interest = parse_date_serial(last_interest)?;
    let frequency = parse_frequency(frequency)?;
    let basis = parse_basis(basis.unwrap_or(0.0))?;
    if !(last_interest < settlement && settlement < maturity) {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }

    let months_per_coupon = 12 / frequency;
    let mut nc = 0i64;
    let mut cursor = last_interest;
    while cursor < maturity {
        cursor = add_months_clamped(cursor, months_per_coupon)
            .ok_or(OddBondEvalError::Domain(WorksheetErrorCode::Num))?;
        nc += 1;
    }
    let mut early_coupon = last_interest;
    let mut dcnl = 0.0;
    let mut anl = 0.0;
    let mut dscnl = 0.0;
    for index in 1..=nc {
        let late_coupon = add_months_clamped(early_coupon, months_per_coupon)
            .ok_or(OddBondEvalError::Domain(WorksheetErrorCode::Num))?;
        let nl = day_count_non_negative_with_us_hack(early_coupon, late_coupon, basis)?;
        if nl <= 0.0 {
            return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
        }
        let dci = if index < nc {
            nl
        } else {
            day_count_non_negative_with_us_hack(early_coupon, maturity, basis)?
        };
        let a = if late_coupon < settlement {
            dci
        } else if early_coupon < settlement {
            day_count_non_negative(early_coupon, settlement, basis)?
        } else {
            0.0
        };
        let start_date = settlement.max(early_coupon);
        let end_date = maturity.min(late_coupon);
        let dsc = day_count_non_negative(start_date, end_date, basis)?;
        early_coupon = late_coupon;
        dcnl += dci / nl;
        anl += a / nl;
        dscnl += dsc / nl;
    }
    if dscnl <= 0.0 {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    let x = 100.0 * rate / frequency as f64;
    let term1 = dcnl * x + redemption;
    let term2 = dscnl * yld / frequency as f64 + 1.0;
    let term3 = anl * x;
    let price = term1 / term2 - term3;
    if price.is_finite() {
        Ok(price)
    } else {
        Err(OddBondEvalError::Domain(WorksheetErrorCode::Num))
    }
}

fn oddfyield_years_to_maturity(
    settlement: i64,
    maturity: i64,
    basis: DayCountBasis,
) -> Result<f64, OddBondEvalError> {
    let (sy, sm, sd) = ymd(settlement)?;
    let (my, mm, md) = ymd(maturity)?;
    let whole_years = (my - sy) - i64::from(mm < sm || (mm == sm && md < sd));
    let shifted_start =
        excel_serial_from_ymd(WorkbookDateSystem::System1900, sy + whole_years, sm, 1)
            .ok_or(OddBondEvalError::Domain(WorksheetErrorCode::Num))? as i64
            + (sd - 1);
    let frac = match basis {
        DayCountBasis::Us30_360 => days360_us_accrued(shifted_start, maturity)? / 360.0,
        DayCountBasis::ActualActual => (maturity - shifted_start) as f64 / days_in_year(my),
        DayCountBasis::Actual360 => (maturity - shifted_start) as f64 / 360.0,
        DayCountBasis::Actual365 => (maturity - shifted_start) as f64 / 365.0,
        DayCountBasis::European30_360 => days360_eu(shifted_start, maturity)? / 360.0,
    };
    Ok(whole_years as f64 + frac)
}

pub fn oddfyield_kernel(
    settlement: f64,
    maturity: f64,
    issue: f64,
    first_coupon: f64,
    rate: f64,
    pr: f64,
    redemption: f64,
    frequency: f64,
    basis: Option<f64>,
) -> Result<f64, OddBondEvalError> {
    // Numeric date admission belongs to ODDFYIELD before price inversion.
    // Excel 20527 gives #NUM! for zero serials, and requires first_coupon to be
    // on maturity's schedule. All 820 independent date-grid calls agree with
    // the public COUPPCD test, including clamped and month-end dates.
    let date = |value| {
        parse_date_serial(value).map_err(|_| OddBondEvalError::Domain(WorksheetErrorCode::Num))
    };
    let settlement_serial = date(settlement)?;
    let maturity_serial = date(maturity)?;
    let issue_serial = date(issue)?;
    let first_coupon_serial = date(first_coupon)?;
    if !(issue_serial < settlement_serial
        && settlement_serial < first_coupon_serial
        && first_coupon_serial < maturity_serial)
    {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    let previous = crate::ported::coupon_family::couppcd_kernel(
        first_coupon_serial as f64,
        maturity_serial as f64,
        frequency,
        basis,
    )
    .map_err(OddBondEvalError::Domain)?;
    if previous != first_coupon_serial as f64 {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    if pr <= 0.0 || !pr.is_finite() {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    validate_positive_inputs(&[rate, redemption, frequency])?;
    if rate < 0.0 || redemption <= 0.0 {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    let freq = parse_frequency(frequency)?;
    let basis_enum = parse_basis(basis.unwrap_or(0.0))?;

    // Excel's ODDFYIELD solver (dc-j9k.40, measured on Excel 16.0.20527):
    // 1. Par-100 seed `y_0` from schedule-free years-to-maturity `T` (`whole_years + frac(S_shifted, M)`
    //    with `DATE(sy + whole_years, sm, sd)` rollover and `d = pr - 100.0` hardcoded).
    // 2. Analytic Newton iteration on ODDFPRICE for up to 100 iterations using the regular-coupon
    //    derivative (`n_s = COUPNUM(S, M, freq)`, `off = day_count_accrued(S, F, basis) / E`,
    //    omitting the first-period `DFC/E` or `DCNL` factor), stopping when `|step| <= 1e-10`
    //    and publishing the un-updated iterate `y` (or `y_100` after 100 iterations).
    let f = freq as f64;
    let tau = f * oddfyield_years_to_maturity(settlement_serial, maturity_serial, basis_enum)?;
    let d = pr - 100.0;
    let tc = (tau * rate * 100.0) / f;
    let den = (tau * 100.0 + (d * tau) / 2.0) + (d * f) / 4.0;
    let mut y = f * ((tc - d) / den);
    if !y.is_finite() {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }

    let e = coup_days(settlement_serial, first_coupon_serial, freq, basis_enum)?;
    if e <= 0.0 {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    let n_s = number_of_coupons(settlement_serial, maturity_serial, freq)? as i64;
    let dsc_dir = day_count_accrued(settlement_serial, first_coupon_serial, basis_enum)?;
    let off = dsc_dir / e;
    let cp = (100.0 * rate) / f;

    for iter in 0..100u32 {
        let base = 1.0 + y / f;
        if !(base > 0.0) || !base.is_finite() {
            return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
        }
        let price = oddfprice_inner(
            settlement_serial,
            maturity_serial,
            issue_serial,
            first_coupon_serial,
            rate,
            y,
            redemption,
            freq,
            basis_enum,
        )?;
        let mut slope = 0.0_f64;
        for k in 0..n_s {
            let ex = off + k as f64;
            let disc1 = excel_bond_pow(base, ex + 1.0);
            if !disc1.is_finite() {
                return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
            }
            slope += (ex * cp) / disc1;
        }
        let rex = off + (n_s - 1) as f64;
        let r_disc1 = excel_bond_pow(base, rex + 1.0);
        if !r_disc1.is_finite() {
            return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
        }
        slope += (rex * redemption) / r_disc1;
        slope = -slope / f;
        let step = (price - pr) / slope;
        if !step.is_finite() {
            return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
        }
        if step.abs() <= 1e-10 {
            return if iter == 99 {
                Err(OddBondEvalError::Domain(WorksheetErrorCode::Num))
            } else {
                Ok(y)
            };
        }
        y -= step;
    }
    if !y.is_finite() {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    Ok(y)
}

pub fn oddlyield_kernel(
    settlement: f64,
    maturity: f64,
    last_interest: f64,
    rate: f64,
    pr: f64,
    redemption: f64,
    frequency: f64,
    basis: Option<f64>,
) -> Result<f64, OddBondEvalError> {
    validate_positive_inputs(&[
        settlement,
        maturity,
        last_interest,
        rate,
        pr,
        redemption,
        frequency,
    ])?;
    if rate < 0.0 || pr <= 0.0 || redemption <= 0.0 {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    let settlement = parse_date_serial(settlement)?;
    let maturity = parse_date_serial(maturity)?;
    let last_interest = parse_date_serial(last_interest)?;
    let frequency = parse_frequency(frequency)?;
    let basis = parse_basis(basis.unwrap_or(0.0))?;
    if !(last_interest < settlement && settlement < maturity) {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    let months_per_coupon = 12 / frequency;
    let mut nc = 0i64;
    let mut cursor = last_interest;
    while cursor < maturity {
        cursor = add_months_clamped(cursor, months_per_coupon)
            .ok_or(OddBondEvalError::Domain(WorksheetErrorCode::Num))?;
        nc += 1;
    }
    let mut early_coupon = last_interest;
    let mut dcnl = 0.0;
    let mut anl = 0.0;
    let mut dscnl = 0.0;
    for index in 1..=nc {
        let late_coupon = add_months_clamped(early_coupon, months_per_coupon)
            .ok_or(OddBondEvalError::Domain(WorksheetErrorCode::Num))?;
        let nl = day_count_non_negative_with_us_hack(early_coupon, late_coupon, basis)?;
        if nl <= 0.0 {
            return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
        }
        let dci = if index < nc {
            nl
        } else {
            day_count_non_negative_with_us_hack(early_coupon, maturity, basis)?
        };
        let a = if late_coupon < settlement {
            dci
        } else if early_coupon < settlement {
            day_count_non_negative(early_coupon, settlement, basis)?
        } else {
            0.0
        };
        let start_date = settlement.max(early_coupon);
        let end_date = maturity.min(late_coupon);
        let dsc = day_count_non_negative(start_date, end_date, basis)?;
        early_coupon = late_coupon;
        dcnl += dci / nl;
        anl += a / nl;
        dscnl += dsc / nl;
    }
    if dscnl <= 0.0 {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    let x = 100.0 * rate / frequency as f64;
    let term1 = dcnl * x + redemption;
    let term2 = anl * x + pr;
    let term3 = frequency as f64 / dscnl;
    Ok((term1 - term2) / term2 * term3)
}

pub fn map_odd_bond_error_to_ws(error: &OddBondEvalError) -> WorksheetErrorCode {
    match error {
        OddBondEvalError::ArityMismatch { .. } => WorksheetErrorCode::Value,
        OddBondEvalError::Domain(code) => *code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::value::{CalcArray, CalcValue, CoreValue, ExcelText};

    fn serial(year: i64, month: i64, day: i64) -> f64 {
        excel_serial_from_ymd(WorkbookDateSystem::System1900, year, month, day).unwrap()
    }

    fn assert_close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual}"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn days360_us_rolls_end_day_31_into_next_month() {
        assert_eq!(
            days360_us(serial(2023, 11, 15) as i64, serial(2024, 1, 31) as i64),
            Ok(76.0)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn days360_us_start_at_least_30_collapses_end_in_place() {
        assert_eq!(
            days360_us(serial(2024, 1, 30) as i64, serial(2024, 3, 31) as i64),
            Ok(60.0)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn oddfprice_matches_bounded_microsoft_lane() {
        let got = oddfprice_kernel(
            serial(2008, 11, 11),
            serial(2021, 3, 1),
            serial(2008, 10, 15),
            serial(2009, 3, 1),
            0.0785,
            0.0625,
            100.0,
            2.0,
            Some(1.0),
        )
        .expect("oddfprice should succeed");
        assert_close(got, 113.597_717_474_079, 1.0e-9);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn oddfyield_inverts_bounded_price_lane() {
        let got = oddfyield_kernel(
            serial(2008, 11, 11),
            serial(2021, 3, 1),
            serial(2008, 10, 15),
            serial(2009, 3, 1),
            0.0785,
            113.597_717_474_079,
            100.0,
            2.0,
            Some(1.0),
        )
        .expect("oddfyield should succeed");
        assert_close(got, 0.0625, 1.0e-10);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn oddlprice_matches_bounded_example_lane() {
        let got = oddlprice_kernel(
            serial(2008, 2, 7),
            serial(2008, 6, 15),
            serial(2007, 10, 15),
            0.0375,
            0.0405,
            100.0,
            2.0,
            Some(0.0),
        )
        .expect("oddlprice should succeed");
        assert_close(got, 99.878_286_014_721_34, 1.0e-10);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn oddlyield_inverts_bounded_price_lane() {
        let got = oddlyield_kernel(
            serial(2008, 2, 7),
            serial(2008, 6, 15),
            serial(2007, 10, 15),
            0.0375,
            99.878_286_014_721_34,
            100.0,
            2.0,
            Some(0.0),
        )
        .expect("oddlyield should succeed");
        assert_close(got, 0.0405, 1.0e-10);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn long_odd_first_coupon_now_computes() {
        // BUG-FUNC-032: a long (multi-quasi-coupon-period) odd first coupon is valid
        // in Excel; the kernel no longer rejects it with #NUM!. The witness (basis 0,
        // 2-period odd first) matches live Excel 16.0 b20026 (98.51287878857208) to
        // ~1 ULP. (Actual-day bases remain a tracked numeric residual.)
        let price = oddfprice_kernel(
            serial(2020, 7, 1),
            serial(2022, 1, 1),
            serial(2020, 1, 1),
            serial(2021, 1, 1),
            0.05,
            0.06,
            100.0,
            2.0,
            Some(0.0),
        )
        .expect("long odd first coupon computes");
        assert!((price - 98.512_878_788_572_08).abs() < 1e-9, "got {price}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn oddfprice_actual_bases_bit_exact_vs_excel() {
        // BUG-FUNC-032 / G6 three-way: a long odd first coupon over the actual-day
        // bases (1=act/act, 2=act/360, 3=act/365) was materially wrong under the old
        // single-period-length closed form (10^10-10^12 ULP). The faithful oddFPrice
        // port sums per-period day-count fractions and is now bit-exact with live
        // Excel 16.0 b20026 and the F# reference (all_bit_exact in the G6 ledger).
        // Values pinned by exact bits.
        let price = |basis: f64| {
            oddfprice_kernel(
                serial(2020, 9, 15),
                serial(2022, 1, 1),
                serial(2020, 1, 1),
                serial(2021, 1, 1),
                0.05,
                0.06,
                100.0,
                2.0,
                Some(basis),
            )
            .expect("oddfprice should succeed")
        };
        assert_eq!(price(1.0).to_bits(), 98.721_102_743_891_15_f64.to_bits());
        assert_eq!(price(2.0).to_bits(), 98.658_251_302_161_82_f64.to_bits());
        assert_eq!(price(3.0).to_bits(), 98.698_153_923_488_63_f64.to_bits());
    }

    // W109 (2026-07-14): on a US 30/360 long odd-first-coupon bond the quasi-period
    // exponents are integers, so the discount factor hits the C-runtime `pow` binexp path
    // (same as PRICE, G6-03b). Before the `excel_bond_pow` fix these drifted 1-2 ULP; now
    // bit-exact vs live Excel (build 20131). settle 44013, mat 44562, issue 43831,
    // first_coupon 44197, rate 5%, red 100, freq 2, basis 0.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn oddfprice_us30360_integer_exponents_bit_exact_vs_excel() {
        let ladder: [(f64, u64); 5] = [
            (0.04, 0x40595925a26f8cfb),
            (0.05, 0x4058fc18f9c18f9c),
            (0.06, 0x4058a0d3018deeab),
            (0.08, 0x4057ef6ffa682bcf),
            (0.10, 0x405744a948d6bab2),
        ];
        for (yld, exp) in ladder {
            let got = oddfprice_kernel(
                44013.0,
                44562.0,
                43831.0,
                44197.0,
                0.05,
                yld,
                100.0,
                2.0,
                Some(0.0),
            )
            .expect("oddfprice should succeed");
            assert_eq!(
                got.to_bits(),
                exp,
                "ODDFPRICE yld={yld}: {:#018x} != {exp:#018x}",
                got.to_bits()
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn oddfyield_corpus_bonds_match_excel_bits() {
        // Live Excel 16.0.20527 oracle corpus pins (dc-j9k.40):
        // Base bond: settle=39763 (2008-11-11), mat=44256 (2021-03-01),
        // issue=39736 (2008-10-15), first_coupon=39873 (2009-03-01), pr=84.5, red=100, freq=2, basis=0.
        let cases: [(&str, f64, f64, f64, u64); 6] = [
            ("base r=0.0575", 39763.0, 39736.0, 0.0575, 0x3fb3c65d22ec15b6),
            ("r23!D658 S-1", 39762.0, 39736.0, 0.0575, 0x3fb3c62fc291504a),
            ("r23!D708 I-1", 39763.0, 39735.0, 0.0575, 0x3fb3c6546aed9963),
            (
                "r24!D3474 r+1ulp",
                39763.0,
                39736.0,
                f64::from_bits(0x3fad70a3d70a3d72),
                0x3fb3c65d22ec15b9,
            ),
            ("r24!D3475 r=0", 39763.0, 39736.0, 0.0, 0x3f8c20381118fdf7),
            ("r24!D3477 r=1", 39763.0, 39736.0, 1.0, 0x3ff301bf41778b14),
        ];
        for (label, s, i, rate, expected_bits) in cases {
            let got = oddfyield_kernel(s, 44256.0, i, 39873.0, rate, 84.5, 100.0, 2.0, Some(0.0))
                .expect("oddfyield should succeed");
            assert_eq!(
                got.to_bits(),
                expected_bits,
                "ODDFYIELD {label}: got {:#018x} ({got}), expected {expected_bits:#018x} ({})",
                got.to_bits(),
                f64::from_bits(expected_bits)
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn oddfyield_negative_yield_and_iteration_budget() {
        // Excel 16.0.20527 allows negative yields (pr above ODDFPRICE(0)) down to y > -freq,
        // and publishes y_100 when the 100-iteration budget finishes without |step| <= 1e-10.
        let neg = oddfyield_kernel(
            serial(2020, 3, 1),
            serial(2025, 7, 1),
            serial(2020, 2, 1),
            serial(2020, 7, 1),
            0.06,
            500.0,
            100.0,
            2.0,
            Some(0.0),
        )
        .expect("negative yield should succeed");
        assert_close(neg, -0.253_060_006_882_943_1, 1.0e-15);

        // Long odd-first bond (Nq=19, nF=1, rate=2.0) that hits the 100-iteration budget:
        let pr = oddfprice_kernel(
            serial(2010, 2, 1),
            serial(2020, 7, 1),
            serial(2010, 1, 1),
            serial(2020, 1, 1),
            2.0,
            0.1,
            100.0,
            2.0,
            Some(0.0),
        )
        .expect("oddfprice should succeed");
        let y100 = oddfyield_kernel(
            serial(2010, 2, 1),
            serial(2020, 7, 1),
            serial(2010, 1, 1),
            serial(2020, 1, 1),
            2.0,
            pr,
            100.0,
            2.0,
            Some(0.0),
        )
        .expect("100-iteration budget publishes y_100");
        assert_close(y100, -0.708_592_655_210_270_3, 1.0e-14);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn oddfyield_diverse_short_long_bases_match_excel_bits() {
        // Subset of the 250/250 exact frozen-prediction holdout against Excel 16.0.20527
        // (short & long odd first periods, all 5 bases, frequencies 1/2/4, varied redemptions
        // and rates, positive and negative yields).
        let cases: [(&str, f64, f64, f64, f64, f64, f64, f64, f64, f64, u64); 10] = [
            ("short_b0_f4", 49024.0, 49796.0, 48993.0, 49065.0, 0.085, 162.3938, 125.0, 4.0, 0.0, 0xbfb03576e27923fc),
            ("long_b0_f4", 49423.0, 50190.0, 49308.0, 49551.0, 0.085, 126.5634, 100.0, 4.0, 0.0, 0xbfa271348aefd4bf),
            ("short_b1_f4", 46613.0, 47635.0, 46612.0, 46631.0, 0.015, 74.7736, 100.0, 4.0, 1.0, 0x3fbf7548b227ac60),
            ("long_b1_f1", 44334.0, 48790.0, 43411.0, 44407.0, 0.0, 79.9908, 100.0, 1.0, 1.0, 0x3f92e95e0964692b),
            ("short_b2_f4", 47391.0, 48791.0, 47357.0, 47422.0, 0.085, 47.2902, 80.0, 4.0, 2.0, 0x3fd23b53825dc9e1),
            ("long_b2_f1", 44096.0, 45288.0, 43553.0, 44558.0, 0.015, 31.1121, 80.0, 1.0, 2.0, 0x3fd709f3e4d5a6a8),
            ("short_b3_f1", 44321.0, 45458.0, 44234.0, 44362.0, 0.015, 35.2708, 100.0, 1.0, 3.0, 0x3fdb764e8beedcaf),
            ("long_b3_f1", 47488.0, 47967.0, 47181.0, 47602.0, 0.015, 104.0951, 100.0, 1.0, 3.0, 0xbf8ffa815997cf46),
            ("short_b4_f2", 43293.0, 46218.0, 43230.0, 43296.0, 0.0, 237.339, 200.0, 2.0, 4.0, 0xbf95c54c2a8a489f),
            ("long_b4_f2", 47635.0, 49337.0, 47545.0, 48057.0, 0.015, 103.2134, 100.0, 2.0, 4.0, 0x3f80442c6f749823),
        ];
        for (label, s, m, i, f, r, pr, red, freq, basis, expected_bits) in cases {
            let got = oddfyield_kernel(s, m, i, f, r, pr, red, freq, Some(basis))
                .expect("oddfyield should succeed");
            assert_eq!(
                got.to_bits(),
                expected_bits,
                "ODDFYIELD {label}: got {:#018x}, expected {expected_bits:#018x}",
                got.to_bits()
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn invalid_frequency_and_basis_are_rejected() {
        assert_eq!(
            oddlprice_kernel(
                serial(2008, 2, 7),
                serial(2008, 6, 15),
                serial(2007, 10, 15),
                0.0375,
                0.0405,
                100.0,
                3.0,
                Some(0.0),
            ),
            Err(OddBondEvalError::Domain(WorksheetErrorCode::Num))
        );
        assert_eq!(
            oddlprice_kernel(
                serial(2008, 2, 7),
                serial(2008, 6, 15),
                serial(2007, 10, 15),
                0.0375,
                0.0405,
                100.0,
                2.0,
                Some(9.0),
            ),
            Err(OddBondEvalError::Domain(WorksheetErrorCode::Num))
        );
    }

    /// dc-j9k.98 (cb26): discount factor overflow to +inf returns #NUM! in ODDFPRICE/ODDFYIELD.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn oddfprice_and_oddfyield_pow_overflow_returns_num() {
        let num_err = Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
        // Short odd first coupon with huge yield causing pow overflow
        assert_eq!(
            oddfprice_kernel(
                44000.0,
                60000.0,
                43950.0,
                44100.0,
                0.05,
                8000.0,
                100.0,
                2.0,
                Some(0.0),
            ),
            num_err
        );
    }

    /// dc-j9k.291 (live Excel 16.0.20625): ODDFPRICE needs the first coupon on the maturity's
    /// coupon schedule and before the maturity, as ODDFYIELD does.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn oddfprice_first_coupon_must_sit_on_the_maturity_schedule() {
        let price = |first_coupon: f64, basis: Option<f64>| {
            oddfprice_kernel(38353.0, 40179.0, 37987.0, first_coupon, 0.05, 0.06, 100.0, 2.0, basis)
        };
        // 2005-07-01 is on the schedule back from 2010-01-01.
        assert_eq!(price(38534.0, None).map(f64::to_bits), Ok(0x4057e5b68f16ee76));
        assert_eq!(price(38534.0, Some(1.0)).map(f64::to_bits), Ok(0x4057e5b68f16ee76));
        let num_err = Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
        assert_eq!(price(38565.0, None), num_err); // 2005-08-01
        assert_eq!(price(38535.0, None), num_err); // 2005-07-02
        assert_eq!(price(40179.0, None), num_err); // the maturity itself
        // The cluster's representative, off schedule: #NUM! where the engine priced it.
        assert_eq!(
            oddfprice_kernel(
                f64::from_bits(0x40e2c8e000800000),
                f64::from_bits(0x40e9c99fffffffe0),
                f64::from_bits(0x40d4d07ffc000000),
                f64::from_bits(0x40e54b6000000010),
                f64::from_bits(0x4077afe000000000),
                f64::from_bits(0x4058800000100000),
                f64::from_bits(0x4074a00000000000),
                f64::from_bits(0x4010000000000000),
                None,
            ),
            num_err
        );
    }

    /// dc-j9k.304 (live Excel 16.0.20625): ODDLPRICE has no limit on the number of coupons to the maturity.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn oddlprice_prices_a_maturity_thousands_of_coupons_away() {
        let price = oddlprice_kernel(
            f64::from_bits(0x40e1ef0000000000),
            f64::from_bits(0x41378d9200000000),
            f64::from_bits(0x40e1d5c000000000),
            f64::from_bits(0x3ff0000000000000),
            f64::from_bits(0x3fe300930c0ac769),
            f64::from_bits(0x412592da7bd93208),
            f64::from_bits(0x4000000000000000),
            None,
        );
        assert_eq!(price.map(f64::to_bits), Ok(0x407913d505265e50));
        // dc-j9k.305 (basis 4). dc-j9k.306's representative still differs from Excel (staging, see its bead).
        let args = |words: [u64; 6], frequency: f64, basis: Option<f64>| {
            let f = f64::from_bits;
            oddlprice_kernel(f(words[0]), f(words[1]), f(words[2]), f(words[3]), f(words[4]), f(words[5]), frequency, basis)
        };
        assert_eq!(
            args(
                [0x412c3a8400000000, 0x413b75fd66744f1f, 0x4103ac6000000000, 0x3fbb5602e93fdff8, 0x3fd939bbb39c49f0, 0x41138370310dd509],
                1.0,
                Some(4.0)
            )
            .map(f64::to_bits),
            Ok(0xc0d56d82da7cc539)
        );
    }

    /// Live Excel 16.0.20625 (Touchstone, dc-shqi1): ODDLYIELD has no 64-coupon cap.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn oddlyield_computes_past_64_coupons() {
        assert_eq!(
            oddlyield_kernel(36526.0, 43831.0, 32874.0, 0.05, 100.0, 100.0, 4.0, Some(0.0))
                .map(f64::to_bits),
            Ok(0x3fa1111111111111)
        );
        assert_eq!(
            oddlyield_kernel(40000.0, 40000.0 + 365.0 * 50.0, 36000.0, 0.05, 95.0, 100.0, 4.0, Some(0.0))
                .map(f64::to_bits),
            Ok(0x3fa16ff23a633683)
        );
    }

    /// dc-j9k.99 (cb26 live Excel 16.0.20625.20108, PZ-ODD): ODDFYIELD shares the cIter--
    /// off-by-one where convergence on the 100th evaluation (iter == 99) returns #NUM!,
    /// while >100 non-converged steps publish y_100.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn oddfyield_iter_99_returns_num_while_over_100_publishes_y100() {
        let num_err = Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
        let (s, m, i, fc) = (
            f64::from_bits(0x40e575c000000000),
            f64::from_bits(0x40ebbb8000000000),
            f64::from_bits(0x40e56f8000000000),
            f64::from_bits(0x40e57da000000000),
        );
        // PZO-02, PZO-05: iter == 99 -> #NUM!
        assert_eq!(
            oddfyield_kernel(s, m, i, fc, 0.0, f64::from_bits(0x40e330ca85242bef), 100.0, 2.0, Some(0.0)),
            num_err
        );
        assert_eq!(
            oddfyield_kernel(
                s, m, i, fc,
                f64::from_bits(0x3f947ae147ae147b),
                f64::from_bits(0x40ddf6baf98cde8e),
                100.0, 1.0, Some(1.0),
            ),
            num_err
        );
        // PZO-01, PZO-03, PZO-04, PZO-06: <=99 and >100 -> exact finite bits
        assert_eq!(
            oddfyield_kernel(s, m, i, fc, 0.0, f64::from_bits(0x40e210ccc5b900c0), 100.0, 2.0, Some(0.0))
                .unwrap()
                .to_bits(),
            0xbfc4a407c2d4f2f4
        );
        assert_eq!(
            oddfyield_kernel(s, m, i, fc, 0.0, f64::from_bits(0x40e42ee6a682bb72), 100.0, 2.0, Some(0.0))
                .unwrap()
                .to_bits(),
            0xbfd59f49a5b978c0
        );
        assert_eq!(
            oddfyield_kernel(
                s, m, i, fc,
                f64::from_bits(0x3f947ae147ae147b),
                f64::from_bits(0x40dc88cfe837e6ef),
                100.0, 1.0, Some(1.0),
            )
            .unwrap()
            .to_bits(),
            0xbfc2aee73d82fab4
        );
        assert_eq!(
            oddfyield_kernel(
                s, m, i, fc,
                f64::from_bits(0x3f947ae147ae147b),
                f64::from_bits(0x40de3a7e35a615e4),
                100.0, 1.0, Some(1.0),
            )
            .unwrap()
            .to_bits(),
            0xbfc3958fdcff839d
        );
    }
}
