use crate::ported::excel_numeric::{
    excel_atan_reduced, excel_sqrt_staged, excel_x87_div, excel_x87_mul, excel_x87_sub,
};
use crate::value::WorksheetErrorCode;

/// ASIN; #NUM! outside -1..1.
pub fn asin_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    if !(-1.0..=1.0).contains(&n) {
        return Err(WorksheetErrorCode::Num);
    }
    // The second factor reuses the published difference. Computing 1+n
    // independently loses the observed asymmetry for negative inputs.
    // Each arithmetic primitive publishes binary64 after RN64 evaluation.
    let t = excel_x87_sub(1.0, n);
    let u = excel_x87_sub(2.0, t);
    let root = excel_sqrt_staged(excel_x87_mul(t, u));
    let angle = excel_atan_reduced(excel_x87_div(n, root));
    Ok(if angle.abs() < f64::MIN_POSITIVE {
        0.0
    } else {
        angle
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn asin_matches_live_excel_pins() {
        assert_eq!(asin_kernel(0.5).unwrap().to_bits(), 0x3fe0c152382d7366);
        assert_eq!(asin_kernel(1.0).unwrap().to_bits(), 0x3ff921fb54442d18);
    }
}
