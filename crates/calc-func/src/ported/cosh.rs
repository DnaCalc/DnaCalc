use crate::ported::real_policy::ExcelRealPolicy;
use crate::value::WorksheetErrorCode;

pub fn cosh_kernel(n: f64) -> f64 {
    // Live Excel 16.0 b20326 Range.Value2: COSH(x)=(EXP(x)+EXP(-x))/2
    // 40/40, including spilled EXP cells and E/2+EXP(-x)/2. libm cosh is
    // 1 ULP off Excel at 0.001, 0.01, and 10.
    let e = crate::ported::excel_numeric::excel_exp(n);
    let em = crate::ported::excel_numeric::excel_exp(-n);
    crate::ported::excel_numeric::excel_x87_add(e, em) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: its witness is the platform libm's cosh, one ULP from Excel at 1 on wasm32.
    #[test]
    fn cosh_kernel_matches_std_at_one() {
        // x=1 is on the EXP identity and also matches libm.
        assert_eq!(cosh_kernel(1.0), 1.0f64.cosh());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cosh_matches_live_excel_exp_pair_pins() {
        // Live Excel 16.0 b20326: COSH=(EXP+EXP(-))/2. These include the
        // three libm misses (0.001, 0.01, 10).
        assert_eq!(cosh_kernel(0.0).to_bits(), 0x3ff0000000000000);
        assert_eq!(cosh_kernel(0.5).to_bits(), 0x3ff20ac1862ae8d0);
        assert_eq!(cosh_kernel(1.0).to_bits(), 0x3ff8b07551d9f550);
        assert_eq!(cosh_kernel(0.001).to_bits(), 0x3ff000008637bdc2);
        assert_eq!(cosh_kernel(0.01).to_bits(), 0x3ff000346de27852);
        assert_eq!(cosh_kernel(10.0).to_bits(), 0x40c5829dd053712e);
        assert_eq!(cosh_kernel(-10.0).to_bits(), 0x40c5829dd053712e);
    }

    /// The libm misses are those of the MSVC runtime's `cosh`: glibc's `cosh` returns Excel's
    /// bits at 0.001 (Linux sweep, dc-ns0), so `f64::cosh` is no witness on other targets.
    #[cfg(target_env = "msvc")]
    /// Native-only: the MSVC runtime's libm is its witness, so it runs on Windows only.
    #[test]
    fn cosh_differs_from_the_msvc_libm_where_excel_does() {
        assert_ne!(cosh_kernel(0.001).to_bits(), 0.001_f64.cosh().to_bits());
        assert_ne!(cosh_kernel(10.0).to_bits(), 10.0_f64.cosh().to_bits());
    }

    // BUG-FUNC-027 CLASS-A3: live Excel 16.0 b20026 COSH(-24230)=#NUM!.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cosh_overflow_maps_to_num() {
        assert_eq!(
            ExcelRealPolicy::FINITE
                .publish(-24230.0, cosh_kernel(-24230.0)),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            ExcelRealPolicy::FINITE.publish(1.0, cosh_kernel(1.0)),
            // Excel's COSH(1), as cosh_matches_live_excel_exp_pair_pins pins it.
            Ok(f64::from_bits(0x3ff8b07551d9f550))
        );
    }
}
