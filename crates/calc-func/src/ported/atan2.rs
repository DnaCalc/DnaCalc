use crate::value::WorksheetErrorCode;

pub fn atan2_kernel(x_num: f64, y_num: f64) -> Result<f64, WorksheetErrorCode> {
    if x_num == 0.0 && y_num == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    // Excel returns #NUM! when the implicit y/x ratio overflows to infinity
    // (x != 0 and |y| >> |x|), e.g. ATAN2(1e-200, 1e109). A finite ratio stays a
    // number, and the axis case x == 0 (ratio undefined, not overflowing) keeps
    // its ±pi/2 value. Pinned against live Excel 16.0 b20026 (BUG-FUNC-027 B3).
    if x_num != 0.0 && (y_num / x_num).is_infinite() {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(y_num.atan2(x_num))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn atan2_kernel_matches_excel_axis_order() {
        assert_eq!(atan2_kernel(1.0, 0.0), Ok(0.0));
        assert_eq!(atan2_kernel(0.0, 1.0), Ok(std::f64::consts::FRAC_PI_2));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn atan2_kernel_zero_vector_is_div0() {
        assert_eq!(atan2_kernel(0.0, 0.0), Err(WorksheetErrorCode::Div0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn atan2_overflowing_ratio_is_num_axis_stays_finite() {
        // BUG-FUNC-027 B3: Excel returns #NUM! when y/x overflows to infinity
        // (verified vs live Excel 16.0 b20026), while the axis case x==0 stays valid.
        assert_eq!(atan2_kernel(1e-200, 1e109), Err(WorksheetErrorCode::Num)); // y/x = inf
        assert_eq!(atan2_kernel(-1e-200, -6e199), Err(WorksheetErrorCode::Num)); // witness
        assert!(atan2_kernel(1e-200, 1e108).is_ok()); // y/x = 1e308, finite
        assert!(atan2_kernel(0.0, 5.0).is_ok()); // axis x==0 -> +pi/2, not #NUM!
        assert!(atan2_kernel(5.0, 0.0).is_ok()); // axis y==0 -> 0
    }
}
