use crate::ported::excel_numeric::crt_exp;
use crate::ported::excel_numeric::crt_log;
use crate::ported::special_math_common::{
    bisect_inverse_closest, gratio, ln_gamma, regularized_beta, regularized_gamma_p,
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

/// Excel's cumulative continued fraction fails in this absorbed-shape region.
pub(crate) fn beta_absorbed_shape_error(alpha: f64, beta: f64, z: f64) -> bool {
    let large = alpha.max(beta);
    let small = alpha.min(beta);
    let coordinate = if alpha > beta { z } else { 1.0 - z };
    if !large.is_finite() || large + small != large || !(z > 0.0 && z < 1.0) || !(coordinate <= 0.99) {
        return false;
    }
    if small >= 40.0 {
        return true;
    }
    if small < 2.0 || small.fract() != 0.0 {
        return false;
    }
    // For an integer shape below 40 the beta recurrence reduces the
    // smaller shape to two. Excel reports NUM when this stored product
    // vanishes; the stable reduction alone would hide that failure.
    let mut shape = small;
    let mut product = 1.0;
    for _ in 0..(small as u32 - 2) {
        shape -= 1.0;
        product *= shape / (shape + large);
        if product == 0.0 {
            return true;
        }
    }
    false
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
    use crate::ported::excel_numeric::{excel_x87_div, excel_x87_sub};
    let width = excel_x87_sub(upper, lower);
    let z = excel_x87_div(excel_x87_sub(x, lower), width);
    if cumulative {
        // Excel's continued-fraction arm returns NUM when the larger shape
        // absorbs a smaller shape of at least 40, within this coordinate
        // region. The adjacent shape and coordinate boundaries are measured.
        if beta_absorbed_shape_error(alpha, beta, z) {
            return Err(WorksheetErrorCode::Num);
        }
        // W109 lane-3 (2026-07-18, b30): Excel has NO integer-shape beta
        // fast path — bratio scores integer shapes at the same wall rate as
        // fractional (344/768 vs the binomial-sum shortcut's 254/768, and
        // the shortcut loses the disagreement rows 177:87). Route all
        // shapes through the identified BRATIO.
        let cdf = regularized_beta(z, alpha, beta);
        return if cdf.is_finite() {
            Ok(cdf)
        } else {
            Err(WorksheetErrorCode::Num)
        };
    }
    if z > 0.0 && z < 1.0 {
        if let Some(pdf) = crate::ported::discrete_dist::beta_density(z, alpha, beta) {
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

/// Lower-tail starting approximations from Abramowitz and Stegun 26.4.17 and
/// 26.5.20. Keep the small-shape power proposal separate from a rounded upper
/// endpoint: Excel's iteration treats them differently.
#[derive(Clone, Copy)]
enum BetaStart {
    Interior(f64),
    LowerPower(f64),
    UpperEndpoint,
    Failed,
    LowerEndpoint,
}

fn beta_inverse_start(p: f64, a: f64, b: f64) -> Option<BetaStart> {
    use crate::ported::excel_numeric::{
        excel_exp, excel_log, excel_log1p_times, excel_sqrt_staged, excel_x87_add, excel_x87_div,
        excel_x87_mul, excel_x87_sub,
    };
    let z = -crate::ported::normal_log_family::norm_s_inv_kernel(p).ok()?;
    if a > 1.0 && b > 1.0 {
        // A&S 26.5.20, with both reciprocals and the final denominator stored.
        let al = (z * z - 3.0) / 6.0;
        let ra = 1.0 / (2.0 * a - 1.0);
        let rb = 1.0 / (2.0 * b - 1.0);
        let h = 2.0 / (ra + rb);
        let w = z * excel_sqrt_staged(al + h) / h - (rb - ra) * (al + 5.0 / 6.0 - 2.0 / (3.0 * h));
        let x = excel_x87_div(a, excel_x87_add(a, excel_x87_mul(b, excel_exp(2.0 * w))));
        return if x == 1.0 && a + b == a {
            Some(BetaStart::Failed)
        } else if x <= 0.0 {
            Some(BetaStart::LowerEndpoint)
        } else if x >= 1.0 {
            Some(BetaStart::UpperEndpoint)
        } else {
            (x > 0.0 && x < 1.0).then_some(BetaStart::Interior(x))
        };
    }
    let correction = excel_x87_div(1.0, excel_x87_mul(9.0, b));
    let base = excel_x87_add(
        excel_x87_sub(1.0, correction),
        excel_x87_mul(z, excel_sqrt_staged(correction)),
    );
    let log_beta = || crate::ported::special_math_common::excel_ln_beta(a, b);
    if base <= 0.0 {
        if !(base * base * base).is_finite() {
            return Some(BetaStart::Interior(0.5));
        }
        let t = excel_x87_div(
            excel_x87_add(
                excel_log1p_times(1.0, -p),
                excel_x87_add(excel_log(b), log_beta()),
            ),
            b,
        );
        let x = excel_x87_sub(1.0, excel_exp(t));
        return if !x.is_finite() {
            Some(BetaStart::Interior(0.5))
        } else if x == 1.0 {
            Some(BetaStart::UpperEndpoint)
        } else if x <= 0.0 {
            Some(BetaStart::LowerEndpoint)
        } else {
            (x > 0.0 && x < 1.0).then_some(BetaStart::Interior(x))
        };
    }
    // Excel stores EXP(3 * LN(base)) here. Multiplying the three factors
    // gives a different seed even when the mathematical cube is finite.
    let chi = excel_x87_mul(2.0 * b, excel_exp(excel_x87_mul(3.0, excel_log(base))));
    let numerator = excel_x87_sub(excel_x87_add(4.0 * a, 2.0 * b), 2.0);
    let ratio = excel_x87_div(numerator, chi);
    if ratio <= 1.0 {
        let x = if b == 1.0 {
            excel_exp(excel_x87_div(excel_log(p), a))
        } else {
            excel_exp(excel_x87_div(
                excel_x87_add(excel_log(p), excel_x87_add(excel_log(a), log_beta())),
                a,
            ))
        };
        return Some(if x <= 0.0 {
            BetaStart::LowerEndpoint
        } else if x >= 1.0 {
            BetaStart::UpperEndpoint
        } else {
            BetaStart::LowerPower(x)
        });
    }
    if !ratio.is_finite() {
        return Some(BetaStart::Failed);
    }
    let x = excel_x87_sub(1.0, excel_x87_div(2.0, excel_x87_add(ratio, 1.0)));
    if x == 1.0 {
        Some(BetaStart::UpperEndpoint)
    } else if x <= 0.0 {
        Some(BetaStart::LowerEndpoint)
    } else {
        (x > 0.0 && x < 1.0).then_some(BetaStart::Interior(x))
    }
}

/// AS 109 damped Newton corrections on the lower beta coordinate. On sign
/// changes `step_limit` clamps subsequent steps; inadmissible corrections are
/// divided by three, counting only step-limit-admitted trials against Excel
/// 20527's 700-trial cap, and returning the iterate with strictly smallest
/// `|I_x(a,b) - p|`.
fn beta_newton_lower(p: f64, a: f64, b: f64) -> Result<Option<f64>, WorksheetErrorCode> {
    use crate::ported::excel_numeric::{excel_x87_div, excel_x87_mul, excel_x87_sub};
    const MAX_TRIALS: usize = 700;
    let Some(start) = beta_inverse_start(p, a, b) else {
        return Ok(None);
    };
    let mut x = match start {
        BetaStart::Failed => return Err(WorksheetErrorCode::Num),
        BetaStart::Interior(x) => x,
        BetaStart::LowerEndpoint => 1e-6,
        BetaStart::LowerPower(x) if x > 0.0 && x < 1.0 => x,
        BetaStart::LowerPower(_) => return Ok(None),
        BetaStart::UpperEndpoint => 1.0 - 1e-6,
    };
    // The 1,040 frozen boundary controls separate the unrounded exponent
    // from its floor and truncation. Both tails use an absolute correction.
    let exponent = (-13.0 - 5.0 / (a * a) - libm::pow(p, -0.2)).max(-150.0);
    let tolerance = libm::pow(10.0, exponent).min(f64::EPSILON);
    let mut last_correction = 0.0;
    let mut last_adjustment: f64 = 1.0;
    let mut step_limit = f64::INFINITY;
    let mut best_x = x;
    let mut best_err = f64::INFINITY;
    let mut trials = 0;
    loop {
        let (Ok(cdf), Ok(density)) = (
            beta_dist_kernel(x, a, b, true, 0.0, 1.0),
            beta_dist_kernel(x, a, b, false, 0.0, 1.0),
        ) else {
            return Err(WorksheetErrorCode::Num);
        };
        let df = excel_x87_sub(cdf, p);
        let err_df = df.abs();
        if err_df < best_err {
            best_err = err_df;
            best_x = x;
        }
        let dens_zero = density == 0.0
            || ((a < 1.0 || b < 1.0)
                && flush_subnormal(excel_x87_mul(
                    excel_x87_mul(x, excel_x87_sub(1.0, x)),
                    density,
                )) == 0.0);
        if dens_zero || err_df == 0.0 {
            return Ok(Some(best_x));
        }
        let correction = excel_x87_div(df, density);
        if !correction.is_finite() || correction.abs() <= tolerance {
            return Ok(Some(best_x));
        }
        if last_correction != 0.0 && correction * last_correction <= 0.0 {
            step_limit = last_adjustment.abs().max(1e-300);
        }
        last_correction = correction;
        let mut factor = 1.0;
        loop {
            last_adjustment = excel_x87_div(excel_x87_mul(factor, df), density);
            if last_adjustment.abs() >= step_limit {
                factor = excel_x87_div(factor, 3.0);
                if factor == 0.0 {
                    return Ok(Some(best_x));
                }
                continue;
            }
            trials += 1;
            if trials >= MAX_TRIALS {
                return if best_err <= 1e-12 * p {
                    Ok(Some(best_x))
                } else {
                    Err(WorksheetErrorCode::Num)
                };
            }
            let next = excel_x87_sub(x, last_adjustment);
            if next > 0.0 && next < 1.0 {
                if next == x {
                    return Ok(Some(best_x));
                }
                x = next;
                break;
            }
            factor = excel_x87_div(factor, 3.0);
        }
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
    // Both beta inverse names reject the support endpoints (Excel 20527).
    if probability <= 0.0 || probability >= 1.0 {
        return Err(WorksheetErrorCode::Num);
    }
    // Upper probabilities search the reflected beta coordinate. Fresh paired
    // controls identify this staging, including the tiny-shape root floor.
    let (probability, alpha, beta, reflected) = if probability > 0.5 {
        (1.0 - probability, beta, alpha, true)
    } else {
        (probability, alpha, beta, false)
    };
    use crate::ported::excel_numeric::{excel_x87_add, excel_x87_mul, excel_x87_sub};
    let z = flush_subnormal(beta_newton_lower(probability, alpha, beta)?.unwrap_or_else(|| {
        bisect_inverse_closest(probability, 0.0, 1.0, |v| regularized_beta(v, alpha, beta))
    }));
    let z = if reflected {
        excel_x87_add(0.5, excel_x87_sub(0.5, z))
    } else {
        z
    };
    Ok(flush_subnormal(excel_x87_add(
        lower,
        excel_x87_mul(z, excel_x87_sub(upper, lower)),
    )))
}

pub(crate) fn flush_subnormal(value: f64) -> f64 {
    if value.is_subnormal() { 0.0 } else { value }
}

/// Full 7-term Goldstein (1973, ACM Algorithm 451) unit-scale starting approximation
/// `a * ((c * c) * c)` for `a > 1.0`, with `f = 1 / (2*a)`, `u = z * sqrt(f)`, and
/// stored Horner `c = sum_{k=0..6} (A[k] + f * (B[k] + f * C[k])) * u^k`.
pub(super) fn goldstein_initial_unit(z: f64, a: f64) -> Option<f64> {
    use crate::ported::excel_numeric::{
        excel_sqrt_staged, excel_x87_add, excel_x87_div, excel_x87_mul,
    };
    const A: [f64; 7] = [
        1.0000886,
        0.4713941,
        0.0001348028,
        -0.008553069,
        0.003125580,
        -0.0008426812,
        0.00009780499,
    ];
    const B: [f64; 7] = [
        -0.2237368,
        0.02607083,
        0.01128186,
        -0.01153761,
        0.005169654,
        0.002530010,
        -0.001450117,
    ];
    const C: [f64; 7] = [
        -0.01513904,
        -0.008986007,
        0.02277679,
        -0.01323293,
        -0.006950356,
        0.001060438,
        0.001565326,
    ];
    let n = excel_x87_mul(2.0, a);
    let f = excel_x87_div(1.0, n);
    let f1 = excel_sqrt_staged(f);
    let u = excel_x87_mul(z, f1);
    let mut c = excel_x87_add(A[6], excel_x87_mul(f, excel_x87_add(B[6], excel_x87_mul(C[6], f))));
    for k in (0..6).rev() {
        let ck = excel_x87_add(A[k], excel_x87_mul(f, excel_x87_add(B[k], excel_x87_mul(C[k], f))));
        c = excel_x87_add(excel_x87_mul(c, u), ck);
    }
    if !(c > 0.0) {
        return None;
    }
    Some(excel_x87_mul(a, excel_x87_mul(excel_x87_mul(c, c), c)))
}

/// Excel 20527's stored unit result in the measured very-large-shape region.
pub(crate) fn large_shape_inverse_unit(shape: f64) -> f64 {
    crate::ported::excel_numeric::excel_x87_mul(shape, f64::from_bits(0x3ff0_0116_bc77_4cb6))
}

/// Excel's measured large-shape inverse steps around the saturation factor.
/// The step count follows the normal quantile divided by the square root of
/// twice the shape; neighboring factors are three binary64 words apart. The
/// measured scale is kept away from step transitions, whose rounding is open.
pub(crate) fn large_shape_inverse_factor(probability: f64, shape: f64) -> Option<f64> {
    if !(0.05..=0.95).contains(&probability)
        || !(2.0_f64.powi(95)..2.0_f64.powi(109)).contains(&shape)
    {
        return None;
    }
    let z = crate::ported::normal_log_family::norm_s_inv_kernel(probability).ok()?;
    let coordinate = (2_124_007_406_095_908.0 * z) / (2.0 * shape).sqrt();
    let fraction = coordinate.abs().fract();
    if (fraction - 0.5).abs() < 0.01 {
        return None;
    }
    let step = coordinate.round();
    if step.abs() > 8.0 {
        return None;
    }
    let bits = 0x3ff0_0116_bc77_4cb6_i64 + 3 * step as i64;
    Some(f64::from_bits(bits as u64))
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
        // Excel 16.0.20527.20196 (dc-ymjf.2.141, frozen Touchstone controls SHA-256 50fe12bf):
        // subnormal `y = x / beta` flushes to `0.0` across all shapes.
        let y = flush_subnormal(x / beta);
        // Inverse-problem identity, live Excel 16.0 b20228:
        // GAMMA.DIST(x,0.5,beta,TRUE) == ERF.PRECISE(SQRT(x/beta))
        // for beta in {1,2,4} on 68/68 (and beta=2 on the earlier 154/154
        // bank). Staging is divide-then-sqrt, matching the df=1 chi route
        // when beta=2. Other shapes are not claimed.
        if alpha == 0.5 && beta > 0.0 && beta.is_finite() {
            return crate::ported::special_dist_family::erf_precise_kernel(y.sqrt())
                .map(flush_subnormal);
        }
        // Inverse-problem identity, live Excel 16.0 b20228:
        // GAMMA.DIST(x,1,beta,TRUE) == EXPON.DIST(x/beta,1,TRUE)
        // (85/85 on dyadic scales; 11/11 on beta=3 and beta=5).
        // EXPON.DIST(x,1/beta,TRUE) misses by 1 ULP when 1/beta is inexact.
        // PDF identity is false (33/85).
        if alpha == 1.0 {
            return crate::ported::discrete_dist::expon_dist_kernel(y, 1.0, true);
        }
        // W109 lane-3 (2026-07-18): the pre-W109 integer-shape fast path
        // (1 − e^{-x}·Σ x^k/k!) is an OxFunc-side shortcut Excel does NOT
        // take — on the b26 integer-a corpora it scored 8.1% with ±4,400-ULP
        // catastrophics vs 39.4% (worst −10) through the identified GRATIO
        // path. All other shapes route through regularized_gamma_p.
        let cdf = regularized_gamma_p(alpha, y);
        if !cdf.is_finite() {
            return Err(WorksheetErrorCode::Num);
        }
        // Excel 16.0.20527.20196 (dc-ymjf.2.141): wide-shape cumulative controls
        // (0.25..15000, including Temme) flush subnormal CDFs to 0.0 across all shapes.
        return Ok(flush_subnormal(cdf));
    }
    // Gamma(1) = 1 exactly; Lanczos leaves a small nonzero log at this point.
    if x > 0.0 {
        // The density is the Poisson mass at `alpha` of mean `x/beta`, stored, times the stored
        // `alpha/x`: R30's scaled public calls read the ratio, and with this mass every one of
        // the 750 unit-shape rows of the bank agrees, where `exp(-x/beta)/beta` does not.
        // Excel 16.0.20527.20196 (dc-ymjf.2.141): subnormal `y = x/beta` flushes to 0.0 (causing
        // `poisson_mass(alpha, 0.0)` -> #NUM!), subnormal `poisson_mass` flushes to 0.0 before
        // scaling, and subnormal final `pdf` flushes to 0.0.
        use crate::ported::excel_numeric::{excel_x87_div, excel_x87_mul};
        let y = flush_subnormal(excel_x87_div(x, beta));
        // Excel 16.0.20527.20196 (dc-ymjf.2.154, 1-ULP bisected at 0x7fc45f306dc9c883):
        // Loader-branch GAMMA.DIST/GAMMADIST PDF returns `#DIV/0!` whenever `2 * pi * alpha`
        // overflows binary64 (preceding deviance overflow).
        if alpha > 1.1 && y > 1.1 && !excel_x87_mul(2.0 * std::f64::consts::PI, alpha).is_finite() {
            return Err(WorksheetErrorCode::Div0);
        }
        let r = crate::ported::discrete_dist::poisson_mass(alpha, y);
        let pdf = excel_x87_mul(r, excel_x87_div(alpha, x));
        return if pdf.is_finite() {
            Ok(flush_subnormal(pdf))
        } else {
            Err(WorksheetErrorCode::Num)
        };
    }
    let log_normalizer = if alpha == 1.0 { 0.0 } else { ln_gamma(alpha) };
    let log_pdf = (alpha - 1.0) * crt_log(x) - x / beta - log_normalizer - alpha * crt_log(beta);
    let pdf = crt_exp(log_pdf);
    if pdf.is_finite() {
        Ok(flush_subnormal(pdf))
    } else {
        Err(WorksheetErrorCode::Num)
    }
}

/// Excel's iterative unit-scale gamma inverse solver (cb6, dc-j9k.60; Goldstein 451 start dc-j9k.63):
/// - AS 91 `-1.24 * ln(p)` power seed `exp(((ln(p) + gammaln(a)) + ln(a)) / a)` (dc-iuwct.58),
///   Goldstein (1973) Algorithm 451 full 7-term start for `a > 1.0`, and Wilson-Hilferty start
///   for `a < 1.0` (with `z = -NORM.S.INV(q)` for right-tail chi-square callers so tiny `q`
///   survives without `1 - q` cancellation);
/// - complement residual switch strictly above `0.9999` (`0x3fefff2e48e8a71e`);
/// - `a < 1.0` Wilson-Hilferty cube `a * exp(3 * ln(c))` (dc-j9k.64, 57/60 vs 1/60 on frozen
///   live Excel 16.0.20527 holdouts);
/// - unfactored Halley step `2*df / (2*pdf - df*((a-1)/x - 1))` with AS 109 sign-change
///   thirding damping (stopping at `best_x` when damping triggers without residual improvement),
///   `105`-step cap returning `best_x` iff `best_err <= 1e-12 * target` else `#NUM!` (dc-iuwct.58),
///   `|next - x| <= f64::EPSILON * x` relative-step stop returning `best_x` without evaluating
///   `next`, and strict-minimum residual `best_x` publication.
pub(super) fn gamma_halley_unit(
    prob: f64,
    a: f64,
    is_upper_q: bool,
) -> Result<f64, WorksheetErrorCode> {
    use crate::ported::excel_numeric::{
        excel_exp, excel_log, excel_sqrt_staged, excel_x87_add, excel_x87_div, excel_x87_mul,
        excel_x87_sub, gammaln_excel,
    };
    const MAX_TRIALS: usize = 105;

    let mut x = if !is_upper_q {
        // The lower-tail power estimate is exact at the underflow scale.
        // Excel 20527: 56 controls separate an estimate rounded to zero
        // from a nonzero subnormal, which fails with #DIV/0! before scale is applied.
        // AS 91 `exp(((ln(p) + gammaln(a)) + ln(a)) / a)` avoids `1 + a` absorption at
        // small `a` and matches live Excel's addition order on `a ~ exp(-8)` probes (dc-iuwct.58).
        let power_seed = || {
            let log_seed = excel_x87_div(
                excel_x87_add(
                    excel_x87_add(excel_log(prob), gammaln_excel(a)),
                    excel_log(a),
                ),
                a,
            );
            excel_exp(log_seed)
        };
        let small_shape_seed = if a < 1.0 {
            let seed = power_seed();
            if seed == 0.0 {
                return Ok(0.0);
            }
            if seed.is_subnormal() {
                return Err(WorksheetErrorCode::Div0);
            }
            Some(seed)
        } else {
            None
        };
        if 2.0 * a < excel_x87_mul(-1.24, excel_log(prob)) {
            let seed = small_shape_seed.unwrap_or_else(power_seed);
            if seed == 0.0 {
                return Ok(0.0);
            }
            if seed.is_subnormal() {
                return Err(WorksheetErrorCode::Div0);
            }
            seed
        } else if a > 1.0 {
            let z = crate::ported::normal_log_family::norm_s_inv_kernel(prob)?;
            goldstein_initial_unit(z, a).ok_or(WorksheetErrorCode::Num)?
        } else {
            let v = excel_x87_div(1.0, excel_x87_mul(9.0, a));
            let z = crate::ported::normal_log_family::norm_s_inv_kernel(prob)?;
            let c = excel_x87_add(
                excel_x87_sub(1.0, v),
                excel_x87_mul(z, excel_sqrt_staged(v)),
            );
            if !(c > 0.0) {
                return Err(WorksheetErrorCode::Num);
            }
            excel_x87_mul(a, excel_exp(excel_x87_mul(3.0, excel_log(c))))
        }
    } else {
        let p_comp = excel_x87_add(0.5, excel_x87_sub(0.5, prob));
        if p_comp < 1.0 && 2.0 * a < excel_x87_mul(-1.24, excel_log(p_comp)) {
            let log_seed = excel_x87_div(
                excel_x87_add(
                    excel_x87_add(excel_log(p_comp), gammaln_excel(a)),
                    excel_log(a),
                ),
                a,
            );
            let seed = excel_exp(log_seed);
            if seed == 0.0 || seed.is_subnormal() {
                return Ok(0.0);
            }
            seed
        } else {
            let z = -crate::ported::normal_log_family::norm_s_inv_kernel(prob)?;
            goldstein_initial_unit(z, a).ok_or(WorksheetErrorCode::Num)?
        }
    };

    if x == 0.0 {
        return Ok(0.0);
    }
    if !(x > 0.0 && x.is_finite()) {
        return Err(WorksheetErrorCode::Num);
    }

    // Complement residual switch strictly above 0.9999 (0x3fefff2e48e8a71e).
    let comp_prob = excel_x87_add(0.5, excel_x87_sub(0.5, prob));
    let switch_comp = prob > 0.9999;
    let target = if switch_comp { comp_prob } else { prob };
    let eval_df = |x_val: f64| -> Result<f64, WorksheetErrorCode> {
        if !is_upper_q && !switch_comp {
            let cdf = gamma_dist_kernel(x_val, a, 1.0, true)?;
            Ok(excel_x87_sub(cdf, prob))
        } else {
            let (p_val, q_val) = gratio(a, x_val);
            if !p_val.is_finite() || p_val > 1.5 {
                return Err(WorksheetErrorCode::Num);
            }
            if !is_upper_q {
                Ok(excel_x87_sub(comp_prob, flush_subnormal(q_val)))
            } else if !switch_comp {
                Ok(excel_x87_sub(prob, flush_subnormal(q_val)))
            } else {
                Ok(excel_x87_sub(flush_subnormal(p_val), comp_prob))
            }
        }
    };

    let mut best_x = x;
    let mut best_err = f64::INFINITY;
    let mut last_corr = 0.0_f64;
    let mut last_adj = 1.0_f64;
    let mut step_limit = f64::INFINITY;
    let mut trials = 0usize;

    loop {
        let df = eval_df(x)?;
        let err_df = df.abs();
        let improved = err_df < best_err;
        if improved {
            best_err = err_df;
            best_x = x;
        }
        if err_df == 0.0 {
            return Ok(best_x);
        }
        if trials >= MAX_TRIALS {
            if best_err <= excel_x87_mul(1e-12, target) {
                return Ok(best_x);
            }
            return Err(WorksheetErrorCode::Num);
        }

        let pdf = gamma_dist_kernel(x, a, 1.0, false)?;
        if pdf == 0.0 {
            return Ok(best_x);
        }

        let log_slope = excel_x87_sub(excel_x87_div(a - 1.0, x), 1.0);
        let denom = excel_x87_sub(2.0 * pdf, excel_x87_mul(df, log_slope));
        let corr = if denom == 0.0 || !denom.is_finite() {
            excel_x87_div(df, pdf)
        } else {
            excel_x87_div(2.0 * df, denom)
        };
        if !corr.is_finite() {
            return Ok(best_x);
        }

        if last_corr != 0.0 && corr * last_corr <= 0.0 {
            step_limit = last_adj.abs();
        }
        last_corr = corr;

        let mut factor = 1.0_f64;
        loop {
            last_adj = excel_x87_mul(factor, corr);
            if last_adj.abs() >= step_limit {
                if !improved {
                    return Ok(best_x);
                }
                factor = excel_x87_div(factor, 3.0);
                if factor == 0.0 || excel_x87_sub(x, last_adj) == x {
                    return Ok(best_x);
                }
                continue;
            }
            let next = excel_x87_sub(x, last_adj);
            if !(next > 0.0 && next.is_finite()) {
                return Err(WorksheetErrorCode::Num);
            }
            if excel_x87_sub(next, x).abs() <= f64::EPSILON * x {
                return Ok(best_x);
            }
            trials += 1;
            if next == x {
                return Ok(best_x);
            }
            x = next;
            break;
        }
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
    use crate::ported::excel_numeric::excel_x87_mul;
    // Unified 7-term Goldstein (Algorithm 451) across all large shapes 2*alpha in [5e10, 2^1000)
    // (Carbon cb20 / dc-j9k.71).
    if (5e10..f64::from_bits(0x7e70_0000_0000_0000)).contains(&(2.0 * alpha))
        && let Ok(z) = crate::ported::normal_log_family::norm_s_inv_kernel(probability)
        && let Some(unit) = goldstein_initial_unit(z, alpha)
    {
        let result = excel_x87_mul(beta, unit);
        return if result.is_finite() {
            Ok(flush_subnormal(result))
        } else {
            Err(WorksheetErrorCode::Num)
        };
    }
    // Scale only after the independently measured unit result has been stored.
    if (2.0_f64.powi(-40)..2.0_f64.powi(40)).contains(&beta)
        && let Some(factor) = large_shape_inverse_factor(probability, alpha)
    {
        let unit = excel_x87_mul(alpha, factor);
        let result = excel_x87_mul(beta, unit);
        return if result.is_finite() {
            Ok(flush_subnormal(result))
        } else {
            Err(WorksheetErrorCode::Num)
        };
    }
    // At these large shapes Excel's unit search saturates to the same stored
    // x87 factor as the right-tail chi-square inverse. The scale is
    // applied in its own stored multiplication.
    if probability >= f64::MIN_POSITIVE
        && alpha >= f64::from_bits(0x4720_0000_0000_0000) // 2^115
        && alpha < f64::from_bits(0x6570_0000_0000_0000) // 2^600
        && beta >= f64::from_bits(0x26f0_0000_0000_0000) // 2^-400
        && beta < f64::from_bits(0x58f0_0000_0000_0000) // 2^400
    {
        let result = excel_x87_mul(beta, large_shape_inverse_unit(alpha));
        return if result.is_finite() {
            Ok(flush_subnormal(result))
        } else {
            Err(WorksheetErrorCode::Num)
        };
    }
    if alpha == 0.5 {
        let unit = crate::ported::normal_log_family::gamma_inv_half_unit(probability);
        let result = excel_x87_mul(beta, unit);
        return if result.is_finite() {
            Ok(flush_subnormal(result))
        } else {
            Err(WorksheetErrorCode::Num)
        };
    }
    if alpha == 1.0 {
        use crate::ported::excel_numeric::{excel_log, excel_log1p_times};
        // Exponential inverse: the distribution logarithm keeps the small
        // probability before the final stored scale. Fresh Excel20527: 698/698
        // after final subnormal publication is flushed, plus 48 scaling controls.
        let unit = if probability < 0.375 {
            excel_log1p_times(-1.0, -probability)
        } else {
            -excel_log(1.0 - probability)
        };
        let result = excel_x87_mul(beta, unit);
        return if result.is_finite() {
            Ok(flush_subnormal(result))
        } else {
            Err(WorksheetErrorCode::Num)
        };
    }
    let unit = flush_subnormal(gamma_halley_unit(probability, alpha, false)?);
    let result = excel_x87_mul(beta, unit);
    if result.is_finite() {
        Ok(flush_subnormal(result))
    } else {
        Err(WorksheetErrorCode::Num)
    }
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

    /// Native and browser: from a shape of 2.7e10 GAMMA.INV is the quantile times Excel's stored
    /// factor 1.000265823551. Excel 16.0.20527 answers through Touchstone (2026-11-20) and four
    /// full-bank rows (bank2 gamma.inv scalar, admit2 gammainv enrich, bank2 gammainv more);
    /// approximate to 1e-9 relative.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn gamma_inv_at_huge_shapes_carries_excel_s_factor() {
        for (p, a, b, excel) in [
            (0.5, 1e11, 1.0, 100026582354.72188),
            (0.5, 1e13, 1.0, 10002658235505.422),
            (0.1, 1e14, 1.0, 100026569537554.47),
            (0.9, 1e14, 1.0, 100026595172561.05),
            (0.5, 1e14, 3.0, 300079747065171.6),
            (0.01, 1e12, 1.0, 1000263496843.359),
            (0.999, 1e12, 1.0, 1000268914265.0203),
            (1e-10, 1e12, 1.0, 1000259461236.2035),
            (0.6378628363375689, 1628774109488000.0, 2.0, 3258414180486943.5),
            (0.3848162801459085, 1.2087509178025512e16, 0.5, 6045361145215179.0),
            (0.3980664124178507, 8.816833097759955e20, 1.0, 8.819176819562106e20),
            (0.7601870723963965, 118312400027.68289, 1.1755857128890748, 139123625450.94748),
        ] {
            let got = gamma_inv_kernel(p, a, b).expect("Excel answers");
            assert!(((got - excel) / excel).abs() < 1e-9, "GAMMA.INV({p},{a},{b}) = {got}, Excel {excel}");
        }
        // Below the factor's region the quantile stands: GAMMA.INV(0.5, 4e9, 1) is
        // 3999999999.6666665 in Excel.
        let got = gamma_inv_kernel(0.5, 4e9, 1.0).unwrap();
        assert!((got / 3999999999.6666665 - 1.0).abs() < 1e-12, "{got}");
    }

    /// Excel 20527. Fractional degrees separate gamma from the integer chi-square
    /// wrapper. The small-shape error occurs on the first proposal at unit scale.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn gamma_inverse_matches_large_fractional_shapes_and_failed_start() {
        for (p, shape, scale, expected) in [
            (0.7601870723963965, 118312400027.68289, 1.1755857128890748, 0x4240_3235_06f5_7947),
            (0.3848162801459085, 1.2087509178025512e16, 0.5, 0x4335_7a39_5399_78cb),
            (0.6378628363375689, 1628774109488000.0, 2.0, 0x4327_2705_5784_9e3f),
        ] {
            assert_eq!(gamma_inv_kernel(p, shape, scale).unwrap().to_bits(), expected);
        }
        let shape = 0.019414129356634446;
        for scale in [0.25, 0.5, 1.0, 11.0] {
            assert_eq!(gamma_inv_kernel(0.9956675931736523, shape, scale), Err(WorksheetErrorCode::Num));
            assert!(gamma_inv_kernel(0.9980675931736522, shape, scale).is_ok());
        }
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

    /// Excel 16.0.20527 (cb6, dc-j9k.60): alpha == 0.5 closed-form `erfinv(p)^2` across
    /// pieces A/B/C/AS 241 tail, piece boundaries (`sqrt(0.5)`, `15/16`), and unit-level
    /// subnormal flush before outer `beta = 2^100` scaling.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cb6_gamma_inv_half_shape_boundaries_and_subnormal_flush() {
        let rows: [(u64, u64, u64, u64); 14] = [
            // Piece A/B boundary triple around sqrt(0.5).
            (0x3fe6a09e667f3bcc, 0x3fe0000000000000, 0x3ff0000000000000, 0x3fe1b34cec5057e4),
            (0x3fe6a09e667f3bcd, 0x3fe0000000000000, 0x3ff0000000000000, 0x3fe1b34cec5057e1),
            (0x3fe6a09e667f3bce, 0x3fe0000000000000, 0x3ff0000000000000, 0x3fe1b34cec5057e1),
            // Piece C / AS 241 tail boundary triple around 15/16.
            (0x3fedffffffffffff, 0x3fe0000000000000, 0x3ff0000000000000, 0x3ffbc216c7699397),
            (0x3fee000000000000, 0x3fe0000000000000, 0x3ff0000000000000, 0x3ffbc216c76993a9),
            (0x3fee000000000001, 0x3fe0000000000000, 0x3ff0000000000000, 0x3ffbc216c76993b4),
            // Frozen holdouts across pieces A, B, C, and tail.
            (0x3fe57325d4e00884, 0x3fe0000000000000, 0x40a7f8bbd0d50b5e, 0x4096c6828a21e7fd),
            (0x3fead7729b622ade, 0x3fe0000000000000, 0x407d6aa848a0c7f1, 0x407cdf1445a28768),
            (0x3fedeeb5d436bb05, 0x3fe0000000000000, 0x40a39dcc01097bb8, 0x40b0bf39dc86442c),
            (0x3fefffe5c57c55a5, 0x3fe0000000000000, 0x4072fcf1519ab284, 0x40a6a5fd55aae6b5),
            // Unit subnormal flush before outer beta = 2^100 multiply.
            (0x202ad0cc33744e4b, 0x3fe0000000000000, 0x3ff0000000000000, 0x0061a6187ba5ce19),
            (0x202ad0cc33744e4b, 0x3fe0000000000000, 0x4630000000000000, 0x06a1a6187ba5ce19),
            (0x1ff573d68f903ea2, 0x3fe0000000000000, 0x3ff0000000000000, 0x0000000000000000),
            (0x1ff573d68f903ea2, 0x3fe0000000000000, 0x4630000000000000, 0x0000000000000000),
        ];
        for (p, a, b, expected) in rows {
            let got = gamma_inv_kernel(f64::from_bits(p), f64::from_bits(a), f64::from_bits(b))
                .unwrap();
            assert_eq!(got.to_bits(), expected, "GAMMA.INV({p:#x}, {a:#x}, {b:#x})");
        }
    }

    /// Excel 16.0.20527 (cb6, dc-j9k.60): damped Halley solver, strict `prob > 0.9999`
    /// complement switch (both below and inside Temme), and `[2.5e10, 2^95)` Goldstein
    /// when `|z| > z_bound`.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cb6_gamma_inv_halley_complement_switch_and_goldstein() {
        let rows: [(u64, u64, u64, u64); 7] = [
            // Strictly above 0.9999 (0x3fefff2e48e8a71e -> 0x3fefff2e48e8a71f):
            // complement residual switches to comp(p) - Q, both at a = 2.5 and inside Temme (a = 1000).
            (0x3fefff2e48e8a71f, 0x4004000000000000, 0x3ff0000000000000, 0x4029bead4ea92c1f),
            (0x3fefff2e48e8a71f, 0x408f400000000000, 0x3ff0000000000000, 0x4091879de0b4c4bd),
            // High-shape [2.5e10, 2^95) Goldstein at p = 0.999 (|z| > 2.75).
            (0x3feff7ced916872b, 0x42174876e8000000, 0x3ff0000000000000, 0x42174a2a59101a34),
            // Frozen holdouts across small, mid, large (cb6-holdout-071; cb6-holdout-076 now hits
            // exact 0-ULP forward GAMMA.DIST on br=10 at x-1u while Excel GAMMA.INV returns x),
            // and extreme-upper regimes.
            (0x3aa38dfb8f935b70, 0x3fe79ac9e4a2e18c, 0x40345ca531b3bf9f, 0x39043831c141a704),
            (0x3fc4db2992fab098, 0x402d6c641b472c29, 0x3fc49fb9f745f0b2, 0x3ffc4b612a9b924b),
            (0x3febc083e32ecd56, 0x4071050a6a4203c1, 0x40205d169e84b527, 0x40a295dee771b057),
            (0x3fefffffe1cb8b1e, 0x40587f06ef3445e4, 0x3fc502a9de9266f8, 0x403a3c50587838ba),
        ];
        for (p, a, b, expected) in rows {
            let got = gamma_inv_kernel(f64::from_bits(p), f64::from_bits(a), f64::from_bits(b))
                .unwrap();
            assert_eq!(got.to_bits(), expected, "GAMMA.INV({p:#x}, {a:#x}, {b:#x})");
        }
        // At exactly 0.9999 (0x3fefff2e48e8a71e), the solver stays on the P - p branch
        // and lands on the binary64 CDF plateau (GAMMA.DIST == p).
        let p_eq = f64::from_bits(0x3fefff2e48e8a71e);
        let x_eq = gamma_inv_kernel(p_eq, 2.5, 1.0).unwrap();
        assert_eq!(
            gamma_dist_kernel(x_eq, 2.5, 1.0, true).unwrap().to_bits(),
            p_eq.to_bits()
        );
    }

    /// Excel 16.0.20527 (dc-j9k.63): Goldstein (1973, Algorithm 451) 7-term starting proposal
    /// for `a > 1.0` combined with cb7 half-integer Horner across GAMMA.INV and right-tail
    /// chi-square inverse frozen holdouts and plateau witnesses (`oc59`, `oc76`, `oc70`).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_j9k_63_goldstein_451_start_matches_excel_bits() {
        let gamma_rows: [(u64, u64, u64, u64); 6] = [
            (0x3fcbb825ab49ca4b, 0x40638cf8e9c2c35f, 0x4008819e805f3445, 0x407c0b9b88dde21a),
            (0x3febcf9c683e8b05, 0x4077ec0f12e42733, 0x3fed30e1434e4b74, 0x4077140364228a54),
            (0x3fef2a22de855414, 0x4058b2ba8da2e53a, 0x3ff4e0f36eea7c0a, 0x406368de69acf270),
            (0x3fbb4b41901b6438, 0x4015c0930a0ace1e, 0x3ffc51cc65d2be9f, 0x4013d08a9a5eda89),
            (0x3fb670d8fae51f3c, 0x4015fea0e6caf30f, 0x40057acb7446d72a, 0x401cc96e31473339),
            (0x3fc8322556a2f9c9, 0x3ff33e0b087d0945, 0x3ff4ef4cb3cee98e, 0x3fda1fc1add81c2f),
        ];
        for (p, a, b, expected) in gamma_rows {
            let got = gamma_inv_kernel(f64::from_bits(p), f64::from_bits(a), f64::from_bits(b))
                .unwrap();
            assert_eq!(got.to_bits(), expected, "GAMMA.INV({p:#x}, {a:#x}, {b:#x})");
        }
        let chi_rt_rows: [(u64, u64, u64); 6] = [
            // oc59 (df=10, q=0.99), oc76 (df=7, q=0.97), oc70 (df=7, q=0.02)
            (0x3fefae147ae147ae, 0x4024000000000000, 0x40047737efe1de5b),
            (0x3fef0a3d70a3d70a, 0x401c000000000000, 0x3ffcd374fa538aea),
            (0x3f947ae147ae147b, 0x401c000000000000, 0x40309f570a2cbc80),
            // nx6 unseen frozen CHIINV / CHISQ.INV.RT holdouts
            (0x3f93b18667327922, 0x4031000000000000, 0x403f227a3644eb3d),
            (0x3fefdf3d1412a6d7, 0x403a000000000000, 0x4025b62a489a4e3a),
            (0x3faefaca4493affa, 0x403b000000000000, 0x40439c1394f3bed0),
        ];
        for (q, df, expected) in chi_rt_rows {
            let got = crate::ported::chi_f_t_family::chisq_inv_rt_kernel(
                f64::from_bits(q),
                f64::from_bits(df),
            )
            .unwrap();
            assert_eq!(got.to_bits(), expected, "CHIINV({q:#x}, {df:#x})");
        }
    }

    /// Excel 16.0.20527 (dc-j9k.64): `a < 1.0` Wilson-Hilferty cube `a * exp(3 * ln(c))`
    /// (frozen proof SHA-256 fb2f7609), `poisson_mass <= 1.1` inverse consistency, and
    /// Halley stopping (`|next - x| <= f64::EPSILON * x`, damping-without-improvement stop,
    /// frozen proof SHA-256 e727a10f).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_j9k_64_cb8_halley_and_small_shape_start_match_excel_bits() {
        let gamma_rows: [(u64, u64, u64, u64); 11] = [
            // a < 1.0 Wilson-Hilferty cube a * exp(3 * ln(c)) (Proof 1).
            (0x3fee_9691_352d_12ea, 0x3fe9_eda4_0e40_d6d1, 0x3ff0_0000_0000_0000, 0x4005_e147_17bd_5ec3),
            (0x3fed_0ee9_06e2_ef5a, 0x3fe7_00f7_d30d_76c2, 0x3ff0_0000_0000_0000, 0x3ffd_e73b_affa_5e2c),
            // 1.0 < a <= 1.1 poisson_mass <= 1.1 inverse holdouts (Proof 2 Bucket B).
            (0x3fe7_5229_a68b_7652, 0x3ff0_0000_0000_0004, 0x4009_f866_57b7_6ba0, 0x4010_f17e_9577_24bc),
            (0x3fe5_8e6f_9ac9_7738, 0x3ff0_0000_0000_0003, 0x4008_7d38_dc07_c006, 0x400b_6bef_21b1_d7ae),
            (0x3fec_a532_66a5_0e92, 0x3ff0_0000_0000_0003, 0x3ffa_a15e_cd54_821c, 0x400e_07db_d282_7fc8),
            // Damping-without-improvement stop (Proof 2 Buckets D and E).
            (0x3fed_bdc5_0d17_005b, 0x3fd8_687b_7b12_9cf7, 0x3ff4_b714_4760_7aa3, 0x3ffb_dc4e_0488_6909),
            (0x3fef_9063_f6b7_39bb, 0x3fe7_6167_6eb0_e9ce, 0x401f_3b00_0000_c02e, 0x403c_9913_89c5_2349),
            (0x3fed_9ab8_b8da_f6a4, 0x3fdc_5598_b6e8_873a, 0x3feb_3e8d_56ab_50b4, 0x3ff3_c844_0bae_e674),
            (0x3feb_0119_ab30_bef8, 0x3fed_db36_bbd9_6c2b, 0x4003_3abf_fbaf_82d9, 0x4010_d4dc_252b_bf46),
            (0x3fea_7287_8996_325e, 0x3fe6_7c1f_b0b0_ad47, 0x401f_475c_24b3_280f, 0x4023_f9ae_a6a2_cc29),
            (0x3fe9_ecf6_728d_31c8, 0x3fe6_43fc_b549_4ea3, 0x401b_3bb2_1d70_c8b0, 0x4020_2f55_d986_1460),
        ];
        for (p, a, b, expected) in gamma_rows {
            let got = gamma_inv_kernel(f64::from_bits(p), f64::from_bits(a), f64::from_bits(b))
                .unwrap();
            assert_eq!(got.to_bits(), expected, "GAMMA.INV({p:#x}, {a:#x}, {b:#x})");
        }
        // |next - x| <= f64::EPSILON * x without x_cand eval (Proof 2 Buckets C1 and C2).
        let chi_rt_rows: [(u64, u64, u64); 4] = [
            (0x3fe4_c850_bdb3_2700, 0x4045_8000_0000_0000, 0x4043_73e0_1b0b_a871),
            (0x3fea_9f20_3b37_4a96, 0x403e_0000_0000_0000, 0x4036_95c9_8946_a5dd),
            (0x3fec_3b5b_51b2_61af, 0x4035_0000_0000_0000, 0x402b_6508_8ae4_d4d7),
            (0x3fed_5652_4e51_6c1c, 0x403b_0000_0000_0000, 0x4031_8bbe_46b5_20ca),
        ];
        for (q, df, expected) in chi_rt_rows {
            let got = crate::ported::chi_f_t_family::chisq_inv_rt_kernel(
                f64::from_bits(q),
                f64::from_bits(df),
            )
            .unwrap();
            assert_eq!(got.to_bits(), expected, "CHISQ.INV.RT({q:#x}, {df:#x})");
        }
        let chi_inv_rows: [(u64, u64, u64); 2] = [
            (0x3fc3_3374_1214_842c, 0x401c_0000_0000_0000, 0x400a_ddeb_bdfe_a876),
            (0x3fdc_b55c_ccdb_25bd, 0x4024_0000_0000_0000, 0x4021_9848_5306_f46d),
        ];
        for (p, df, expected) in chi_inv_rows {
            let got = crate::ported::chi_f_t_family::chisq_inv_kernel(
                f64::from_bits(p),
                f64::from_bits(df),
            )
            .unwrap();
            assert_eq!(got.to_bits(), expected, "CHISQ.INV({p:#x}, {df:#x})");
        }
    }

    /// At these beta shapes GAMMALN's recurrence rounds 1 + beta to one.
    /// cb21 G1 (Excel 20625) resolves the normalizer on both sides of one.
    /// ax2 beta-end-initial-derivative [1424] and [2148] (Excel 20527)
    /// retain the original series result inside its stated accuracy.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn beta_cdf_tiny_shape_gamma_normalizer_matches_excel() {
        for (x, alpha, beta, expected) in [
            (0x3feffec7bfaa2300, 0x4062d44250b4743f, 0x2746cc43efa13925, 0x27627fb0d4c9e52f),
            (0x3fefff2d5e27c634, 0x404cb4b660ab5615, 0x2351638f41f86f9f, 0x2373f7656d0b8a7d),
            (0x3feffffde7210be9, 0x405bfab85ed4672a, 0x3b242fe5a4c77b4a, 0x3b55831dd4ae0a2d),
            (0x3feffffde7210be9, 0x40add219dde80e07, 0x3afe42bd0b9a4728, 0x3b22e4dfeb21d174),
        ] {
            let cdf = beta_dist_kernel(
                f64::from_bits(x),
                f64::from_bits(alpha),
                f64::from_bits(beta),
                true,
                0.0,
                1.0,
            )
            .unwrap();
            assert_eq!(
                cdf.to_bits(), expected,
                "x={x:#018x} alpha={alpha:#018x} beta={beta:#018x}"
            );
        }
    }

    /// Excel 16.0.20527 (dc-iuwct.50): BETA.INV structural and billion-ULP gross rows
    /// surfaced by the discrepancies bank (`ax2`), plus live Touchstone boundary edges.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_iuwct_50_beta_inv_structural_and_gross_boundaries() {
        // 1. bfrac overflow at b >= 2^512 * (a + 2n - 1) / sqrt(n * (a + n - 1))
        //    (ax2/beta-seed-exponents, 800 structural rows + live Touchstone bisected edges).
        assert_eq!(
            beta_inv_kernel(
                f64::from_bits(0x3f789374bc6a7efa),
                f64::from_bits(0x3ff0000000000001),
                f64::from_bits(0x5fffffffffffffff),
                0.0,
                1.0,
            )
            .unwrap()
            .to_bits(),
            0x1f52121a985ffcca,
        );
        assert_eq!(
            beta_inv_kernel(
                f64::from_bits(0x3f789374bc6a7efa),
                f64::from_bits(0x3ff0000000000001),
                f64::from_bits(0x6000000000000000),
                0.0,
                1.0,
            ),
            Err(WorksheetErrorCode::Num),
        );
        // ax2/beta-seed-exponents $[1684]
        assert_eq!(
            beta_inv_kernel(0.006, 1.0000000000000002, 3.432398830065305e156, 0.0, 1.0),
            Err(WorksheetErrorCode::Num),
        );
        // ax2/beta-seed-exponents $[65] and live Touchstone PDF-overflow edge (0x7d72f8939aa27197/8)
        assert_eq!(
            beta_inv_kernel(0.006, 0.15, f64::from_bits(0x7d72f8939aa27197), 0.0, 1.0),
            Ok(0.0),
        );
        assert_eq!(
            beta_inv_kernel(0.006, 0.15, f64::from_bits(0x7d72f8939aa27198), 0.0, 1.0),
            Err(WorksheetErrorCode::Num),
        );
        assert_eq!(
            beta_inv_kernel(
                f64::from_bits(0x3f789374bc6a7efa),
                f64::from_bits(0x3fc3333333333333),
                f64::from_bits(0x7d80000000000000),
                0.0,
                1.0,
            ),
            Err(WorksheetErrorCode::Num),
        );

        // 2. beta_density n - k <= 0 error propagation (ax2/beta-absorbed-inverse 58 rows,
        //    ax2/beta-prior-controls 23 rows).
        for (p, a, b) in [
            (0x3f76b2492e99b060_u64, 0x42f7b6254d824baa_u64, 0x3ff0000000000001_u64), // absorbed $[327]
            (0x3f9dda0d451cf9d9_u64, 0x434ddb2cb42a218e_u64, 0x4000000000000000_u64), // absorbed $[785]
            (0x3fe15e8d0e79251f_u64, 0x3ff0000000000003_u64, 0x434f1b7924d5db6a_u64), // prior $[561]
        ] {
            assert_eq!(
                beta_inv_kernel(f64::from_bits(p), f64::from_bits(a), f64::from_bits(b), 0.0, 1.0),
                Err(WorksheetErrorCode::Num),
                "expected #NUM! for ({p:#x}, {a:#x}, {b:#x})",
            );
        }

        // 3. ax2/beta-prior-controls $[3] (Ext80 a/(a+b*exp(2w))) and $[4] (Ext80 n = st(a+b-2)).
        assert_eq!(
            beta_inv_kernel(
                f64::from_bits(0x3fe0000000000001),
                f64::from_bits(0x3ff000001dc7b303),
                f64::from_bits(0x434805f26460ed59),
                1.0,
                100.0,
            )
            .unwrap()
            .to_bits(),
            0x3ff0000000000032,
        );
        assert_eq!(
            beta_inv_kernel(
                f64::from_bits(0x3fe00a4b00f095ec),
                f64::from_bits(0x3ff0000001ffffbd),
                f64::from_bits(0x419799a9a3392c51),
                1.0,
                100.0,
            )
            .unwrap()
            .to_bits(),
            0x3ff00000bac41696,
        );
        // Live Touchstone exact x87 double-rounding edges for beta_density n = st(a + b - 2):
        assert_eq!(
            beta_dist_kernel(
                f64::from_bits(0x3feffffffa8e2da7),
                f64::from_bits(0x419799a9a3392c51),
                f64::from_bits(0x3ff0000001ffbfff),
                false,
                0.0,
                1.0,
            ),
            Err(WorksheetErrorCode::Num),
        );
        assert!(
            beta_dist_kernel(
                f64::from_bits(0x3feffffffa8e2da7),
                f64::from_bits(0x419799a9a3392c51),
                f64::from_bits(0x3ff0000001ffc000),
                false,
                0.0,
                1.0,
            )
            .is_ok(),
        );

        // 4. ax2/beta-lower-probability $[231] (700-trial exhaustion -> #NUM!).
        assert_eq!(
            beta_inv_kernel(1.1078772577623568e-31, 292.24366148288243, 1.428573466563099, 0.0, 1.0),
            Err(WorksheetErrorCode::Num),
        );

        // 5. ax2/beta-power-starts $[381] (subnormal LowerPower kept, flushed at publication;
        //    PDF overflow edge at 0x3fc4b54e1a4059d6/7).
        assert_eq!(
            beta_inv_kernel(0.1653057172940107, 0.002537240689259808, 0.8278454102205709, 0.0, 1.0),
            Ok(0.0),
        );
        assert_eq!(
            beta_inv_kernel(
                f64::from_bits(0x3fc4b54e1a4059d6),
                0.002537240689259808,
                0.8278454102205709,
                0.0,
                1.0,
            ),
            Err(WorksheetErrorCode::Num),
        );
        assert_eq!(
            beta_inv_kernel(
                f64::from_bits(0x3fc4b54e1a4059d7),
                0.002537240689259808,
                0.8278454102205709,
                0.0,
                1.0,
            ),
            Ok(0.0),
        );
    }

    /// Excel 16.0.20527 (dc-iuwct.58): GAMMA.INV 105-trial exhaustion tolerance
    /// (`best_err <= 1e-12 * target`, both `prob` and `comp_prob` arms) and AS 91
    /// `power_seed` sum order `((ln(p) + gammaln(a)) + ln(a)) / a`.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_iuwct_58_gamma_inv_structural_and_gross_boundaries() {
        // 1. Five live Touchstone single-bit bisected MAX_TRIALS = 105 edges
        //    (p_err -> #NUM!, p_ok -> x_105 where best_err <= 1e-12 * target).
        //    Edges 3..5 carry a <=3 ULP forward gratio st200 residual (dc-j9k.59) at x_0 < 1.1;
        //    all five single-bit #NUM! -> finite transitions match live Excel bit-for-bit.
        let edges: [(u64, u64, u64, u64, u64); 5] = [
            // a = 0.0252 (!switch_comp)
            (0x3f99ce075f6fd220, 0x3fefa4e3873b90a2, 0x3fefa4e3873b90a3, 0x3fe3eeb2ef1e000f, 0),
            // a = 0.02055 (!switch_comp)
            (0x3f950b0f27bb2fec, 0x3fefdb099876b81d, 0x3fefdb099876b81e, 0x3ff03121a2dfde85, 0),
            // a = 0.019 (!switch_comp)
            (0x3f9374bc6a7ef9db, 0x3fefe649f3cec807, 0x3fefe649f3cec808, 0x3ff2fedd97d00a21, 2),
            // a = 0.0128 (!switch_comp)
            (0x3f8a36e2eb1c432d, 0x3feffca516ac6df6, 0x3feffca516ac6df7, 0x40029f26191448a4, 4),
            // a = 0.0097 (p > 0.9999, switch_comp = true, target = comp_prob)
            (0x3f83dd97f62b6ae8, 0x3fefff6c4cbc26a1, 0x3fefff6c4cbc26a2, 0x400bdcc7b0943823, 4),
        ];
        for (a_bits, p_err, p_ok, x_ok, max_ulp) in edges {
            let a = f64::from_bits(a_bits);
            assert_eq!(
                gamma_inv_kernel(f64::from_bits(p_err), a, 1.0),
                Err(WorksheetErrorCode::Num),
                "expected #NUM! at p={p_err:#x}, a={a_bits:#x}",
            );
            let got = gamma_inv_kernel(f64::from_bits(p_ok), a, 1.0)
                .expect("expected finite at p_ok")
                .to_bits();
            assert!(
                got.abs_diff(x_ok) <= max_ulp,
                "expected {x_ok:#x} +/- {max_ulp}u at p={p_ok:#x}, a={a_bits:#x}, got {got:#x}",
            );
        }

        // 2. Bank structural rows (Group A #NUM!, Group B finite) and 105-step gross rows.
        for (p_bits, a_bits) in [
            (0x3fefa4e36c3bdd1e_u64, 0x3f99ce075f6fd220_u64), // halley[5010]
            (0x3fefe649e3539e41_u64, 0x3f9374bc6a7ef9db_u64), // halley[4903]
            (0x3fefff6c4c8c49c4_u64, 0x3f83dd97f62b6ae8_u64), // cb6[237]
        ] {
            assert_eq!(
                gamma_inv_kernel(f64::from_bits(p_bits), f64::from_bits(a_bits), 1.0),
                Err(WorksheetErrorCode::Num),
            );
        }
        // Group B finite recoveries (halley[5008] and cb6[244] carry 1-ULP forward gratio st200
        // residuals -> +46u / +3u at x_105, owned by dc-j9k.59; halley[4829] is bit-exact).
        let got_5008 = gamma_inv_kernel(
            f64::from_bits(0x3fefa4e3c3261e13),
            f64::from_bits(0x3f99ce075f6fd220),
            1.0,
        )
        .expect("halley[5008] must be finite")
        .to_bits();
        assert!(got_5008.abs_diff(0x3fe3eebdb25852fa) <= 46, "halley[5008]={got_5008:#x}");
        let got_cb6_244 = gamma_inv_kernel(
            f64::from_bits(0x3fefff6c4cdc47e3),
            f64::from_bits(0x3f83dd97f62b6ae8),
            1.0,
        )
        .expect("cb6[244] must be finite")
        .to_bits();
        assert!(got_cb6_244.abs_diff(0x400bdcc918e32713) <= 3, "cb6[244]={got_cb6_244:#x}");

        for (p_bits, a_bits, expected) in [
            (0x3fefef942c5c3563_u64, 0x3f91b71758e21965_u64, 0x3ff6aa4ac43db371_u64), // halley[4829]
            (0x3fefe649f7ff93f5_u64, 0x3f9374bc6a7ef9db_u64, 0x3ff2fedf3efed5cf_u64), // halley[4902]
            (0x3fefeb923ac27462_u64, 0x3f9288ce703afb7f_u64, 0x3ff4d9bbff0238ef_u64), // halley[4866]
            (0x3fefa4e39778f829_u64, 0x3f99ce075f6fd220_u64, 0x3fe3eeb5d9e16df9_u64), // halley[5009]
        ] {
            assert_eq!(
                gamma_inv_kernel(f64::from_bits(p_bits), f64::from_bits(a_bits), 1.0)
                    .unwrap()
                    .to_bits(),
                expected,
                "GAMMA.INV({p_bits:#x}, {a_bits:#x}, 1)",
            );
        }

        // 3. AS 91 power_seed `((ln(p) + gammaln(a)) + ln(a)) / a`: small-shape plateau gross
        //    rows and live Touchstone `a ~ exp(-8)` sum-order separating probes (0 ULP exact).
        for (p_bits, a_bits, expected) in [
            (0x3feffdad4b376a31_u64, 0x3f04062c83ec6f53_u64, 0x3f35ec85ab6bab65_u64), // halley[3955]
            (0x3feffeac9d2f16ed_u64, 0x3ef818990fed1d12_u64, 0x3f40148394102a50_u64), // halley[2604]
            (0x3feffecf145b62b3_u64, 0x3ef7f09ba7583970_u64, 0x3f4f909edec4f7e7_u64), // halley[1868]
            (0x3fefff034d91b21e_u64, 0x3ef63c60040f69a2_u64, 0x3f5f61c9e1d5b3ea_u64), // halley[982]
            (0.9985_f64.to_bits(), 0.0003354_f64.to_bits(), 0x3f7a5a9ce89bedd5_u64), // live (lp+ga)+la vs (lp+la)+ga
            (0.9975_f64.to_bits(), 0.0003355_f64.to_bits(), 0x3f352ceb5a248ca1_u64), // live (lp+ga)+la vs (lp+la)+ga
        ] {
            assert_eq!(
                gamma_inv_kernel(f64::from_bits(p_bits), f64::from_bits(a_bits), 1.0)
                    .unwrap()
                    .to_bits(),
                expected,
                "GAMMA.INV({p_bits:#x}, {a_bits:#x}, 1)",
            );
        }
    }

    /// Excel 16.0.20527.20196 (Carbon cb20 / dc-j9k.71):
    /// - Unified 7-term Goldstein (`goldstein_initial_unit`) across `2 * alpha in [5e10, 2^1000)`
    ///   in `gamma_inv_kernel` and `n >= 5e10` (`z = -norm_s_inv_kernel(p)`) in `chisq_inv_rt_kernel`
    ///   (removing the truncated 3-term `goldstein_cube_from_z` and `|z| <= 2.75` `#NUM!` cliff).
    /// - Unified AS 91 right-tail `power_seed` `((ln(p_comp) + gammaln_excel(a)) + ln(a)) / a`
    ///   in `gamma_halley_unit` (`is_upper_q == true`).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cb20_goldstein_7term_and_chi_rt_as91_power_seed_match_excel() {
        use crate::ported::chi_f_t_family::chisq_inv_rt_kernel;

        // P9: 7-term Goldstein on GAMMA.INV (2*alpha >= 5e10, large |z|).
        for &(label, p_bits, a_bits, b_bits, expected) in &[
            ("P9_gamma_161", 0x3139_9e31_2db4_bb33_u64, 0x4248_4e65_ce72_cef6_u64, 0x4134_2775_bc67_eeb0_u64, 0x438e_9fb8_31b2_1a07_u64),
            ("P9_gamma_162", 0x300f_c3b1_8a88_ebb4, 0x422c_af07_78fb_9a7b, 0x3eb5_4fea_e050_f03e, 0x40f3_1b6e_b632_eb80),
        ] {
            let got = gamma_inv_kernel(
                f64::from_bits(p_bits),
                f64::from_bits(a_bits),
                f64::from_bits(b_bits),
            )
            .unwrap()
            .to_bits();
            assert_eq!(got, expected, "cb20 {label}: got {got:#018x}, expected {expected:#018x}");
        }

        // P9: 7-term Goldstein on CHIINV / CHISQ.INV.RT (5e10 <= df < 2^40, |z| > 2.75).
        // P10 & H8: AS 91 right-tail power_seed `((ln(p_comp) + gammaln(a)) + ln(a)) / a`.
        for &(label, q_bits, df_bits, expected) in &[
            ("P9_chi_171", 0x3aa8_d644_1dcc_75b9_u64, 0x4241_4c13_edaa_8000_u64, 0x4241_4d6c_f7e4_b6c8_u64),
            ("P9_chi_172", 0x3ce3_14cb_c7a5_ee91, 0x4261_2e28_4344_a800, 0x4261_2f63_d3e1_7ae4),
            ("P9_chi_176", 0x3ee7_8576_429f_5b17, 0x4258_cb13_548e_0000, 0x4258_ccd2_2ea0_a272),
            ("P9_chi_177", 0x3ab4_e2a3_75fd_5047, 0x4229_8b8d_dd26_8000, 0x4229_8db4_9dd0_a576),
            ("P10_chi_181", 0x3fef_ff57_d35c_3415, 0x4018_0000_0000_0000, 0x3fc4_76d7_f4f4_27ea),
            ("P10_chi_182", 0x3fef_ffff_ff92_b092, 0x4010_0000_0000_0000, 0x3f14_e91c_5558_28a4),
            ("P10_chi_191", 0x3fef_ffff_ffff_f545, 0x4028_0000_0000_0000, 0x3fa9_3d89_20d2_4712),
            ("P10_chi_192", 0x3fef_ffff_eea7_1b96, 0x4008_0000_0000_0000, 0x3ef9_b8b4_c933_e888),
            ("H8_chi_199", 0x3fef_ffff_ff1d_b45b, 0x4008_0000_0000_0000, 0x3ecc_4862_8a6e_35b4),
            ("H8_chi_202", 0x3fef_ffff_fffe_c35b, 0x4014_0000_0000_0000, 0x3f20_2d04_148d_fb15),
            ("H8_chi_223", 0x3fef_9e51_58c4_84e1, 0x4014_0000_0000_0000, 0x3fe3_262d_fc34_5476),
            ("H8_chi_224", 0x3fef_ffff_ffff_c4b2, 0x4014_0000_0000_0000, 0x3f10_8dcb_962a_79ef),
        ] {
            let got = chisq_inv_rt_kernel(f64::from_bits(q_bits), f64::from_bits(df_bits))
                .unwrap()
                .to_bits();
            assert_eq!(got, expected, "cb20 {label}: got {got:#018x}, expected {expected:#018x}");
        }
    }
}
