use crate::ported::real_policy::ExcelRealPolicy;
use crate::value::WorksheetErrorCode;

pub fn cot_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    ExcelRealPolicy::CIRCULAR_TRIG.check_arg(n)?;
    // W109 G4-01: COT = RN53(RN64(1/TAN)) over the published fFTAN chain
    // (the double-rounded x87 reciprocal, unique surviving staging).
    let tan = crate::ported::excel_numeric::excel_tan(n);
    if tan == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    Ok(crate::ported::excel_numeric::excel_x87_recip(tan))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cot_kernel_zero_is_div0() {
        assert_eq!(cot_kernel(0.0), Err(WorksheetErrorCode::Div0));
    }

    // BUG-FUNC-027 CLASS-B2: |x| >= 2^27 -> #NUM! (live Excel COT(2^27)=#NUM!).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cot_kernel_large_argument_is_num() {
        assert_eq!(cot_kernel(134_217_728.0), Err(WorksheetErrorCode::Num));
        assert_eq!(cot_kernel(-134_217_728.0), Err(WorksheetErrorCode::Num));
        assert!(cot_kernel(1.0).is_ok());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cot_matches_live_excel_recip_tan_pins() {
        // Live Excel 16.0 b20326: COT(x)=1/TAN(x) 10/10.
        assert_eq!(cot_kernel(0.5).unwrap().to_bits(), 0x3ffd49ad7e47c0a3);
        assert_eq!(cot_kernel(1.0).unwrap().to_bits(), 0x3fe48c05d04e1cfd);
        assert_eq!(cot_kernel(-0.5).unwrap().to_bits(), 0xbffd49ad7e47c0a3);
    }
}
