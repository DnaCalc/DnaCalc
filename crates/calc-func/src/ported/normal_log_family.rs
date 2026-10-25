use crate::ported::normal_dist_common::{erf_approx, phi_kernel, stored_normal_z, FRAC_1_SQRT_2_BITS, GAM1_HALF_H_BITS, GAUSS_TINY_MAX_BITS};
use crate::ported::special_dist_family::erfc_precise_kernel;
use crate::value::WorksheetErrorCode;

fn erf_based_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf_approx(x / std::f64::consts::SQRT_2))
}

fn flush_subnormal(value: f64) -> f64 {
    if value.abs() < f64::MIN_POSITIVE {
        0.0
    } else {
        value
    }
}

/// TOMS 654 branch-190 `j` series at `a = 1/2`, binary64 arithmetic.
fn branch190_j(x: f64) -> f64 {
    let a = 0.5;
    let mut an = 3.0;
    let mut c = x;
    let mut sum = x / (a + 3.0);
    let tolerance = (3.0 * 5e-15) / (a + 1.0);
    for _ in 0..200 {
        an += 1.0;
        c = -(c * (x / an));
        let term = c / (a + an);
        sum += term;
        if term.abs() <= tolerance {
            break;
        }
    }
    let inner_poly = (sum / 6.0 - 0.5 / (a + 2.0)) * x + 1.0 / (a + 1.0);
    a * x * inner_poly
}

/// Tiny-direct GAUSS (G-A400 dataflow): reuse stored `z` as `w`, `g = 1+h`,
/// inner `0.5+(0.5-j)`, `(w*g)*inner`, then `0.5 * product`. Identified on
/// the 14 separator inputs (2026-08-21, stored-z class 14/14). The Ext80
/// continuous series of the racer is collapsed here to binary64; j is
/// negligible on the deep-tiny separators and first visible near 1e-15.
fn gauss_tiny_direct(x: f64) -> f64 {
    let z = stored_normal_z(x);
    if z == 0.0 {
        return 0.0;
    }
    let xx = z * z;
    let j = branch190_j(xx);
    let g = 1.0 + f64::from_bits(GAM1_HALF_H_BITS);
    let inner = 0.5 + (0.5 - j);
    let product = (z * g) * inner;
    let mut y = 0.5 * product;
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
        1.0 - half_q
    };
    flush_subnormal(ns)
}

/// GAUSS is NORMSDIST's graph minus 0.5 on the ordinary route
/// (`abs(x) > 1e-15`). Inclusive tiny-direct uses stored-z branch 190.
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

fn inverse_standard_normal_as241_tail(p: f64) -> f64 {
    const C: [f64; 8] = [
        1.423_437_110_749_683_6,
        4.630_337_846_156_545,
        5.769_497_221_460_691,
        3.647_848_324_763_204_5,
        1.270_458_252_452_368_4,
        2.417_807_251_774_506_2e-1,
        2.272_384_498_926_918_5e-2,
        7.745_450_142_783_414e-4,
    ];
    const D: [f64; 8] = [
        1.0,
        2.053_191_626_637_759,
        1.676_384_830_183_803_8,
        6.897_673_349_851e-1,
        1.481_039_764_274_800_7e-1,
        1.519_866_656_361_645_7e-2,
        5.475_938_084_995_345e-4,
        1.050_750_071_644_416_8e-9,
    ];
    const E: [f64; 8] = [
        6.657_904_643_501_104,
        5.463_784_911_164_114,
        1.784_826_539_917_291_3,
        2.965_605_718_285_049e-1,
        2.653_218_952_657_612_3e-2,
        1.242_660_947_388_078_4e-3,
        2.711_555_568_743_487_6e-5,
        2.010_334_399_292_288_2e-7,
    ];
    const F: [f64; 8] = [
        1.0,
        5.998_322_065_558_879e-1,
        1.369_298_809_227_358e-1,
        1.487_536_129_085_061_5e-2,
        7.868_691_311_456_133e-4,
        1.846_318_317_510_054_7e-5,
        1.421_511_758_316_445_9e-7,
        2.044_263_103_389_939_7e-15,
    ];

    let q = p - 0.5;
    let tail_p = if q < 0.0 { p } else { 1.0 - p };
    let mut r = libm::sqrt(-libm::log(tail_p));
    let x = if r <= 5.0 {
        r -= 1.6;
        let numerator =
            ((((((C[7] * r + C[6]) * r + C[5]) * r + C[4]) * r + C[3]) * r + C[2]) * r + C[1]) * r
                + C[0];
        let denominator =
            ((((((D[7] * r + D[6]) * r + D[5]) * r + D[4]) * r + D[3]) * r + D[2]) * r + D[1]) * r
                + D[0];
        numerator / denominator
    } else {
        r -= 5.0;
        let numerator =
            ((((((E[7] * r + E[6]) * r + E[5]) * r + E[4]) * r + E[3]) * r + E[2]) * r + E[1]) * r
                + E[0];
        let denominator =
            ((((((F[7] * r + F[6]) * r + F[5]) * r + F[4]) * r + F[3]) * r + F[2]) * r + F[1]) * r
                + F[0];
        numerator / denominator
    };

    if q < 0.0 {
        -x
    } else {
        x
    }
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
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= P_HIGH {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };

    let err = erf_based_cdf(x) - p;
    x - err / phi_kernel(x)
}

fn inverse_standard_normal(p: f64) -> Result<f64, WorksheetErrorCode> {
    validate_probability_open_unit(p)?;

    // Fresh Excel probes match the AS241/Wichura tail polynomials on the
    // retained upper-tail blocker and several outer-tail witnesses. The central
    // band keeps the existing refined Acklam path until we have a stronger
    // theory for Excel's mixed middle behavior.
    if p < 0.025 || (0.95..0.99).contains(&p) {
        Ok(inverse_standard_normal_as241_tail(p))
    } else {
        Ok(inverse_standard_normal_acklam_refined(p))
    }
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
    let z = (crate::ported::excel_numeric::excel_log(x) - mean) / sigma;
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
    fn norm_family_matches_exact_excel_value_witnesses() {
        assert_eq!(norm_dist_kernel(0.0, 0.0, 1.0, true).unwrap(), 0.5);
        assert_eq!(norm_s_dist_kernel(0.0, true).unwrap(), 0.5);
        assert_bits(
            norm_inv_kernel(0.975, 0.0, 1.0).unwrap(),
            0x3fff_5c03_31ee_ff82,
        );
        assert_bits(norm_s_inv_kernel(0.975).unwrap(), 0x3fff_5c03_31ee_ff82);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn norm_inv_tail_witnesses_match_excel_bits() {
        for (p, expected_bits) in [
            (0.0001_f64, 0xc00d_c08b_b712_893a),
            (0.001_f64, 0xc008_b8cb_b720_4470),
            (0.01_f64, 0xc002_9c5c_4630_ff0e),
            (0.95_f64, 0x3ffa_5152_0967_6ab8),
            (0.975_f64, 0x3fff_5c03_31ee_ff82),
            (0.97575_f64, 0x3fff_913f_9b7a_a942),
        ] {
            assert_bits(norm_s_inv_kernel(p).unwrap(), expected_bits);
            assert_bits(norm_inv_kernel(p, 0.0, 1.0).unwrap(), expected_bits);
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
        let mean = 4.0_f64.ln();
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
