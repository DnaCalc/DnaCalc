use crate::value::WorksheetErrorCode;

/// Exact W109 switch from the binary64 cubic body to the x87-spilled ratio
/// body. The preceding positive input is observationally equal under both
/// bodies; the negative input at this bit pattern selects the ratio route.
const ATANH_RATIO_FLOOR: f64 = f64::from_bits(0x3f1a_f82b_729c_1d83);

pub fn atanh_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    let a = n.abs();
    if a >= 1.0 {
        return Err(WorksheetErrorCode::Num);
    }
    // Current-reference Excel observes DAZ inputs in this legacy body. Both
    // signs of every subnormal input publish positive zero.
    if a < f64::MIN_POSITIVE {
        return Ok(0.0);
    }
    // W109 G4-02 (build 20228/CV2, NoCache): the old apparent x87-ln1p pair
    // was observationally equivalent only on the sparse small-input corpus.
    // Dense disagreement mapping identifies the actual body as the ordinary
    // binary64 cubic `x + x^3/3`, up to the exact threshold above.
    if a < ATANH_RATIO_FLOOR {
        return Ok(n + (n * n * n) / 3.0);
    }

    // The ratio body is a legacy x87 spill unit: each wrapper operation is
    // RN53(RN64(op)) before the established x87 LN publication. All three
    // add/sub/div stores are independently required by the fresh wrapper-mask
    // held-out. The signed ratio is evaluated directly, so this region is not
    // globally odd.
    let numerator = crate::ported::excel_numeric::excel_x87_add(1.0, n);
    let denominator = crate::ported::excel_numeric::excel_x87_sub(1.0, n);
    let ratio = crate::ported::excel_numeric::excel_x87_div(numerator, denominator);
    Ok(0.5 * crate::ported::excel_numeric::excel_log(ratio))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn atanh_kernel_rejects_abs_one() {
        assert_eq!(atanh_kernel(1.0), Err(WorksheetErrorCode::Num));
        assert_eq!(atanh_kernel(-1.0), Err(WorksheetErrorCode::Num));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn atanh_matches_live_excel_pins_near_minus_one() {
        // BUG-FUNC-027 C4 historical boundary witnesses. The identified signed
        // ratio graph matches both rows; this particular adjacent positive/
        // negative pair happens to be an exact sign flip, but ATANH is not
        // globally odd (the independent 0.2 discriminator below proves that).
        assert_eq!(
            atanh_kernel(-0.9999999999999990).unwrap().to_bits(),
            0xc031_9dc9_df78_50b1
        );
        assert_eq!(
            atanh_kernel(-0.999999999).unwrap().to_bits(),
            0xc025_6a9a_0b9b_2416
        );
        let x = 0.9999999999999990_f64;
        assert_eq!(atanh_kernel(-x).unwrap(), -atanh_kernel(x).unwrap());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn atanh_ratio_candidate_is_not_global() {
        // The ratio-log is the production path only for |x| >= the floor. A
        // tiny *normal* input is the canonical discriminator: the ratio rounds
        // to 1 and collapses to zero, while Excel's cubic body preserves the
        // input. Subnormals are a separate DAZ lane pinned below.
        let candidate = |x: f64| 0.5 * crate::ported::excel_numeric::excel_log((1.0 + x) / (1.0 - x));
        let tiny = f64::from_bits(0x01a5_6e1f_c2f8_f359); // 1e-300
        assert_eq!(candidate(tiny).to_bits(), 0.0_f64.to_bits());
        assert_eq!(atanh_kernel(tiny).unwrap().to_bits(), tiny.to_bits());
    }

    /// W109 (2026-07-12): region-C ratio-log is bit-exact to live Excel 16.0
    /// build 20131 across the catalog mid-small band and up to the domain edge
    /// (163/163 answered rows; pins from G4-hyp / G4-02 answer sets).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn atanh_region_c_matches_live_excel_pinned_witnesses() {
        let pins: [(u64, u64); 5] = [
            (0x3fb9_9999_9999_999a, 0x3fb9_af93_cd23_4415), // 0.1
            (0x3fc9_9999_9999_999a, 0x3fc9_f323_ecbf_9849), // 0.2
            (0x3f94_7ae1_47ae_147b, 0x3f94_7b94_47a9_a9f8), // 0.02 (band floor)
            (0x3fe0_0000_0000_0000, 0x3fe1_93ea_7aad_030b), // 0.5
            (0x3fde_147a_e147_ae14, 0x3fe0_527f_06cd_3e63), // 0.47
        ];
        for (xb, want) in pins {
            let x = f64::from_bits(xb);
            assert_eq!(atanh_kernel(x).unwrap().to_bits(), want, "x={x}");
        }
    }

    /// W109 (2026-07-12): Excel ATANH is NOT exactly odd in region C — it
    /// evaluates the signed ratio 0.5·ln((1+x)/(1-x)) directly, and the negative
    /// argument rounds independently. Live Excel: ATANH(-0.2) = 0xbfc9f323ecbf984a,
    /// which is 1 ULP away from -ATANH(0.2) = 0xbfc9f323ecbf9849. The prior
    /// copysign-forced oddness therefore introduced a divergence; the signed ratio
    /// reproduces Excel's actual (non-odd) value.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn atanh_region_c_is_not_odd_and_matches_signed_ratio() {
        let x = f64::from_bits(0x3fc9_9999_9999_999a); // 0.2
        assert_eq!(atanh_kernel(-x).unwrap().to_bits(), 0xbfc9_f323_ecbf_984a);
        assert_ne!(
            atanh_kernel(-x).unwrap().to_bits(),
            (-atanh_kernel(x).unwrap()).to_bits()
        );
    }

    /// W109 (2026-08-09): the normal-input small body is ordinary binary64
    /// `x + x^3/3`, not the formerly inferred x87 `fyl2xp1` pair. Pins span the
    /// exact-input regime and the top of the cubic lane; subnormal DAZ is pinned
    /// separately by `atanh_exact_three_regime_w109_pins`.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn atanh_cubic_body_matches_live_excel_pinned_witnesses() {
        let pins: [(u64, u64); 4] = [
            (0x01a5_6e1f_c2f8_f359, 0x01a5_6e1f_c2f8_f359), // 1e-300 (passthrough)
            (0x3e6c_775b_423f_371b, 0x3e6c_775b_423f_3723), // 5.302250e-08
            (0x3eab_9139_70d9_aa3f, 0x3eab_9139_70d9_b111), // 8.215690e-07
            (0x3f15_0c68_6bf5_4163, 0x3f15_0c68_6cb7_882b), // 8.029353e-05 (near boundary)
        ];
        for (xb, want) in pins {
            let x = f64::from_bits(xb);
            assert_eq!(atanh_kernel(x).unwrap().to_bits(), want, "x={x:e}");
        }
    }

    /// The binary64 cubic body is odd on this pinned normal-input row, unlike
    /// the independently evaluated signed-ratio route above the exact seam.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn atanh_cubic_body_is_odd_on_pinned_normal_row() {
        let x = f64::from_bits(0x3f15_0c68_6bf5_4163); // 8.029353e-05
        assert_eq!(atanh_kernel(-x).unwrap().to_bits(), 0xbf15_0c68_6cb7_882b);
        assert_eq!(
            atanh_kernel(-x).unwrap().to_bits(),
            (-atanh_kernel(x).unwrap()).to_bits()
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn atanh_exact_three_regime_w109_pins() {
        // Fresh Excel 16.0 build 20228 x64, CV2, Value2/Formula2, NoCache.
        // The subnormal pair pins DAZ/+0 publication; the six seam rows pin
        // the sign-sensitive representative threshold; the final three rows
        // independently require x87 double rounding at the ratio wrapper.
        for (input_bits, expected_bits) in [
            (0x0004_8b60_699b_04c8, 0x0000_0000_0000_0000),
            (0x8004_8b60_699b_04c8, 0x0000_0000_0000_0000),
            (0x3f1a_f82b_729c_1d82, 0x3f1a_f82b_7434_c925),
            (0xbf1a_f82b_729c_1d82, 0xbf1a_f82b_7434_c925),
            (0x3f1a_f82b_729c_1d83, 0x3f1a_f82b_7434_c926),
            (0xbf1a_f82b_729c_1d83, 0xbf1a_f82b_7434_b84c),
            (0x3f1a_f82b_729c_1d84, 0x3f1a_f82b_7434_c926),
            (0xbf1a_f82b_729c_1d84, 0xbf1a_f82b_7434_b84c),
            (0x3f21_e551_424c_dd9f, 0x3f21_e551_442a_7334),
            (0x3f24_358b_30f8_d001, 0x3f24_358b_33a8_99ac),
            (0xbf24_358b_30f8_d001, 0xbf24_358b_33a8_9164),
        ] {
            let actual = atanh_kernel(f64::from_bits(input_bits)).expect("valid ATANH witness");
            assert_eq!(actual.to_bits(), expected_bits, "input=0x{input_bits:016x}");
        }
    }
}
