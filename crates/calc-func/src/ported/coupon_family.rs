use crate::locale::{DateSystem as WorkbookDateSystem, excel_serial_from_ymd, ymd_from_excel_serial};
use crate::value::CalcValue;
use crate::value::WorksheetErrorCode;

#[derive(Debug, Clone, PartialEq)]
pub enum CouponEvalError {
    ArityMismatch {
        expected_min: usize,
        expected_max: usize,
        actual: usize,
    },
    Domain(WorksheetErrorCode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CouponBasis {
    Us30_360,
    ActualActual,
    Actual360,
    Actual365,
    European30_360,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CouponContext {
    settlement: i64,
    maturity: i64,
    frequency: i64,
    basis: CouponBasis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CouponPeriod {
    previous: i64,
    next: i64,
    remaining_coupons: i64,
}

fn max_excel_serial() -> i64 {
    excel_serial_from_ymd(WorkbookDateSystem::System1900, 9999, 12, 31).unwrap() as i64
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
            if leap { 29 } else { 28 }
        }
        _ => 30,
    }
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn excel_serial_from_ymd_unbounded_1900(year: i64, month: i64, day: i64) -> i64 {
    if year == 1900 && month == 2 && day == 29 {
        return 60;
    }
    let base = days_from_civil(1899, 12, 31);
    let days = days_from_civil(year, month, 1) - base + (day - 1);
    if days >= 60 { days + 1 } else { days }
}

#[derive(Clone, Copy)]
enum CalendarDayAdjustment {
    European,
    CouponPeriod,
    CouponAccrued,
}

fn calendar_days360(
    start: i64,
    end: i64,
    adjustment: CalendarDayAdjustment,
) -> Result<f64, WorksheetErrorCode> {
    let (sy, sm, mut sd) = ymd_from_excel_serial(WorkbookDateSystem::System1900, start as f64)
        .ok_or(WorksheetErrorCode::Value)?;
    let (mut ey, mut em, mut ed) = ymd_from_excel_serial(WorkbookDateSystem::System1900, end as f64)
        .ok_or(WorksheetErrorCode::Value)?;
    if matches!(adjustment, CalendarDayAdjustment::CouponAccrued) {
        let start_last_feb = sm == 2 && sd == days_in_month(sy, sm);
        let end_last_feb = em == 2 && ed == days_in_month(ey, em);
        // Coupon accrual adjusts a day-31 end using the original start day, before
        // normalizing a February-end start. The shared DAYS360 convention differs.
        // Fn7 paired COUPDAYBS/COUPDAYSNC controls cover real and fictitious February ends.
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
        return Ok(((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64);
    }
    if sd == 31
        || (matches!(adjustment, CalendarDayAdjustment::CouponPeriod)
            && sm == 2 && sd == days_in_month(sy, sm))
    {
        sd = 30;
    }
    if ed == 31
        || (matches!(adjustment, CalendarDayAdjustment::CouponPeriod)
            && em == 2 && ed == days_in_month(ey, em))
    {
        ed = 30;
    }
    Ok(((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64)
}

fn actual_days(start: i64, end: i64) -> f64 {
    (end - start) as f64
}

fn coupon_day_count(
    start: i64,
    end: i64,
    basis: CouponBasis,
    frequency: i64,
) -> Result<f64, WorksheetErrorCode> {
    match basis {
        CouponBasis::Us30_360 => Ok(360.0 / frequency as f64),
        CouponBasis::ActualActual => Ok(actual_days(start, end)),
        CouponBasis::Actual360 => Ok(360.0 / frequency as f64),
        CouponBasis::Actual365 => Ok(365.0 / frequency as f64),
        CouponBasis::European30_360 => Ok(360.0 / frequency as f64),
    }
}

fn accrued_or_remaining_days(
    start: i64,
    end: i64,
    basis: CouponBasis,
) -> Result<f64, WorksheetErrorCode> {
    match basis {
        CouponBasis::Us30_360 => {
            calendar_days360(start, end, CalendarDayAdjustment::CouponAccrued)
        }
        CouponBasis::ActualActual | CouponBasis::Actual360 | CouponBasis::Actual365 => {
            Ok(actual_days(start, end))
        }
        CouponBasis::European30_360 => {
            calendar_days360(start, end, CalendarDayAdjustment::European)
        }
    }
}

fn parse_basis(value: f64) -> Result<CouponBasis, WorksheetErrorCode> {
    if !value.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    match value.trunc() as i64 {
        0 => Ok(CouponBasis::Us30_360),
        1 => Ok(CouponBasis::ActualActual),
        2 => Ok(CouponBasis::Actual360),
        3 => Ok(CouponBasis::Actual365),
        4 => Ok(CouponBasis::European30_360),
        _ => Err(WorksheetErrorCode::Num),
    }
}

fn parse_frequency(value: f64) -> Result<i64, WorksheetErrorCode> {
    if !value.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    match value.trunc() as i64 {
        1 | 2 | 4 => Ok(value.trunc() as i64),
        _ => Err(WorksheetErrorCode::Num),
    }
}

/// The check of a COUP function's dates on their own: each a date, and settlement before
/// maturity; #NUM! otherwise.
pub(crate) fn check_coupon_dates(settlement: f64, maturity: f64) -> Result<(), WorksheetErrorCode> {
    let settlement = parse_coupon_date(settlement)?;
    let maturity = parse_coupon_date(maturity)?;
    if settlement >= maturity {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(())
}

fn parse_coupon_date(value: f64) -> Result<i64, WorksheetErrorCode> {
    if !value.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    let serial = value.trunc() as i64;
    if serial < 0 || serial > max_excel_serial() {
        return Err(WorksheetErrorCode::Num);
    }
    ymd_from_excel_serial(WorkbookDateSystem::System1900, serial as f64)
        .ok_or(WorksheetErrorCode::Num)?;
    Ok(serial)
}

fn parse_coupon_context(
    settlement: f64,
    maturity: f64,
    frequency: f64,
    basis: Option<f64>,
) -> Result<CouponContext, WorksheetErrorCode> {
    let settlement = parse_coupon_date(settlement)?;
    let maturity = parse_coupon_date(maturity)?;
    let frequency = parse_frequency(frequency)?;
    let basis = parse_basis(basis.unwrap_or(0.0))?;
    if settlement >= maturity {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(CouponContext {
        settlement,
        maturity,
        frequency,
        basis,
    })
}

/// Coupon date `k` periods before maturity, computed from maturity itself rather than by
/// chaining one period at a time: a maturity on the 28th-30th keeps that day of month in every
/// month long enough for it (clamped only in shorter months), and only a month-end maturity
/// gives month-end coupons. Chaining off an already-clamped date drifted to month ends after
/// the first short month (maturity 2046-08-28 quarterly gave a next coupon of 2045-05-31
/// where Excel gives 2045-05-28). Live Excel 20430, W111-5 G8-08.
fn coupon_date_before_maturity(ctx: CouponContext, periods: i64, preserve_month_end: bool) -> Option<i64> {
    let months_per_coupon = 12 / ctx.frequency;
    let (year, month, day) =
        ymd_from_excel_serial(WorkbookDateSystem::System1900, ctx.maturity as f64)?;
    let month_index = year
        .checked_mul(12)?
        .checked_add(month - 1)?
        .checked_sub(periods.checked_mul(months_per_coupon)?)?;
    let target_year = month_index.div_euclid(12);
    let target_month = month_index.rem_euclid(12) + 1;
    let month_end = days_in_month(target_year, target_month);
    let maturity_is_month_end = preserve_month_end && day == days_in_month(year, month);
    let target_day = if maturity_is_month_end {
        month_end
    } else {
        day.min(month_end)
    };
    Some(excel_serial_from_ymd_unbounded_1900(
        target_year,
        target_month,
        target_day,
    ))
}

fn locate_coupon_period(ctx: CouponContext) -> Result<CouponPeriod, WorksheetErrorCode> {
    let mut next = ctx.maturity;
    let mut coupons = 1i64;
    loop {
        let previous =
            coupon_date_before_maturity(ctx, coupons, true).ok_or(WorksheetErrorCode::Num)?;
        if previous <= ctx.settlement {
            return Ok(CouponPeriod {
                previous: previous.max(0),
                next,
                remaining_coupons: coupons,
            });
        }
        next = previous;
        coupons += 1;
    }
}

pub fn coupdaybs_kernel(
    settlement: f64,
    maturity: f64,
    frequency: f64,
    basis: Option<f64>,
) -> Result<f64, WorksheetErrorCode> {
    let ctx = parse_coupon_context(settlement, maturity, frequency, basis)?;
    let period = locate_coupon_period(ctx)?;
    accrued_or_remaining_days(period.previous, ctx.settlement, ctx.basis)
}

pub fn coupdays_kernel(
    settlement: f64,
    maturity: f64,
    frequency: f64,
    basis: Option<f64>,
) -> Result<f64, WorksheetErrorCode> {
    let ctx = parse_coupon_context(settlement, maturity, frequency, basis)?;
    if ctx.basis == CouponBasis::ActualActual {
        // COUPDAYS has its own recorded calendar interval. Walk backwards
        // with ordinary clamped month shifts; a February visit can reduce
        // the day used by every earlier step. At the next coupon, measure
        // one period backwards with month-end preservation. Reusing the
        // anchored COUPNCD/COUPPCD pair changes these integer day counts.
        // Original Excel20131/20430 readings:965/965 Actual/Actual outcomes in the
        // R24 finite Gregorian study. No numerical tolerance enters this rule.
        let mut next = ctx;
        loop {
            let previous = coupon_date_before_maturity(next, 1, false)
                .ok_or(WorksheetErrorCode::Num)?;
            if previous <= ctx.settlement {
                let start = coupon_date_before_maturity(next, 1, true)
                    .ok_or(WorksheetErrorCode::Num)?;
                return Ok(actual_days(start, next.maturity));
            }
            next.maturity = previous;
        }
    }
    let period = locate_coupon_period(ctx)?;
    coupon_day_count(period.previous, period.next, ctx.basis, ctx.frequency)
}

pub fn coupdaysnc_kernel(
    settlement: f64,
    maturity: f64,
    frequency: f64,
    basis: Option<f64>,
) -> Result<f64, WorksheetErrorCode> {
    let ctx = parse_coupon_context(settlement, maturity, frequency, basis)?;
    let period = locate_coupon_period(ctx)?;
    if ctx.basis == CouponBasis::Us30_360 {
        // The whole period normalizes both actual coupon endpoints: day 31 and Gregorian
        // February ends become day 30. It need not equal COUPDAYS' 360/frequency when a
        // non-month-end maturity's day was clamped in February. Subtract the distinct US
        // accrual count, whose end adjustment depends on its original start day.
        // Fn7 paired date/accrual/remaining-day controls cover calendar zero, fictitious
        // February 29 and independent month-end schedules; all readings are retained.
        let coupon_days = calendar_days360(
            period.previous, period.next, CalendarDayAdjustment::CouponPeriod,
        )?;
        let accrued = accrued_or_remaining_days(period.previous, ctx.settlement, ctx.basis)?;
        return Ok(coupon_days - accrued);
    }
    accrued_or_remaining_days(ctx.settlement, period.next, ctx.basis)
}

pub fn coupncd_kernel(
    settlement: f64,
    maturity: f64,
    frequency: f64,
    basis: Option<f64>,
) -> Result<f64, WorksheetErrorCode> {
    let ctx = parse_coupon_context(settlement, maturity, frequency, basis)?;
    let period = locate_coupon_period(ctx)?;
    Ok(period.next as f64)
}

pub fn coupnum_kernel(
    settlement: f64,
    maturity: f64,
    frequency: f64,
    basis: Option<f64>,
) -> Result<f64, WorksheetErrorCode> {
    let ctx = parse_coupon_context(settlement, maturity, frequency, basis)?;
    let period = locate_coupon_period(ctx)?;
    // Argument errors are resolved before this calendar limit. Fn4 qd225..qd236 and
    // fn7 COUPPCD/COUPNUM paired controls retain the zero-previous-coupon boundary.
    if period.previous == 0 {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(period.remaining_coupons as f64)
}

pub fn couppcd_kernel(
    settlement: f64,
    maturity: f64,
    frequency: f64,
    basis: Option<f64>,
) -> Result<f64, WorksheetErrorCode> {
    let ctx = parse_coupon_context(settlement, maturity, frequency, basis)?;
    let period = locate_coupon_period(ctx)?;
    Ok(period.previous as f64)
}

pub fn map_coupon_error_to_ws(error: &CouponEvalError) -> WorksheetErrorCode {
    match error {
        CouponEvalError::ArityMismatch { .. } => WorksheetErrorCode::Value,
        CouponEvalError::Domain(code) => *code,
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use crate::value::{CalcArray, CalcValue, CoreValue, ExcelText};
    use super::*;

    struct NoResolver;

    fn serial(year: i64, month: i64, day: i64) -> f64 {
        excel_serial_from_ymd(WorkbookDateSystem::System1900, year, month, day).unwrap()
    }

    fn num(n: f64) -> CalcValue {
        CalcValue::number(n)
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn coupdaybs_us30_360_settlement_on_31st_rolls_end_into_next_month() {
        let settlement = serial(2024, 7, 31);
        let maturity = serial(2024, 8, 15);
        assert_eq!(
            couppcd_kernel(settlement, maturity, 2.0, Some(0.0)),
            Ok(serial(2024, 2, 15))
        );
        assert_eq!(
            coupdaybs_kernel(settlement, maturity, 2.0, Some(0.0)),
            Ok(166.0)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn regular_semiannual_actual_actual_matches_known_example() {
        let settlement = serial(2011, 1, 25);
        let maturity = serial(2011, 11, 15);
        assert_eq!(
            couppcd_kernel(settlement, maturity, 2.0, Some(1.0)),
            Ok(serial(2010, 11, 15))
        );
        assert_eq!(
            coupncd_kernel(settlement, maturity, 2.0, Some(1.0)),
            Ok(serial(2011, 5, 15))
        );
        assert_eq!(
            coupdaybs_kernel(settlement, maturity, 2.0, Some(1.0)),
            Ok(71.0)
        );
        assert_eq!(
            coupdaysnc_kernel(settlement, maturity, 2.0, Some(1.0)),
            Ok(110.0)
        );
        assert_eq!(
            coupdays_kernel(settlement, maturity, 2.0, Some(1.0)),
            Ok(181.0)
        );
        assert_eq!(
            coupnum_kernel(settlement, maturity, 2.0, Some(1.0)),
            Ok(2.0)
        );
    }

    /// W111-5 (2026-09-24): the three b37-battery rows where a direct basis-0 count was one
    /// day high. Excel publishes COUPDAYSNC = COUPDAYS - COUPDAYBS on basis 0.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn coupdaysnc_basis_zero_settlement_on_the_31st_is_coupdays_minus_coupdaybs() {
        let cases = [
            (serial(2020, 7, 31), serial(2025, 1, 1), 4.0, 60.0),
            (serial(2023, 10, 31), serial(2025, 3, 1), 2.0, 120.0),
            (serial(2020, 7, 31), serial(2025, 1, 1), 2.0, 150.0),
        ];
        for (settlement, maturity, frequency, excel) in cases {
            assert_eq!(
                coupdaysnc_kernel(settlement, maturity, frequency, Some(0.0)),
                Ok(excel)
            );
            let e = coupdays_kernel(settlement, maturity, frequency, Some(0.0)).unwrap();
            let a = coupdaybs_kernel(settlement, maturity, frequency, Some(0.0)).unwrap();
            assert_eq!(e - a, excel);
        }
    }

    /// W111-5 G8-08, live Excel 20430: coupon dates are computed from maturity, keeping a
    /// 28th-30th maturity day in every month long enough for it; only a month-end maturity
    /// gives month-end coupons.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn coupon_schedule_keeps_the_maturity_day_across_february() {
        assert_eq!(
            coupncd_kernel(serial(2045, 2, 28), serial(2046, 8, 28), 4.0, Some(2.0)),
            Ok(serial(2045, 5, 28))
        );
        assert_eq!(
            couppcd_kernel(serial(2018, 1, 17), serial(2018, 8, 28), 4.0, Some(4.0)),
            Ok(serial(2017, 11, 28))
        );
        assert_eq!(
            couppcd_kernel(serial(2028, 7, 31), serial(2029, 7, 30), 4.0, Some(4.0)),
            Ok(serial(2028, 7, 30))
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn basis_zero_two_three_and_four_follow_bounded_contract() {
        let settlement = serial(2011, 1, 25);
        let maturity = serial(2011, 11, 15);
        assert_eq!(
            coupdaybs_kernel(settlement, maturity, 2.0, Some(0.0)),
            Ok(70.0)
        );
        assert_eq!(
            coupdaysnc_kernel(settlement, maturity, 2.0, Some(0.0)),
            Ok(110.0)
        );
        assert_eq!(
            coupdays_kernel(settlement, maturity, 2.0, Some(0.0)),
            Ok(180.0)
        );
        assert_eq!(
            coupdays_kernel(settlement, maturity, 2.0, Some(2.0)),
            Ok(180.0)
        );
        assert_eq!(
            coupdays_kernel(settlement, maturity, 2.0, Some(3.0)),
            Ok(182.5)
        );
        assert_eq!(
            coupdays_kernel(settlement, maturity, 2.0, Some(4.0)),
            Ok(180.0)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn quarterly_end_of_month_schedule_clamps_coupon_dates() {
        let settlement = serial(2024, 2, 15);
        let maturity = serial(2024, 11, 30);
        assert_eq!(
            couppcd_kernel(settlement, maturity, 4.0, Some(1.0)),
            Ok(serial(2023, 11, 30))
        );
        assert_eq!(
            coupncd_kernel(settlement, maturity, 4.0, Some(1.0)),
            Ok(serial(2024, 2, 29))
        );
        assert_eq!(
            coupnum_kernel(settlement, maturity, 4.0, Some(1.0)),
            Ok(4.0)
        );
        assert_eq!(
            coupdays_kernel(settlement, maturity, 4.0, Some(1.0)),
            Ok(91.0)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn invalid_frequency_basis_and_order_are_rejected() {
        let settlement = serial(2011, 1, 25);
        let maturity = serial(2011, 11, 15);
        assert_eq!(
            coupnum_kernel(settlement, maturity, 3.0, Some(1.0)),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            coupnum_kernel(settlement, maturity, 2.0, Some(9.0)),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            coupnum_kernel(maturity, settlement, 2.0, Some(1.0)),
            Err(WorksheetErrorCode::Num)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn serial_zero_is_admitted_but_negative_or_nonfinite_dates_are_num_errors() {
        let maturity = serial(2011, 11, 15);
        assert_eq!(coupdaybs_kernel(0.0, maturity, 2.0, Some(1.0)), Ok(0.0));
        assert_eq!(couppcd_kernel(0.0, maturity, 2.0, Some(1.0)), Ok(0.0));
        assert_eq!(
            coupdaybs_kernel(-1.0, maturity, 2.0, Some(1.0)),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            coupdaybs_kernel(f64::NAN, maturity, 2.0, Some(1.0)),
            Err(WorksheetErrorCode::Num)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn settlement_on_coupon_date_advances_to_following_period() {
        let maturity = serial(2011, 11, 15);
        assert_eq!(couppcd_kernel(136.0, maturity, 2.0, Some(1.0)), Ok(136.0));
        assert_eq!(coupncd_kernel(136.0, maturity, 2.0, Some(1.0)), Ok(320.0));
        assert_eq!(coupnum_kernel(136.0, maturity, 2.0, Some(1.0)), Ok(223.0));
    }
}
