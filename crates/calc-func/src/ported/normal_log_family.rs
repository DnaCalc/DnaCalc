//! The normal, lognormal and related inverse kernels.
//!
//! The inverse uses the published Acklam rational approximation and one Newton
//! refinement. Exact Excel parity for this generic method remains open.

use crate::ported::excel_numeric::crt_log;
use crate::ported::normal_dist_common::{
    FRAC_1_SQRT_2_BITS, GAUSS_TINY_MAX_BITS, erf_approx, phi_kernel, stored_normal_z,
};
use crate::ported::special_dist_family::erfc_precise_kernel;
use crate::value::WorksheetErrorCode;

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
    let z =
        crate::ported::excel_numeric::excel_x87_mul(x.abs(), f64::from_bits(FRAC_1_SQRT_2_BITS));
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

/// The tail of the inverse standard normal by the rational approximations of
/// M. J. Wichura, "Algorithm AS 241: The Percentage Points of the Normal Distribution",
/// Applied Statistics 37(3), 1988, pp. 477-484 (PPND16): the published coefficients of its
/// tail approximations evaluated in `sqrt(-ln p)`. Every Horner product and sum and the final
/// quotient is stored to binary64 from precision 64 (R42: the lower windows near 6.4e-82 and
/// 1.8e-26 separate it from strict binary64; the R39 upper answers agree with both).
fn inverse_standard_normal_as241_tail(p: f64) -> f64 {
    use crate::ported::excel_numeric::{excel_x87_add, excel_x87_div, excel_x87_mul};
    let horner = |c: &[f64; 8], r: f64| {
        c[..7]
            .iter()
            .rev()
            .fold(c[7], |v, &a| excel_x87_add(excel_x87_mul(v, r), a))
    };
    const C: [f64; 8] = [
        1.4234371107496835,
        4.630337846156546,
        5.769497221460691,
        3.6478483247632045,
        1.2704582524523684,
        0.2417807251774506,
        0.022723844989269184,
        0.0007745450142783414,
    ];
    const D: [f64; 8] = [
        1.0,
        2.053191626637759,
        1.6763848301838038,
        0.6897673349851,
        0.14810397642748008,
        0.015198666563616457,
        0.0005475938084995345,
        1.0507500716444169e-09,
    ];
    const E: [f64; 8] = [
        6.657904643501103,
        5.463784911164114,
        1.7848265399172913,
        0.29656057182850487,
        0.026532189526576124,
        0.0012426609473880784,
        2.7115555687434876e-05,
        2.0103343992922881e-07,
    ];
    const F: [f64; 8] = [
        1.0,
        0.599832206555888,
        0.1369298809227358,
        0.014875361290850615,
        0.0007868691311456133,
        1.8463183175100548e-05,
        1.421511758316446e-07,
        2.0442631033899397e-15,
    ];

    let q = p - 0.5;
    let tail_p = if q < 0.0 { p } else { 1.0 - p };
    let mut r = libm::sqrt(-crt_log(tail_p));
    let x = if r <= 5.0 {
        r -= 1.6;
        excel_x87_div(horner(&C, r), horner(&D, r))
    } else {
        r -= 5.0;
        excel_x87_div(horner(&E, r), horner(&F, r))
    };

    if q < 0.0 { -x } else { x }
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

    // R39 single-bit regions separate the upper AS241 route at 15/16. Below AS241's
    // central bound (`p - 0.5 < -0.425`) Excel publishes the same AS241 tail evaluated at
    // `p` itself (R42: 393,140 of 393,216 retained R41 answers there). The central route and
    // the upper interval (0.925, 15/16] keep their existing numerical program.
    Ok(if p > 15.0 / 16.0 || p - 0.5 < -0.425 {
        inverse_standard_normal_as241_tail(p)
    } else {
        inverse_standard_normal_acklam_refined(p)
    })
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
        use crate::ported::excel_numeric::{
            excel_exp, excel_x87_div, excel_x87_mul, excel_x87_sub,
        };
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
    use crate::ported::excel_numeric::{excel_x87_add, excel_x87_mul};
    Ok(excel_x87_add(mean, excel_x87_mul(sigma, inverse_standard_normal(p)?)))
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
        return Ok(norm_cdf(z));
    }
    // The density never multiplies by 1 / x: answers on adjacent x repeat until the stored LN moves.
    // On adjacent means the answers' exponent sits on one grid at a fine offset of minus the stored
    // logarithms, which names `excel_exp_of_sum`. `(d*d / sigma*sigma) / 2` holds all 7,809 rows of
    // the bank in this order of the logarithms; the other five orders hold 7,398 to 7,680.
    use crate::ported::excel_numeric::{
        excel_exp_of_sum, excel_log, excel_x87_div, excel_x87_mul, excel_x87_sub,
    };
    let log_x = excel_log(x);
    let d = excel_x87_sub(log_x, mean);
    let half_square = excel_x87_div(excel_x87_mul(d, d), excel_x87_mul(sigma, sigma)) / 2.0;
    if half_square.is_nan() {
        return Err(WorksheetErrorCode::Num);
    }
    const LN_SQRT_2PI: f64 = 0.9189385332046728;
    Ok(flush_subnormal(excel_exp_of_sum(
        half_square,
        &[excel_log(sigma), LN_SQRT_2PI, log_x],
    )))
}

pub fn lognorm_inv_kernel(p: f64, mean: f64, sigma: f64) -> Result<f64, WorksheetErrorCode> {
    // Excel 20527: both lognormal inverse names match EXP of the stored
    // scaled normal inverse on independent midpoint and general controls.
    Ok(crate::ported::excel_numeric::excel_exp(norm_inv_kernel(p, mean, sigma)?))
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
            assert!(
                (actual - expected).abs() < 4e-13,
                "p={p:e}, calculated {actual:e}, mathematical {expected:e}"
            );
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
        // R39 restores the measured upper tail through the shared inverse.
        assert_bits(
            norm_inv_kernel(0.975, 0.0, 1.0).unwrap(),
            0x3fff5c0331eeff82,
        );
        assert_bits(norm_s_inv_kernel(0.975).unwrap(), 0x3fff5c0331eeff82);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn norm_inv_tail_records_known_gaps() {
        for (p, expected_bits, calculated_bits) in [
            (0.0001_f64, 0xc00d_c08b_b712_893au64, 0xc00dc08bb712893a),
            (0.001_f64, 0xc008_b8cb_b720_4470u64, 0xc008b8cbb7204470),
            (0.01_f64, 0xc002_9c5c_4630_ff0eu64, 0xc0029c5c4630ff0e),
            (0.95_f64, 0x3ffa_5152_0967_6ab8u64, 0x3ffa515209676ab8),
            (0.975_f64, 0x3fff_5c03_31ee_ff82u64, 0x3fff5c0331eeff82),
            (0.97575_f64, 0x3fff_913f_9b7a_a942u64, 0x3fff913f9b7aa942),
        ] {
            assert_eq!(
                norm_s_inv_kernel(p).unwrap().to_bits(),
                calculated_bits,
                "recorded Excel {expected_bits:#018x}"
            );
            assert_eq!(
                norm_inv_kernel(p, 0.0, 1.0).unwrap().to_bits(),
                calculated_bits,
                "recorded Excel {expected_bits:#018x}"
            );
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
    /// Fresh R39 upper-tail controls distinguish the CRT logarithm and full tables.
    /// R42: below p - 0.5 = -0.425 Excel 16.0.20527 publishes the AS241 tail at p, its Horner
    /// steps and quotient stored from precision 64 (R41 dense sweep, retained in
    /// DnaCalc-Research). The first input is the last double below 0.075; the other two separate
    /// the stored evaluation from strict binary64 by one ulp.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn norm_inv_lower_tail_is_stored_as241() {
        for (p, expected) in [
            (0x3fb3333333333332u64, 0xbff7085226d3e522u64),
            (0x2f136cef52c0864du64, 0xc03322ac3e8e938du64),
            (0x3a95b463aeea39cau64, 0xc0252b6664118c59u64),
        ] {
            let p = f64::from_bits(p);
            assert_bits(norm_s_inv_kernel(p).unwrap(), expected);
            assert_bits(norm_inv_kernel(p, 0.0, 1.0).unwrap(), expected);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn norm_inv_upper_as241_uses_full_coefficients_and_crt_log() {
        for (p, expected) in [
            (0x3fee000000000001u64, 0x3ff88bc1fbe1dac1u64),
            (0x3fee9b9ca4d07cf6u64, 0x3ffb61dc5570acfdu64),
            (0x3fee7918e8ecf545u64, 0x3ffaada9db55cbb9u64),
            (0x3fefffffffffffffu64, 0x40206b48528cea51u64),
        ] {
            let p = f64::from_bits(p);
            assert_bits(norm_s_inv_kernel(p).unwrap(), expected);
            assert_bits(norm_inv_kernel(p, 0.0, 1.0).unwrap(), expected);
        }
    }
}
