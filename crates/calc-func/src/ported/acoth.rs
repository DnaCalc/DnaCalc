use crate::value::WorksheetErrorCode;

/// First binary64 `|x|` routed to Excel's direct inverse odd-power series.
///
/// W109 pinned the adjacent distinguishing endpoints to
/// `0x400d92b14ec204ef` (ratio) and `0x400d92b14ec204f3` (series); the three
/// doubles between them are observational overlap. The decimal literal below
/// is exactly the latter bit pattern.
const ACOTH_SERIES_FLOOR: f64 = f64::from_bits(0x400d_92b1_4ec2_04f3);
const ACOTH_SERIES_TERM_CAP: usize = 32;

fn acoth_series_magnitude(a: f64) -> f64 {
    let reciprocal = crate::ported::excel_numeric::excel_x87_div(1.0, a);

    // Excel flushes the reciprocal-to-series handoff when it is subnormal. In
    // particular, both signs of a sufficiently large finite input publish +0.
    if reciprocal < f64::MIN_POSITIVE {
        return 0.0;
    }

    let square = crate::ported::excel_numeric::excel_x87_mul(a, a);
    let mut power = a;
    let mut sum = reciprocal;

    for k in 1..ACOTH_SERIES_TERM_CAP {
        power = crate::ported::excel_numeric::excel_x87_mul(power, square);
        let denominator = crate::ported::excel_numeric::excel_x87_mul((2 * k + 1) as f64, power);
        let term = crate::ported::excel_numeric::excel_x87_div(1.0, denominator);
        let next = crate::ported::excel_numeric::excel_x87_add(sum, term);
        if next == sum {
            break;
        }
        sum = next;
    }

    sum
}

pub fn acoth_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    let a = n.abs();
    if a <= 1.0 {
        return Err(WorksheetErrorCode::Num);
    }
    // W109 build-20228/CV2 identification: ACOTH computes on |x| and restores
    // the sign after one of two distinct calculation graphs:
    //   * below the pinned switch, native binary64 +/-1 staging, an x87-PC64
    //     stored division, then FYL2X and half-scale;
    //   * at and above it, the direct inverse odd-power series with every
    //     reciprocal, multiply, divide, and accumulator add stored through
    //     x87 PC64.
    // The frozen graph scored 202,217/202,217 discovery inputs and
    // 66,552/66,552 disjoint held-out inputs exactly.
    let mag = if a < ACOTH_SERIES_FLOOR {
        let ratio = crate::ported::excel_numeric::excel_x87_div(a + 1.0, a - 1.0);
        // Storing the FYL2X result before multiplying by the exact power-of-two
        // scale is bit-equivalent here to keeping the half-scale in x87.
        0.5 * crate::ported::excel_numeric::excel_log(ratio)
    } else {
        acoth_series_magnitude(a)
    };

    // Excel publishes positive zero for both signs after the reciprocal flush.
    Ok(if mag == 0.0 { 0.0 } else { mag.copysign(n) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn acoth_kernel_rejects_abs_one() {
        assert_eq!(acoth_kernel(1.0), Err(WorksheetErrorCode::Num));
        assert_eq!(acoth_kernel(-1.0), Err(WorksheetErrorCode::Num));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn acoth_just_above_one_is_finite_matching_excel() {
        // BUG-FUNC-027 C5: the "Excel #NUM!" near-1 witness was a formula-literal
        // artifact (the parser rounded 1+ULP down to 1.0). With the exact input
        // 1 + 2^-52, live Excel 16.0 b20026 returns 18.36840028483855, which this
        // kernel matches bit-for-bit — so it must stay finite, not collapse to #NUM!.
        assert_eq!(
            acoth_kernel(f64::from_bits(0x3ff0_0000_0000_0001))
                .expect("finite just above 1")
                .to_bits(),
            0x4032_5e4f_7b27_37fa
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn acoth_large_and_negative_args_bit_exact() {
        // BUG-FUNC-027 C5: the direct ratio form loses the reciprocal tail for
        // large |x|. W109 identified Excel's far-field inverse-power series.
        assert_eq!(
            acoth_kernel(1_000_000.0).unwrap().to_bits(),
            0x3eb0_c6f7_a0b5_f3b3
        );
        assert_eq!(
            acoth_kernel(1.001).unwrap().to_bits(),
            0x400e_67d6_037b_1a46
        );
        // Excel's ACOTH is exactly odd-symmetric.
        assert_eq!(acoth_kernel(-2.0).unwrap(), -acoth_kernel(2.0).unwrap());
        assert_eq!(
            acoth_kernel(-1_000_000.0).unwrap(),
            -acoth_kernel(1_000_000.0).unwrap()
        );
    }

    /// W109: pinned witnesses for both identified calculation graphs.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn acoth_two_regime_matches_live_excel_pinned_witnesses() {
        assert_eq!(acoth_kernel(2.0).unwrap().to_bits(), 0x3fe1_93ea_7aad_030b); // ratio branch
        assert_eq!(acoth_kernel(20.0).unwrap().to_bits(), 0x3fa9_9f11_cd5f_7091); // series branch
        assert_eq!(
            acoth_kernel(-20.0).unwrap().to_bits(),
            0xbfa9_9f11_cd5f_7091
        ); // odd
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn acoth_switch_and_series_staging_match_live_excel() {
        // Last ratio-only discriminator and first series-only discriminator.
        assert_eq!(
            acoth_kernel(f64::from_bits(0x400d_92b1_4ec2_04ef))
                .unwrap()
                .to_bits(),
            0x3fd1_c145_ba62_96bf
        );
        assert_eq!(
            acoth_kernel(f64::from_bits(0x400d_92b1_4ec2_04f3))
                .unwrap()
                .to_bits(),
            0x3fd1_c145_ba62_96bb
        );

        // These mid-range rows distinguish the direct inverse-power body from
        // the superseded reciprocal-ln1p-pair hypothesis.
        assert_eq!(acoth_kernel(5.0).unwrap().to_bits(), 0x3fc9_f323_ecbf_984d);
        assert_eq!(
            acoth_kernel(f64::from_bits(0x4020_3333_35a5_6e96))
                .unwrap()
                .to_bits(),
            0x3fbf_c459_9b38_1b0e
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn acoth_reciprocal_flush_publishes_positive_zero_for_both_signs() {
        let magnitude = f64::from_bits(0x7fd1_7e73_a05c_2b97);
        assert_eq!(acoth_kernel(magnitude).unwrap().to_bits(), 0);
        assert_eq!(acoth_kernel(-magnitude).unwrap().to_bits(), 0);
    }
}
