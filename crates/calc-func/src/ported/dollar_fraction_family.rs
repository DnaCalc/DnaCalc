use crate::ported::excel_numeric::excel_x87_add_quotient;
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
    // A denominator above 1e308 has no decimal scale (10^309): #NUM!, in both functions (Excel
    // 20430: `=DOLLARFR(0.5,1.0000134e308)`; 1e308 itself still calculates).
    if truncated > 1e308 {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(truncated)
}

/// The smallest power of ten that is at least the denominator: 10^ceil(log10(d)). Excel uses
/// this, not 10^(digit count): the two differ exactly when d is a power of ten
/// (`DOLLARDE(3.69, 10)` is 3.69, not 9.9; `DOLLARDE(x, 1)` is x). Live Excel 20430, W111-5
/// G8-01.
fn decimal_scale(denominator: f64) -> f64 {
    let d = denominator.abs();
    // The exponent is the smallest k with 10^k >= d; the power itself is Excel's x87 integer
    // power, which is one ULP above the correctly rounded 10^126 and equal to it elsewhere (a
    // product of tens drifts from 1e23 on: Excel 20430, dc-j9k.361/.363/.364).
    let exact = |k: u32| format!("1e{k}").parse::<f64>().unwrap_or(f64::INFINITY);
    let mut k = if d > 1.0 { d.log10().ceil() as u32 } else { 0 };
    while k > 0 && exact(k - 1) >= d {
        k -= 1;
    }
    while exact(k) < d {
        k += 1;
    }
    crate::ported::excel_numeric::excel_x87_pow10(k)
}

pub fn dollarde_kernel(number: f64, fraction: f64) -> Result<f64, WorksheetErrorCode> {
    let denominator = normalized_fraction_denominator(fraction)?;
    let scale = decimal_scale(denominator);
    let whole = number.trunc();
    let fractional = number - whole;
    // Binary64 after every step: the pinned W111 rows DOLLARDE(3.7.., 26.9..) and
    // DOLLARDE(6.4.., 22.2..) fail with the quotient held at precision 64, as DOLLARFR needs.
    Ok(whole + fractional * scale / denominator)
}

pub fn dollarfr_kernel(number: f64, fraction: f64) -> Result<f64, WorksheetErrorCode> {
    let denominator = normalized_fraction_denominator(fraction)?;
    let scale = decimal_scale(denominator);
    let whole = number.trunc();
    let fractional = number - whole;
    Ok(excel_x87_add_quotient(whole, fractional * denominator, scale))
}

pub fn map_dollar_fraction_error_to_ws(e: &DollarFractionEvalError) -> WorksheetErrorCode {
    match e {
        DollarFractionEvalError::ArityMismatch { .. } => WorksheetErrorCode::Value,
        DollarFractionEvalError::MissingArg => WorksheetErrorCode::NA,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::value::{CalcArray, CalcValue, CoreValue, ExcelText};

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

    /// Excel 20430: a denominator above 1e308 is #NUM! in both functions, 1e308 itself is not.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn denominators_above_1e308_are_num() {
        let b = f64::from_bits;
        for denominator in [0x7fe1cd0333333332, 0x7fefffffffffffff] {
            assert_eq!(dollarfr_kernel(0.5, b(denominator)), Err(WorksheetErrorCode::Num));
            assert_eq!(dollarde_kernel(0.5, b(denominator)), Err(WorksheetErrorCode::Num));
        }
        assert_eq!(dollarfr_kernel(0.5, b(0x7fe1ccf385ebc8a0)).unwrap().to_bits(), 0x3fe0000000000000);
    }

    /// Excel's scale for a denominator past 1e125: 10^126 is one ULP above the correctly rounded
    /// power (probed for 308 exponents, live Excel 20430).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn decimal_scale_is_the_x87_power_of_ten() {
        for k in [1, 22, 23, 37, 125, 127, 210, 283, 291, 308] {
            let exact: f64 = format!("1e{k}").parse().unwrap();
            assert_eq!(decimal_scale(exact).to_bits(), exact.to_bits(), "k = {k}");
        }
        let exact: f64 = "1e126".parse().unwrap();
        assert_eq!(decimal_scale(exact).to_bits(), exact.to_bits() + 1);
        // 0.7 / 1e126 scaled: DOLLARDE(0.7, 1e126) as Excel answers it.
        assert_eq!(dollarde_kernel(0.7, exact).unwrap().to_bits(), (0.7 * decimal_scale(exact) / exact).to_bits());
    }

    /// Fuzz patient zero (dc-j9k.361, .363, .364), live Excel 20430 bits: for a denominator past
    /// 1e22 the scale must be the correctly rounded power of ten, not a product of tens.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn huge_denominators_use_the_correctly_rounded_power_of_ten() {
        let b = f64::from_bits;
        assert_eq!(
            dollarde_kernel(b(0xbf97398605e69c5f), b(0x7c421d140d3c5d54)).unwrap().to_bits(),
            0xbfb0722bf6b72ec7
        );
        assert_eq!(
            dollarfr_kernel(b(0x3e9f97ad6940b829), b(0x7a99f53b0954a464)).unwrap().to_bits(),
            0x3e87d1550a802d6f
        );
        // One more row: whole + product / scale held at precision 64 until the final store.
        assert_eq!(
            dollarfr_kernel(b(0x41e79d94748c3b30), b(0x4d95829bb490e006)).unwrap().to_bits(),
            0x41e79d947486ed42
        );
        assert_eq!(
            dollarfr_kernel(b(0x3ea326d507f8f1fe), b(0x7a99f53b0954a464)).unwrap().to_bits(),
            0x3e8ce08784df9fc1
        );
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
