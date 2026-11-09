use crate::value::WorksheetErrorCode;

pub fn csch_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    let sinh = crate::ported::sinh::sinh_kernel(n);
    if sinh == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    // Live Excel 16.0 b20326: CSCH(x)=1/SINH(x) 5/5; follows the SINH kernel.
    let value = crate::ported::excel_numeric::excel_x87_recip(sinh);
    Ok(if value.abs() < f64::MIN_POSITIVE {
        0.0
    } else {
        value
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn csch_kernel_zero_is_div0() {
        assert_eq!(csch_kernel(0.0), Err(WorksheetErrorCode::Div0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn csch_matches_live_excel_one_over_sinh_pins() {
        // Live Excel 16.0 b20326: CSCH(x)=1/SINH(x) 5/5.
        assert_eq!(csch_kernel(0.5).unwrap().to_bits(), 0x3ffeb45dc88defed);
        assert_eq!(csch_kernel(1.0).unwrap().to_bits(), 0x3feb3ab8a78b90c1);
    }
}
