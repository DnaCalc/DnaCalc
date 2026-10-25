use crate::ported::factorial_common::trunc_nonnegative;
use crate::value::WorksheetErrorCode;

/// Largest first argument admitted by the current reference COMBIN lane.
///
/// The paired W109 boundary sweep pins `2_147_483_646` as admitted and
/// `2_147_483_647` as `#NUM!`, independently of the cyclic publication body.
pub(crate) const COMBIN_MAX_N: i64 = 2_147_483_646;

/// Worksheet `COMBIN` on the current x64 Excel reference build.
///
/// W109 identified a cyclic rising-factor graph rather than the conventional
/// multiply-first recurrence. After `k = min(k, n-k)`, Excel evaluates
///
/// `product(i=2..=k, (n-k+i-1)/i) * n`
///
/// in ascending `i`, storing every quotient and accumulator product from x87
/// PC64 to binary64 (`RN53(RN64(op))`), then multiplying `n` last through the
/// same stored-x87 operation. This graph is exact on 505 legacy rows, 20,713
/// current-build discovery rows, and a frozen 1,024-row publication heldout.
pub fn combin_kernel(n: f64, k: f64) -> Result<f64, WorksheetErrorCode> {
    use crate::ported::excel_numeric::{excel_x87_div, excel_x87_mul};

    // Range.Value2 cannot inject NaN/infinity into the live worksheet probe,
    // but the direct Rust kernel must not inherit saturating float-to-int casts.
    if !n.is_finite() || !k.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    // The current x64 reference uses denormals-are-zero at this admission
    // seam. Preserve the sign on the zero itself; ordinary `< 0` checks then
    // admit either subnormal sign as zero while rejecting negative normals.
    let n = if n.is_subnormal() {
        0.0_f64.copysign(n)
    } else {
        n
    };
    let k = if k.is_subnormal() {
        0.0_f64.copysign(k)
    } else {
        k
    };
    let n = trunc_nonnegative(n)?;
    let raw_k = trunc_nonnegative(k)?;
    if n > COMBIN_MAX_N {
        return Err(WorksheetErrorCode::Num);
    }
    if raw_k > n {
        return Err(WorksheetErrorCode::Num);
    }
    let k = raw_k.min(n - raw_k);
    if k == 0 {
        return Ok(1.0);
    }

    let mut acc = 1.0;
    for i in 2..=k {
        let numerator = (n - k + i - 1) as f64;
        let factor = excel_x87_div(numerator, i as f64);
        acc = excel_x87_mul(acc, factor);
        // With k reduced to min(k,n-k), every remaining factor is greater
        // than one. Once the accumulator becomes nonfinite it cannot recover;
        // short-circuiting preserves the typed #NUM result and prevents an
        // admitted near-central 32-bit input from walking ~1e9 iterations.
        if !acc.is_finite() {
            return Err(WorksheetErrorCode::Num);
        }
    }
    let result = excel_x87_mul(acc, n as f64);
    if result.is_finite() {
        Ok(result)
    } else {
        Err(WorksheetErrorCode::Num)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_bits(actual: f64, expected: f64) {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "{actual} vs {expected}"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn combin_kernel_matches_excel_truncation_and_num_lanes() {
        assert_eq!(combin_kernel(5.0, 2.0), Ok(10.0));
        assert_eq!(combin_kernel(5.9, 2.2), Ok(10.0));
        assert_eq!(combin_kernel(5.0, 6.0), Err(WorksheetErrorCode::Num));
        assert_eq!(combin_kernel(-1.0, 1.0), Err(WorksheetErrorCode::Num));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn combin_exact_publication_controls_remain_exact() {
        assert_bits(combin_kernel(10.0, 3.0).expect("combin(10,3)"), 120.0_f64);
        assert_bits(combin_kernel(9.0, 2.0).expect("combin(9,2)"), 36.0_f64);
    }

    /// W109 current-build discovery pins. These representable integers are
    /// published one ULP away from the correctly rounded value, which rules
    /// out the former multiply-first kernel.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn combin_matches_live_excel_cyclic_spill_pins() {
        assert_eq!(
            combin_kernel(9.0, 3.0).unwrap().to_bits(),
            0x4054_ffff_ffff_ffff
        );
        assert_eq!(
            combin_kernel(23.0, 10.0).unwrap().to_bits(),
            0x4131_7502_0000_0001
        );
        assert_eq!(
            combin_kernel(200.0, 3.0).unwrap().to_bits(),
            0x4134_0a77_ffff_ffff
        );
        assert_eq!(
            combin_kernel(23.0, 13.0).unwrap().to_bits(),
            0x4131_7502_0000_0001
        );
    }

    /// Frozen 1,024-row publication-heldout discriminator. The plain RN53
    /// recurrence is one ULP low here; the selected stored-x87 graph matches.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn combin_matches_frozen_publication_heldout_pin() {
        assert_eq!(
            combin_kernel(258.0, 49.0).unwrap().to_bits(),
            0x4afe_ff3e_f80e_71b4
        );
    }

    /// Paired current-build COMBIN controls for the COMBINA wrapper campaign
    /// locate the inherited signed-32-bit admission ceiling exactly.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn combin_matches_live_excel_integer_ceiling() {
        assert_eq!(combin_kernel(2_147_483_646.0, 0.0), Ok(1.0));
        assert_eq!(combin_kernel(2_147_483_646.0, 1.0), Ok(2_147_483_646.0));
        assert_eq!(combin_kernel(2_147_483_646.0, 2_147_483_646.0), Ok(1.0));
        assert_eq!(
            combin_kernel(2_147_483_647.0, 0.0),
            Err(WorksheetErrorCode::Num)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn combin_large_central_overflow_short_circuits_to_num() {
        assert_eq!(
            combin_kernel(2_000_000_000.0, 1_000_000_000.0),
            Err(WorksheetErrorCode::Num)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn combin_direct_nonfinite_inputs_are_defensively_num() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(combin_kernel(value, 0.0), Err(WorksheetErrorCode::Num));
            assert_eq!(combin_kernel(1.0, value), Err(WorksheetErrorCode::Num));
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn combin_matches_live_excel_daz_boundary() {
        assert_eq!(combin_kernel(-f64::from_bits(1), 0.25), Ok(1.0));
        assert_eq!(combin_kernel(1.25, -f64::from_bits(1)), Ok(1.0));
        assert_eq!(
            combin_kernel(-f64::MIN_POSITIVE, 0.25),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            combin_kernel(1.25, -f64::MIN_POSITIVE),
            Err(WorksheetErrorCode::Num)
        );
    }
}
