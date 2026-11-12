use crate::ported::excel_numeric::crt_exp;
use crate::ported::excel_numeric::crt_log;
use crate::ported::special_math_common::{
    bisect_inverse, ln_gamma, regularized_beta, regularized_gamma_p,
};
use crate::value::WorksheetErrorCode;

fn bool_flag(value: f64) -> bool {
    value != 0.0
}

fn beta_log_norm(alpha: f64, beta: f64) -> f64 {
    ln_gamma(alpha) + ln_gamma(beta) - ln_gamma(alpha + beta)
}

fn validate_beta_shape(
    alpha: f64,
    beta: f64,
    lower: f64,
    upper: f64,
) -> Result<(), WorksheetErrorCode> {
    if !alpha.is_finite() || !beta.is_finite() || !lower.is_finite() || !upper.is_finite() {
        return Err(WorksheetErrorCode::Value);
    }
    if alpha <= 0.0 || beta <= 0.0 || upper <= lower {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(())
}

pub(crate) fn beta_dist_kernel(
    x: f64,
    alpha: f64,
    beta: f64,
    cumulative: bool,
    lower: f64,
    upper: f64,
) -> Result<f64, WorksheetErrorCode> {
    validate_beta_shape(alpha, beta, lower, upper)?;
    if !x.is_finite() {
        return Err(WorksheetErrorCode::Value);
    }
    if x < lower || x > upper {
        return Err(WorksheetErrorCode::Num);
    }
    let z = (x - lower) / (upper - lower);
    if cumulative {
        // W109 lane-3 (2026-07-18, b30): Excel has NO integer-shape beta
        // fast path — bratio scores integer shapes at the same wall rate as
        // fractional (344/768 vs the binomial-sum shortcut's 254/768, and
        // the shortcut loses the disagreement rows 177:87). Route all
        // shapes through the identified BRATIO.
        return Ok(regularized_beta(z, alpha, beta));
    }
    // Excel normalises from the lower endpoint, then divides its unit density
    // by the width, with each operation stored through PC64 (dc-bc1o,
    // statistical-density-scaling-2026-11-12.md). The CDF keeps its own route.
    use crate::ported::excel_numeric::{excel_x87_div, excel_x87_sub};
    let width = excel_x87_sub(upper, lower);
    let unit_x = excel_x87_div(excel_x87_sub(x, lower), width);
    if unit_x > 0.0 && unit_x < 1.0 {
        if let Some(pdf) = crate::ported::discrete_dist::beta_density(unit_x, alpha, beta) {
            let pdf = excel_x87_div(pdf, width);
            return if pdf.is_finite() {
                Ok(pdf)
            } else {
                Err(WorksheetErrorCode::Num)
            };
        }
    }
    let log_pdf = (alpha - 1.0) * crt_log(z) + (beta - 1.0) * crt_log(1.0 - z)
        - beta_log_norm(alpha, beta)
        - crt_log(upper - lower);
    let pdf = crt_exp(log_pdf);
    if pdf.is_finite() {
        Ok(pdf)
    } else {
        Err(WorksheetErrorCode::Num)
    }
}

pub(crate) fn beta_inv_kernel(
    probability: f64,
    alpha: f64,
    beta: f64,
    lower: f64,
    upper: f64,
) -> Result<f64, WorksheetErrorCode> {
    validate_beta_shape(alpha, beta, lower, upper)?;
    if !probability.is_finite() {
        return Err(WorksheetErrorCode::Value);
    }
    if !(0.0..=1.0).contains(&probability) {
        return Err(WorksheetErrorCode::Num);
    }
    let z = bisect_inverse(probability, 0.0, 1.0, |v| regularized_beta(v, alpha, beta));
    Ok(lower + z * (upper - lower))
}

fn validate_gamma_shape(alpha: f64, beta: f64) -> Result<(), WorksheetErrorCode> {
    if !alpha.is_finite() || !beta.is_finite() {
        return Err(WorksheetErrorCode::Value);
    }
    if alpha <= 0.0 || beta <= 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(())
}

pub fn gamma_dist_kernel(
    x: f64,
    alpha: f64,
    beta: f64,
    cumulative: bool,
) -> Result<f64, WorksheetErrorCode> {
    validate_gamma_shape(alpha, beta)?;
    if !x.is_finite() {
        return Err(WorksheetErrorCode::Value);
    }
    if x < 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    if cumulative {
        // Inverse-problem identity, live Excel 16.0 b20228:
        // GAMMA.DIST(x,0.5,beta,TRUE) == ERF.PRECISE(SQRT(x/beta))
        // for beta in {1,2,4} on 68/68 (and beta=2 on the earlier 154/154
        // bank). Staging is divide-then-sqrt, matching the df=1 chi route
        // when beta=2. Other shapes are not claimed.
        if alpha == 0.5 && beta > 0.0 && beta.is_finite() {
            return crate::ported::special_dist_family::erf_precise_kernel((x / beta).sqrt());
        }
        // Inverse-problem identity, live Excel 16.0 b20228:
        // GAMMA.DIST(x,1,beta,TRUE) == EXPON.DIST(x/beta,1,TRUE)
        // (85/85 on dyadic scales; 11/11 on beta=3 and beta=5).
        // EXPON.DIST(x,1/beta,TRUE) misses by 1 ULP when 1/beta is inexact.
        // PDF identity is false (33/85).
        if alpha == 1.0 {
            return crate::ported::discrete_dist::expon_dist_kernel(x / beta, 1.0, true);
        }
        // W109 lane-3 (2026-07-18): the pre-W109 integer-shape fast path
        // (1 − e^{-x}·Σ x^k/k!) is an OxFunc-side shortcut Excel does NOT
        // take — on the b26 integer-a corpora it scored 8.1% with ±4,400-ULP
        // catastrophics vs 39.4% (worst −10) through the identified GRATIO
        // path. All other shapes route through regularized_gamma_p.
        return Ok(regularized_gamma_p(alpha, x / beta));
    }
    // Gamma(1) = 1 exactly; Lanczos leaves a small nonzero log at this point.
    if x > 0.0 {
        // The density is the Poisson mass at `alpha` of mean `x/beta`, stored, times the stored
        // `alpha/x`: R30's scaled public calls read the ratio, and with this mass every one of
        // the 750 unit-shape rows of the bank agrees, where `exp(-x/beta)/beta` does not.
        use crate::ported::excel_numeric::{excel_x87_div, excel_x87_mul};
        let y = excel_x87_div(x, beta);
        let r = crate::ported::discrete_dist::poisson_mass(alpha, y);
        let pdf = excel_x87_mul(r, excel_x87_div(alpha, x));
        return if pdf.is_finite() {
            Ok(pdf)
        } else {
            Err(WorksheetErrorCode::Num)
        };
    }
    let log_normalizer = if alpha == 1.0 { 0.0 } else { ln_gamma(alpha) };
    let log_pdf = (alpha - 1.0) * crt_log(x) - x / beta - log_normalizer - alpha * crt_log(beta);
    let pdf = crt_exp(log_pdf);
    if pdf.is_finite() {
        Ok(pdf)
    } else {
        Err(WorksheetErrorCode::Num)
    }
}

pub fn gamma_inv_kernel(
    probability: f64,
    alpha: f64,
    beta: f64,
) -> Result<f64, WorksheetErrorCode> {
    validate_gamma_shape(alpha, beta)?;
    if !probability.is_finite() {
        return Err(WorksheetErrorCode::Value);
    }
    // Excel: p==0 returns the support lower bound (0); p==1 -> #NUM! (+inf);
    // p<0 or p>1 -> #NUM!. Verified vs live Excel 16.0 build 20026:
    // GAMMA.INV(0,3,2)=0, GAMMA.INV(1,3,2)=#NUM! (BUG-FUNC-039 / oxf-99zz).
    if probability < 0.0 || probability >= 1.0 {
        return Err(WorksheetErrorCode::Num);
    }
    if probability == 0.0 {
        return Ok(0.0);
    }
    // design/fable52-last-bit.md section 5 (Claude Fable 5.2), measured 300/300 on Excel:
    // GAMMA.INV(p,a,b) = st(b * GAMMA.INV(p,a,1)), CHISQ.INV(p,2a) = GAMMA.INV(p,a,2): a
    // unit-scale search followed by one stored multiply.
    let mut hi = (alpha + 10.0 * alpha.sqrt() + 10.0).max(1.0);
    // bisect_inverse requires f(hi) >= target on entry.
    for _ in 0..200 {
        if regularized_gamma_p(alpha, hi) >= probability {
            break;
        }
        hi *= 2.0;
    }
    let unit = bisect_inverse(probability, 0.0, hi, |v| regularized_gamma_p(alpha, v));
    let x = crate::ported::excel_numeric::excel_x87_mul(beta, unit);
    Ok(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    // BUG-FUNC-039 item 3: GAMMA.INV p=0 and p=1 boundary cases.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn gamma_inv_boundary_cases() {
        // p=0 -> 0 (Excel: GAMMA.INV(0,..) returns the support lower bound; live
        // Excel 16.0 build 20026 GAMMA.INV(0,3,2)=0).
        assert_eq!(gamma_inv_kernel(0.0, 1.0, 1.0), Ok(0.0));
        // p<0 -> #NUM!.
        assert_eq!(
            gamma_inv_kernel(-0.1, 1.0, 1.0),
            Err(WorksheetErrorCode::Num)
        );
        // p=1 -> #NUM! (Excel: p must be < 1).
        assert_eq!(
            gamma_inv_kernel(1.0, 1.0, 1.0),
            Err(WorksheetErrorCode::Num)
        );
        // Normal interior case still works.
        let v = gamma_inv_kernel(0.5, 3.0, 2.0).unwrap();
        assert!(
            (v - 5.348_).abs() < 0.01,
            "GAMMA.INV(0.5,3,2) ~ 5.348, got {v}"
        );
    }

    /// BETA.DIST(x, a, b, TRUE) answers read from Excel 16.0 build 20527 (the question service,
    /// 2026-11-07): the b = 1 arm exp(a * log x) up to x = 0.9 and the a = 1 arm
    /// 0.5 + (0.5 - exp(b * log(1 - x))) from x = 0.1, at tiny x and at both edges.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn beta_dist_unit_shape_arms_match_recorded_answers() {
        let rows: [(u64, u64, u64, u64); 10] = [
            (
                0x3f20000000000000,
                0x4000000000000000,
                0x3ff0000000000000,
                0x3e50000000000006,
            ),
            (
                0x3d00000000000000,
                0x4000000000000000,
                0x3ff0000000000000,
                0x3a0fffffffffffd6,
            ),
            (
                0x3c40000000000000,
                0x4008000000000000,
                0x3ff0000000000000,
                0x34dfffffffffff64,
            ),
            (
                0x3e10000000000000,
                0x3ff0000000000000,
                0x3ff0000000000000,
                0x3e10000000000004,
            ),
            (
                0x3feccccccccccccd,
                0x401c000000000000,
                0x3ff0000000000000,
                0x3fde9c6a99d1cdbf,
            ),
            (
                0x3feccccccccccccd,
                0x4047800000000000,
                0x3ff0000000000000,
                0x3f7cf510da63bbeb,
            ),
            (
                0x3fb999999999999a,
                0x3ff0000000000000,
                0x401c000000000000,
                0x3fe0b1cab3171920,
            ),
            (
                0x3fb999999999999a,
                0x3ff0000000000000,
                0x3fb999999999999a,
                0x3f8576ed59efe180,
            ),
            (
                0x3fe1667d2c686bfa,
                0x3ff0000000000000,
                0x401c000000000000,
                0x3fefde4aa862bc76,
            ),
            (
                0x3fdecc4833c7c22e,
                0x3ff0000000000000,
                0x4044000000000000,
                0x3fefffffffff7420,
            ),
        ];
        for (x, a, b, expected) in rows {
            let (x, a, b) = (f64::from_bits(x), f64::from_bits(a), f64::from_bits(b));
            let got = beta_dist_kernel(x, a, b, true, 0.0, 1.0).unwrap();
            assert_eq!(got.to_bits(), expected, "BETA.DIST({x:e},{a},{b},TRUE)");
        }
    }

    /// BETA.DIST(x, 1, b, TRUE) below x = 0.1 is -expm1(b * log1p(-x)) on the x87 chain
    /// (R43). Excel 16.0 build 20527, the question service, 2026-11-09.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn beta_dist_unit_first_shape_below_a_tenth_matches_excel() {
        let rows: [(f64, f64, u64); 6] = [
            (0.05, 3.0, 0x3fc24189374bc6a8),
            (0.001, 2.5, 0x3f6476f2d098e78f),
            (0.01, 10.0, 0x3fb87a6a94bfa203),
            (0.07, 0.5, 0x3fa23ebda17f7999),
            (0.003, 7.0, 0x3f954fb9cbf27f4d),
            (0.02, 1.5, 0x3f9e90ddc821a601),
        ];
        for (x, b, expected) in rows {
            let got = beta_dist_kernel(x, 1.0, b, true, 0.0, 1.0).unwrap();
            assert_eq!(got.to_bits(), expected, "BETA.DIST({x},1,{b},TRUE)");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn gamma_dist_half_shape_scale_two_matches_erf_sqrt_half_x() {
        use crate::ported::special_dist_family::erf_precise_kernel;
        for (x, beta) in [
            (0.0, 2.0),
            (0.5, 1.0),
            (1.0, 1.0),
            (1.0, 2.0),
            (2.0, 4.0),
            (6.6348966010212145, 2.0),
        ] {
            let got = gamma_dist_kernel(x, 0.5, beta, true).unwrap();
            let expect = erf_precise_kernel((x / beta).sqrt()).unwrap();
            assert_eq!(got.to_bits(), expect.to_bits(), "x={x} beta={beta}");
        }
        for (x, beta) in [(1e-12, 3.0), (3.0, 5.0), (1.0, 3.0), (8.0, 5.0)] {
            let got = gamma_dist_kernel(x, 1.0, beta, true).unwrap();
            let expect =
                crate::ported::discrete_dist::expon_dist_kernel(x / beta, 1.0, true).unwrap();
            assert_eq!(got.to_bits(), expect.to_bits(), "shape1 x={x} beta={beta}");
        }
    }
}
