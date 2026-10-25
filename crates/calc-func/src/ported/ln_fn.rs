use crate::value::WorksheetErrorCode;

pub fn ln_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    if n <= 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    // Excel's LN is the x87 `fldln2` + `fyl2x` fused op; match it bit-for-bit.
    Ok(crate::ported::excel_numeric::excel_log(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn ln_kernel_zero_is_num() {
        assert_eq!(ln_kernel(0.0), Err(WorksheetErrorCode::Num));
    }
}
