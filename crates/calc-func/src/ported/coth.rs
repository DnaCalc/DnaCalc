use crate::ported::real_policy::ExcelRealPolicy;
use crate::value::WorksheetErrorCode;

pub fn coth_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    let t = crate::ported::tanh::tanh_kernel(n);
    if t == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    // W111 retains the reciprocal of the published TANH composition, including
    // its cancellation-safe small-argument denominator. Non-finite 1/TANH
    // (large |n|) saturates to sign(n) under COTH's real policy.
    ExcelRealPolicy::SATURATE_SIGN.publish(n, crate::ported::excel_numeric::excel_x87_recip(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn coth_kernel_zero_is_div0() {
        assert_eq!(coth_kernel(0.0), Err(WorksheetErrorCode::Div0));
    }

    // BUG-FUNC-027 CLASS-C3.h: large |n| saturates to ±1 (live Excel COTH(800)=1).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn coth_kernel_large_argument_saturates() {
        assert_eq!(coth_kernel(800.0), Ok(1.0));
        assert_eq!(coth_kernel(-800.0), Ok(-1.0));
        assert_eq!(coth_kernel(1.0).unwrap().to_bits(), 0x3ff50231499b6b1e);
        assert_eq!(coth_kernel(0.5).unwrap().to_bits(), 0x40014fc6ceb099bf);
        assert_eq!(coth_kernel(-0.5).unwrap().to_bits(), 0xc0014fc6ceb099bf);
    }
}
