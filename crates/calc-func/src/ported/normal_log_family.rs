//! The normal, lognormal and related inverse kernels.
//!
//! The inverse uses the published Acklam rational approximation and one Newton
//! refinement. Exact Excel parity for this generic method remains open.

use crate::ported::normal_dist_common::{erf_approx, phi_kernel, stored_normal_z, FRAC_1_SQRT_2_BITS, GAUSS_TINY_MAX_BITS};
use crate::ported::special_dist_family::erfc_precise_kernel;
use crate::value::WorksheetErrorCode;
use crate::ported::excel_numeric::crt_log;

fn flush_subnormal(value: f64) -> f64 {
    if value.abs() < f64::MIN_POSITIVE {
        0.0
    } else {
        value
    }
}

/// Inclusive tiny GAUSS uses its stored native normal argument and the shared
/// ERF body, followed by halving. The witness report counts its remaining gaps.
fn gauss_tiny_direct(x: f64) -> f64 {
    let z = stored_normal_z(x);
    let mut y = 0.5 * erf_approx(z);
    if x.is_sign_negative() {
        y = -y;
    }
    flush_subnormal(y)
}

/// G-F3: `NORMSDIST` / `NORM.S.DIST` CDF wrapper. W111 mathematical
/// discriminators require `z = RN53(RN64(|x|*RN(1/√2)))`; native binary64
/// multiplication fails 133/512 fresh public-ERFC composition controls.
/// `Q` is the published ERFC body at `z` (still an open kernel);
/// `x < 0 → RN53(0.5*Q)`, `x ≥ 0 → RN53(1-0.5*Q)`; PHI-class flush after
/// the 0.5 multiply (`NORMSDIST(-37.52) = +0` with ERFC still finite).
pub fn identified_std_normal_cdf(x: f64) -> f64 {
    if x == 0.0 {
        return 0.5;
    }
    let z = crate::ported::excel_numeric::excel_x87_mul(x.abs(), f64::from_bits(FRAC_1_SQRT_2_BITS));
    let q = match erfc_precise_kernel(z) {
        Ok(value) => value,
        Err(_) => return f64::NAN,
    };
    let half_q = 0.5 * q;
    let ns = if x.is_sign_negative() {
        half_q
    } else {
        crate::ported::excel_numeric::excel_x87_sub(1.0, half_q)
    };
    flush_subnormal(ns)
}

/// GAUSS is NORMSDIST's graph minus 0.5 on the ordinary route
/// (`abs(x) > 1e-15`). Inclusive tiny-direct uses the stored-z ERF route.
pub fn identified_gauss(x: f64) -> f64 {
    if x == 0.0 {
        return 0.0;
    }
    if x.abs() <= f64::from_bits(GAUSS_TINY_MAX_BITS) {
        return gauss_tiny_direct(x);
    }
    identified_std_normal_cdf(x) - 0.5
}

fn norm_cdf(x: f64) -> f64 {
    identified_std_normal_cdf(x)
}

fn validate_positive_sigma(sigma: f64) -> Result<(), WorksheetErrorCode> {
    if sigma <= 0.0 || !sigma.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(())
}

fn validate_probability_open_unit(p: f64) -> Result<(), WorksheetErrorCode> {
    if !p.is_finite() || p <= 0.0 || p >= 1.0 {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(())
}

fn inverse_standard_normal_acklam_refined(p: f64) -> f64 {
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    const P_LOW: f64 = 0.02425;
    const P_HIGH: f64 = 1.0 - P_LOW;

    let x = if p < P_LOW {
        let q = (-2.0 * crt_log(p)).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= P_HIGH {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * crt_log(1.0 - p)).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };

    // Form the lower tail directly. Subtracting erf from 1 loses it entirely
    // for small probabilities; at positive x compute the complementary error
    // before subtracting, so the Newton residual does not round through 1.
    let tail = 0.5 * libm::erfc(x.abs() / std::f64::consts::SQRT_2);
    let err = if x < 0.0 { tail - p } else { (1.0 - p) - tail };
    let density = phi_kernel(x);
    if density == 0.0 { x } else { x - err / density }
}

fn inverse_standard_normal(p: f64) -> Result<f64, WorksheetErrorCode> {
    validate_probability_open_unit(p)?;

    Ok(inverse_standard_normal_acklam_refined(p))
}

pub fn norm_s_dist_kernel(z: f64, cumulative: bool) -> Result<f64, WorksheetErrorCode> {
    if cumulative {
        Ok(norm_cdf(z))
    } else {
        crate::ported::phi_fn::phi_kernel(z)
    }
}

pub fn norm_dist_kernel(
    x: f64,
    mean: f64,
    sigma: f64,
    cumulative: bool,
) -> Result<f64, WorksheetErrorCode> {
    validate_positive_sigma(sigma)?;
    if cumulative {
        let z = (x - mean) / sigma;
        Ok(norm_cdf(z))
    } else {
        // W111 density observations distinguish all these publication points:
        // subtract and divide at RN64 then store binary64; divide the unflushed
        // exponential by sigma before multiplying the normal constant. Calling
        // worksheet PHI first loses both scale ordering and tiny intermediates.
        use crate::ported::excel_numeric::{excel_exp, excel_x87_div, excel_x87_mul, excel_x87_sub};
        let z = excel_x87_div(excel_x87_sub(x, mean), sigma);
        let square = excel_x87_mul(z, z);
        if !square.is_finite() {
            return Err(WorksheetErrorCode::Num);
        }
        let exponential = excel_exp(-(square / 2.0));
        let value = excel_x87_mul(
            excel_x87_div(exponential, sigma),
            f64::from_bits(0x3fd9884533d43651),
        );
        Ok(flush_subnormal(value))
    }
}

pub fn norm_s_inv_kernel(p: f64) -> Result<f64, WorksheetErrorCode> {
    inverse_standard_normal(p)
}

pub fn norm_inv_kernel(p: f64, mean: f64, sigma: f64) -> Result<f64, WorksheetErrorCode> {
    validate_positive_sigma(sigma)?;
    Ok(mean + sigma * inverse_standard_normal(p)?)
}

pub fn lognorm_dist_kernel(
    x: f64,
    mean: f64,
    sigma: f64,
    cumulative: bool,
) -> Result<f64, WorksheetErrorCode> {
    validate_positive_sigma(sigma)?;
    if x <= 0.0 || !x.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    // Inverse-problem identity, live Excel 16.0 b20228: LOGNORM.DIST(x,μ,σ,TRUE)
    // == NORM.S.DIST((LN(x)-μ)/σ,TRUE) == NORMSDIST((LN(x)-μ)/σ)
    // == NORM.DIST(LN(x),μ,σ,TRUE) on 45/45 mixed (x,μ,σ) rows.
    // The full recorded banks distinguish the internal CRT logarithm at oc3066;
    // the public LN chain differs there. The earlier 45-row identity still holds.
    let z = (crt_log(x) - mean) / sigma;
    if cumulative {
        Ok(norm_cdf(z))
    } else {
        Ok(phi_kernel(z) / (x * sigma))
    }
}

pub fn lognorm_inv_kernel(p: f64, mean: f64, sigma: f64) -> Result<f64, WorksheetErrorCode> {
    validate_positive_sigma(sigma)?;
    // Same capture: LOGNORM.INV(0.5,μ,σ) == EXP(NORM.INV(0.5,μ,σ)) on 45/45.
    Ok(crate::ported::excel_numeric::excel_exp(
        mean + sigma * inverse_standard_normal(p)?,
    ))
}

pub fn confidence_norm_kernel(
    alpha: f64,
    standard_dev: f64,
    size: f64,
) -> Result<f64, WorksheetErrorCode> {
    validate_probability_open_unit(alpha)?;
    if standard_dev <= 0.0 || !standard_dev.is_finite() || !size.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    // Excel docs: size is truncated toward integer; size < 1 -> #NUM!.
    let size_int = size.trunc();
    if size_int < 1.0 {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(inverse_standard_normal(1.0 - alpha / 2.0)? * standard_dev / size_int.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_bits(actual: f64, expected_bits: u64) {
        assert_eq!(
            actual.to_bits(),
            expected_bits,
            "{actual} vs {}",
            f64::from_bits(expected_bits)
        );
    }

    // Mathematical controls from an independent AS241 evaluation (Python's
    // statistics.NormalDist), not Excel readings.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn inverse_normal_refinement_keeps_tiny_mathematical_tails() {
        for (p, expected) in [
            (1e-20, -9.262340089798405_f64),
            (1e-100, -21.27345356096532),
            (1e-300, -37.0470962993612),
        ] {
            let actual = norm_s_inv_kernel(p).unwrap();
            assert!((actual - expected).abs() < 4e-13,
                "p={p:e}, calculated {actual:e}, mathematical {expected:e}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn normsdist_gf3_wrapper_pins_live_excel_20228() {
        // Live Excel 16.0 b20228 G-F3 ladder. NS(-1) = RN53(0.5*Q) transports
        // the still-open ERFC body (1 ULP high at this z, same witness as
        // CHIDIST(1,1)). NS(1) = RN53(1-0.5*Q) matches Excel because the
        // complement absorbs that ULP. The dispatch claims the wrapper, not
        // the ERFC body.
        let ns_neg1 = identified_std_normal_cdf(-1.0);
        assert!(
            ns_neg1.to_bits().abs_diff(0x3fc44ed0bb7cb209) <= 1,
            "G-F3 should not add error beyond the ERFC body; got {:#x}",
            ns_neg1.to_bits()
        );
        let q = crate::ported::special_dist_family::erfc_precise_kernel(
            crate::ported::normal_dist_common::stored_normal_z(-1.0),
        )
        .unwrap();
        assert_eq!(ns_neg1.to_bits(), (0.5 * q).to_bits());
        assert_eq!(identified_std_normal_cdf(1.0).to_bits(), 0x3feaec4bd120d37e);
        assert_eq!(identified_std_normal_cdf(0.0).to_bits(), 0x3fe0000000000000);
        // Trailing-halving + PHI-class flush: NS(-37.52) publishes +0.
        assert_eq!(identified_std_normal_cdf(-37.52).to_bits(), 0);
        assert_eq!(
            norm_s_dist_kernel(1.0, true).unwrap().to_bits(),
            identified_std_normal_cdf(1.0).to_bits()
        );
        // GAUSS is NS minus 0.5 on the ordinary route.
        assert_eq!(
            identified_gauss(1.0).to_bits(),
            (identified_std_normal_cdf(1.0) - 0.5).to_bits()
        );
        assert_eq!(
            identified_gauss(-1.0).to_bits(),
            (identified_std_normal_cdf(-1.0) - 0.5).to_bits()
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn norm_family_matches_excel_probe_lanes() {
        assert!(
            (norm_dist_kernel(42.0, 40.0, 1.5, false).unwrap() - 0.109340049783996).abs() < 1e-12
        );
        assert!(
            (norm_dist_kernel(42.0, 40.0, 1.5, true).unwrap() - 0.908788780274132).abs() < 1e-7
        );
        assert!((norm_s_dist_kernel(1.0, false).unwrap() - 0.241970724519143).abs() < 1e-12);
        assert!((norm_s_dist_kernel(1.0, true).unwrap() - 0.841344746068543).abs() < 1e-7);
        assert!((norm_inv_kernel(0.9, 40.0, 1.5).unwrap() - 41.9223273483169).abs() < 1e-7);
        assert!((norm_s_inv_kernel(0.9).unwrap() - 1.2815515655446).abs() < 1e-7);
        assert!(
            (confidence_norm_kernel(0.05, 2.5, 100.0).unwrap() - 0.489990996135013).abs() < 1e-7
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn norm_family_records_exact_controls_and_inverse_gap() {
        assert_eq!(norm_dist_kernel(0.0, 0.0, 1.0, true).unwrap(), 0.5);
        assert_eq!(norm_s_dist_kernel(0.0, true).unwrap(), 0.5);
        // Recorded Excel is 0x3fff5c0331eeff82; the uniform Acklam method differs.
        assert_bits(norm_inv_kernel(0.975, 0.0, 1.0).unwrap(), 0x3fff5c0331eeff84);
        assert_bits(norm_s_inv_kernel(0.975).unwrap(), 0x3fff5c0331eeff84);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn norm_inv_tail_records_known_gaps() {
        for (p, expected_bits, calculated_bits) in [
            (0.0001_f64, 0xc00d_c08b_b712_893au64, 0xc00dc08bb712893b),
            (0.001_f64, 0xc008_b8cb_b720_4470u64, 0xc008b8cbb7204471),
            (0.01_f64, 0xc002_9c5c_4630_ff0eu64, 0xc0029c5c4630ff0f),
            (0.95_f64, 0x3ffa_5152_0967_6ab8u64, 0x3ffa515209676abc),
            (0.975_f64, 0x3fff_5c03_31ee_ff82u64, 0x3fff5c0331eeff84),
            (0.97575_f64, 0x3fff_913f_9b7a_a942u64, 0x3fff913f9b7aa943),
        ] {
            assert_eq!(norm_s_inv_kernel(p).unwrap().to_bits(), calculated_bits, "recorded Excel {expected_bits:#018x}");
            assert_eq!(norm_inv_kernel(p, 0.0, 1.0).unwrap().to_bits(), calculated_bits, "recorded Excel {expected_bits:#018x}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn norm_inv_central_witnesses_keep_existing_excel_matches() {
        for (p, expected_bits) in [
            (0.15_f64, 0xbff0_953b_2d85_bb6c),
            (0.35_f64, 0xbfd8_a917_2c6c_cc51),
            (0.4_f64, 0xbfd0_36d6_c4a0_4b5a),
            (0.5_f64, 0x0000_0000_0000_0000),
            (0.925_f64, 0x3ff7_0852_26d3_e526),
        ] {
            assert_bits(norm_s_inv_kernel(p).unwrap(), expected_bits);
            assert_bits(norm_inv_kernel(p, 0.0, 1.0).unwrap(), expected_bits);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn lognorm_family_matches_excel_probe_lanes() {
        let mean = crt_log(4.0_f64);
        assert!(
            (lognorm_dist_kernel(4.0, mean, 0.2, false).unwrap() - 0.498677850501791).abs() < 1e-12
        );
        assert!((lognorm_dist_kernel(4.0, mean, 0.2, true).unwrap() - 0.5).abs() < 1e-7);
        assert!((lognorm_inv_kernel(0.9, mean, 0.2).unwrap() - 5.16861455178106).abs() < 1e-6);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn family_domain_errors_match_excel_probe_lanes() {
        assert_eq!(
            norm_dist_kernel(1.0, 0.0, 0.0, true),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(norm_inv_kernel(0.0, 0.0, 1.0), Err(WorksheetErrorCode::Num));
        assert_eq!(norm_s_inv_kernel(1.0), Err(WorksheetErrorCode::Num));
        assert_eq!(
            lognorm_dist_kernel(0.0, 0.0, 1.0, true),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            lognorm_inv_kernel(0.5, 0.0, 0.0),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            confidence_norm_kernel(0.0, 1.0, 10.0),
            Err(WorksheetErrorCode::Num)
        );
    }

    // BUG-FUNC-039 item 4: CONFIDENCE size truncation and size<1 rejection.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn confidence_size_truncation_and_sub1_rejection() {
        // Fractional size: result should match the integer-truncated value.
        let with_fraction = confidence_norm_kernel(0.05, 2.5, 100.9).unwrap();
        let truncated = confidence_norm_kernel(0.05, 2.5, 100.0).unwrap();
        assert_eq!(
            with_fraction, truncated,
            "CONFIDENCE(0.05,2.5,100.9) must equal CONFIDENCE(0.05,2.5,100)"
        );
        // size < 1 -> #NUM!
        assert_eq!(
            confidence_norm_kernel(0.05, 2.5, 0.5),
            Err(WorksheetErrorCode::Num),
            "CONFIDENCE with size=0.5 must return #NUM!"
        );
        // size = 0 -> #NUM!
        assert_eq!(
            confidence_norm_kernel(0.05, 2.5, 0.0),
            Err(WorksheetErrorCode::Num)
        );
    }
}
