use crate::value::WorksheetErrorCode;

pub fn acot_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    // W111 distinguishes this reciprocal/angle graph from direct atan2 and
    // from a binary64-only reciprocal. It is an observed arithmetic model,
    // not a claim about Excel internals or universal FPATAN parity.
    if n == 0.0 {
        return Ok(std::f64::consts::FRAC_PI_2);
    }
    let reciprocal = crate::ported::excel_numeric::excel_x87_recip(n);
    let angle = crate::ported::excel_numeric::excel_atan_reduced(reciprocal);
    let result = if n < 0.0 {
        angle + std::f64::consts::PI
    } else {
        angle
    };
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
    fn acot_kernel_zero_is_pi_over_two() {
        assert_eq!(acot_kernel(0.0), Ok(std::f64::consts::FRAC_PI_2));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn acot_matches_live_excel_pins() {
        // Live Excel 16.0 b20326 ATAN2(x,1) identity.
        assert_eq!(acot_kernel(0.5).unwrap().to_bits(), 0x3ff1b6e192ebbe44);
        assert_eq!(acot_kernel(1.0).unwrap().to_bits(), 0x3fe921fb54442d18);
        assert_eq!(acot_kernel(2.0).unwrap().to_bits(), 0x3fddac670561bb4f);
        assert_eq!(acot_kernel(-2.0).unwrap().to_bits(), 0x40056c6e7397f5ae);
        assert_eq!(acot_kernel(4.0).unwrap().to_bits(), 0x3fcf5b75f92c80dd);
    }
}
