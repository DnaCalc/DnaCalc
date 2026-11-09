use crate::value::WorksheetErrorCode;

pub fn sech_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    // Live Excel 16.0 b20326: SECH(x)=1/COSH(x) 40/40 on the COSH EXP-pair grid.
    let result = crate::ported::excel_numeric::excel_x87_recip(crate::ported::cosh::cosh_kernel(n));
    Ok(if result.abs() < f64::MIN_POSITIVE {
        0.0
    } else {
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sech_kernel_zero_is_one() {
        assert_eq!(sech_kernel(0.0), Ok(1.0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sech_matches_live_excel_one_over_cosh_pins() {
        // Live Excel 16.0 b20326: SECH(x)=1/COSH(x) 5/5.
        assert_eq!(sech_kernel(0.5).unwrap().to_bits(), 0x3fec60d1ff040dd1);
        assert_eq!(sech_kernel(1.0).unwrap().to_bits(), 0x3fe4bcdc50ed6be8);
    }
}
