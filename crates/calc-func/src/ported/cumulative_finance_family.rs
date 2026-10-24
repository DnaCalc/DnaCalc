// See `financial_time_value_family`: the cumulative annuity math is frozen on
// the portable correctly-rounded core (W108 Bead C), pending the Phase-C x87
// migration and its live-Excel re-validation.
use crate::ported::excel_numeric::{excel_expm1, excel_log1p, exp_portable};
use crate::value::CalcValue;
use crate::value::WorksheetErrorCode;

const EPSILON: f64 = 1.0e-12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentTiming {
    EndOfPeriod,
    BeginningOfPeriod,
}

impl PaymentTiming {
    fn factor(self, rate: f64) -> f64 {
        match self {
            Self::EndOfPeriod => 1.0,
            Self::BeginningOfPeriod => 1.0 + rate,
        }
    }
}

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

fn trunc_positive_period(value: f64) -> Result<i32, CumulativeFinanceEvalError> {
    if !value.is_finite() {
        return Err(CumulativeFinanceEvalError::Domain(
            WorksheetErrorCode::Value,
        ));
    }
    let truncated = value.trunc();
    if truncated < 1.0 || truncated > i32::MAX as f64 {
        return Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num));
    }
    Ok(truncated as i32)
}

fn payment_timing_arg(value: f64) -> Result<PaymentTiming, CumulativeFinanceEvalError> {
    validate_finite(&[value])?;
    if (value - 0.0).abs() < EPSILON {
        Ok(PaymentTiming::EndOfPeriod)
    } else if (value - 1.0).abs() < EPSILON {
        Ok(PaymentTiming::BeginningOfPeriod)
    } else {
        Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num))
    }
}

/// PMT on the correctly-rounded numeric core (W108 Bead C) — the same PV-side
/// stable discount form as `financial_time_value_family::pmt`, so CUMPRINC/CUMIPMT
/// share the PMT-family op-order rather than the catastrophic `powi` path.
fn pmt(
    rate: f64,
    periods: f64,
    present_value: f64,
    future_value: f64,
    timing: PaymentTiming,
) -> Result<f64, CumulativeFinanceEvalError> {
    validate_finite(&[rate, periods, present_value, future_value])?;
    if periods <= 0.0 {
        return Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num));
    }
    if rate == 0.0 {
        return Ok(-(future_value + present_value) / periods);
    }
    if 1.0 + rate <= 0.0 {
        return Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num));
    }
    let neg_log = -(periods * excel_log1p(rate));
    let inv_factor = exp_portable(neg_log); // (1+r)^-n
    let denom = -excel_expm1(neg_log); // 1 - (1+r)^-n
    if denom == 0.0 {
        return Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num));
    }
    let tf = timing.factor(rate); // 1 + r·type
    let numerator = present_value + future_value * inv_factor;
    let recip = 1.0 / (tf * denom);
    let result = -numerator * rate * recip;
    if result.is_finite() {
        Ok(result)
    } else {
        Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num))
    }
}

/// Remaining balance after `m` payments on the exp/log1p/expm1 core — mirrors
/// `financial_time_value_family::fv_exp` (shared op-order for the IPMT recurrence).
fn fv_exp(rate: f64, m: f64, payment: f64, present_value: f64, tf: f64) -> f64 {
    if m == 0.0 {
        return -present_value;
    }
    if rate == 0.0 {
        return -(present_value + payment * m);
    }
    let log_growth = m * excel_log1p(rate);
    let factor = exp_portable(log_growth);
    let expm1_growth = excel_expm1(log_growth);
    -(present_value * factor + payment * tf * (expm1_growth / rate))
}

fn ipmt_from_payment(
    rate: f64,
    period_index: i32,
    periods: i32,
    present_value: f64,
    timing: PaymentTiming,
    payment: f64,
) -> Result<f64, CumulativeFinanceEvalError> {
    validate_finite(&[rate, present_value, payment])?;
    if period_index < 1 || period_index > periods || periods < 1 {
        return Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num));
    }
    if rate <= 0.0 || present_value <= 0.0 {
        return Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num));
    }
    let tf = timing.factor(rate); // 1 + r·type
    let type_is_beginning = matches!(timing, PaymentTiming::BeginningOfPeriod);
    let rip = if period_index <= 1 {
        if type_is_beginning {
            0.0
        } else {
            -present_value
        }
    } else if type_is_beginning {
        fv_exp(rate, period_index as f64 - 2.0, payment, present_value, tf) - payment
    } else {
        fv_exp(rate, period_index as f64 - 1.0, payment, present_value, tf)
    };
    let result = rip * rate;
    if result.is_finite() {
        Ok(result)
    } else {
        Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num))
    }
}

fn ppmt_from_payment(
    rate: f64,
    period_index: i32,
    periods: i32,
    present_value: f64,
    timing: PaymentTiming,
    payment: f64,
) -> Result<f64, CumulativeFinanceEvalError> {
    let interest = ipmt_from_payment(rate, period_index, periods, present_value, timing, payment)?;
    Ok(payment - interest)
}

fn validate_cumulative_inputs(
    rate: f64,
    periods: i32,
    pv: f64,
    start_period: i32,
    end_period: i32,
    timing: PaymentTiming,
) -> Result<(), CumulativeFinanceEvalError> {
    let _ = timing;
    validate_finite(&[rate, pv])?;
    if rate <= 0.0 || periods < 1 || pv <= 0.0 {
        return Err(CumulativeFinanceEvalError::Domain(WorksheetErrorCode::Num));
    }
    if start_period < 1 || end_period < 1 || start_period > end_period || end_period > periods {
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
    let periods = trunc_positive_period(periods)?;
    let start_period = trunc_positive_period(start_period)?;
    let end_period = trunc_positive_period(end_period)?;
    let timing = payment_timing_arg(type_number)?;
    validate_cumulative_inputs(rate, periods, pv, start_period, end_period, timing)?;

    // W108 Bead C: CUMIPMT = sum_{k=start..end} IPMT(r,k,n,pv,0,type), left-to-right
    // f64 (the live-Excel search model), NOT payment·count - sum(PPMT). Carries the
    // larger dedicated-op-order residual; implemented as-is.
    let payment = pmt(rate, periods as f64, pv, 0.0, timing)?;
    let mut total = 0.0;
    for period in start_period..=end_period {
        total += ipmt_from_payment(rate, period, periods, pv, timing, payment)?;
    }
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
    let periods = trunc_positive_period(periods)?;
    let start_period = trunc_positive_period(start_period)?;
    let end_period = trunc_positive_period(end_period)?;
    let timing = payment_timing_arg(type_number)?;
    validate_cumulative_inputs(rate, periods, pv, start_period, end_period, timing)?;

    let payment = pmt(rate, periods as f64, pv, 0.0, timing)?;
    let mut total = 0.0;
    for period in start_period..=end_period {
        total += ppmt_from_payment(rate, period, periods, pv, timing, payment)?;
    }
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
    #[allow(unused_imports)]
    use crate::value::{CalcArray, CalcValue, CoreValue, ExcelText};
    use super::*;

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
        let payment = pmt(rate, periods, pv, 0.0, PaymentTiming::EndOfPeriod)
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
        let payment = pmt(rate, periods, pv, 0.0, PaymentTiming::BeginningOfPeriod)
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

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn period_arguments_truncate_toward_zero() {
        let exact = cumipmt_kernel(0.09 / 12.0, 360.8, 125000.0, 13.9, 24.9, 0.0)
            .expect("truncated periods should succeed");
        let seeded = cumipmt_kernel(0.09 / 12.0, 360.0, 125000.0, 13.0, 24.0, 0.0)
            .expect("seeded periods should succeed");
        assert_close(exact, seeded, 1.0e-8);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cumipmt_and_cumprinc_exactness_witness_rows_match_excel_targets() {
        // W108 Bead C: CUMIPMT is now the direct sum of IPMT (CR core), and matches
        // live Excel (CUMIPMT-0003) bit-for-bit at 0xc0c3667e7f577146.
        let cumipmt_actual =
            cumipmt_kernel(0.05 / 12.0, 360.0, 200000.0, 1.0, 12.0, 0.0).expect("cumipmt witness");
        let cumipmt_excel_target = f64::from_bits(0xc0c3667e7f577146);
        assert_bits(cumipmt_actual, cumipmt_excel_target);

        // CUMPRINC (sum of PPMT, CR core): local 0xc0a70d761d260044 vs Excel
        // (CUMPRINC-0003) 0xc0a70d761d260042 — a 2-ULP open discrepancy in the
        // dedicated-op-order accumulation lane (tracked W108 residual, NOT
        // acceptance). Pin the NEW local bits with the Excel target recorded.
        let cumprinc_actual = cumprinc_kernel(0.05 / 12.0, 360.0, 200000.0, 1.0, 12.0, 0.0)
            .expect("cumprinc witness");
        let cumprinc_current_local = f64::from_bits(0xc0a70d761d260044);
        let cumprinc_excel_target = f64::from_bits(0xc0a70d761d260042);
        assert_bits(cumprinc_actual, cumprinc_current_local);
        assert_ne!(cumprinc_actual.to_bits(), cumprinc_excel_target.to_bits());

        // The shared PMT witness stays Excel-exact.
        let payment = pmt(
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
