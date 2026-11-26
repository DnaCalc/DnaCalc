//! The normal, lognormal and related inverse kernels.
//!
//! The inverse follows Excel's route: the AS241 tails and, between them, stored x87 products of
//! three rational approximations to the inverse error function.

use crate::ported::excel_numeric::crt_log;
use crate::ported::normal_dist_common::{
    FRAC_1_SQRT_2_BITS, GAUSS_TINY_MAX_BITS, erf_approx, stored_normal_z,
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

/// `sqrt(-ln v)` as Excel's normal inverse forms it on the x87 (fn9, 2026-11-17): the logarithm
/// by `FYL2X` and the root by `FSQRT`, each stored to binary64. With this staging the AS241 lower
/// tail matches 393,216 of 393,216 retained R41 answers at normal inputs (the runtime `log` with
/// a binary64 root left 76) and piece B 244,928 of 244,928.
fn x87_sqrt_of_minus_log(v: f64) -> f64 {
    use crate::ported::excel_numeric::{excel_log, excel_sqrt_staged};
    excel_sqrt_staged(-excel_log(v))
}

/// Magnitude of the AS 241 tail deviate for upper/lower tail probability `tail_p` in `(0, 0.5]`.
/// Shared by `NORM.S.INV` and the `alpha == 0.5` (`df == 1`) gamma/chi-square inverse.
fn as241_tail_magnitude(tail_p: f64) -> f64 {
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

    let mut r = x87_sqrt_of_minus_log(tail_p);
    if r <= 5.0 {
        r -= 1.6;
        excel_x87_div(horner(&C, r), horner(&D, r))
    } else {
        r -= 5.0;
        excel_x87_div(horner(&E, r), horner(&F, r))
    }
}

/// The tail of the inverse standard normal by the rational approximations of
/// M. J. Wichura, "Algorithm AS 241: The Percentage Points of the Normal Distribution",
/// Applied Statistics 37(3), 1988, pp. 477-484 (PPND16): the published coefficients of its
/// tail approximations evaluated in `sqrt(-ln p)`. Every Horner product and sum and the final
/// quotient is stored to binary64 from precision 64 (R42: the lower windows near 6.4e-82 and
/// 1.8e-26 separate it from strict binary64; the R39 upper answers agree with both).
fn inverse_standard_normal_as241_tail(p: f64) -> f64 {
    let q = p - 0.5;
    let tail_p = if q < 0.0 { p } else { 1.0 - p };
    let x = as241_tail_magnitude(tail_p);
    if q < 0.0 { -x } else { x }
}

/// Horner evaluation with a binary64 store after each x87 product and sum.
fn stored_horner(x: f64, coefficients: &[u64]) -> f64 {
    use crate::ported::excel_numeric::{excel_x87_add, excel_x87_mul};
    let (leading, rest) = coefficients.split_last().expect("nonempty polynomial");
    rest.iter().rev().fold(f64::from_bits(*leading), |value, &bits| {
        excel_x87_add(excel_x87_mul(value, x), f64::from_bits(bits))
    })
}

/// Excel's `erfinv(t) / t` for `s = RN(t * t) <= 1/2` (dc-j9k.47, fn7): `RN(P(s) / Q(s))`, P and Q
/// degree 7 in Horner form with every product and sum stored from x87 precision 64, Q monic.
/// The rational interpolates `erfinv(sqrt s) / sqrt s` at the 15 zeros of T15 on [0, 0.73]; the
/// constants are that interpolant moved by -2.0e-15 along the smallest singular vector of its
/// 15 x 15 system (what a limited-precision solve leaves), then rounded to binary64. They match
/// 656,542 of 656,542 retained Excel answers and 44,739 of 44,739 fresh ones
/// (docs/research/normal-inverse-central-2026-11-17.md).
fn central_inverse_erf_ratio(s: f64) -> f64 {
    use crate::ported::excel_numeric::excel_x87_div;
    const P: [u64; 8] = [
        0x3fec_5bf8_91b4_ef6b,
        0xc00b_0359_9ad6_a33a,
        0x4014_92fc_431b_904d,
        0xc00f_ce6e_6f12_a56b,
        0x3ffa_0cbd_20e4_ce55,
        0xbfd5_444e_88a5_1cee,
        0x3f9c_034f_2417_c47b,
        0xbf3c_6105_b261_ae5d,
    ];
    const Q: [u64; 8] = [
        0x3ff0_0000_0000_0000,
        0xc010_49a6_a945_1cab,
        0x401a_e761_f165_bea6,
        0xc017_08d9_97e8_09b9,
        0x4005_9bb8_7268_973c,
        0xbfe5_70c8_8781_d342,
        0x3fb3_8664_c121_c0db,
        0xbf65_ae11_dcad_f003,
    ];
    excel_x87_div(stored_horner(s, &P), stored_horner(s, &Q))
}

/// Excel's ratio for piece B of the central inverse (dc-j9k.47, fn9 after the fable-r2 review):
/// `1/2 < s = RN(t * t)` with `|t| <= 0.85`. `RN(P(s) / Q(s))`, P degree 7 and Q degree 5 monic in
/// Horner form, every product and sum stored from x87 precision 64. The rational is the exact
/// interpolant of `erfinv(sqrt s) / sqrt(-ln(1 - sqrt s))` at the 13 zeros of T13 on [0.5, 0.73],
/// rounded to binary64; the caller multiplies it by `sqrt(-ln(1 - |t|))`.
fn piece_b_inverse_erf_ratio(s: f64) -> f64 {
    use crate::ported::excel_numeric::excel_x87_div;
    const P: [u64; 8] = [
        0x3fd1_99fb_95ef_4e09,
        0x401f_f5f6_0cbe_8bba,
        0x4015_a1d4_5395_4a8d,
        0xc043_aefb_9a3e_c59f,
        0x403e_f9ff_b45a_3761,
        0xc008_6cf9_d01e_16b2,
        0xc003_2e5a_7e82_7056,
        0x3fd0_8ad0_740a_6d4a,
    ];
    const Q: [u64; 6] = [
        0x3ff0_0000_0000_0000,
        0x402c_caea_b51a_6b87,
        0xc01a_4548_a23b_3e33,
        0xc045_9120_f7e3_3292,
        0x4047_ea3a_c1d3_95ff,
        0xc02a_d7d1_c366_79b0,
    ];
    excel_x87_div(stored_horner(s, &P), stored_horner(s, &Q))
}

/// Carbon (dc-j9k.47): the (9,9) Padé approximant of erfinv(t)/sqrt(-ln(1-t))
/// expanded at t = 225/256. The coefficients are listed in ascending degree.
fn piece_c_inverse_erf_ratio(t: f64) -> f64 {
    use crate::ported::excel_numeric::excel_x87_div;
    const P: [u64; 10] = [
        0x3fe8_2425_56d3_68b6,
        0xc036_abfe_9390_c9e6,
        0x4070_a97c_7664_c331,
        0xc097_b1a5_e1d9_a10c,
        0x40af_255c_31ac_92e5,
        0xc0a2_c8f8_55be_2320,
        0xc0bc_cdd3_c313_1b29,
        0x40be_d8c7_2b7a_0424,
        0x40ad_a1d6_9743_02b0,
        0xc0a1_b8ef_95ab_8b0e,
    ];
    const Q: [u64; 10] = [
        0x3ff0_0000_0000_0000,
        0xc03e_c8f6_36f7_0044,
        0x4077_6d84_2df4_b9aa,
        0xc0a1_a2a2_7132_af74,
        0x40ba_1d6d_c9f4_d349,
        0xc0bb_b914_1a0c_b063,
        0xc0b5_df83_d4e2_7971,
        0x40c7_5b4b_f097_172e,
        0x4089_f372_995a_fd52,
        0xc0a2_2d35_0f18_1b5e,
    ];
    let z = t - 225.0 / 256.0;
    excel_x87_div(stored_horner(z, &P), stored_horner(z, &Q))
}

fn inverse_standard_normal(p: f64) -> Result<f64, WorksheetErrorCode> {
    validate_probability_open_unit(p)?;

    // R39 single-bit regions separate the upper AS241 route at 15/16. Below AS241's
    // central bound (`p - 0.5 < -0.425`) Excel publishes the same AS241 tail evaluated at
    // `p` itself (R42: 393,140 of 393,216 retained R41 answers there).
    let q = p - 0.5;
    if p > 15.0 / 16.0 || q < -0.425 {
        return Ok(inverse_standard_normal_as241_tail(p));
    }
    // Piece A uses the stored square to select its rational at the boundary.
    use crate::ported::excel_numeric::excel_x87_mul;
    let t = 2.0 * q;
    let s = excel_x87_mul(t, t);
    if s <= 0.5 {
        let y = excel_x87_mul(t, central_inverse_erf_ratio(s));
        return Ok(excel_x87_mul(y, std::f64::consts::SQRT_2));
    }
    // Piece B (fn9): y = st(sqrt(-ln(1 - |t|)) * R(s)) for |t| <= 0.85. On the lower side that is
    // AS241's bound q >= -0.425; on the upper the last input is the double below p = 0.925
    // (p = 0.925 itself, t = 0.85 + 1 ulp, is piece C in Excel).
    let ratio = if t.abs() <= 0.85 {
        piece_b_inverse_erf_ratio(s)
    } else {
        piece_c_inverse_erf_ratio(t)
    };
    let a = x87_sqrt_of_minus_log(1.0 - t.abs());
    let y = excel_x87_mul(a, ratio);
    let x = excel_x87_mul(y, std::f64::consts::SQRT_2);
    Ok(if t < 0.0 { -x } else { x })
}

/// Excel's closed-form unit-scale gamma inverse at `alpha == 0.5` (`df == 1`):
/// `erfinv(p)^2`, sharing the four `NORM.S.INV` rational/AS 241 pieces without
/// the outer `sqrt(2)` factor on pieces A-C and scaling the AS 241 tail by
/// `1/sqrt(2)` (cb6, dc-j9k.60). Subnormal `y^2` flushes to `0.0` before the
/// caller applies the outer scale.
pub(crate) fn gamma_inv_half_unit(p: f64) -> f64 {
    use crate::ported::excel_numeric::{excel_x87_add, excel_x87_mul, excel_x87_sub};
    let q1 = excel_x87_add(0.5, excel_x87_sub(0.5, p));
    let y = if p > 15.0 / 16.0 {
        let z = as241_tail_magnitude(0.5 * q1);
        excel_x87_mul(z, f64::from_bits(FRAC_1_SQRT_2_BITS))
    } else {
        let s = excel_x87_mul(p, p);
        if s <= 0.5 {
            excel_x87_mul(p, central_inverse_erf_ratio(s))
        } else {
            let ratio = if p <= 0.85 {
                piece_b_inverse_erf_ratio(s)
            } else {
                piece_c_inverse_erf_ratio(p)
            };
            let a = x87_sqrt_of_minus_log(q1);
            excel_x87_mul(a, ratio)
        }
    };
    flush_subnormal(excel_x87_mul(y, y))
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
    // Excel 20527 (dc-j9k.324-327): a variance that is subnormal or whose double overflows is
    // #DIV/0!, never a density.
    let variance = excel_x87_mul(sigma, sigma);
    if variance.is_subnormal() || variance == 0.0 || !(variance * 2.0).is_finite() {
        return Err(WorksheetErrorCode::Div0);
    }
    let half_square = excel_x87_div(excel_x87_mul(d, d), variance) / 2.0;
    if half_square.is_nan() {
        return Err(WorksheetErrorCode::Num);
    }
    // The squared distance over the variance overflowed: exp(-inf) is zero (Excel 20527).
    if half_square.is_infinite() {
        return Ok(0.0);
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
    let exponent = norm_inv_kernel(p, mean, sigma)?;
    if !exponent.is_finite() {
        // An overflowing normal inverse is #NUM! before EXP (dc-j9k.404).
        return Err(WorksheetErrorCode::Num);
    }
    Ok(crate::ported::excel_numeric::excel_exp(exponent))
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
    // dc-bc1o: Excel scales the upper inverse by (sd / sqrt(n)), 121 of 121 against Excel's own
    // NORM.S.INV values (docs/research/confidence-normal-arithmetic-2026-11-12.md).
    // Each operation is stored from x87 (dc-j9k.359: with a huge deviation the binary64 product is
    // one ulp below Excel's).
    use crate::ported::excel_numeric::{excel_sqrt_staged, excel_x87_div, excel_x87_mul};
    let z = inverse_standard_normal(1.0 - alpha / 2.0)?;
    Ok(excel_x87_mul(z, excel_x87_div(standard_dev, excel_sqrt_staged(size_int))))
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

    /// Excel 20527, retained R41 near-half windows, including both measured bounds.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn inverse_normal_near_half_uses_stored_leading_term() {
        for (p, expected) in [
            (0x3fdffffffffff000, 0xbd640d931ff62706),
            (0x3fdffffffffff001, 0xbd640c5246c427a4),
            (0x3fdfffffffffffff, 0xbca40d931ff62706),
            (0x3fe0000000000000, 0x0000000000000000),
            (0x3fe0000000000001, 0x3cb40d931ff62706),
            (0x3fe0000000000800, 0x3d640d931ff62706),
        ] {
            assert_eq!(norm_s_inv_kernel(f64::from_bits(p)).unwrap().to_bits(), expected);
        }
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
            (0.4_f64, 0xbfd0_36d6_c4a0_4b5a_u64),
            (0.5_f64, 0x0000_0000_0000_0000),
            (0.925_f64, 0x3ff7_0852_26d3_e526),
        ] {
            assert_bits(norm_s_inv_kernel(p).unwrap(), expected_bits);
            assert_bits(norm_inv_kernel(p, 0.0, 1.0).unwrap(), expected_bits);
        }
    }

    /// Excel 16.0.20527 through Touchstone on 2026-11-17: the central piece s = t^2 <= 1/2 is
    /// Excel's stored rational (dc-j9k.47, fn7). p = 0.15 and 0.35 were one and two units low
    /// with the correctly rounded ratio; the table holds fresh answers asked after the constants
    /// were frozen (random inputs across s, the ends of dense windows at s = 0.46 to 0.4995 on
    /// both sides, and inputs where the last bit of the x^7 numerator constant decides).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn norm_inv_central_rational_matches_excel() {
        for (p, excel_bits) in [
            (0.15_f64.to_bits(), 0xbff0_953b_2d85_bb6c_u64),
            (0.35_f64.to_bits(), 0xbfd8_a917_2c6c_cc51),
            (0x3fe00016c61b4f00, 0x3efc8af482986bc0),
            (0x3fe07f9823c0dfb6, 0x3fa3fe9aae69cf26),
            (0x3fe0ff12ab06b2ff, 0x3fb4003029677f3a),
            (0x3fdcfd0308e85f26, 0xbfbe4456246a9da1),
            (0x3fdc044011c33310, 0xbfc40d2f44e0c020),
            (0x3fdb0eccbc8f34ff, 0xbfc8eee8e3d8a784),
            (0x3fe2f87387eed8fa, 0x3fce0ed059507870),
            (0x3fd91188d27dcccc, 0xbfd1980486de7031),
            (0x3fe3f8647de6e9d8, 0x3fd43c7a8ec4aac2),
            (0x3fd713e3b3e4d10c, 0xbfd6d7341b664ecd),
            (0x3fd616e5e1e12266, 0xbfd9804776515ea4),
            (0x3fe56f43cc58b418, 0x3fdc1ff190ab1c48),
            (0x3fe5eeb04cd992b2, 0x3fdee67506ac638c),
            (0x3fe66c6086621e06, 0x3fe0d916cef0c9eb),
            (0x3fd22bc37749b488, 0xbfe24788d3e5d044),
            (0x3fd12abb423d2b61, 0xbfe3c803073c015b),
            (0x3fd03aabf509940d, 0xbfe5397382135789),
            (0x3fe861b7c8980ec8, 0x3fe6ccf5ddfab228),
            (0x3fe8e11174c575fa, 0x3fe87027078e77a1),
            (0x3fe95d184c12b886, 0x3fea18c849c80578),
            (0x3fc87e67fcf4aa7a, 0xbfebeed5134524f5),
            (0x3fc68b0ad6908b02, 0xbfedc4b0ab3264ba),
            (0x3fead4dfe39a0f56, 0x3fef9fc520e02dc7),
            (0x3fc2c08d64d60a72, 0xbff0d32e4f5988be),
            (0x3feada0ad82d180e, 0x3fefb4e7f83da5e2),
            (0x3feada0ad82d200d, 0x3fefb4e7f83dc6a2),
            (0x3feada0ad82d280d, 0x3fefb4e7f83de75e),
            (0x3feb15cac3567d11, 0x3ff056a992aadcac),
            (0x3feb15cac3568510, 0x3ff056a992aaed8f),
            (0x3feb15cac3568d10, 0x3ff056a992aafe73),
            (0x3feb41ca9bff5f31, 0x3ff0b4a5941d658d),
            (0x3feb41ca9bff6730, 0x3ff0b4a5941d76e5),
            (0x3feb41ca9bff6f30, 0x3ff0b4a5941d8824),
            (0x3fc41fb20c770b5c, 0xbff0186a1b194a7c),
            (0x3fc41fb20c77135b, 0xbff0186a1b194662),
            (0x3fc41fb20c771b5b, 0xbff0186a1b194236),
            (0x3fc3333333332b34, 0xbff0953b2d85bfb7),
            (0x3fc3333333333333, 0xbff0953b2d85bb6c),
            (0x3fc3333333333b33, 0xbff0953b2d85b714),
            (0x3feb4edc613e8bbc, 0x3ff0d10027b178f0),
            (0x3feb4edc613e93bb, 0x3ff0d10027b18a54),
            (0x3feb4edc613e9bbb, 0x3ff0d10027b19bc4),
            (0x3feb06fccda8b49c, 0x3ff03787cf33a8f7),
            (0x3fc717429853e0fd, 0xbfed3e4647887078),
            (0x3fea69997a100da8, 0x3fedf49389baf684),
            (0x3feaf6b5ec1636ab, 0x3ff01593b2838662),
            (0x3fc81534d912be93, 0xbfec4fd420e44499),
        ] {
            let p = f64::from_bits(p);
            assert_bits(norm_s_inv_kernel(p).unwrap(), excel_bits);
            assert_bits(norm_inv_kernel(p, 0.0, 1.0).unwrap(), excel_bits);
        }
    }

    /// Piece B of the central inverse and the AS241 tail (dc-j9k.47, fn9): Excel 16.0.20527 answers
    /// retained in DnaCalc-Research (R41 dense sweep) and asked through Touchstone on 2026-11-17 by
    /// fn7 and the fable-r2 review. Both form `sqrt(-ln v)` by FYL2X and FSQRT, each stored; the old
    /// route (the runtime log, a binary64 root, and in B the correctly rounded `erfinv(t)/t`) missed
    /// every one of these, the last three because a binary64 logarithm is one ulp off there.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn norm_inv_piece_b_and_tail_use_x87_log_and_root() {
        for (p, excel_bits) in [
            (0x0145_16b7_cda5_4b1f_u64, 0xc042_946b_b420_e517_u64), // tail, old route +1 ulp
            (0x2d55_3f13_5c24_ab79, 0xc034_1e02_3283_2e85), // tail, old route +1 ulp
            (0x3b19_89d0_5328_61e3, 0xc024_128e_04ba_b0e1), // tail, old route -1 ulp
            (0x3d18_418f_d786_1609, 0xc01e_348c_bd98_858d), // tail, old route +2 ulp
            (0x3d18_418f_d786_1613, 0xc01e_348c_bd98_858d), // tail, old route +2 ulp
            (0x3d18_418f_d786_1615, 0xc01e_348c_bd98_858d), // tail, old route +2 ulp
            (0x3fb3_74bc_6a7e_fc8f, 0xbff6_eb87_f916_0a9d), // B lower, old route -1 ulp
            (0x3fb3_74bc_6a7f_00db, 0xbff6_eb87_f916_08b5), // B lower, old route +6 ulp
            (0x3fb3_74bc_6a7f_03c7, 0xbff6_eb87_f916_0770), // B lower, old route +4 ulp
            (0x3fb7_9c32_76ac_ebd8, 0xbff5_3c10_e3d2_a3e2), // B lower, old route -2 ulp
            (0x3fbf_ffff_ffff_f2aa, 0xbff2_67d4_c07b_0974), // B lower, old route +1 ulp
            (0x3fc1_b27a_4b27_fc28, 0xbff1_6928_de49_d604), // B lower, old route -2 ulp
            (0x3feb_504f_333f_9e4d, 0x3ff0_d427_e557_a614), // B upper, old route -2 ulp
            (0x3feb_8684_b047_dfc9, 0x3ff1_4c21_69be_7d58), // B upper, old route -2 ulp
            (0x3fec_02ea_4093_133e, 0x3ff2_6eeb_2352_419b), // B upper, old route -2 ulp
            (0x3fec_a316_9164_bd20, 0x3ff4_0c89_8dac_a5ca), // B upper, old route +4 ulp
            (0x3fec_a316_9164_c692, 0x3ff4_0c89_8dac_bfc5), // B upper, old route -3 ulp
            (0x3fec_cccc_cccc_d53b, 0x3ff4_813c_36e2_8535), // B upper, old route +2 ulp
            (0x3feb_51eb_851e_b462, 0x3ff0_d7aa_cd5c_646b), // B, binary64 log one ulp off
            (0x3fec_4d7b_e7d7_4a68, 0x3ff3_2943_13a5_f4a6), // B, binary64 log one ulp off
            (0x3fed_4456_d061_2345, 0x3ff5_e9d4_c769_38db), // B, binary64 log one ulp off
        ] {
            let p = f64::from_bits(p);
            assert_bits(norm_s_inv_kernel(p).unwrap(), excel_bits);
            assert_bits(norm_inv_kernel(p, 0.0, 1.0).unwrap(), excel_bits);
        }
    }

    /// Carbon's Excel 20527 controls at both edges of C and two frozen inputs
    /// distinguishing the Q[8] coefficient and x87 product/sum staging.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn norm_inv_piece_c_edges_and_stores() {
        for (p, expected) in [
            (0x3fed_9999_9999_9999, 0x3ff7_0852_26d3_e527),
            (0x3fed_9999_9999_999a, 0x3ff7_0852_26d3_e526),
            (0x3fed_999b_c5f0_efd8, 0x3ff7_0859_d3f0_2e4d),
            (0x3fed_c0d1_b820_8c5d, 0x3ff7_965d_7782_f57e),
            (0x3fee_0000_0000_0000, 0x3ff8_8bc1_fbe1_dabf),
            (0x3fee_0000_0000_0001, 0x3ff8_8bc1_fbe1_dac1),
        ] {
            assert_bits(norm_s_inv_kernel(f64::from_bits(p)).unwrap(), expected);
        }
    }

    /// Near one half the old Newton residual cancelled: Excel's answer for bank row
    /// 0x3fe00160d8d68c36 was 580 ULP from the old route and is matched now.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn norm_inv_central_near_half_is_accurate() {
        let x = norm_s_inv_kernel(f64::from_bits(0x3fe0_0160_d8d6_8c36)).unwrap();
        assert_bits(x, 0x3f3b_a3a6_9c29_238c);
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
    /// Fresh R39 upper-tail controls distinguish the full tables.
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
    fn norm_inv_upper_as241_uses_full_coefficients() {
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

