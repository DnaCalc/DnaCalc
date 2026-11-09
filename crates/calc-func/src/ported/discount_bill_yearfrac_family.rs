use crate::locale::{
    DateSystem as WorkbookDateSystem, excel_serial_from_ymd, ymd_from_excel_serial,
};
use crate::ported::excel_numeric::{excel_x87_div, excel_x87_mul, excel_x87_sub};
use crate::value::CalcValue;
use crate::value::WorksheetErrorCode;

#[derive(Debug, Clone, PartialEq)]
pub enum DiscountBillYearfracEvalError {
    ArityMismatch {
        expected_min: usize,
        expected_max: usize,
        actual: usize,
    },
    Domain(WorksheetErrorCode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayCountBasis {
    Us30_360,
    ActualActual,
    Actual360,
    Actual365,
    European30_360,
}

fn max_excel_serial() -> i64 {
    excel_serial_from_ymd(WorkbookDateSystem::System1900, 9999, 12, 31).unwrap() as i64
}

fn parse_basis(value: f64) -> Result<DayCountBasis, DiscountBillYearfracEvalError> {
    if !value.is_finite() {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Num,
        ));
    }
    match value.trunc() as i64 {
        0 => Ok(DayCountBasis::Us30_360),
        1 => Ok(DayCountBasis::ActualActual),
        2 => Ok(DayCountBasis::Actual360),
        3 => Ok(DayCountBasis::Actual365),
        4 => Ok(DayCountBasis::European30_360),
        _ => Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Num,
        )),
    }
}

fn parse_date_serial(value: f64, minimum: i64) -> Result<i64, DiscountBillYearfracEvalError> {
    if !value.is_finite() {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Value,
        ));
    }
    let serial = value.trunc() as i64;
    if serial < minimum || serial > max_excel_serial() {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Value,
        ));
    }
    ymd_from_excel_serial(WorkbookDateSystem::System1900, serial as f64).ok_or(
        DiscountBillYearfracEvalError::Domain(WorksheetErrorCode::Value),
    )?;
    Ok(serial)
}

fn actual_days(start: i64, end: i64) -> f64 {
    (end - start) as f64
}

fn days360_us(start: i64, end: i64) -> Result<f64, DiscountBillYearfracEvalError> {
    crate::ported::day_count_common::us_30_360(start, end)
        .map_err(DiscountBillYearfracEvalError::Domain)
}

fn days360_eu(start: i64, end: i64) -> Result<f64, DiscountBillYearfracEvalError> {
    let (sy, sm, mut sd) = ymd_from_excel_serial(WorkbookDateSystem::System1900, start as f64)
        .ok_or(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Value,
        ))?;
    let (ey, em, mut ed) = ymd_from_excel_serial(WorkbookDateSystem::System1900, end as f64)
        .ok_or(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Value,
        ))?;
    if sd == 31 {
        sd = 30;
    }
    if ed == 31 {
        ed = 30;
    }
    Ok(((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64)
}

/// Excel's actual/actual (YEARFRAC basis 1): see
/// [`crate::ported::day_count_common::excel_actual_actual_fraction`].
fn actual_actual_positive(start: i64, end: i64) -> Result<f64, DiscountBillYearfracEvalError> {
    crate::ported::day_count_common::excel_actual_actual_fraction(start, end)
        .map_err(DiscountBillYearfracEvalError::Domain)
}

fn yearfrac_positive(
    start: i64,
    end: i64,
    basis: DayCountBasis,
) -> Result<f64, DiscountBillYearfracEvalError> {
    match basis {
        DayCountBasis::Us30_360 => Ok(days360_us(start, end)? / 360.0),
        DayCountBasis::ActualActual => actual_actual_positive(start, end),
        DayCountBasis::Actual360 => Ok(actual_days(start, end) / 360.0),
        DayCountBasis::Actual365 => Ok(actual_days(start, end) / 365.0),
        DayCountBasis::European30_360 => Ok(days360_eu(start, end)? / 360.0),
    }
}

pub fn yearfrac_kernel(
    start_date: f64,
    end_date: f64,
    basis: Option<f64>,
) -> Result<f64, DiscountBillYearfracEvalError> {
    // A negative date serial is #NUM! in YEARFRAC, not #VALUE! (live Excel 20430, G8-05).
    if start_date.trunc() < 0.0 || end_date.trunc() < 0.0 {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Num,
        ));
    }
    let start = parse_date_serial(start_date, 1)?;
    let end = parse_date_serial(end_date, 1)?;
    let basis = parse_basis(basis.unwrap_or(0.0))?;
    // Excel YEARFRAC is always non-negative: a reversed date order yields the
    // same positive fraction (it is |end - start| based), not a negated value.
    if end >= start {
        yearfrac_positive(start, end, basis)
    } else {
        yearfrac_positive(end, start, basis)
    }
}

fn security_fraction(
    settlement: f64,
    maturity: f64,
    basis: Option<f64>,
) -> Result<f64, DiscountBillYearfracEvalError> {
    let settlement = parse_date_serial(settlement, 1)?;
    let maturity = parse_date_serial(maturity, 1)?;
    let basis = parse_basis(basis.unwrap_or(0.0))?;
    if settlement >= maturity {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Num,
        ));
    }
    yearfrac_positive(settlement, maturity, basis)
}

/// The security's day count as the pair (days `D`, year basis `B`), whose quotient is
/// [`security_fraction`]: DISC, INTRATE, RECEIVED and PRICEDISC use the two apart (dc-j9k).
fn security_days_and_year(
    settlement: f64,
    maturity: f64,
    basis: Option<f64>,
) -> Result<(f64, f64), DiscountBillYearfracEvalError> {
    let settlement = parse_date_serial(settlement, 1)?;
    let maturity = parse_date_serial(maturity, 1)?;
    let basis = parse_basis(basis.unwrap_or(0.0))?;
    if settlement >= maturity {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Num,
        ));
    }
    match basis {
        DayCountBasis::Us30_360 => Ok((days360_us(settlement, maturity)?, 360.0)),
        DayCountBasis::ActualActual => {
            crate::ported::day_count_common::excel_actual_actual_year(settlement, maturity)
                .map_err(DiscountBillYearfracEvalError::Domain)
        }
        DayCountBasis::Actual360 => Ok((actual_days(settlement, maturity), 360.0)),
        DayCountBasis::Actual365 => Ok((actual_days(settlement, maturity), 365.0)),
        DayCountBasis::European30_360 => Ok((days360_eu(settlement, maturity)?, 360.0)),
    }
}

pub fn disc_kernel(
    settlement: f64,
    maturity: f64,
    pr: f64,
    redemption: f64,
    basis: Option<f64>,
) -> Result<f64, DiscountBillYearfracEvalError> {
    if !pr.is_finite() || !redemption.is_finite() {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Value,
        ));
    }
    if pr <= 0.0 || redemption <= 0.0 {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Num,
        ));
    }
    // Excel: (1 - pr/redemption)*(B/D) (dc-j9k: 60 of 60 readings in the catalogue oracle's
    // sheet dcj9k_closed_form; dividing by the fraction D/B matched 41).
    let (days, year) = security_days_and_year(settlement, maturity, basis)?;
    // Build20430's 12,000 original security readings distinguish stored PC64
    // operations from binary64 arithmetic. Keep the published association.
    let ratio = excel_x87_div(pr, redemption);
    let spread = excel_x87_sub(1.0, ratio);
    Ok(excel_x87_mul(spread, excel_x87_div(year, days)))
}

pub fn intrate_kernel(
    settlement: f64,
    maturity: f64,
    investment: f64,
    redemption: f64,
    basis: Option<f64>,
) -> Result<f64, DiscountBillYearfracEvalError> {
    if !investment.is_finite() || !redemption.is_finite() {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Value,
        ));
    }
    if investment <= 0.0 || redemption <= 0.0 {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Num,
        ));
    }
    // Excel: ((redemption - investment)/investment)*(B/D) (dc-j9k: 40 of 40 readings; dividing
    // by the fraction D/B matched 28).
    let (days, year) = security_days_and_year(settlement, maturity, basis)?;
    let spread = excel_x87_sub(redemption, investment);
    let ratio = excel_x87_div(spread, investment);
    Ok(excel_x87_mul(ratio, excel_x87_div(year, days)))
}

pub fn received_kernel(
    settlement: f64,
    maturity: f64,
    investment: f64,
    discount: f64,
    basis: Option<f64>,
) -> Result<f64, DiscountBillYearfracEvalError> {
    if !investment.is_finite() || !discount.is_finite() {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Value,
        ));
    }
    if investment <= 0.0 || discount <= 0.0 {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Num,
        ));
    }
    // Excel: investment/(1 - discount*D/B), the product before the division (dc-j9k: 38 of 38
    // readings; discount times the fraction D/B matched 33).
    let (days, year) = security_days_and_year(settlement, maturity, basis)?;
    let accrual = excel_x87_div(excel_x87_mul(discount, days), year);
    let denom = excel_x87_sub(1.0, accrual);
    if denom <= 0.0 {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Num,
        ));
    }
    Ok(excel_x87_div(investment, denom))
}

pub fn pricedisc_kernel(
    settlement: f64,
    maturity: f64,
    discount: f64,
    redemption: f64,
    basis: Option<f64>,
) -> Result<f64, DiscountBillYearfracEvalError> {
    if !discount.is_finite() || !redemption.is_finite() {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Value,
        ));
    }
    if discount <= 0.0 || redemption <= 0.0 {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Num,
        ));
    }
    // Excel: redemption - discount*redemption*D/B, from left to right (dc-j9k: 40 of 40
    // readings; redemption*(1 - discount*fraction) matched 21).
    let (days, year) = security_days_and_year(settlement, maturity, basis)?;
    let amount = excel_x87_mul(discount, redemption);
    let accrual = excel_x87_div(excel_x87_mul(amount, days), year);
    Ok(excel_x87_sub(redemption, accrual))
}

fn tbill_days(settlement: f64, maturity: f64) -> Result<f64, DiscountBillYearfracEvalError> {
    let settlement = parse_date_serial(settlement, 0)?;
    let maturity = parse_date_serial(maturity, 0)?;
    if settlement >= maturity {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Num,
        ));
    }
    // "More than one year after settlement" is DATE(year+1, month, day) with Excel's day
    // rollover, not a month-end-clamped add: from 2044-02-29 the limit is 2045-03-01, and from
    // 1991-02-28 it is 1992-02-28 (live Excel 20430, W111-5 G8-02/G8-03).
    let num = || DiscountBillYearfracEvalError::Domain(WorksheetErrorCode::Num);
    let (year, month, day) =
        ymd_from_excel_serial(WorkbookDateSystem::System1900, settlement as f64).ok_or_else(num)?;
    let one_year_out = excel_serial_from_ymd(WorkbookDateSystem::System1900, year + 1, month, day)
        .ok_or_else(num)? as i64;
    if maturity > one_year_out {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Num,
        ));
    }
    Ok(actual_days(settlement, maturity))
}

pub fn tbillprice_kernel(
    settlement: f64,
    maturity: f64,
    discount: f64,
) -> Result<f64, DiscountBillYearfracEvalError> {
    if !discount.is_finite() {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Value,
        ));
    }
    if discount <= 0.0 {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Num,
        ));
    }
    let dsm = tbill_days(settlement, maturity)?;
    let price = 100.0 * (1.0 - discount * dsm / 360.0);
    // Excel publishes #NUM! rather than a non-positive price (live Excel 20430, W111-5 G8-03).
    if price <= 0.0 {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Num,
        ));
    }
    Ok(price)
}

pub fn tbillyield_kernel(
    settlement: f64,
    maturity: f64,
    pr: f64,
) -> Result<f64, DiscountBillYearfracEvalError> {
    if !pr.is_finite() {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Value,
        ));
    }
    if pr <= 0.0 {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Num,
        ));
    }
    let dsm = tbill_days(settlement, maturity)?;
    // Excel groups the day-scale factor before the final multiplication:
    // ((100-pr)/pr) * (360/dsm).  The mathematically equivalent left-associative
    // `*360/dsm` form differs by 1 ULP on hundreds of settlement/price rows.
    let spread = excel_x87_sub(100.0, pr);
    let ratio = excel_x87_div(spread, pr);
    Ok(excel_x87_mul(ratio, excel_x87_div(360.0, dsm)))
}

pub fn tbilleq_kernel(
    settlement: f64,
    maturity: f64,
    discount: f64,
) -> Result<f64, DiscountBillYearfracEvalError> {
    if !discount.is_finite() {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Value,
        ));
    }
    if discount <= 0.0 {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Num,
        ));
    }
    let dsm = tbill_days(settlement, maturity)?;
    if dsm > 182.0 {
        return tbilleq_long_bill(dsm, discount);
    }
    let denom = 360.0 - discount * dsm;
    if denom <= 0.0 {
        return Err(DiscountBillYearfracEvalError::Domain(
            WorksheetErrorCode::Num,
        ));
    }
    Ok(365.0 * discount / denom)
}

/// TBILLEQ for bills of more than 182 days: Excel switches to the bond-equivalent yield of a
/// bill longer than one coupon period, the root of a quadratic in the price. With
/// t = dsm / year and p = price / 100:
///
///   TBILLEQ = (-t + sqrt(t*t - (2t - 1) * (1 - 1/p))) / (t - 0.5)
///
/// `year` is 365, except that a 366-day bill (possible only across a Feb 29) uses 366, so t = 1.
/// The shared staged SQRT resolves the recorded cancellation discrepancy in
/// the long-bill numerator; all 2,400 TBILLEQ bank outcomes agree at R23.
fn tbilleq_long_bill(dsm: f64, discount: f64) -> Result<f64, DiscountBillYearfracEvalError> {
    let num = || DiscountBillYearfracEvalError::Domain(WorksheetErrorCode::Num);
    let price = 100.0 - discount * 100.0 * dsm / 360.0;
    if price <= 0.0 {
        return Err(num());
    }
    let year = if dsm == 366.0 { 366.0 } else { 365.0 };
    let t = dsm / year;
    let term = 1.0 - 1.0 / (price / 100.0);
    let disc = t * t - (2.0 * t - 1.0) * term;
    if disc < 0.0 {
        return Err(num());
    }
    Ok((-t + crate::ported::excel_numeric::excel_sqrt_staged(disc)) / (t - 0.5))
}

pub fn map_discount_bill_yearfrac_error_to_ws(
    error: &DiscountBillYearfracEvalError,
) -> WorksheetErrorCode {
    match error {
        DiscountBillYearfracEvalError::ArityMismatch { .. } => WorksheetErrorCode::Value,
        DiscountBillYearfracEvalError::Domain(code) => *code,
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

    fn assert_bits(actual: f64, expected: f64) {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "{actual} vs {expected}"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn yearfrac_us30_360_same_month_forward_range_is_non_negative() {
        let start = serial(2024, 1, 15);
        let end = serial(2024, 1, 31);
        assert_close(
            yearfrac_kernel(start, end, Some(0.0)).unwrap(),
            16.0 / 360.0,
            1.0e-12,
        );
        assert_close(
            yearfrac_kernel(end, start, Some(0.0)).unwrap(),
            16.0 / 360.0,
            1.0e-12,
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

    /// W111-5 G8-05, live Excel 20430: basis 1 is Excel's actual/actual rule (not the ISDA
    /// split); the multi-year average uses the real calendar; negative serials are #NUM!.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn yearfrac_basis_one_follows_excel_actual_actual() {
        // within a year, span ends on a Feb 29: 314 / 366
        assert_eq!(
            yearfrac_kernel(71335.0, 71649.0, Some(1.0)).unwrap(),
            314.0 / 366.0
        );
        // reversed dates over more than a year: 707 / ((366 + 365 + 365) / 3)
        assert_eq!(
            yearfrac_kernel(14184.0, 13477.0, Some(1.0)).unwrap(),
            707.0 / (1096.0 / 3.0)
        );
        // a span starting in 1900: serial days include the fictitious 1900-02-29,
        // the year total does not (1900..=1904 is 1826 days)
        assert_eq!(
            yearfrac_kernel(54.0, 1805.0, Some(1.0)).unwrap(),
            1751.0 / (1826.0 / 5.0)
        );
        assert!(matches!(
            yearfrac_kernel(213.0, -1784.0, Some(1.0)),
            Err(DiscountBillYearfracEvalError::Domain(
                WorksheetErrorCode::Num
            ))
        ));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn yearfrac_matches_microsoft_examples() {
        let start = serial(2012, 1, 1);
        let end = serial(2012, 7, 30);
        assert_close(
            yearfrac_kernel(start, end, None).unwrap(),
            0.580_555_56,
            1.0e-8,
        );
        assert_close(
            yearfrac_kernel(start, end, Some(1.0)).unwrap(),
            0.576_502_73,
            1.0e-8,
        );
        assert_close(
            yearfrac_kernel(start, end, Some(3.0)).unwrap(),
            0.578_082_19,
            1.0e-8,
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn yearfrac_is_nonnegative_and_order_independent() {
        // Excel YEARFRAC is always non-negative: reversing the date order
        // yields the same positive fraction (verified vs Excel, run yf-rev-001:
        // both orders -> 0.5765027322404371).
        let start = serial(2012, 1, 1);
        let end = serial(2012, 7, 30);
        let forward = yearfrac_kernel(start, end, Some(1.0)).unwrap();
        let reverse = yearfrac_kernel(end, start, Some(1.0)).unwrap();
        assert_close(forward, reverse, 1.0e-12);
        assert!(
            forward > 0.0,
            "yearfrac should be non-negative, got {forward}"
        );
        assert_close(forward, 0.5765027322404371, 1.0e-12);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn pricedisc_matches_microsoft_example() {
        let got = pricedisc_kernel(
            serial(2008, 2, 16),
            serial(2008, 3, 1),
            0.0525,
            100.0,
            Some(2.0),
        )
        .unwrap();
        assert_close(got, 99.795_833_333_333_33, 1.0e-10);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn intrate_matches_microsoft_example() {
        let got = intrate_kernel(
            serial(2008, 2, 15),
            serial(2008, 5, 15),
            1_000_000.0,
            1_014_420.0,
            Some(2.0),
        )
        .unwrap();
        assert_close(got, 0.057_68, 1.0e-5);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn received_matches_microsoft_example() {
        let got = received_kernel(
            serial(2008, 2, 15),
            serial(2008, 5, 15),
            1_000_000.0,
            0.0575,
            Some(2.0),
        )
        .unwrap();
        assert_close(got, 1_014_584.654_407_102, 1.0e-6);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn tbill_functions_match_microsoft_examples() {
        let settlement = serial(2008, 3, 31);
        let maturity = serial(2008, 6, 1);
        assert_close(
            tbillprice_kernel(settlement, maturity, 0.09).unwrap(),
            98.45,
            1.0e-10,
        );
        assert_close(
            tbillyield_kernel(settlement, maturity, 98.45).unwrap(),
            0.091_416_962_925_342_6,
            1.0e-12,
        );
        assert_close(
            tbilleq_kernel(settlement, maturity, 0.0914).unwrap(),
            0.094_151_493_565_943,
            1.0e-12,
        );
    }

    /// W111-5 G8-02/G8-03, live Excel 20430 bits: the long-bill (> 182 days) TBILLEQ branch,
    /// the 366-day year for a 366-day bill, the DATE-rollover one-year limit, and TBILLPRICE's
    /// #NUM! for a non-positive price.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn tbill_long_bills_and_one_year_limit_match_excel() {
        let b = f64::from_bits;
        // 365-day bill, long-bill quadratic
        let v = tbilleq_kernel(
            b(0x40e7_fa00_0000_0000),
            b(0x40e8_27a0_0000_0000),
            b(0x3fda_d9e2_b7ff_ac42),
        );
        assert_eq!(v.unwrap().to_bits(), 0x3fe4_6da4_7a35_0f90);
        // 366-day bill across a Feb 29: t = 366/366
        let v = tbilleq_kernel(
            b(0x40ea_4fc0_0000_0000),
            b(0x40ea_7d80_0000_0000),
            b(0x3fdc_f1bb_1a5c_75f4),
        );
        assert_eq!(v.unwrap().to_bits(), 0x3fe7_137f_fe33_28bc);
        let serial =
            |y, m, d| excel_serial_from_ymd(WorkbookDateSystem::System1900, y, m, d).unwrap();
        // from 2044-02-29 the limit rolls over to 2045-03-01; from 1991-02-28 it is 1992-02-28
        assert!(tbillprice_kernel(serial(2044, 2, 29), serial(2045, 3, 1), 0.1).is_ok());
        assert!(tbillprice_kernel(serial(1991, 2, 28), serial(1992, 2, 29), 0.1).is_err());
        // a discount large enough to price the bill at or below zero is #NUM!
        assert!(tbillprice_kernel(serial(2020, 1, 1), serial(2020, 12, 1), 2.0).is_err());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn tbillyield_groups_day_scale_like_excel() {
        let actual = tbillyield_kernel(36526.0, 36533.0, 98.45).unwrap();
        let excel = f64::from_bits(0x3fe9_e901_8124_6c32);
        let prior_left_associative: f64 = (100.0 - 98.45) / 98.45 * 360.0 / 7.0;

        assert_eq!(actual.to_bits(), excel.to_bits());
        assert_eq!(prior_left_associative.to_bits(), 0x3fe9_e901_8124_6c31);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn disc_round_trips_pricedisc_in_bounded_slice() {
        let settlement = serial(2008, 2, 16);
        let maturity = serial(2008, 3, 1);
        let price = pricedisc_kernel(settlement, maturity, 0.0525, 100.0, Some(2.0)).unwrap();
        let recovered = disc_kernel(settlement, maturity, price, 100.0, Some(2.0)).unwrap();
        assert_close(recovered, 0.0525, 1.0e-12);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn disc_exactness_witness_matches_excel_target_and_fraction_is_unity() {
        let settlement = 44927.0_f64;
        let maturity = 45292.0_f64;
        let actual = disc_kernel(settlement, maturity, 97.0, 100.0, None).expect("disc witness");
        let prior_local = f64::from_bits(0x3f9eb851eb851eb8);
        let excel_target = f64::from_bits(0x3f9eb851eb851ec0);
        let fraction = security_fraction(settlement, maturity, None).expect("disc witness frac");

        assert_bits(fraction, 1.0_f64);
        assert_bits(actual, excel_target);
        assert_ne!(actual.to_bits(), prior_local.to_bits());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn tbill_rejects_maturities_more_than_one_calendar_year_out() {
        assert_eq!(
            tbillprice_kernel(serial(2024, 1, 1), serial(2025, 1, 2), 0.05),
            Err(DiscountBillYearfracEvalError::Domain(
                WorksheetErrorCode::Num
            ))
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn invalid_basis_is_num_error() {
        assert_eq!(
            yearfrac_kernel(serial(2012, 1, 1), serial(2012, 7, 30), Some(9.0)),
            Err(DiscountBillYearfracEvalError::Domain(
                WorksheetErrorCode::Num
            ))
        );
    }
}
