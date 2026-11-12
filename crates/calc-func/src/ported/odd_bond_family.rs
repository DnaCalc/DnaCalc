//! Odd-first and odd-last period bond kernels (ODDFPRICE, ODDFYIELD, ODDLPRICE, ODDLYIELD).
//!
//! Parts of this file are a translation, changed, of ExcelFinancialFunctions by Luca Bolognese
//! (https://github.com/fsprojects/ExcelFinancialFunctions, `oddbonds.fs` and `daycountbasis.fs`;
//! Copyright 2010-2013, Apache License 2.0): `oddfprice_kernel` (`oddFPrice`) and the helpers named
//! after their F# counterparts in their doc comments (`changeMonth`, `findCouponDates`,
//! `numberOfCoupons`, `CoupDays`, `coupNumber`, `aggrBetween`). They were rewritten in Rust and
//! changed where Excel measurement required it. The licence text is in
//! `deploy/notices/ported.txt`. The rest of the file is our own work.

use crate::locale::{
    DateSystem as WorkbookDateSystem, excel_serial_from_ymd, ymd_from_excel_serial,
};
use crate::ported::excel_numeric::crt_pow;
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

fn parse_frequency(value: f64) -> Result<i64, OddBondEvalError> {
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
/// exponentiation for non-negative integer exponents, `powf` otherwise. Identical to
/// `bond_core_family::excel_bond_pow` (W109, held-out validated 25/25 on PRICE). For
/// fractional exponents this equals `base.powf(exp)`, so only integer-exponent (e.g.
/// on-coupon / whole-quasi-period) odd bonds change.
fn excel_bond_pow(base: f64, exp: f64) -> f64 {
    if exp >= 0.0 && exp < 1024.0 && exp.fract() == 0.0 {
        let mut n = exp as u64;
        let mut r = 1.0_f64;
        let mut b = base;
        while n > 0 {
            if n & 1 == 1 {
                r *= b;
            }
            n >>= 1;
            if n > 0 {
                b *= b;
            }
        }
        r
    } else {
        crt_pow(base, exp)
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
    let frequency = parse_frequency(frequency)?;
    let basis = parse_basis(basis.unwrap_or(0.0))?;
    if !(issue < settlement && settlement < first_coupon && first_coupon <= maturity) {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }

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
    if x <= 0.0 {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }

    let price = if dfc < e {
        // Short odd first coupon.
        let n = number_of_coupons(settlement, maturity, frequency)?;
        let dsc = day_count_non_negative(settlement, first_coupon, basis)?;
        let a = day_count_non_negative(issue, settlement, basis)?;
        let y = dsc / e;
        let term1 = redemption / excel_bond_pow(x, n - 1.0 + y);
        let term2 = 100.0 * rate / m * dfc / e / excel_bond_pow(x, y);
        let mut term3 = 0.0;
        for index in aggr_indices(2, n as i64) {
            term3 += 100.0 * rate / m / excel_bond_pow(x, index as f64 - 1.0 + y);
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
        let term1 = redemption / excel_bond_pow(x, y + nq + n);
        let term2 = 100.0 * rate / m * dcnl / excel_bond_pow(x, nq + y);
        let mut term3 = 0.0;
        for index in aggr_indices(1, n as i64) {
            term3 += 100.0 * rate / m / excel_bond_pow(x, index as f64 + nq + y);
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
        if nc > 64 {
            return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
        }
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

fn solve_yield_bisection(
    target_price: f64,
    price_at_yield: impl Fn(f64) -> Result<f64, OddBondEvalError>,
) -> Result<f64, OddBondEvalError> {
    if target_price <= 0.0 || !target_price.is_finite() {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    let low = 0.0;
    let price_low = price_at_yield(low)?;
    if target_price > price_low {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    let mut hi = 0.1;
    let mut price_hi = price_at_yield(hi)?;
    for _ in 0..32 {
        if price_hi <= target_price {
            break;
        }
        hi *= 2.0;
        price_hi = price_at_yield(hi)?;
    }
    if price_hi > target_price {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }

    // Bisection to adjacent binary64 bounds (full convergence, dc-ifc: Excel's ODDFYIELD
    // converges past the former 1e-12 price tolerance), then the bound with the smaller residual.
    let mut lo = low;
    let mut hi_mut = hi;
    for _ in 0..2200 {
        let mid = (lo + hi_mut) / 2.0;
        if mid <= lo || mid >= hi_mut {
            break;
        }
        let price_mid = price_at_yield(mid)?;
        if price_mid == target_price {
            return Ok(mid);
        }
        if price_mid > target_price {
            lo = mid;
        } else {
            hi_mut = mid;
        }
    }
    let residual = |y: f64| price_at_yield(y).map(|p| (p - target_price).abs());
    Ok(match (residual(lo), residual(hi_mut)) {
        (Ok(a), Ok(b)) if a < b => lo,
        (Ok(_), Err(_)) => lo,
        _ => hi_mut,
    })
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
    // Excel20527 gives NUM for zero serials, and requires first_coupon to be
    // on maturity's schedule. All820 independent date-grid calls agree with
    // the public COUPPCD test, including clamped and month-end dates.
    let date = |value| {
        parse_date_serial(value).map_err(|_| OddBondEvalError::Domain(WorksheetErrorCode::Num))
    };
    let settlement = date(settlement)? as f64;
    let maturity = date(maturity)? as f64;
    let issue = date(issue)? as f64;
    let first_coupon = date(first_coupon)? as f64;
    if !(issue < settlement && settlement < first_coupon && first_coupon < maturity) {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    let previous =
        crate::ported::coupon_family::couppcd_kernel(first_coupon, maturity, frequency, basis)
            .map_err(OddBondEvalError::Domain)?;
    if previous != first_coupon {
        return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
    }
    solve_yield_bisection(pr, |y| {
        oddfprice_kernel(
            settlement,
            maturity,
            issue,
            first_coupon,
            rate,
            y,
            redemption,
            frequency,
            basis,
        )
    })
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
        if nc > 64 {
            return Err(OddBondEvalError::Domain(WorksheetErrorCode::Num));
        }
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
}
