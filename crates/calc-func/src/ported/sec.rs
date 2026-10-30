use crate::ported::real_policy::ExcelRealPolicy;
use crate::value::WorksheetErrorCode;

pub fn sec_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    ExcelRealPolicy::CIRCULAR_TRIG.check_arg(n)?;
    // W109 G4-01: SEC = RN53(RN64(1/cos)) over the RAW COS chain (the
    // published-COS small-argument 1.0 shortcut composes identically here:
    // recip-dr of 1-ulp ties back to exactly 1.0).
    Ok(crate::ported::excel_numeric::excel_x87_recip(
        crate::ported::excel_numeric::excel_cos(n),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sec_kernel_zero_is_one() {
        assert_eq!(sec_kernel(0.0), Ok(1.0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sec_matches_live_excel_recip_cos_pins() {
        // Live Excel 16.0 b20326: SEC(x)=1/COS(x) 8/8.
        assert_eq!(sec_kernel(0.5).unwrap().to_bits(), 0x3ff23b5dfbfd97b6);
        assert_eq!(sec_kernel(1.0).unwrap().to_bits(), 0x3ffd9cf0f125cc29);
    }
}
