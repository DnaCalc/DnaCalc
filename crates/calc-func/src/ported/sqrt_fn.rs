use crate::value::WorksheetErrorCode;

pub fn sqrt_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    if n < 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(crate::ported::excel_numeric::excel_sqrt_staged(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sqrt_kernel_negative_is_num() {
        assert_eq!(sqrt_kernel(-1.0), Err(WorksheetErrorCode::Num));
    }
}
