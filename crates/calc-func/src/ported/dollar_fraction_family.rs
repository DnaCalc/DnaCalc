use crate::value::{CalcValue, CoreValue, WorksheetErrorCode};

#[derive(Debug, Clone, PartialEq)]
pub enum DollarFractionEvalError {
    ArityMismatch { expected: usize, actual: usize },
    MissingArg,
}

fn parse_numeric_text(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed = trimmed.parse::<f64>().ok()?;
    if parsed.is_finite() {
        Some(parsed)
    } else {
        None
    }
}

/// The fraction argument truncated to an integer, kept as a double: an `i32` saturates at
/// 2^31-1 and made `DOLLARDE(1.5, 1e10)` 3.328 where Excel gives 1.5.
fn normalized_fraction_denominator(fraction: f64) -> Result<f64, WorksheetErrorCode> {
    if fraction < 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    let truncated = fraction.trunc();
    if truncated == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    Ok(truncated)
}

/// The smallest power of ten that is at least the denominator: 10^ceil(log10(d)). Excel uses
/// this, not 10^(digit count): the two differ exactly when d is a power of ten
/// (`DOLLARDE(3.69, 10)` is 3.69, not 9.9; `DOLLARDE(x, 1)` is x). Live Excel 20430, W111-5
/// G8-01.
fn decimal_scale(denominator: f64) -> f64 {
    let d = denominator.abs();
    let mut scale = 1.0;
    while scale < d {
        scale *= 10.0;
    }
    scale
}

pub fn dollarde_kernel(number: f64, fraction: f64) -> Result<f64, WorksheetErrorCode> {
    let denominator = normalized_fraction_denominator(fraction)?;
    let scale = decimal_scale(denominator);
    let whole = number.trunc();
    let fractional = number - whole;
    Ok(whole + fractional * scale / denominator)
}

pub fn dollarfr_kernel(number: f64, fraction: f64) -> Result<f64, WorksheetErrorCode> {
    let denominator = normalized_fraction_denominator(fraction)?;
    let scale = decimal_scale(denominator);
    let whole = number.trunc();
    let fractional = number - whole;
    Ok(whole + fractional * denominator / scale)
}

pub fn map_dollar_fraction_error_to_ws(e: &DollarFractionEvalError) -> WorksheetErrorCode {
    match e {
        DollarFractionEvalError::ArityMismatch { .. } => WorksheetErrorCode::Value,
        DollarFractionEvalError::MissingArg => WorksheetErrorCode::NA,
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use crate::value::{CalcArray, CalcValue, CoreValue, ExcelText};
    use super::*;

    struct NoResolver;

    fn text_arg(s: &str) -> CalcValue {
        CalcValue::text(ExcelText::from_utf16_code_units(s.encode_utf16().collect()))
    }

    fn assert_close(got: f64, expected: f64) {
        assert!(
            (got - expected).abs() < 1e-12,
            "expected {expected}, got {got}"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dollarde_kernel_matches_native_seed_rows() {
        assert_close(dollarde_kernel(1.02, 16.0).unwrap(), 1.125);
        assert_close(dollarde_kernel(1.02, 8.0).unwrap(), 1.025);
        assert_close(dollarde_kernel(1.02, 16.9).unwrap(), 1.125);
        assert_close(dollarde_kernel(1.01, 32.0).unwrap(), 1.03125);
        assert_eq!(dollarde_kernel(1.02, 0.0), Err(WorksheetErrorCode::Div0));
        assert_eq!(dollarde_kernel(1.02, 0.9), Err(WorksheetErrorCode::Div0));
        assert_eq!(dollarde_kernel(1.02, -0.1), Err(WorksheetErrorCode::Num));
    }

    /// W111-5 G8-01 live Excel 20430 bits: the decimal scale is 10^ceil(log10(d)), so a
    /// power-of-ten or unit denominator leaves the number unchanged, and a huge denominator
    /// must not saturate.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dollarde_power_of_ten_and_huge_denominators_match_excel_bits() {
        let b = |h: u64| f64::from_bits(h);
        for (number, fraction) in [
            (0x400d_852c_82bc_cb80, 0x4024_0000_0000_0000), // DOLLARDE(3.690026.., 10)
            (0x4021_ed12_f088_5a20, 0x3ff0_0000_0000_0000), // DOLLARDE(8.963035.., 1)
            (0x3ff8_0000_0000_0000, 0x4202_a05f_2000_0000), // DOLLARDE(1.5, 1e10)
        ] {
            assert_eq!(
                dollarde_kernel(b(number), b(fraction)).unwrap().to_bits(),
                number,
                "Excel returns the number unchanged"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dollarfr_kernel_matches_native_seed_rows() {
        assert_close(dollarfr_kernel(1.125, 16.0).unwrap(), 1.02);
        assert_close(dollarfr_kernel(1.125, 8.0).unwrap(), 1.1);
        assert_close(dollarfr_kernel(1.125, 16.9).unwrap(), 1.02);
        assert_close(dollarfr_kernel(1.03125, 32.0).unwrap(), 1.01);
        assert_close(dollarfr_kernel(-1.125, 16.0).unwrap(), -1.02);
        assert_eq!(dollarfr_kernel(1.125, 0.0), Err(WorksheetErrorCode::Div0));
        assert_eq!(dollarfr_kernel(1.125, -1.0), Err(WorksheetErrorCode::Num));
    }
}
