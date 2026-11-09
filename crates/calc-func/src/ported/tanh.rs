use crate::value::WorksheetErrorCode;

pub fn tanh_kernel(n: f64) -> f64 {
    // W111: small arguments reuse the cancellation-safe expm1 pair for the
    // denominator too. The public COSH result is observably different there.
    let denominator = if n.abs() < 1.0 {
        let sum = crate::ported::excel_numeric::excel_x87_add(
            crate::ported::excel_numeric::excel_expm1_internal(n),
            crate::ported::excel_numeric::excel_expm1_internal(-n),
        );
        crate::ported::excel_numeric::excel_x87_add(sum, 2.0) / 2.0
    } else {
        crate::ported::cosh::cosh_kernel(n)
    };
    crate::ported::excel_numeric::excel_x87_div(crate::ported::sinh::sinh_kernel(n), denominator)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn tanh_matches_live_excel_sinh_cosh_pins() {
        // Live Excel 16.0 b20326: TANH(x)=SINH(x)/COSH(x) 8/8.
        assert_eq!(tanh_kernel(0.5).to_bits(), 0x3fdd9353d7568af4);
        assert_eq!(tanh_kernel(1.0).to_bits(), 0x3fe85efab514f394);
        assert_eq!(tanh_kernel(-0.5).to_bits(), 0xbfdd9353d7568af4);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn tanh_kernel_matches_std() {
        assert_eq!(tanh_kernel(1.0), 1.0f64.tanh());
    }
}
