use crate::ported::combin::combin_kernel;
use crate::value::WorksheetErrorCode;

pub fn combina_kernel(n: f64, k: f64) -> Result<f64, WorksheetErrorCode> {
    // Range.Value2 cannot inject NaN/infinity into the live worksheet probe,
    // but the direct Rust kernel keeps that unobserved seam deterministic.
    if !n.is_finite() || !k.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }

    // The current x64 reference treats binary64 subnormals as signed zero at
    // this admission seam (DAZ). Truncation then precedes the asymmetric
    // negative-choice guard.
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
    let truncated_n = n.trunc();
    let truncated_k = k.trunc();

    // This zero/zero publication precedes the negative-choice guard. Thus
    // values in (-1,0), including a negative raw choice, publish one when both
    // arguments truncate to zero.
    if truncated_n == 0.0 && truncated_k == 0.0 {
        return Ok(1.0);
    }
    if truncated_n < 0.0 || k < 0.0 {
        return Err(WorksheetErrorCode::Num);
    }

    let total = truncated_n + truncated_k - 1.0;

    // W109 current-reference identification: COMBINA truncates its two
    // arguments separately, then routes C(n+k-1,k) through the same cyclic
    // stored-x87 publication graph as COMBIN. In particular, this is not the
    // worksheet-visible composition COMBIN(n+k-1,k), whose addition would
    // occur before COMBIN truncates fractional arguments.
    combin_kernel(total, truncated_k)
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

    #[test]
    fn combina_kernel_matches_excel_boundary_lanes() {
        assert_eq!(combina_kernel(4.0, 3.0), Ok(20.0));
        assert_eq!(combina_kernel(5.9, 2.2), Ok(15.0));
        assert_eq!(combina_kernel(0.0, 0.0), Ok(1.0));
        assert_eq!(combina_kernel(0.0, 1.0), Err(WorksheetErrorCode::Num));
    }

    #[test]
    fn combina_exact_publication_controls_remain_exact() {
        assert_bits(combina_kernel(4.0, 3.0).expect("combina(4,3)"), 20.0_f64);
        assert_bits(combina_kernel(10.0, 3.0).expect("combina(10,3)"), 220.0_f64);
    }

    /// W109 current-build discovery pins where the former multiply-first
    /// product returns the mathematical integer but Excel publishes the
    /// cyclic stored-x87 COMBIN graph one ULP away.
    #[test]
    fn combina_matches_live_excel_transformed_combin_pins() {
        assert_eq!(
            combina_kernel(7.0, 3.0).unwrap().to_bits(),
            0x4054_ffff_ffff_ffff
        );
        assert_eq!(
            combina_kernel(9.0, 6.0).unwrap().to_bits(),
            0x40a7_75ff_ffff_ffff
        );
        assert_eq!(
            combina_kernel(881.0, 23.0).unwrap().to_bits(),
            0x495f_4737_a5ed_35eb
        );
    }

    /// COMBINA truncates each source argument before constructing the
    /// transformed COMBIN total. A worksheet composition would add first and
    /// therefore selects a different total for this exact-binary fraction.
    #[test]
    fn combina_truncates_arguments_before_transformed_total() {
        let got = combina_kernel(7.75, 3.75).unwrap();
        assert_eq!(got.to_bits(), 0x4054_ffff_ffff_ffff);
        let worksheet_pretrunc = combin_kernel(7.75 + 3.75 - 1.0, 3.75).unwrap();
        assert_ne!(got.to_bits(), worksheet_pretrunc.to_bits());
    }

    #[test]
    fn combina_matches_live_excel_daz_and_asymmetric_guard_order() {
        assert_eq!(combina_kernel(-0.25, 0.75), Ok(1.0));
        assert_eq!(combina_kernel(-0.25, 1.0), Err(WorksheetErrorCode::Num));
        assert_eq!(combina_kernel(1.0, -0.25), Err(WorksheetErrorCode::Num));
        assert_eq!(combina_kernel(1.0, -f64::from_bits(1)), Ok(1.0));
        assert_eq!(
            combina_kernel(1.0, -f64::MIN_POSITIVE),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            combina_kernel(-f64::from_bits(1), 1.0),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            combina_kernel(f64::from_bits(1), 1.0),
            Err(WorksheetErrorCode::Num)
        );
    }

    #[test]
    fn combina_matches_live_excel_transformed_total_ceiling() {
        assert_eq!(combina_kernel(2_147_483_647.0, 0.0), Ok(1.0));
        assert_eq!(combina_kernel(2_147_483_646.0, 1.0), Ok(2_147_483_646.0));
        assert_eq!(combina_kernel(1.0, 2_147_483_646.0), Ok(1.0));
        assert_eq!(
            combina_kernel(2_147_483_647.0, 1.0),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            combina_kernel(2.0, 2_147_483_646.0),
            Err(WorksheetErrorCode::Num)
        );
    }

    #[test]
    fn combina_large_central_overflow_short_circuits_to_num() {
        assert_eq!(
            combina_kernel(1_000_000_000.0, 1_000_000_000.0),
            Err(WorksheetErrorCode::Num)
        );
    }

    #[test]
    fn combina_direct_nonfinite_inputs_are_defensively_num() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(combina_kernel(value, 0.0), Err(WorksheetErrorCode::Num));
            assert_eq!(combina_kernel(1.0, value), Err(WorksheetErrorCode::Num));
        }
    }
}
