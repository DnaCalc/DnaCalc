use crate::value::WorksheetErrorCode;

/// W109 G4-01: the fFCOS π/2-quadrant chain with the |x| < 2^-26 -> 1.0
/// shortcut (see `excel_numeric::excel_cos`).
pub fn cos_kernel(n: f64) -> f64 {
    crate::ported::excel_numeric::excel_cos(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cos_kernel_matches_std() {
        assert_eq!(cos_kernel(1.0), 1.0f64.cos());
    }
}
