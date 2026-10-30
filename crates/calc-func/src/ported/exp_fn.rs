use crate::ported::real_policy::ExcelRealPolicy;
use crate::value::WorksheetErrorCode;

pub fn exp_kernel(n: f64) -> f64 {
    // Excel's EXP is the x87 chain, not the platform `exp`; the
    // backend reproduces it bit-for-bit on x86_64. See `crate::ported::excel_numeric`.
    crate::ported::excel_numeric::excel_exp(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn exp_kernel_matches_std() {
        assert_eq!(exp_kernel(1.0), std::f64::consts::E);
    }

    // BUG-FUNC-027 / oxf-vgxs: live Excel EXP(1000)=#NUM! (overflow guard on the surface).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn exp_overflow_maps_to_num() {
        assert_eq!(
            ExcelRealPolicy::FINITE
                .publish(1000.0, exp_kernel(1000.0)),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            ExcelRealPolicy::FINITE.publish(1.0, exp_kernel(1.0)),
            Ok(std::f64::consts::E)
        );
    }
}
