// CUMIPMT and CUMPRINC share the time-value primitives and the stored discounted
// principal span. Its publications differ from sums of the public PPMT values.
pub use crate::ported::financial_time_value_family::PaymentTiming;
use crate::ported::financial_time_value_family::{self as time_value, FinancialError};
use crate::value::CalcValue;
use crate::value::WorksheetErrorCode;

#[derive(Debug, Clone, PartialEq)]
pub enum CumulativeFinanceEvalError {
    ArityMismatch {
        expected_min: usize,
        expected_max: usize,
        actual: usize,
    },
    Domain(WorksheetErrorCode),
}

fn validate_finite(values: &[f64]) -> Result<(), CumulativeFinanceEvalError> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(CumulativeFinanceEvalError::Domain(
            WorksheetErrorCode::Value,
        ))
    }
}

/// A period argument as Excel reads it: the number itself, not its integer part. CUMIPMT and
/// CUMPRINC keep a fractional nper in the annuity and admit fractional start and end periods,
/// summing the whole periods between them (Excel 16.0.20527: CUMPRINC(0.1013,352.5,79905.9,352,
/// 352.5,0) is the principal of period 352 of a 352.5-period loan, CUMPRINC(..,1.9,10.2,0) the
/// periods 2 to 10, and CUMPRINC(..,1.5,1.5,0) zero).
fn period_argument(value: f64) -> Result<f64, CumulativeFinanceEvalError> {
    if !value.is_finite() {
        return Err(CumulativeFinanceEvalError::Domain(
            WorksheetErrorCode::Value,
        ));
    }
    if value < 1.0 || value > i32::MAX as f64 {
        return Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num));
    }
    Ok(value)
}

/// The whole periods from `start` to `end`: the first whole period at or after the start and the
/// last at or before the end. An interval holding no whole period is empty.
fn whole_periods(start: f64, end: f64) -> (i32, i32) {
    (start.ceil() as i32, end.floor() as i32)
}

fn payment_timing_arg(value: f64) -> Result<PaymentTiming, CumulativeFinanceEvalError> {
    validate_finite(&[value])?;
    // Excel20527 rejects both neighbours of one and nonzero values near zero.
    if value == 0.0 {
        Ok(PaymentTiming::EndOfPeriod)
    } else if value == 1.0 {
        Ok(PaymentTiming::BeginningOfPeriod)
    } else {
        Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num))
    }
}

fn kernel_error(error: FinancialError) -> CumulativeFinanceEvalError {
    CumulativeFinanceEvalError::Domain(match error {
        FinancialError::Num | FinancialError::NoConvergence => WorksheetErrorCode::Num,
        FinancialError::Div0 => WorksheetErrorCode::Div0,
        FinancialError::Value => WorksheetErrorCode::Value,
    })
}

/// The shared principal span, with the existing fallback outside its quotient domain.
fn principal_sum(
    rate: f64,
    periods: f64,
    pv: f64,
    start_period: i32,
    end_period: i32,
    timing: PaymentTiming,
) -> Result<f64, CumulativeFinanceEvalError> {
    if let Some(total) =
        time_value::cumulative_principal(rate, periods, pv, start_period, end_period, timing)
    {
        return Ok(total);
    }
    let beginning_payment = if timing == PaymentTiming::BeginningOfPeriod {
        Some(time_value::pmt(rate, periods, pv, 0.0, timing).map_err(kernel_error)?)
    } else {
        None
    };
    let mut total = 0.0;
    for period in start_period..=end_period {
        let principal = if let Some(payment) = beginning_payment {
            // Keep the previous numeric admission as well as its expression.
            // CUMPRINC is not a sum of the public PPMT publications in Excel.
            let interest = time_value::ipmt(rate, period as f64, periods, pv, 0.0, timing)
                .map_err(kernel_error)?;
            if period == 1 {
                payment - interest
            } else {
                time_value::discounted_payment_principal(
                    rate,
                    period as f64,
                    periods,
                    payment,
                    0.0,
                    timing,
                )
            }
        } else {
            time_value::ppmt(rate, period as f64, periods, pv, 0.0, timing).map_err(kernel_error)?
        };
        if !principal.is_finite() {
            return Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num));
        }
        total += principal;
    }
    Ok(total)
}

/// CUMIPMT stores both the count-times-payment result and its subtraction from PC64.
/// Eleven fresh count-product ties (3665 and above) distinguish it from strict
/// multiplication, conditioned on independent public PMT and CUMPRINC operands.
/// Seventeen further count-one ties isolate the subtraction from multiplication.
fn interest_finish(count: f64, payment: f64, principal: f64) -> f64 {
    use crate::ported::excel_numeric::{excel_x87_mul, excel_x87_sub};
    excel_x87_sub(excel_x87_mul(count, payment), principal)
}

fn validate_cumulative_inputs(
    rate: f64,
    periods: f64,
    pv: f64,
    start_period: f64,
    end_period: f64,
) -> Result<(), CumulativeFinanceEvalError> {
    validate_finite(&[rate, pv])?;
    if rate <= 0.0 || pv <= 0.0 {
        return Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num));
    }
    // The bounds compare the numbers as given: an end past a fractional nper is #NUM!.
    if start_period > end_period || end_period > periods {
        return Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num));
    }
    Ok(())
}

pub fn cumipmt_kernel(
    rate: f64,
    periods: f64,
    pv: f64,
    start_period: f64,
    end_period: f64,
    type_number: f64,
) -> Result<f64, CumulativeFinanceEvalError> {
    let periods = period_argument(periods)?;
    let start_period = period_argument(start_period)?;
    let end_period = period_argument(end_period)?;
    let timing = payment_timing_arg(type_number)?;
    validate_cumulative_inputs(rate, periods, pv, start_period, end_period)?;
    let (start_period, end_period) = whole_periods(start_period, end_period);

    // dc-j9k: Excel's CUMIPMT is the payments less the principal, count * PMT - sum(PPMT), not
    // the sum of IPMT. Over 400 Excel readings (rates 0.001 to 0.25, up to 400 periods, both
    // types) it matches 131 of 195 CUMIPMT bit for bit against 53 for the sum of IPMT, and the
    // others are within a few ulp.
    let payment = time_value::pmt(rate, periods, pv, 0.0, timing).map_err(kernel_error)?;
    // Beginning payments have no first-period interest. Excel omits that
    // period from both the payment count and the principal sum: all252
    // eight-period pairs and216 independent longer-loan pairs agree.
    let start_period = if timing == PaymentTiming::BeginningOfPeriod && start_period == 1 {
        2
    } else {
        start_period
    };
    if start_period > end_period {
        return Ok(0.0);
    }
    let principal = principal_sum(rate, periods, pv, start_period, end_period, timing)?;
    let total = interest_finish((end_period - start_period + 1) as f64, payment, principal);
    if total.is_finite() {
        Ok(total)
    } else {
        Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num))
    }
}

pub fn cumprinc_kernel(
    rate: f64,
    periods: f64,
    pv: f64,
    start_period: f64,
    end_period: f64,
    type_number: f64,
) -> Result<f64, CumulativeFinanceEvalError> {
    let periods = period_argument(periods)?;
    let start_period = period_argument(start_period)?;
    let end_period = period_argument(end_period)?;
    let timing = payment_timing_arg(type_number)?;
    validate_cumulative_inputs(rate, periods, pv, start_period, end_period)?;
    let (start_period, end_period) = whole_periods(start_period, end_period);
    if start_period > end_period {
        return Ok(0.0);
    }

    let total = principal_sum(rate, periods, pv, start_period, end_period, timing)?;
    if total.is_finite() {
        Ok(total)
    } else {
        Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num))
    }
}

pub fn map_cumulative_finance_error_to_ws(
    error: &CumulativeFinanceEvalError,
) -> WorksheetErrorCode {
    match error {
        CumulativeFinanceEvalError::ArityMismatch { .. } => WorksheetErrorCode::Value,
        CumulativeFinanceEvalError::Domain(code) => *code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::value::{CalcArray, CalcValue, CoreValue, ExcelText};

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

    /// Recorded public PMT and CUMPRINC operands isolate the finish; the
    /// principal producer remains a separate, unresolved calculation.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cumulative_interest_finish_stores_its_count_product() {
        for (count, payment, principal, interest) in [
            (
                5275.0,
                0xbff4_51a5_7553_a993u64,
                0xc0a0_fac2_640c_097e,
                0xc0b1_ad84_9383_29b3,
            ),
            (
                3665.0,
                0xbfb4_54e1_e6d9_c2b1,
                0xc055_351e_1a7d_fb0e,
                0xc069_c7d7_b5ff_7401,
            ),
            (
                4527.0,
                0xbfb4_54e1_e6d9_c2b1,
                0xc05b_bb33_94ab_0301,
                0xc06f_1387_0dd3_17bc,
            ),
        ] {
            let payment = f64::from_bits(payment);
            let principal = f64::from_bits(principal);
            assert_ne!((count * payment - principal).to_bits(), interest);
            assert_eq!(
                interest_finish(count, payment, principal).to_bits(),
                interest
            );
        }
    }

    /// Fresh count-one Excel20527 answers isolate the final subtraction.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cumulative_interest_finish_stores_its_subtraction() {
        for (payment, principal, interest) in [
            (
                0xc35f_ffff_ffff_ffffu64,
                0xbfff_ffff_ffff_fff4,
                0xc35f_ffff_ffff_fffe,
            ),
            (
                0xc49f_ffff_ffff_fffe,
                0xc140_0000_0000_001a,
                0xc49f_ffff_ffff_fffe,
            ),
            (
                0xc4a0_0000_0000_0000,
                0xc140_0000_0000_001b,
                0xc4a0_0000_0000_0000,
            ),
        ] {
            let payment = f64::from_bits(payment);
            let principal = f64::from_bits(principal);
            assert_ne!((payment - principal).to_bits(), interest);
            assert_eq!(interest_finish(1.0, payment, principal).to_bits(), interest);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cumipmt_matches_microsoft_seed_rows() {
        let rate = 0.09 / 12.0;
        let periods = 30.0 * 12.0;
        let pv = 125000.0;
        assert_close(
            cumipmt_kernel(rate, periods, pv, 13.0, 24.0, 0.0).expect("cumipmt second year"),
            -11135.23213,
            1.0e-5,
        );
        assert_close(
            cumipmt_kernel(rate, periods, pv, 1.0, 1.0, 0.0).expect("cumipmt first month"),
            -937.5,
            1.0e-9,
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cumprinc_matches_microsoft_seed_rows() {
        let rate = 0.09 / 12.0;
        let periods = 30.0 * 12.0;
        let pv = 125000.0;
        assert_close(
            cumprinc_kernel(rate, periods, pv, 13.0, 24.0, 0.0).expect("cumprinc second year"),
            -934.1071234,
            1.0e-6,
        );
        assert_close(
            cumprinc_kernel(rate, periods, pv, 1.0, 1.0, 0.0).expect("cumprinc first month"),
            -68.27827118,
            1.0e-8,
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cumulative_interest_and_principal_partition_total_payment() {
        let rate = 0.09 / 12.0;
        let periods = 30.0 * 12.0;
        let pv = 125000.0;
        let payment = time_value::pmt(rate, periods, pv, 0.0, PaymentTiming::EndOfPeriod)
            .expect("payment should succeed");
        let interest =
            cumipmt_kernel(rate, periods, pv, 13.0, 24.0, 0.0).expect("cumipmt should succeed");
        let principal =
            cumprinc_kernel(rate, periods, pv, 13.0, 24.0, 0.0).expect("cumprinc should succeed");
        assert_close(interest + principal, payment * 12.0, 1.0e-8);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn type_one_has_zero_interest_in_first_period() {
        let rate = 0.09 / 12.0;
        let periods = 30.0 * 12.0;
        let pv = 125000.0;
        assert_close(
            cumipmt_kernel(rate, periods, pv, 1.0, 1.0, 1.0).expect("type one cumipmt"),
            0.0,
            1.0e-12,
        );
    }

    // BUG-FUNC-034: annuity-due (type=1) interest for ranges touching per>=2 must
    // subtract the beginning-of-period payment. Closed-form pins are derived from
    // the corrected per-period formula; fixed-pending Excel bit-pinning.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn type_one_range_witness_candidates_pending_excel_bit_pinning() {
        let rate = 0.09 / 12.0;
        let periods = 360.0;
        let pv = 125000.0;

        assert_close(
            cumipmt_kernel(rate, periods, pv, 1.0, 3.0, 1.0).expect("cumipmt type1 1..3"),
            -1859.5135466458305,
            1.0e-7,
        );
        assert_close(
            cumipmt_kernel(rate, periods, pv, 2.0, 3.0, 1.0).expect("cumipmt type1 2..3"),
            -1859.5135466458305,
            1.0e-7,
        );
        assert_close(
            cumipmt_kernel(rate, periods, pv, 1.0, 12.0, 1.0).expect("cumipmt type1 1..12"),
            -10201.332884494608,
            1.0e-6,
        );

        assert_close(
            cumprinc_kernel(rate, periods, pv, 1.0, 3.0, 1.0).expect("cumprinc type1 1..3"),
            -1135.3597174166357,
            1.0e-7,
        );
        assert_close(
            cumprinc_kernel(rate, periods, pv, 2.0, 3.0, 1.0).expect("cumprinc type1 2..3"),
            -137.06862939581367,
            1.0e-7,
        );
        assert_close(
            cumprinc_kernel(rate, periods, pv, 1.0, 12.0, 1.0).expect("cumprinc type1 1..12"),
            -1778.1601717552567,
            1.0e-6,
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn type_one_interest_and_principal_partition_total_payment() {
        let rate = 0.09 / 12.0;
        let periods = 360.0;
        let pv = 125000.0;
        let payment = time_value::pmt(rate, periods, pv, 0.0, PaymentTiming::BeginningOfPeriod)
            .expect("type-one payment");
        let interest = cumipmt_kernel(rate, periods, pv, 2.0, 3.0, 1.0).expect("type-one cumipmt");
        let principal =
            cumprinc_kernel(rate, periods, pv, 2.0, 3.0, 1.0).expect("type-one cumprinc");
        assert_close(interest + principal, payment * 2.0, 1.0e-8);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn invalid_type_is_num_error() {
        assert_eq!(
            cumipmt_kernel(0.09 / 12.0, 360.0, 125000.0, 1.0, 1.0, 2.0),
            Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num))
        );
    }

    /// Excel 16.0.20527 keeps a fractional nper and sums the whole periods between fractional
    /// start and end periods: CUMIPMT(0.0075,360.8,125000,13.9,24.9,0) is the same
    /// 0xc0c3ee6a32355f50 as CUMIPMT(0.0075,360.8,125000,14,24,0), and neither is the
    /// 0xc0c5bf9db675dfa1 of CUMIPMT(0.0075,360,125000,13,24,0). The same holds for CUMPRINC.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn period_arguments_keep_nper_and_sum_whole_periods() {
        let fractional = cumipmt_kernel(0.09 / 12.0, 360.8, 125000.0, 13.9, 24.9, 0.0)
            .expect("fractional periods should succeed");
        let whole = cumipmt_kernel(0.09 / 12.0, 360.8, 125000.0, 14.0, 24.0, 0.0)
            .expect("whole periods should succeed");
        let truncated = cumipmt_kernel(0.09 / 12.0, 360.0, 125000.0, 13.0, 24.0, 0.0)
            .expect("truncated periods should succeed");
        assert_bits(fractional, whole);
        assert_close(fractional, f64::from_bits(0xc0c3_ee6a_3235_5f50), 1.0e-8);
        assert!((fractional - truncated).abs() > 900.0);
        let fractional = cumprinc_kernel(0.09 / 12.0, 360.8, 125000.0, 13.9, 24.9, 0.0)
            .expect("fractional periods should succeed");
        let whole = cumprinc_kernel(0.09 / 12.0, 360.8, 125000.0, 14.0, 24.0, 0.0)
            .expect("whole periods should succeed");
        assert_bits(fractional, whole);
        assert_close(fractional, f64::from_bits(0xc08a_af73_3376_49b4), 1.0e-9);
        // An interval holding no whole period is zero; a start below one or an end past nper
        // is #NUM!.
        assert_eq!(
            cumprinc_kernel(0.1013, 352.0, 79905.9, 1.5, 1.5, 0.0),
            Ok(0.0)
        );
        assert_eq!(
            cumprinc_kernel(0.1013, 352.0, 79905.9, 0.999, 1.0, 0.0),
            Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num))
        );
        assert_eq!(
            cumprinc_kernel(0.1013, 352.0, 79905.9, 352.0, 352.9, 0.0),
            Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num))
        );
        assert!(cumprinc_kernel(0.1013, 352.5, 79905.9, 352.0, 352.5, 0.0).is_ok());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cumipmt_and_cumprinc_exactness_witness_rows_match_excel_targets() {
        // Excel (2026-10-25): CUMPRINC(0.05/12,360,200000,1,12,0) is 0xc0a70d761d260042 and
        // CUMIPMT(...) 0xc0c3667e7f577146, 12 * PMT less that sum (the sum of its twelve IPMT
        // readings is 0xc0c3667e7f577147). The discounted EXPM1 span and the separate
        // ending-payment store reproduce both publications (dc-j9k.49).
        let cumipmt_actual =
            cumipmt_kernel(0.05 / 12.0, 360.0, 200000.0, 1.0, 12.0, 0.0).expect("cumipmt witness");
        assert_bits(cumipmt_actual, f64::from_bits(0xc0c3667e7f577146));

        let cumprinc_actual = cumprinc_kernel(0.05 / 12.0, 360.0, 200000.0, 1.0, 12.0, 0.0)
            .expect("cumprinc witness");
        let cumprinc_excel = f64::from_bits(0xc0a70d761d260042);
        assert_bits(cumprinc_actual, cumprinc_excel);

        // The shared PMT witness stays Excel-exact.
        let payment = time_value::pmt(
            0.05 / 12.0,
            360.0,
            200000.0,
            0.0,
            PaymentTiming::EndOfPeriod,
        )
        .expect("cumulative payment witness");
        let payment_excel_target = f64::from_bits(0xc090c692af15f63a);
        assert_bits(payment, payment_excel_target);
    }
}
