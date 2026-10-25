use crate::locale::{DateSystem as WorkbookDateSystem, excel_serial_from_ymd, ymd_from_excel_serial};
use crate::value::WorksheetErrorCode;

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

/// NASD US (30/360) day count between two Excel serial dates.
///
/// Excel's basis-0 rule rolls the end date to the 1st of the following month
/// when the end day is 31 and the (already adjusted) start day is below 30,
/// rather than collapsing the end day to 30 in place. The month roll keeps the
/// count non-negative for forward ranges; the in-place form produces short and
/// even negative counts for same-month spans.
pub fn us_30_360(start: i64, end: i64) -> Result<f64, WorksheetErrorCode> {
    let (sy, sm, mut sd) = ymd_from_excel_serial(WorkbookDateSystem::System1900, start as f64)
        .ok_or(WorksheetErrorCode::Value)?;
    let (ey, em, mut ed) = ymd_from_excel_serial(WorkbookDateSystem::System1900, end as f64)
        .ok_or(WorksheetErrorCode::Value)?;

    // Excel's 1900 calendar has a Feb 29 (serial 60), so in 1900 the 29th is the month end.
    let feb_end = |y: i64, m: i64| if y == 1900 { 29 } else { days_in_month(y, m) };
    let start_last_feb = sm == 2 && sd == feb_end(sy, sm);
    let end_last_feb = em == 2 && ed == feb_end(ey, em);

    if sd == 31 || start_last_feb {
        sd = 30;
    }
    if ed == 31 {
        if sd < 30 {
            let (ny, nm) = if em == 12 { (ey + 1, 1) } else { (ey, em + 1) };
            return Ok((ny - sy) as f64 * 360.0 + (nm - sm) as f64 * 30.0 + (1 - sd) as f64);
        }
        ed = 30;
    }
    if end_last_feb && start_last_feb {
        ed = 30;
    }

    Ok(((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64)
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use crate::value::{CalcArray, CalcValue, CoreValue, ExcelText};
    use super::*;
    use crate::locale::excel_serial_from_ymd;

    fn serial(year: i64, month: i64, day: i64) -> i64 {
        excel_serial_from_ymd(WorkbookDateSystem::System1900, year, month, day).unwrap() as i64
    }

    // Equivalent to the in-place ed==31 && sd>=30 -> ed=30 form used by
    // amor_depreciation_family::days360_us, kept here only to assert agreement.
    fn amor_form(start: i64, end: i64) -> f64 {
        let (sy, sm, mut sd) =
            ymd_from_excel_serial(WorkbookDateSystem::System1900, start as f64).unwrap();
        let (ey, em, mut ed) =
            ymd_from_excel_serial(WorkbookDateSystem::System1900, end as f64).unwrap();
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
        ((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn end_day_31_rolls_into_next_month() {
        assert_eq!(
            us_30_360(serial(2023, 11, 15), serial(2024, 1, 31)),
            Ok(76.0)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn same_month_forward_range_is_non_negative() {
        assert_eq!(
            us_30_360(serial(2024, 1, 15), serial(2024, 1, 31)),
            Ok(16.0)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn start_at_least_30_matches_amor_in_place_form() {
        for (s, e) in [
            (serial(2024, 1, 30), serial(2024, 3, 31)),
            (serial(2024, 1, 31), serial(2024, 5, 31)),
            (serial(2023, 12, 31), serial(2024, 3, 31)),
        ] {
            assert_eq!(
                us_30_360(s, e),
                Ok(amor_form(s, e)),
                "start>=30 case {s}->{e}"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn december_31_end_rolls_across_year_boundary() {
        assert_eq!(
            us_30_360(serial(2023, 11, 15), serial(2023, 12, 31)),
            Ok(46.0)
        );
    }
}

fn real_days_in_year(year: i64) -> f64 {
    if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
        366.0
    } else {
        365.0
    }
}

/// Excel's actual/actual day count (YEARFRAC basis 1, and ACCRINTM basis 1) for `start <= end`
/// serials, as the pair (days, year length) whose quotient is the year fraction. Not the ISDA
/// per-year split:
///
/// * if the dates are at most a year apart (same year, or the next year on an earlier or
///   equal month/day), the year length is 366 when both dates are in one leap year or a Feb 29
///   lies in the span (start before 1 March of a leap start year, or end on/after 1 March of a
///   leap end year, or the end is a Feb 29), else 365, and the fraction is days / year length;
/// * otherwise days / (days in the whole years start..=end / number of years), the year total
///   on the real calendar while `days` is the serial difference (so it includes Excel's
///   fictitious 1900-02-29).
///
/// The rule is David Wheeler's YEARFRAC analysis (used by Gnumeric and LibreOffice);
/// reproduced on live Excel 20430 on two fresh corpora (W111-5 G8-05, G8-06).
pub fn excel_actual_actual_year(start: i64, end: i64) -> Result<(f64, f64), WorksheetErrorCode> {
    let ymd = |serial: i64| {
        ymd_from_excel_serial(WorkbookDateSystem::System1900, serial as f64)
            .ok_or(WorksheetErrorCode::Value)
    };
    let serial_of = |y: i64, m: i64, d: i64| {
        excel_serial_from_ymd(WorkbookDateSystem::System1900, y, m, d)
            .map(|v| v as i64)
            .ok_or(WorksheetErrorCode::Value)
    };
    let is_leap = |y: i64| real_days_in_year(y) == 366.0;
    let (sy, sm, sd) = ymd(start)?;
    let (ey, em, ed) = ymd(end)?;
    let days = (end - start) as f64;
    let within_a_year = sy == ey || (ey == sy + 1 && (sm > em || (sm == em && sd >= ed)));
    if within_a_year {
        let feb29_in_span = (is_leap(sy) && start < serial_of(sy, 3, 1)?)
            || (is_leap(ey) && end >= serial_of(ey, 3, 1)?)
            || (em == 2 && ed == 29);
        let year_length = if (sy == ey && is_leap(sy)) || feb29_in_span {
            366.0
        } else {
            365.0
        };
        return Ok((days, year_length));
    }
    let years = (ey - sy + 1) as f64;
    let days_in_years: f64 = (sy..=ey).map(real_days_in_year).sum();
    Ok((days, days_in_years / years))
}

/// [`excel_actual_actual_year`]'s days divided by its year length.
pub fn excel_actual_actual_fraction(start: i64, end: i64) -> Result<f64, WorksheetErrorCode> {
    let (days, year_length) = excel_actual_actual_year(start, end)?;
    Ok(days / year_length)
}
