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
    let log_pdf = (alpha - 1.0) * z.ln() + (beta - 1.0) * (1.0 - z).ln()
        - beta_log_norm(alpha, beta)
        - (upper - lower).ln();
    let pdf = log_pdf.exp();
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
            return crate::ported::discrete_dist_family::expon_dist_kernel(x / beta, 1.0, true);
        }
        // W109 lane-3 (2026-07-18): the pre-W109 integer-shape fast path
        // (1 − e^{-x}·Σ x^k/k!) is an OxFunc-side shortcut Excel does NOT
        // take — on the b26 integer-a corpora it scored 8.1% with ±4,400-ULP
        // catastrophics vs 39.4% (worst −10) through the identified GRATIO
        // path. All other shapes route through regularized_gamma_p.
        return Ok(regularized_gamma_p(alpha, x / beta));
    }
    let log_pdf = (alpha - 1.0) * x.ln() - x / beta - ln_gamma(alpha) - alpha * beta.ln();
    let pdf = log_pdf.exp();
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
    let mut hi = beta * (alpha + 10.0 * alpha.sqrt() + 10.0).max(1.0);
    // bisect_inverse requires f(hi) >= target on entry.
    for _ in 0..200 {
        if regularized_gamma_p(alpha, hi / beta) >= probability {
            break;
        }
        hi *= 2.0;
    }
    let x = bisect_inverse(probability, 0.0, hi, |v| {
        regularized_gamma_p(alpha, v / beta)
    });
    Ok(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    // BUG-FUNC-039 item 3: GAMMA.INV p=0 and p=1 boundary cases.
    #[test]
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

    #[test]
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
                crate::ported::discrete_dist_family::expon_dist_kernel(x / beta, 1.0, true)
                    .unwrap();
            assert_eq!(got.to_bits(), expect.to_bits(), "shape1 x={x} beta={beta}");
        }
    }
}
