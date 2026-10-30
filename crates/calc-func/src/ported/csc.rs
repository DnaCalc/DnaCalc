use crate::ported::real_policy::ExcelRealPolicy;
use crate::value::WorksheetErrorCode;

pub fn csc_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    ExcelRealPolicy::CIRCULAR_TRIG.check_arg(n)?;
    // W109 G4-01: CSC = RN53(RN64(1/SIN)) over the published SIN chain.
    let sin = crate::ported::excel_numeric::excel_sin(n);
    if sin == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    Ok(crate::ported::excel_numeric::excel_x87_recip(sin))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn csc_kernel_zero_is_div0() {
        assert_eq!(csc_kernel(0.0), Err(WorksheetErrorCode::Div0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn csc_matches_live_excel_recip_sin_pins() {
        // Live Excel 16.0 b20326: CSC(x)=1/SIN(x) 8/8.
        assert_eq!(csc_kernel(0.5).unwrap().to_bits(), 0x4000afc773ab69dc);
        assert_eq!(csc_kernel(1.0).unwrap().to_bits(), 0x3ff303aa9620b224);
        assert_eq!(csc_kernel(-0.5).unwrap().to_bits(), 0xc000afc773ab69dc);
    }
}
