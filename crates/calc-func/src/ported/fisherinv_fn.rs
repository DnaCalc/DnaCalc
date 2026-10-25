use crate::value::WorksheetErrorCode;

pub fn fisherinv_kernel(y: f64) -> Result<f64, WorksheetErrorCode> {
    // Inverse-problem identity, live Excel 16.0 b20228: FISHERINV(y) ==
    // (EXP(2*y)-1)/(EXP(2*y)+1) on 27/27 signed rows. FISHERINV is not
    // TANH (10/27). The algebraically equal 1-2/(EXP(2*y)+1) misses (9/27).
    let e = crate::ported::excel_numeric::excel_exp(2.0 * y);
    // BUG-FUNC-027 CLASS-A6: for large positive y, e=e^(2y) overflows to +Inf and
    // the direct form gives Inf/Inf = NaN; Excel saturates to +1. The large
    // negative side already saturates to -1 via e -> 0, so only +Inf needs a guard.
    if e.is_infinite() {
        return Ok(1.0);
    }
    Ok((e - 1.0) / (e + 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    // BUG-FUNC-027 CLASS-A6: live Excel 16.0 b20026 FISHERINV(817.81)=1.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fisherinv_saturates_at_large_argument() {
        assert_eq!(fisherinv_kernel(817.81), Ok(1.0));
        assert_eq!(fisherinv_kernel(714.11), Ok(1.0));
        // Large negative saturates to -1 through the existing formula (e -> 0).
        assert_eq!(fisherinv_kernel(-817.81), Ok(-1.0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fisherinv_interior_follows_worksheet_exp_identity() {
        let y = 0.5_f64;
        let e = crate::ported::excel_numeric::excel_exp(2.0 * y);
        assert_eq!(fisherinv_kernel(y), Ok((e - 1.0) / (e + 1.0)));
    }
}
