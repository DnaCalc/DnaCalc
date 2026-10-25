use crate::ported::real_policy::ExcelRealPolicy;
use crate::value::WorksheetErrorCode;

pub fn sinh_kernel(n: f64) -> f64 {
    // W111 neighboring-bit observations distinguish the EXP difference at
    // |x| >= 1 from the cancellation-safe expm1 difference below that boundary.
    if n.abs() >= 1.0 {
        return crate::ported::excel_numeric::excel_x87_sub(
            crate::ported::excel_numeric::excel_exp(n),
            crate::ported::excel_numeric::excel_exp(-n),
        ) / 2.0;
    }
    // Live Excel 16.0 b20326 Range.Value2: SINH(x)=(expm1(x)-expm1(-x))/2
    // using Excel's internal Kahan expm1, 37/37 including the worksheet
    // EXP-pair misses at |x|<~0.25. libm sinh is 1 ULP off at 0.01 and 2.
    (crate::ported::excel_numeric::excel_expm1_internal(n) - crate::ported::excel_numeric::excel_expm1_internal(-n))
        / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ported::excel_numeric::crt_sinh;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sinh_kernel_matches_std() {
        assert_eq!(sinh_kernel(1.0), crt_sinh(1.0f64));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sinh_matches_live_excel_expm1_pair_pins() {
        // Live Excel 16.0 b20326 Range.Value2 SINH bits.
        let pins = [
            (0.0_f64, 0x0000000000000000u64),
            (1e-8, 0x3e45798ee2308c3a),
            (1e-6, 0x3eb0c6f7a0b5f0a0),
            (1e-5, 0x3ee4f8b588e4e93f),
            (1e-4, 0x3f1a36e2ebd7e992),
            (3e-4, 0x3f33a92a3547d58e),
            (0.001, 0x3f50624e00c1c9e3),
            (0.002, 0x3f60624e8a322b64),
            (0.003, 0x3f68937726e43ddd),
            (0.005, 0x3f747ae6df566a00),
            (0.008, 0x3f80625946fc00fa),
            (0.01, 0x3f847af7a654e9ee),
            (0.02, 0x3f947b3ac2a1608e),
            (0.03, 0x3f9eb97fec690b21),
            (0.05, 0x3fa99c54bcf10dfc),
            (0.08, 0x3fb4807964dae3d0),
            (0.1, 0x3fb9a487337b59b3),
            (0.15, 0x3fc345a71a6f4ac9),
            (0.2, 0x3fc9c560cd35ef82),
            (0.22, 0x3fcc6340cfa05746),
            (0.25, 0x3fd02accd9d08102),
            (0.28, 0x3fd227b2f27e6efe),
            (0.3, 0x3fd37d42af54b926),
            (0.35, 0x3fd6dc324f999c68),
            (0.4, 0x3fda49c41f850ed2),
            (0.5, 0x3fe0acd00fe63b97),
            (0.75, 0x3fea506b2dd3c690),
            (1.0, 0x3ff2cd9fc44eb982),
            (1.5, 0x400108c3aabd6a60),
            (2.0, 0x400d03cf63b6e1a0),
            (3.0, 0x40240926e70949ae),
            (5.0, 0x40528d0166f07374),
            (10.0, 0x40c5829dced69992),
            (20.0, 0x41aceb088b68e804),
            (-0.5, 0xbfe0acd00fe63b97),
            (-1.0, 0xbff2cd9fc44eb982),
            (-2.0, 0xc00d03cf63b6e1a0),
        ];
        for (x, bits) in pins {
            assert_eq!(sinh_kernel(x).to_bits(), bits, "x={x}");
        }
    }

    /// The libm misses are those of the MSVC runtime's `sinh` (glibc's returns Excel's bits at 0.01,
    /// Linux sweep, dc-ns0): the witness is that runtime's `sinh`, ported (`crt_sinh`, dc-twc).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sinh_differs_from_the_msvc_libm_where_excel_does() {
        assert_ne!(sinh_kernel(0.01).to_bits(), crt_sinh(0.01_f64).to_bits());
        assert_ne!(sinh_kernel(2.0).to_bits(), crt_sinh(2.0_f64).to_bits());
    }

    // BUG-FUNC-027 CLASS-A3: live Excel 16.0 b20026 SINH(-326648.33)=#NUM!.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sinh_overflow_maps_to_num() {
        assert_eq!(
            ExcelRealPolicy::FINITE
                .publish(-326648.33, sinh_kernel(-326648.33)),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            ExcelRealPolicy::FINITE.publish(1.0, sinh_kernel(1.0)),
            Ok(crt_sinh(1.0f64))
        );
    }
}
