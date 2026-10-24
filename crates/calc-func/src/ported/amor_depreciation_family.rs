use crate::locale::{DateSystem as WorkbookDateSystem, excel_serial_from_ymd, ymd_from_excel_serial};
use crate::value::{CalcValue, CoreValue, WorksheetErrorCode};
const EPSILON: f64 = 1.0e-12;

#[derive(Debug, Clone, PartialEq)]
pub enum AmorDepreciationEvalError {
    ArityMismatch {
        expected_min: usize,
        expected_max: usize,
        actual: usize,
    },
    Domain(WorksheetErrorCode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AmorBasis {
    Us30_360,
    ActualActual,
    Actual365,
    European30_360,
}

fn max_excel_serial() -> i64 {
    excel_serial_from_ymd(WorkbookDateSystem::System1900, 9999, 12, 31).unwrap() as i64
}

fn parse_basis(value: f64) -> Result<AmorBasis, AmorDepreciationEvalError> {
    if !value.is_finite() {
        return Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Num));
    }
    match value.trunc() as i64 {
        0 => Ok(AmorBasis::Us30_360),
        1 => Ok(AmorBasis::ActualActual),
        3 => Ok(AmorBasis::Actual365),
        4 => Ok(AmorBasis::European30_360),
        _ => Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Num)),
    }
}

fn parse_date_serial(value: f64) -> Result<i64, AmorDepreciationEvalError> {
    if !value.is_finite() {
        return Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Value));
    }
    let serial = value.trunc() as i64;
    if serial < 1 || serial > max_excel_serial() {
        return Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Value));
    }
    ymd_from_excel_serial(WorkbookDateSystem::System1900, serial as f64)
        .ok_or(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Value))?;
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

fn actual_days(start: i64, end: i64) -> f64 {
    (end - start) as f64
}

fn days360_us(start: i64, end: i64) -> Result<f64, AmorDepreciationEvalError> {
    let (sy, sm, mut sd) = ymd_from_excel_serial(WorkbookDateSystem::System1900, start as f64)
        .ok_or(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Value))?;
    let (ey, em, mut ed) = ymd_from_excel_serial(WorkbookDateSystem::System1900, end as f64)
        .ok_or(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Value))?;

    let start_last_feb = sm == 2 && sd == days_in_month(sy, sm);
    let end_last_feb = em == 2 && ed == days_in_month(ey, em);

    if sd == 31 || start_last_feb {
        sd = 30;
    }
    if ed == 31 && sd >= 30 {
        ed = 30;
    }
    if end_last_feb && start_last_feb {
        ed = 30;
    }

    Ok(((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64)
}

fn days360_eu(start: i64, end: i64) -> Result<f64, AmorDepreciationEvalError> {
    let (sy, sm, mut sd) = ymd_from_excel_serial(WorkbookDateSystem::System1900, start as f64)
        .ok_or(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Value))?;
    let (ey, em, mut ed) = ymd_from_excel_serial(WorkbookDateSystem::System1900, end as f64)
        .ok_or(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Value))?;
    if sd == 31 {
        sd = 30;
    }
    if ed == 31 {
        ed = 30;
    }
    Ok(((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64)
}

/// The actual/actual fraction of the first period: the actual days divided by the days of the
/// purchase year, also when the period ends in a later year (dc-j9k: 40 of 40 AMORLINC and 40 of
/// 40 AMORDEGRC readings in the catalogue oracle's sheet dcj9k_closed_form; the year-split sum
/// this replaces missed every reading that crosses into a year of another length).
fn actual_actual_positive(start: i64, end: i64) -> Result<f64, AmorDepreciationEvalError> {
    if end <= start {
        return Ok(0.0);
    }
    let (sy, _, _) = ymd_from_excel_serial(WorkbookDateSystem::System1900, start as f64)
        .ok_or(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Value))?;
    Ok(actual_days(start, end) / days_in_year(sy))
}

fn first_period_fraction(
    purchase: i64,
    first_period: i64,
    basis: AmorBasis,
) -> Result<f64, AmorDepreciationEvalError> {
    if purchase > first_period {
        return Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Num));
    }
    if purchase == first_period {
        return Ok(1.0);
    }
    match basis {
        AmorBasis::Us30_360 => Ok(days360_us(purchase, first_period)? / 360.0),
        AmorBasis::ActualActual => actual_actual_positive(purchase, first_period),
        AmorBasis::Actual365 => Ok(actual_days(purchase, first_period) / 365.0),
        AmorBasis::European30_360 => Ok(days360_eu(purchase, first_period)? / 360.0),
    }
}

fn normalize_amorlinc_period(value: f64) -> Result<i64, AmorDepreciationEvalError> {
    if !value.is_finite() || value < 0.0 {
        return Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Num));
    }
    if value.abs() <= EPSILON {
        Ok(0)
    } else if value < 1.0 {
        Ok(1)
    } else {
        Ok(value.floor() as i64)
    }
}

fn normalize_amordegrc_period(value: f64) -> Result<i64, AmorDepreciationEvalError> {
    if !value.is_finite() || value < 0.0 {
        return Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Num));
    }
    Ok(value.floor() as i64)
}

fn round_half_away_from_zero(value: f64) -> f64 {
    let rounded = (value.abs() + 0.5).floor().copysign(value);
    if rounded == -0.0 { 0.0 } else { rounded }
}

fn amordegrc_coefficient(rate: f64) -> Result<f64, AmorDepreciationEvalError> {
    if !rate.is_finite() || rate <= 0.0 || rate >= 0.5 {
        return Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Num));
    }
    let life = 1.0 / rate;
    if life < 3.0 {
        Ok(1.0)
    } else if life < 5.0 {
        Ok(1.5)
    } else if life <= 6.0 {
        Ok(2.0)
    } else {
        Ok(2.5)
    }
}

pub fn amorlinc_kernel(
    cost: f64,
    date_purchased: f64,
    first_period: f64,
    salvage: f64,
    period: f64,
    rate: f64,
    basis: Option<f64>,
) -> Result<f64, AmorDepreciationEvalError> {
    if !cost.is_finite() || !salvage.is_finite() || !rate.is_finite() {
        return Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Num));
    }
    if cost <= 0.0 || salvage < 0.0 || salvage > cost || rate <= 0.0 {
        return Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Num));
    }

    let purchase = parse_date_serial(date_purchased)?;
    let first = parse_date_serial(first_period)?;
    let basis = parse_basis(basis.unwrap_or(0.0))?;
    let normalized_period = normalize_amorlinc_period(period)?;
    let annual_depreciation = cost * rate;
    let initial_fraction = first_period_fraction(purchase, first, basis)?;
    // Excel multiplies the rate by the fraction first (dc-j9k: cost*(rate*fraction) matches 78 of
    // the 80 AMORLINC readings of sheet dcj9k_closed_form, and the other two were the basis-1
    // fraction; (cost*rate)*fraction matched 68).
    let first_depreciation = cost * (rate * initial_fraction);
    let depreciable_basis = cost - salvage;

    if normalized_period == 0 {
        return Ok(first_depreciation.min(depreciable_basis).max(0.0));
    }

    let mut remaining = (depreciable_basis - first_depreciation).max(0.0);
    for current in 1..=normalized_period {
        let depreciation = remaining.min(annual_depreciation).max(0.0);
        if current == normalized_period {
            return Ok(depreciation);
        }
        remaining = (remaining - depreciation).max(0.0);
        if remaining <= EPSILON {
            return Ok(0.0);
        }
    }
    Ok(0.0)
}

pub fn amordegrc_kernel(
    cost: f64,
    date_purchased: f64,
    first_period: f64,
    salvage: f64,
    period: f64,
    rate: f64,
    basis: Option<f64>,
) -> Result<f64, AmorDepreciationEvalError> {
    if !cost.is_finite() || !salvage.is_finite() || !rate.is_finite() {
        return Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Num));
    }
    if cost <= 0.0 || salvage < 0.0 || salvage > cost {
        return Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Num));
    }

    let purchase = parse_date_serial(date_purchased)?;
    let first = parse_date_serial(first_period)?;
    let basis = parse_basis(basis.unwrap_or(0.0))?;
    let normalized_period = normalize_amordegrc_period(period)?;
    if period > 0.0 && period < 1.0 {
        return Ok(0.0);
    }
    let coefficient = amordegrc_coefficient(rate)?;
    let depreciation_rate = rate * coefficient;
    let initial_fraction = first_period_fraction(purchase, first, basis)?;
    let first_depreciation = round_half_away_from_zero(cost * depreciation_rate * initial_fraction);

    if normalized_period == 0 {
        return Ok(first_depreciation);
    }

    let total_periods = (1.0 / rate).ceil() as i64;
    let mut book_value = cost - first_depreciation;
    for current in 1..=normalized_period {
        if book_value < salvage || book_value <= 0.0 {
            return Ok(0.0);
        }
        let remaining_periods = total_periods - current;
        let depreciation = if remaining_periods > 2 {
            round_half_away_from_zero(book_value * depreciation_rate)
        } else if remaining_periods == 2 {
            round_half_away_from_zero(book_value * 0.5)
        } else {
            book_value
        };
        if current == normalized_period {
            return Ok(depreciation.max(0.0));
        }
        book_value = (book_value - depreciation).max(0.0);
    }
    Ok(0.0)
}

pub fn map_amor_depreciation_error_to_ws(error: &AmorDepreciationEvalError) -> WorksheetErrorCode {
    match error {
        AmorDepreciationEvalError::ArityMismatch { .. } => WorksheetErrorCode::Value,
        AmorDepreciationEvalError::Domain(code) => *code,
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use crate::value::{CalcArray, CalcValue, CoreValue, ExcelText};
    use super::*;

    struct NoRefResolver;

    fn num(n: f64) -> CalcValue {
        CalcValue::number(n)
    }

    fn text(s: &str) -> CalcValue {
        CalcValue::text(ExcelText::from_utf16_code_units(s.encode_utf16().collect()))
    }

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
    fn amorlinc_matches_seeded_support_example_periods() {
        assert_close(
            amorlinc_kernel(2400.0, 39679.0, 39813.0, 300.0, 0.0, 0.15, Some(1.0)).unwrap(),
            131.80327868852459,
            1.0e-12,
        );
        assert_close(
            amorlinc_kernel(2400.0, 39679.0, 39813.0, 300.0, 1.0, 0.15, Some(1.0)).unwrap(),
            360.0,
            1.0e-12,
        );
        assert_close(
            amorlinc_kernel(2400.0, 39679.0, 39813.0, 300.0, 6.0, 0.15, Some(1.0)).unwrap(),
            168.19672131147541,
            1.0e-12,
        );
        assert_eq!(
            amorlinc_kernel(2400.0, 39679.0, 39813.0, 300.0, 7.0, 0.15, Some(1.0)).unwrap(),
            0.0
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn amordegrc_matches_seeded_support_example_periods() {
        let expected = [330.0, 776.0, 485.0, 303.0, 190.0, 158.0, 0.0];
        for (period, wanted) in expected.iter().copied().enumerate() {
            assert_eq!(
                amordegrc_kernel(
                    2400.0,
                    39679.0,
                    39813.0,
                    300.0,
                    period as f64,
                    0.15,
                    Some(1.0)
                )
                .unwrap(),
                wanted
            );
        }
        let zero_salvage = [330.0, 776.0, 485.0, 303.0, 190.0, 158.0, 158.0, 0.0];
        for (period, wanted) in zero_salvage.iter().copied().enumerate() {
            assert_eq!(
                amordegrc_kernel(
                    2400.0,
                    39679.0,
                    39813.0,
                    0.0,
                    period as f64,
                    0.15,
                    Some(1.0)
                )
                .unwrap(),
                wanted
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn basis_specific_first_period_rows_are_pinned() {
        assert_close(
            amorlinc_kernel(2400.0, 39679.0, 39813.0, 300.0, 0.0, 0.15, Some(0.0)).unwrap(),
            132.0,
            1.0e-12,
        );
        assert_close(
            amorlinc_kernel(2400.0, 39679.0, 39813.0, 300.0, 0.0, 0.15, Some(3.0)).unwrap(),
            132.16438356164383,
            1.0e-12,
        );
        assert_close(
            amorlinc_kernel(2400.0, 39679.0, 39813.0, 300.0, 0.0, 0.15, Some(4.0)).unwrap(),
            131.0,
            1.0e-12,
        );
        assert_eq!(
            amordegrc_kernel(2400.0, 39679.0, 39813.0, 300.0, 0.0, 0.15, Some(0.0)).unwrap(),
            330.0
        );
        assert_eq!(
            amordegrc_kernel(2400.0, 39679.0, 39813.0, 300.0, 0.0, 0.15, Some(3.0)).unwrap(),
            330.0
        );
        assert_eq!(
            amordegrc_kernel(2400.0, 39679.0, 39813.0, 300.0, 0.0, 0.15, Some(4.0)).unwrap(),
            328.0
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn equal_purchase_and_first_period_is_full_first_year() {
        assert_eq!(
            amorlinc_kernel(2400.0, 39679.0, 39679.0, 300.0, 0.0, 0.15, Some(1.0)).unwrap(),
            360.0
        );
        assert_eq!(
            amordegrc_kernel(2400.0, 39679.0, 39679.0, 300.0, 0.0, 0.15, Some(1.0)).unwrap(),
            900.0
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fractional_period_normalization_lanes_are_pinned() {
        assert_eq!(
            amorlinc_kernel(2400.0, 39679.0, 39813.0, 300.0, 0.1, 0.15, Some(1.0)).unwrap(),
            360.0
        );
        assert_eq!(
            amorlinc_kernel(2400.0, 39679.0, 39813.0, 300.0, 1.9, 0.15, Some(1.0)).unwrap(),
            360.0
        );
        assert_eq!(
            amorlinc_kernel(2400.0, 39679.0, 39813.0, 300.0, 2.9, 0.15, Some(1.0)).unwrap(),
            360.0
        );
        assert_eq!(
            amordegrc_kernel(2400.0, 39679.0, 39813.0, 300.0, 0.1, 0.15, Some(1.0)).unwrap(),
            0.0
        );
        assert_eq!(
            amordegrc_kernel(2400.0, 39679.0, 39813.0, 300.0, 1.9, 0.15, Some(1.0)).unwrap(),
            776.0
        );
        assert_eq!(
            amordegrc_kernel(2400.0, 39679.0, 39813.0, 300.0, 2.9, 0.15, Some(1.0)).unwrap(),
            485.0
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn domain_lanes_are_explicit() {
        assert_eq!(
            amorlinc_kernel(2400.0, 39679.0, 39813.0, 300.0, 0.0, 0.15, Some(2.0)),
            Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Num))
        );
        assert_eq!(
            amordegrc_kernel(2400.0, 39679.0, 39813.0, 300.0, 0.0, 0.5, Some(1.0)),
            Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Num))
        );
        assert_eq!(
            amorlinc_kernel(2400.0, 39813.0, 39679.0, 300.0, 0.0, 0.15, Some(1.0)),
            Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Num))
        );
        assert_eq!(
            amordegrc_kernel(2400.0, 39813.0, 39679.0, 300.0, 0.0, 0.15, Some(1.0)),
            Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Num))
        );
        assert_eq!(
            amorlinc_kernel(2400.0, 39679.0, 39813.0, 300.0, -0.1, 0.15, Some(1.0)),
            Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Num))
        );
        assert_eq!(
            amordegrc_kernel(2400.0, 39679.0, 39813.0, 300.0, -0.1, 0.15, Some(1.0)),
            Err(AmorDepreciationEvalError::Domain(WorksheetErrorCode::Num))
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn date_helper_matches_known_serials() {
        assert_eq!(serial(2008, 8, 19), 39679.0);
        assert_eq!(serial(2008, 12, 31), 39813.0);
    }
}
