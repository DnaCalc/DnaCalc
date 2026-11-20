use crate::ported::excel_numeric::crt_exp;
use crate::ported::excel_numeric::crt_log;
use crate::ported::special_math_common::{
    bisect_inverse, bisect_inverse_closest, ln_gamma, regularized_beta, regularized_gamma_p,
    regularized_gamma_q,
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
        // In the measured tiny-second-shape continued-fraction region, the
        // gamma tail's 1e-30 floor survives the twenty-step recurrence. Only
        // use its stored exponential when z * (1 / z) is exactly one; the
        // rounded-down cases need further work. The shape and beta limits
        // bound this arm to the independent controls.
        if lower == 0.0 && upper == 1.0 && z > 0.7 && z < 1.0
            && (0.1..=15.0).contains(&alpha) && alpha != 1.0
            && (f64::MIN_POSITIVE..=2.0_f64.powi(-180)).contains(&beta)
        {
            use crate::ported::excel_numeric::{excel_exp, excel_log, excel_x87_div, excel_x87_mul};
            let nu = (alpha + 20.0) - 0.5;
            let t = -excel_x87_mul(nu, excel_log(z));
            if t >= 1.1 && excel_x87_mul(t, excel_x87_div(1.0, t)) == 1.0 {
                return Ok(excel_x87_mul(1e-30, excel_exp(-t)));
            }
        }
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
        return Ok(regularized_beta(z, alpha, beta));
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
        let x = a / (a + b * excel_exp(2.0 * w));
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
        // At an overflowing cube Excel starts at the midpoint. A zero
        // midpoint density publishes 0.5. The finite-density controls return NUM.
        if !(base * base * base).is_finite() {
            return Some(if beta_dist_kernel(0.5, a, b, false, 0.0, 1.0) == Ok(0.0) {
                BetaStart::Interior(0.5)
            } else {
                BetaStart::Failed
            });
        }
        let t = excel_x87_div(
            excel_x87_add(
                excel_log1p_times(1.0, -p),
                excel_x87_add(excel_log(b), log_beta()),
            ),
            b,
        );
        let x = excel_x87_sub(1.0, excel_exp(t));
        return if x == 1.0 {
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
        return Some(if x < f64::MIN_POSITIVE {
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
    let (mut x, upper_endpoint) = match start {
        BetaStart::Failed => return Err(WorksheetErrorCode::Num),
        BetaStart::Interior(x) => (x, false),
        BetaStart::LowerEndpoint => (1e-6, false),
        BetaStart::LowerPower(x) if x > 0.0 && x < 1.0 => (x, false),
        BetaStart::LowerPower(_) => return Ok(None),
        BetaStart::UpperEndpoint => (1.0 - 1e-6, true),
    };
    // The 1,040 frozen boundary controls separate the unrounded exponent
    // from its floor and truncation. Both tails use an absolute correction.
    let exponent = (-13.0 - 5.0 / (a * a) - libm::pow(p, -0.2)).max(-150.0);
    let tolerance = libm::pow(10.0, exponent).min(f64::EPSILON);
    let outside_upper = upper_endpoint
        && beta_dist_kernel(f64::from_bits(1.0_f64.to_bits() - 1), a, b, true, 0.0, 1.0)
            .is_ok_and(|f| f < p);
    // If the shape sum absorbs a, b*x retains the gamma limit of I_x(a,b).
    // Its Newton correction is in units of b*x and must be divided by b.
    let gamma_limit = matches!(start, BetaStart::LowerPower(_)) && b + a == b;
    let mut last_correction = 0.0;
    let mut last_adjustment: f64 = 1.0;
    let mut step_limit = f64::INFINITY;
    let mut best_x = x;
    let mut best_err = f64::INFINITY;
    let mut trials = 0;
    loop {
        let (cdf, density) = if gamma_limit {
            let u = excel_x87_mul(b, x);
            (
                gamma_dist_kernel(u, a, 1.0, true),
                gamma_dist_kernel(u, a, 1.0, false),
            )
        } else {
            (
                beta_dist_kernel(x, a, b, true, 0.0, 1.0),
                beta_dist_kernel(x, a, b, false, 0.0, 1.0),
            )
        };
        let (Ok(cdf), Ok(density)) = (cdf, density) else {
            return if matches!(start, BetaStart::LowerEndpoint) {
                Err(WorksheetErrorCode::Num)
            } else {
                Ok(Some(best_x))
            };
        };
        let df = excel_x87_sub(cdf, p);
        let err_df = df.abs();
        if err_df < best_err {
            best_err = err_df;
            best_x = x;
        }
        if density == 0.0 || err_df == 0.0 {
            return Ok(Some(best_x));
        }
        let mut correction = excel_x87_div(df, density);
        if gamma_limit {
            correction = excel_x87_div(correction, b);
        }
        if !correction.is_finite() || correction.abs() <= tolerance {
            return Ok(Some(best_x));
        }
        if last_correction != 0.0 && correction * last_correction <= 0.0 {
            step_limit = last_adjustment.abs().max(1e-300);
        }
        last_correction = correction;
        let mut factor = 1.0;
        loop {
            last_adjustment = excel_x87_mul(factor, correction);
            if last_adjustment.abs() >= step_limit {
                factor = excel_x87_div(factor, 3.0);
                if factor == 0.0 {
                    return Ok(Some(best_x));
                }
                continue;
            }
            trials += 1;
            if trials >= MAX_TRIALS {
                return if outside_upper || matches!(start, BetaStart::LowerEndpoint) {
                    Err(WorksheetErrorCode::Num)
                } else {
                    Ok(Some(best_x))
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
    let z = beta_newton_lower(probability, alpha, beta)?.unwrap_or_else(|| {
        bisect_inverse_closest(probability, 0.0, 1.0, |v| regularized_beta(v, alpha, beta))
    });
    let z = if reflected {
        excel_x87_add(0.5, excel_x87_sub(0.5, z))
    } else {
        z
    };
    Ok(excel_x87_add(
        lower,
        excel_x87_mul(z, excel_x87_sub(upper, lower)),
    ))
}

/// Stored cube from Goldstein's chi-square approximation (Algorithm 451, 1973).
/// At large n the cubic's base is Horner in the normal deviate, with coefficients in 1/n.
/// Goldstein's published coefficients and the stored PC64 stages were identified on
/// chi-square (dc-6332.18), including the separately rounded constant term. ax2's
/// 1,410 gamma controls per alias use n = 2*shape, retaining fractional degrees.
/// The higher powers can still shift a few low bits near the lower limit of this arm.
pub(super) fn goldstein_large_degree_factor(probability: f64, n: f64) -> Option<f64> {
    use crate::ported::excel_numeric::{excel_sqrt_staged, excel_x87_add, excel_x87_div, excel_x87_mul};
    if !(5e10..f64::from_bits(0x7e70_0000_0000_0000)).contains(&n) {
        // [5e10, 2^1000)
        return None;
    }
    let z = crate::ported::normal_log_family::norm_s_inv_kernel(probability).ok()?;
    // Goldstein's terms in z^3 f^(3/2) and beyond reach the last place for larger |z| at the
    // smaller degrees (fresh Excel answers: misses only at |z| >= 2.98 with n <= 2.6e11).
    let z_bound = if n >= f64::from_bits(0x4270_0000_0000_0000) { 4.0 } else { 2.75 }; // 2^40
    if !(z.abs() <= z_bound) {
        return None;
    }
    let f = excel_x87_div(1.0, n);
    let f1 = excel_sqrt_staged(f);
    let a0 = excel_x87_add(1.0000886, excel_x87_mul(-0.2237368, f));
    let a1 = excel_x87_mul(0.4713941, f1);
    let a2 = excel_x87_mul(1.348028e-4, f);
    let c = excel_x87_add(excel_x87_mul(excel_x87_add(excel_x87_mul(a2, z), a1), z), a0);
    Some(excel_x87_mul(excel_x87_mul(c, c), c))
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
    if crate::ported::discrete_dist::poisson_compensation_error(alpha, x / beta) {
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
        let cdf = regularized_gamma_p(alpha, x / beta);
        // Two independent Excel 20527 boundary sets (1,257 controls) publish
        // zero for tiny CDFs and keep normal results in this measured shape range.
        if (50.0..=150.0).contains(&alpha) && cdf.is_subnormal() {
            return Ok(0.0);
        }
        return Ok(cdf);
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

/// The Stirling series `S(a) = ln Gamma(a + 1) - (a + 1/2) ln a + a - ln(2 pi) / 2`, to B24,
/// in double-double for `a >= 10`.
fn stirling_series_dd(a: f64) -> crate::ported::double_double::Dd {
    use crate::ported::double_double::Dd;
    // B_2k / (2k (2k - 1)) for k = 1..12.
    const STIRLING: [(f64, f64); 12] = [
        (1.0, 12.0),
        (-1.0, 360.0),
        (1.0, 1260.0),
        (-1.0, 1680.0),
        (1.0, 1188.0),
        (-691.0, 360360.0),
        (1.0, 156.0),
        (-3617.0, 122400.0),
        (43867.0, 244188.0),
        (-174611.0, 125400.0),
        (77683.0, 5796.0),
        (-236364091.0, 1506960.0),
    ];
    let inv = Dd::from(1.0).div_f64(a);
    let inv2 = inv.mul(inv);
    let mut series = Dd::from(0.0);
    for &(num, den) in STIRLING.iter().rev() {
        series = series.mul(inv2).add(Dd::from(num).div_f64(den));
    }
    series.mul(inv)
}

const HALF_LN_2PI: crate::ported::double_double::Dd =
    crate::ported::double_double::Dd(0.9189385332046728, -3.8782941580672414e-17);

/// `ln D(a, x)`, `D = x^a e^-x / Gamma(a + 1)`, in double-double.
/// For `a >= 10` and `u = (x - a) / a` in (-0.75, 3) it is `a (ln(1 + u) - u) - ln(2 pi a) / 2 - S(a)`
/// with `ln(1 + u) = 2 atanh(u / (2 + u))`.
fn gamma_log_prefactor_dd(
    a: f64,
    x: crate::ported::double_double::Dd,
) -> Option<crate::ported::double_double::Dd> {
    use crate::ported::double_double::{Dd, ln};
    if a < 10.0 {
        return None;
    }
    let u = x.sub(Dd::from(a)).div_f64(a);
    if !(u.0 > -0.75 && u.0 < 3.0) {
        return None;
    }
    let w = u.div(Dd::from(2.0).add(u));
    let w2 = w.mul(w);
    let mut term = w;
    let mut sum = w;
    for k in 1..200 {
        term = term.mul(w2);
        let add = term.div_f64((2 * k + 1) as f64);
        sum = sum.add(add);
        if add.0.abs() <= sum.0.abs() * 1e-34 {
            break;
        }
    }
    let log_ratio = sum.add(sum).sub(u);
    let half_ln_a = ln(Dd::from(a)).div_f64(2.0);
    Some(log_ratio.mul_f64(a).sub(half_ln_a).sub(HALF_LN_2PI).sub(stirling_series_dd(a)))
}

/// The smallest shape whose inverse is polished. Measured on the full bank (fn6): polishing
/// integer and half-integer shapes from a = 1 changes CHIINV +14, CHISQ.INV -6, CHISQ.INV.RT -2
/// rows against a = 10; below 10 Excel's root is mostly not the correctly rounded one.
const GAMMA_POLISH_MIN_SHAPE: f64 = 10.0;

/// The root of `P(a, x) = target` (lower regularized gamma, unit scale) rounded once to binary64,
/// polished by Newton steps on a double-double `P = D(a, x) sum x^n / ((a + 1)...(a + n))` from a
/// root `x0` the lattice search already found. Excel 20527 publishes the correctly rounded root at
/// these shapes, where the binary64 forward is too coarse for the search to resolve it (fn6).
pub(crate) fn gamma_unit_root_polished(
    a: f64,
    target: crate::ported::double_double::Dd,
    x0: f64,
) -> Option<f64> {
    use crate::ported::double_double::{Dd, exp};
    if !(GAMMA_POLISH_MIN_SHAPE..=1.0e7).contains(&a) || !(target.0 > 0.0 && target.0 < 1.0) || !(x0 > 0.0) {
        return None;
    }
    let mut x = Dd::from(x0);
    for _ in 0..4 {
        let d = exp(gamma_log_prefactor_dd(a, x)?);
        let mut term = Dd::from(1.0);
        let mut sum = Dd::from(1.0);
        let mut n = 1.0;
        loop {
            term = term.mul(x).div(Dd::from(a).add(Dd::from(n)));
            sum = sum.add(term);
            if term.0 <= sum.0 * 1e-34 {
                break;
            }
            n += 1.0;
            if n > 200_000.0 {
                return None;
            }
        }
        let p = d.mul(sum);
        let density = d.mul_f64(a).div(x);
        let step = p.sub(target).div(density);
        x = x.sub(step);
        if step.0.abs() <= x.0 * 1e-32 {
            break;
        }
    }
    let root = x.to_f64();
    (root.is_finite() && root > 0.0).then_some(root)
}

/// A nonpositive Wilson-Hilferty start or first Halley proposal makes Excel's
/// small-shape inverse return #NUM! before it applies the scale. Best and Roberts'
/// AS 91 boundary selects the power start on the other side. Excel 20527 confirmed
/// the boundary at 420 fresh shapes and the outcome at all 4,705 random controls.
fn gamma_inverse_start_fails(p: f64, a: f64) -> bool {
    use crate::ported::excel_numeric::{
        excel_log, excel_sqrt_staged, excel_x87_add, excel_x87_div, excel_x87_mul, excel_x87_sub,
    };
    if 2.0 * a < excel_x87_mul(-1.24, excel_log(p)) {
        return false;
    }
    let v = excel_x87_div(1.0, excel_x87_mul(9.0, a));
    let Ok(z) = crate::ported::normal_log_family::norm_s_inv_kernel(p) else {
        return false;
    };
    let c = excel_x87_add(excel_x87_sub(1.0, v), excel_x87_mul(z, excel_sqrt_staged(v)));
    if c <= 0.0 {
        return true;
    }
    let x = excel_x87_mul(a, excel_x87_mul(c, excel_x87_mul(c, c)));
    let (Ok(cdf), Ok(density)) = (gamma_dist_kernel(x, a, 1.0, true), gamma_dist_kernel(x, a, 1.0, false)) else {
        return false;
    };
    if density == 0.0 {
        return false;
    }
    let residual = excel_x87_sub(cdf, p);
    let log_slope = excel_x87_sub(excel_x87_div(a - 1.0, x), 1.0);
    let denominator = excel_x87_sub(2.0 * density, excel_x87_mul(residual, log_slope));
    excel_x87_sub(x, excel_x87_div(2.0 * residual, denominator)) <= 0.0
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
    if let Some(factor) = goldstein_large_degree_factor(probability, 2.0 * alpha) {
        use crate::ported::excel_numeric::excel_x87_mul;
        let unit = excel_x87_mul(alpha, factor);
        let result = excel_x87_mul(beta, unit);
        return if result.is_finite() {
            Ok(if result.is_subnormal() { 0.0 } else { result })
        } else {
            Err(WorksheetErrorCode::Num)
        };
    }
    // Shared with the right-tail chi-square inverse. Scale only after the
    // independently measured unit result has been stored.
    if (2.0_f64.powi(-40)..2.0_f64.powi(40)).contains(&beta)
        && let Some(factor) = large_shape_inverse_factor(probability, alpha)
    {
        let unit = crate::ported::excel_numeric::excel_x87_mul(alpha, factor);
        return Ok(crate::ported::excel_numeric::excel_x87_mul(beta, unit));
    }
    // At these large shapes Excel's unit search saturates to the same stored
    // x87 factor as the right-tail chi-square inverse. The scale is
    // applied in its own stored multiplication. Keep unmeasured overflow and
    // underflow ranges on the existing solver.
    if probability >= f64::MIN_POSITIVE
        && alpha >= f64::from_bits(0x4720_0000_0000_0000) // 2^115
        && alpha < f64::from_bits(0x6570_0000_0000_0000) // 2^600
        && beta >= f64::from_bits(0x26f0_0000_0000_0000) // 2^-400
        && beta < f64::from_bits(0x58f0_0000_0000_0000) // 2^400
    {
        return Ok(crate::ported::excel_numeric::excel_x87_mul(
            beta,
            large_shape_inverse_unit(alpha),
        ));
    }
    if alpha == 1.0 {
        use crate::ported::excel_numeric::{excel_log, excel_log1p_times, excel_x87_mul};
        // Exponential inverse: the distribution logarithm keeps the small
        // probability before the final stored scale. Fresh Excel20527: 698/698
        // after final subnormal publication is flushed, plus 48 scaling controls.
        let unit = if probability < 0.375 {
            excel_log1p_times(-1.0, -probability)
        } else {
            -excel_log(1.0 - probability)
        };
        let x = excel_x87_mul(beta, unit);
        return Ok(if x.is_subnormal() { 0.0 } else { x });
    }
    if alpha < 1.0 {
        use crate::ported::excel_numeric::{
            excel_exp, excel_log, excel_x87_add, excel_x87_div, gammaln_excel,
        };
        // The lower-tail power estimate is exact at the underflow scale.
        // Excel 20527: 56 controls separate an estimate rounded to zero
        // from a nonzero subnormal, which fails before scale is applied.
        let log_seed = excel_x87_div(
            excel_x87_add(excel_log(probability), gammaln_excel(1.0 + alpha)),
            alpha,
        );
        let seed = excel_exp(log_seed);
        if seed == 0.0 {
            return Ok(0.0);
        }
        if seed.is_subnormal() {
            return Err(WorksheetErrorCode::Div0);
        }
        if gamma_inverse_start_fails(probability, alpha) {
            return Err(WorksheetErrorCode::Num);
        }
    }
    // design/fable52-last-bit.md section 5 (Claude Fable 5.2), measured 300/300 on Excel:
    // GAMMA.INV(p,a,b) = st(b * GAMMA.INV(p,a,1)), CHISQ.INV(p,2a) = GAMMA.INV(p,a,2): a
    // unit-scale search followed by one stored multiply.
    // Preserve the smaller upper tail: the stored P=1-Q has wide plateaus
    // near one. Fresh Excel20527 controls gain139 exact words, lose four
    // one-ULP words, and reduce worst error from 8.7e13 to76 ULP.
    let upper = probability > 0.5;
    let target = if upper {
        -(1.0 - probability)
    } else {
        probability
    };
    let forward = |x| {
        if upper {
            -regularized_gamma_q(alpha, x)
        } else {
            regularized_gamma_p(alpha, x)
        }
    };
    let mut hi = (alpha + 10.0 * alpha.sqrt() + 10.0).max(1.0);
    for _ in 0..200 {
        if forward(hi) >= target || hi == f64::INFINITY {
            break;
        }
        hi *= 2.0;
    }
    let mut unit = if upper {
        bisect_inverse_closest(target, 0.0, hi, forward)
    } else {
        bisect_inverse(target, 0.0, hi, forward)
    };
    if upper && 1.0 - probability < 1e-12 {
        // The double-double lower tail cannot resolve so small an upper tail.
    } else if let Some(root) = gamma_unit_root_polished(
        alpha,
        crate::ported::double_double::Dd::from(probability),
        unit,
    ) {
        unit = root;
    }
    // From a shape of about 2e10, Excel's unit result is the quantile times the same stored
    // factor as the very-large-shape region (large_shape_inverse_unit), 1.000265823551: Excel
    // 16.0.20527 through Touchstone at p = 0.5 over shapes 3.2e10 to 5.6e29 (a quarter decade
    // apart, 76 calls) and at p from 1e-10 to 0.999 at 1e12 and 1e14, ratio to the quantile
    // constant to 1e-12. Below, a #NUM! window whose edges move with p (at p = 0.5 from
    // 5.70e9 to 1.97e10; at p = 1e-10 the upper edge is 2.70e10) is not modelled, so the
    // factor applies from 2.7e10, above every measured edge. Approximate, not Excel's bits.
    if (2.7e10..2.0_f64.powi(95)).contains(&alpha) && probability >= 1e-10 {
        unit = crate::ported::excel_numeric::excel_x87_mul(
            unit,
            f64::from_bits(0x3ff0_0116_bc77_4cb6),
        );
    }
    let x = crate::ported::excel_numeric::excel_x87_mul(beta, unit);
    Ok(if x.is_subnormal() { 0.0 } else { x })
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
}
