use crate::ported::special_dist_family::erfc_of_sqrt_half_x;
use crate::ported::special_math_common::{
    bisect_inverse, bratio_from_wrapper, regularized_gamma_q,
};
use crate::value::WorksheetErrorCode;

fn truncate_positive_integer(value: f64) -> Result<f64, WorksheetErrorCode> {
    let truncated = value.trunc();
    // Excel docs: #NUM! when deg_freedom > 1e10 (strictly greater); 1e10 itself is admitted.
    if !truncated.is_finite() || truncated < 1.0 || truncated > 1e10 {
        Err(WorksheetErrorCode::Num)
    } else {
        Ok(truncated)
    }
}

fn validate_probability_open_unit(probability: f64) -> Result<f64, WorksheetErrorCode> {
    if !probability.is_finite() || probability <= 0.0 || probability >= 1.0 {
        Err(WorksheetErrorCode::Num)
    } else {
        Ok(probability)
    }
}

fn validate_nonnegative_x(x: f64) -> Result<f64, WorksheetErrorCode> {
    if !x.is_finite() || x < 0.0 {
        Err(WorksheetErrorCode::Num)
    } else {
        Ok(x)
    }
}

fn density_flag(value: f64) -> bool {
    value != 0.0
}

pub fn chisq_pdf_kernel(x: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let x = validate_nonnegative_x(x)?;
    let k = truncate_positive_integer(deg_freedom)?;
    if x == 0.0 {
        return if k == 1.0 {
            // CDF limit is finite but density diverges to +inf; Excel returns #NUM!
            Err(WorksheetErrorCode::Num)
        } else if k == 2.0 {
            Ok(0.5)
        } else {
            Ok(0.0)
        };
    }
    // Excel 20527: 513 paired controls, including overflowing direct gamma
    // normalizers, identify the same stored Poisson-mass density as GAMMA.DIST.
    crate::ported::beta_gamma_stats_family::gamma_dist_kernel(x, k / 2.0, 2.0, false)
}

pub fn chisq_dist_kernel(
    x: f64,
    deg_freedom: f64,
    cumulative: bool,
) -> Result<f64, WorksheetErrorCode> {
    let x = validate_nonnegative_x(x)?;
    let k = truncate_positive_integer(deg_freedom)?;
    if cumulative {
        // Inverse-problem identity, live Excel 16.0 b20228: CHISQ.DIST(x,1,TRUE)
        // == ERF.PRECISE(SQRT(x/2)) == GAMMA.DIST(x,0.5,2,TRUE) on 154/154
        // distinct x. Rival stagings SQRT(x)/SQRT(2) and SQRT(x)*(1/SQRT(2))
        // fail the same bank.
        // Inverse-problem identity, live Excel 16.0 b20228:
        // CHISQ.DIST(x,df,TRUE) == GAMMA.DIST(x, df/2, 2, TRUE) on 88/88
        // across df in {1,2,3,4,5,6,8,10}. 1-CHIDIST is not the CDF (49/88).
        // Routes df=1/2 through the landed ERF/EXPON gamma specials.
        let cdf = crate::ported::beta_gamma_stats_family::gamma_dist_kernel(x, k / 2.0, 2.0, true)?;
        // Excel's CDF stores flush a subnormal probability before publication.
        Ok(if cdf.is_subnormal() { 0.0 } else { cdf })
    } else {
        chisq_pdf_kernel(x, k)
    }
}

pub fn chisq_dist_rt_kernel(x: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let x = validate_nonnegative_x(x)?;
    let k = truncate_positive_integer(deg_freedom)?;
    // Same capture: CHIDIST(x,1) == CHISQ.DIST.RT(x,1) == ERFC.PRECISE(SQRT(x/2))
    // bit-exactly. Routing through the published ERFC kernel is the identified
    // graph, not a numeric fit.
    if k == 1.0 {
        erfc_of_sqrt_half_x(x)
    } else if k == 2.0 {
        // Inverse-problem identity, live Excel 16.0 b20228: CHIDIST(x,2)
        // == EXP(-x/2) == EXP(-(x/2)) on 68/68 nonnegative rows. The
        // worksheet EXP is the signed-off x87 elementary. The complementary
        // CDF 1-EXP(-x/2) is NOT this graph (9/11 on a follow-up bank).
        Ok(crate::ported::excel_numeric::excel_exp(-(x / 2.0)))
    } else {
        Ok(regularized_gamma_q(k / 2.0, x / 2.0))
    }
}

fn search_upper_bound<F>(target: f64, initial_hi: f64, f: F) -> f64
where
    F: Fn(f64) -> f64,
{
    let mut hi = initial_hi.max(1.0);
    for _ in 0..1024 {
        if f(hi) >= target || hi == f64::INFINITY {
            return hi;
        }
        hi *= 2.0;
    }
    hi
}

pub fn chisq_inv_kernel(probability: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let k = truncate_positive_integer(deg_freedom)?;
    // Inverse-problem identity, live Excel 16.0 b20228: CHISQ.INV(p, df)
    // == GAMMA.INV(p, df/2, 2) on 63/63 interior pairs. Endpoints also
    // match: CHISQ.INV(0,2)=0 and CHISQ.INV(1,2)=#NUM!. CHIINV /
    // CHISQ.INV.RT is the right-tail inverse, not GAMMA.INV(1-p,...) (34/63).
    if !probability.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    crate::ported::beta_gamma_stats_family::gamma_inv_kernel(probability, k / 2.0, 2.0).map_err(
        |e| match e {
            code => code,
            _ => WorksheetErrorCode::Value,
        },
    )
}

pub fn chisq_inv_rt_kernel(probability: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let p = if probability == 1.0 {
        1.0
    } else {
        validate_probability_open_unit(probability)?
    };
    if p == 1.0 {
        let k = deg_freedom.trunc();
        return if k.is_finite() && k >= 1.0 {
            Ok(0.0)
        } else {
            Err(WorksheetErrorCode::Num)
        };
    }
    let n = deg_freedom.trunc();
    use crate::ported::beta_gamma_stats_family::{
        flush_subnormal, goldstein_initial_unit, large_shape_inverse_unit,
    };
    // At large degrees Excel publishes Goldstein's starting value. The right
    // tail negates the normal deviate of p itself, preserving small tails that
    // would round away in 1 - p. The cubic uses the truncated degree.
    if n >= 2.0_f64.powi(40) && p >= f64::MIN_POSITIVE {
        if n >= 2.0_f64.powi(1000) {
            use crate::ported::excel_numeric::excel_x87_mul;
            let result = large_shape_inverse_unit(n);
            // The overflow check at 1.1 times the stored starting value takes
            // precedence over overflow in the density normalizer.
            if !excel_x87_mul(1.1, result).is_finite() {
                return Err(WorksheetErrorCode::Num);
            }
            if !excel_x87_mul(std::f64::consts::PI, n).is_finite() {
                return Err(WorksheetErrorCode::Div0);
            }
            return Ok(result);
        }
        let z = -crate::ported::normal_log_family::norm_s_inv_kernel(p)?;
        return goldstein_initial_unit(z, n * 0.5)
            .map(|unit| unit * 2.0)
            .ok_or(WorksheetErrorCode::Num);
    }
    if let Some(factor) =
        crate::ported::beta_gamma_stats_family::goldstein_large_degree_factor(1.0 - p, n)
    {
        let res = crate::ported::excel_numeric::excel_x87_mul(n, factor);
        return if res.is_finite() {
            Ok(flush_subnormal(res))
        } else {
            Err(WorksheetErrorCode::Num)
        };
    }
    let k = truncate_positive_integer(deg_freedom)?;
    if k == 1.0 {
        // Excel 20527 (cb6, dc-j9k.60): df == 1 folds q via comp(q) = 0.5 + (0.5 - q),
        // rejects comp(q) >= 1.0 with #NUM!, and evaluates 2 * erfinv(comp(q))^2.
        use crate::ported::excel_numeric::{excel_x87_add, excel_x87_mul, excel_x87_sub};
        let p_lower = excel_x87_add(0.5, excel_x87_sub(0.5, p));
        if p_lower >= 1.0 {
            return Err(WorksheetErrorCode::Num);
        }
        let unit = crate::ported::normal_log_family::gamma_inv_half_unit(p_lower);
        return Ok(flush_subnormal(excel_x87_mul(2.0, unit)));
    }
    if k == 2.0 {
        use crate::ported::excel_numeric::{excel_log, excel_log1p_times};
        // Exponential right inverse, preserving p rather than first taking
        // 1-p. The near-one arm shares the measured distribution logarithm.
        return Ok(flush_subnormal(if p > 0.625 {
            excel_log1p_times(-2.0, p - 1.0)
        } else {
            -2.0 * excel_log(p)
        }));
    }
    // df >= 3: invert Q(k/2, x/2) = p directly via Excel's damped Halley solver,
    // switching to P(k/2, x/2) = comp(p) strictly above 0.9999.
    let unit = flush_subnormal(crate::ported::beta_gamma_stats_family::gamma_halley_unit(
        p,
        k / 2.0,
        true,
    )?);
    let res = crate::ported::excel_numeric::excel_x87_mul(2.0, unit);
    if res.is_finite() {
        Ok(flush_subnormal(res))
    } else {
        Err(WorksheetErrorCode::Num)
    }
}

pub fn f_pdf_kernel(x: f64, deg1: f64, deg2: f64) -> Result<f64, WorksheetErrorCode> {
    use crate::ported::discrete_dist::stirling_error;
    use crate::ported::excel_numeric::{
        excel_exp_running, excel_log, excel_log1p_times, excel_x87_add, excel_x87_div,
        excel_x87_mul, excel_x87_sub,
    };
    const LN_SQRT_2PI: f64 = 0.9189385332046728;
    let x = validate_nonnegative_x(x)?;
    let d1 = truncate_positive_integer(deg1)?;
    let d2 = truncate_positive_integer(deg2)?;
    let mx = excel_x87_mul(d1, x);
    if !mx.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    if x == 0.0 {
        return if d1 < 2.0 {
            // Density diverges to +inf; Excel returns #NUM!
            Err(WorksheetErrorCode::Num)
        } else if d1 == 2.0 {
            Ok(1.0)
        } else {
            Ok(0.0)
        };
    }
    let a = 0.5 * d1;
    let b = 0.5 * d2;
    let ab = excel_x87_add(a, b);
    let (r0, rx, tx) = if mx >= d2 {
        (
            excel_x87_div(d2, d1),
            excel_x87_div(d2, mx),
            -excel_x87_mul(b, excel_log(x)),
        )
    } else {
        (
            excel_x87_div(d1, d2),
            excel_x87_div(mx, d2),
            excel_x87_mul(a, excel_log(x)),
        )
    };
    let tlop = excel_x87_mul(
        ab,
        excel_x87_sub(excel_log1p_times(1.0, r0), excel_log1p_times(1.0, rx)),
    );
    let exponent = excel_x87_add(tx, tlop);
    let s3 = excel_x87_sub(
        excel_x87_add(stirling_error(a), stirling_error(b)),
        stirling_error(ab),
    );
    let norm = excel_x87_mul(
        0.5,
        excel_log(excel_x87_add(excel_x87_div(1.0, a), excel_x87_div(1.0, b))),
    );
    let density = excel_x87_div(excel_exp_running(exponent, &[s3, norm, LN_SQRT_2PI]), x);
    if density.is_finite() {
        Ok(density)
    } else {
        Err(WorksheetErrorCode::Num)
    }
}

/// Both tails of the F distribution, `(right, left)`, from one incomplete beta call.
///
/// With `n = d1 f` and `r` the ratio of `n` and `d2` that is at least 1, the smaller beta argument
/// is `1 / (1 + r)` and the larger `r / (1 + r)`, each stage stored, and the kernel is called with
/// the smaller one first: `I_small(d2 / 2, d1 / 2)` when `n >= d2`, and otherwise
/// `I_small(d1 / 2, d2 / 2)`, whose complement output is the right tail.
///
/// Read from Excel 16.0.20527 (dc-6332.8). Where the power series runs on the smaller argument,
/// FDIST equals Excel's own BETA.DIST there on 411 of 411 and (as a complement) 1,200 of 1,200
/// calls. Where the recurrence takes both arguments FDIST is not BETA.DIST at either one (about
/// half of 1,043 calls, even when the two sum to one exactly); with the larger argument as
/// `r / (1 + r)` and the switch at `n = d2` this kernel matches all 512 fresh calls of the arms
/// it has exactly, and with `1 / (1 + 1 / r)` or `1 - small` it does not. The orientation shows
/// in the unit-shape arms: at `d1 = 2` and `n < d2` Excel's answer is that of the arm for a
/// first shape of 1 on 154 of 200 calls, 144 for the other.
pub(crate) fn f_tails(f: f64, d1: f64, d2: f64) -> (f64, f64) {
    use crate::ported::excel_numeric::{excel_x87_add, excel_x87_div, excel_x87_mul};
    let n = excel_x87_mul(d1, f);
    if n == 0.0 {
        return (1.0, 0.0);
    }
    if n == f64::INFINITY {
        return (0.0, 1.0);
    }
    let (numerator, denominator) = if n >= d2 { (n, d2) } else { (d2, n) };
    let r = excel_x87_div(numerator, denominator);
    let one_plus = excel_x87_add(1.0, r);
    let (small, large) = (excel_x87_div(1.0, one_plus), excel_x87_div(r, one_plus));
    if n >= d2 {
        bratio_from_wrapper(d2 / 2.0, d1 / 2.0, small, large)
    } else {
        let (left, right) = bratio_from_wrapper(d1 / 2.0, d2 / 2.0, small, large);
        (right, left)
    }
}

pub fn f_dist_kernel(
    x: f64,
    deg1: f64,
    deg2: f64,
    cumulative: bool,
) -> Result<f64, WorksheetErrorCode> {
    let x = validate_nonnegative_x(x)?;
    let d1 = truncate_positive_integer(deg1)?;
    let d2 = truncate_positive_integer(deg2)?;
    if cumulative {
        if !crate::ported::excel_numeric::excel_x87_mul(d1, x).is_finite() {
            return Err(WorksheetErrorCode::Div0);
        }
        Ok(f_tails(x, d1, d2).1)
    } else {
        f_pdf_kernel(x, d1, d2).map(crate::ported::beta_gamma_stats_family::flush_subnormal)
    }
}

pub fn f_dist_rt_kernel(x: f64, deg1: f64, deg2: f64) -> Result<f64, WorksheetErrorCode> {
    let x = validate_nonnegative_x(x)?;
    let d1 = truncate_positive_integer(deg1)?;
    let d2 = truncate_positive_integer(deg2)?;
    if !crate::ported::excel_numeric::excel_x87_mul(d1, x).is_finite() {
        return Err(WorksheetErrorCode::Div0);
    }
    let tail = f_tails(x, d1, d2).0;
    // The general right-tail arm flushes positive subnormals; the d1=2
    // unit-shape arm can publish them and is handled separately by its caller.
    Ok(if d1 != 2.0 && tail > 0.0 && tail.is_subnormal() { 0.0 } else { tail })
}

/// Both F quantiles use the lower-probability beta coordinate. The four
/// logarithms are accumulated by the distribution exponential before publication.
fn f_inverse_beta(p: f64, d1: f64, d2: f64, right: bool) -> Option<f64> {
    use crate::ported::excel_numeric::{excel_exp_running, excel_log, excel_log1p_times};
    let (q, right) = if p > 0.5 {
        (1.0 - p, !right)
    } else {
        (p, right)
    };
    let (a, b) = if right {
        (d2 / 2.0, d1 / 2.0)
    } else {
        (d1 / 2.0, d2 / 2.0)
    };
    let z = crate::ported::beta_gamma_stats_family::beta_inv_kernel(q, a, b, 0., 1.).ok()?;
    if !(z > 0.0 && z < 1.0) {
        return None;
    }
    let lz = excel_log(z);
    let lc = excel_log1p_times(1.0, -z);
    let (ln_num, ln_den) = if right { (lc, lz) } else { (lz, lc) };
    let f = excel_exp_running(excel_log(d2), &[-ln_num, excel_log(d1), ln_den]);
    f.is_finite().then_some(f)
}

pub fn f_inv_kernel(probability: f64, deg1: f64, deg2: f64) -> Result<f64, WorksheetErrorCode> {
    let p = if probability == 0.0 {
        0.0
    } else {
        validate_probability_open_unit(probability)?
    };
    let d1 = truncate_positive_integer(deg1)?;
    let d2 = truncate_positive_integer(deg2)?;
    if p == 0.0 {
        return Ok(0.0);
    }
    if let Some(f) = f_inverse_beta(p, d1, d2, false) {
        return Ok(f);
    }
    let fwd = move |x: f64| f_tails(x, d1, d2).1;
    let hi = search_upper_bound(p, 1.0, fwd);
    Ok(bisect_inverse(p, 0.0, hi, fwd))
}

pub fn f_inv_rt_kernel(probability: f64, deg1: f64, deg2: f64) -> Result<f64, WorksheetErrorCode> {
    let p = if probability == 1.0 {
        1.0
    } else {
        validate_probability_open_unit(probability)?
    };
    let d1 = truncate_positive_integer(deg1)?;
    let d2 = truncate_positive_integer(deg2)?;
    if p == 1.0 {
        return Ok(0.0);
    }
    if let Some(f) = f_inverse_beta(p, d1, d2, true) {
        return Ok(f);
    }
    // Invert the published right-tail surface (f_dist_rt's accurate complement
    // form) directly at p, not the CDF at 1-p — same principle as CHIINV
    // (W109 b14+b19: the complement staging carries a systematic small-p bias).
    // The surface is decreasing in x, so invert the negated forward at -p.
    let f = move |x: f64| -f_tails(x, d1, d2).0;
    let hi = search_upper_bound(-p, 1.0, f);
    Ok(bisect_inverse(-p, 0.0, hi, f))
}

fn t_pdf(x: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    // Density admits finite degrees above the CDF's 1e10 ceiling. A zero
    // truncated degree reaches the density's division before its domain error.
    let v = deg_freedom.trunc();
    if v == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    if !v.is_finite() || v < 0.0 || !x.is_finite() || !(x * x).is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    if v == 1.0 {
        // Live Excel 16.0 b20326: T.DIST(x,1,FALSE)=1/(PI()*(1+x*x)) 20/20.
        // 1/PI()/(1+x*x) is 12/20; 1/(PI()+PI()*x*x) is 18/20.
        return Ok(1.0 / (std::f64::consts::PI * (1.0 + x * x)));
    }
    if v == 2.0 {
        // Live Excel 16.0 b20326: T.DIST(x,2,FALSE)=POWER(2+x*x,-1.5) 15/15.
        // 1/EXP(1.5*LN) is 15/15 in Excel but only 10/15 via spilled excel_exp/log.
        // excel_pow_chain is 15/15 including |x|=4,8.
        return Ok(crate::ported::excel_numeric::excel_pow_chain(
            2.0 + x * x,
            -1.5,
        ));
    }
    use crate::ported::discrete_dist::{deviance, stirling_error};
    use crate::ported::excel_numeric::{
        excel_exp, excel_log, excel_sqrt_staged, excel_x87_add, excel_x87_div, excel_x87_mul,
        excel_x87_sub,
    };
    // Subtract the defining Stirling expansions before exponentiating:
    // log(C_v * sqrt(2*pi)) = s(a+1/2) - s(a) - D(a,a+1/2), a=v/2.
    // The spatial factor is exp(-a*log1p(r))/sqrt(1+r), r=x*x/v.
    // Excel 20527 controls select the fixed-factor quotient before the final
    // multiply; the compensated public-LN graph improves the measured holdout
    // by 401/521 with no losses (dc-bc1o). Remaining last bits are counted.
    let a = v / 2.0;
    let b = (v + 1.0) / 2.0;
    let normalizer = excel_x87_sub(
        excel_x87_sub(stirling_error(b), stirling_error(a)),
        deviance(a, b),
    );
    let ratio = excel_x87_div(excel_x87_mul(x, x), v);
    let base = excel_x87_add(1.0, ratio);
    let dropped = excel_x87_sub(ratio, excel_x87_sub(base, 1.0));
    let log_base = excel_x87_add(excel_log(base), excel_x87_div(dropped, base));
    let exponent = excel_x87_sub(normalizer, excel_x87_mul(a, log_base));
    let factor = excel_x87_div(
        crate::ported::normal_dist_common::INV_SQRT_2PI,
        excel_sqrt_staged(base),
    );
    Ok(excel_x87_mul(excel_exp(exponent), factor))
}

/// Excel's stored beta arguments retain the two quotients, but an argument
/// rounded to one publishes the endpoint even when the other quotient is nonzero.
fn t_two_tail(x: f64, v: f64) -> Result<f64, WorksheetErrorCode> {
    use crate::ported::excel_numeric::{excel_x87_add, excel_x87_div, excel_x87_mul};
    let t2 = excel_x87_mul(x, x);
    if !t2.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    let den = excel_x87_add(v, t2);
    let z = excel_x87_div(v, den);
    if z == 1.0 {
        return Ok(1.0);
    }
    let tail = bratio_from_wrapper(v / 2.0, 0.5, z, excel_x87_div(t2, den)).0;
    if tail.is_finite() {
        Ok(tail)
    } else {
        Err(WorksheetErrorCode::Num)
    }
}

fn t_cdf(x: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let v = truncate_positive_integer(deg_freedom)?;
    let half = 0.5 * t_two_tail(x, v)?;
    Ok(if x >= 0.0 { 1.0 - half } else { half })
}

pub fn t_dist_kernel(
    x: f64,
    deg_freedom: f64,
    cumulative: bool,
) -> Result<f64, WorksheetErrorCode> {
    if cumulative {
        t_cdf(x, deg_freedom)
    } else {
        t_pdf(x, deg_freedom)
    }
}

pub fn t_dist_rt_kernel(x: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let v = truncate_positive_integer(deg_freedom)?;
    let half = 0.5 * t_two_tail(x, v)?;
    let result = if x >= 0.0 { half } else { 1.0 - half };
    // Excel 20527's measured right tails publish a subnormal as zero.
    Ok(if (2.0..=2001.0).contains(&v) && result.is_subnormal() { 0.0 } else { result })
}

pub fn t_dist_2t_kernel(x: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let x = validate_nonnegative_x(x)?;
    let v = truncate_positive_integer(deg_freedom)?;
    let result = t_two_tail(x, v)?;
    // Independent boundary and neighboring-input controls also cover TDIST's
    // two-tail route; retain the normal results and overflow errors.
    Ok(if (2.0..=2001.0).contains(&v) && result.is_subnormal() { 0.0 } else { result })
}

pub fn tdist_kernel(x: f64, deg_freedom: f64, tails: f64) -> Result<f64, WorksheetErrorCode> {
    // The legacy name rejects negative x even with one tail. Its tails slot
    // uses the same tolerant integer boundary as Excel's date/count slots:
    // all three fresh edges at 1, 2 and 3 are exactly 2^-22 + 2^-33 below.
    let x = validate_nonnegative_x(x)?;
    let tails = crate::ported::text_kernels::date_integer_floor(tails);
    match tails as i32 {
        1 => t_dist_rt_kernel(x, deg_freedom),
        2 => t_dist_2t_kernel(x, deg_freedom),
        _ => Err(WorksheetErrorCode::Num),
    }
}

pub fn t_inv_kernel(probability: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    if !probability.is_finite() || probability < 0.0 || probability >= 1.0 {
        return Err(WorksheetErrorCode::Num);
    }
    let v = truncate_positive_integer(deg_freedom)?;
    if probability == 0.5 {
        return Ok(0.0);
    }
    // Excel 20527: 524/524 signed one/two-tail pairs. Form the lower
    // probability directly, so tiny p is never lost in 1 - (1 - p).
    if probability < 0.5 {
        Ok(-t_inv_2t_kernel(2.0 * probability, v)?)
    } else {
        t_inv_2t_kernel(2.0 * (1.0 - probability), v)
    }
}

pub fn t_inv_2t_kernel(probability: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let v = truncate_positive_integer(deg_freedom)?;
    if !probability.is_finite() || probability < 0.0 || probability >= 2.0 {
        return Err(WorksheetErrorCode::Num);
    }
    if probability == 1.0 {
        return Ok(0.0);
    }
    let (p, sign) = if probability > 1.0 {
        (2.0 - probability, -1.0)
    } else {
        (probability, 1.0)
    };
    if v == 1.0 {
        // Reflect before forming the Cauchy cotangent; the stored angular
        // argument at p>1 is not equivalent. Its zero pole is #DIV/0!.
        if p == 0.0 {
            return Err(WorksheetErrorCode::Div0);
        }
        if p == 0.5 {
            return Ok(sign);
        }
        // Excel stores the pi product from x87 precision before halving.
        // Adjacent double-rounding controls distinguish this from a binary64
        // product even though the two expressions usually give the same angle.
        let arg = crate::ported::excel_numeric::excel_x87_mul(std::f64::consts::PI, p) / 2.0;
        return Ok(sign / crate::ported::tan::tan_kernel(arg));
    }
    if p == 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    // Excel 20527: the direct coordinate is used through p=0.9, the
    // complementary coordinate above it. The latter preserves tiny roots.
    use crate::ported::excel_numeric::{
        excel_sqrt_staged, excel_x87_div, excel_x87_mul, excel_x87_sub,
    };
    let t = if p <= 0.9 {
        let z = crate::ported::beta_gamma_stats_family::beta_inv_kernel(p, v / 2.0, 0.5, 0.0, 1.0)?;
        excel_sqrt_staged(excel_x87_mul(v, excel_x87_sub(excel_x87_div(1.0, z), 1.0)))
    } else {
        let y = crate::ported::beta_gamma_stats_family::beta_inv_kernel(
            1.0 - p,
            0.5,
            v / 2.0,
            0.0,
            1.0,
        )?;
        excel_sqrt_staged(excel_x87_div(excel_x87_mul(v, y), excel_x87_sub(1.0, y)))
    };
    Ok(sign * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Full-bank rows (Excel 16.0.20527) where Excel publishes the correctly rounded root and the
    /// lattice search on the binary64 forward was one unit off; the double-double polish matches.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn large_shape_inverses_publish_the_correctly_rounded_root() {
        let chi = [
            (0.05949171, f64::from_bits(0x4052_0000_0000_0001), 0x4056_e618_a857_ec13_u64),
            (0.42169944, 100.0, 0x4059_8927_f500_71cc),
        ];
        for (p, df, excel) in chi {
            assert_eq!(chisq_inv_rt_kernel(p, df).unwrap().to_bits(), excel, "CHIINV({p}, {df})");
        }
        let unit = crate::ported::beta_gamma_stats_family::gamma_inv_kernel(
            f64::from_bits(0x3fbb_c819_39e8_ff40),
            100.0,
            f64::from_bits(0x3fe1_4a8f_0049_03dc),
        );
        assert_eq!(unit.unwrap().to_bits(), 0x4047_bc14_af00_45bc);
    }

    /// Excel 16.0.20527 full-bank rows (dc-6332.18) at degrees 8.9e10 to 3.4e22 where Excel publishes
    /// Goldstein's approximation and the old route refused with #NUM!, then fresh Touchstone answers
    /// (2026-11-17, fn9) at non-integer degrees 9e10 to 1.6e17, which Excel truncates.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn large_degree_right_tail_is_goldsteins_approximation() {
        for (p, df, excel) in [
            (0.11070131_f64, 0x43fa_85f8_1b35_198c_u64, 0x43fa_87c6_2b7b_7a57_u64),
            (f64::from_bits(0x3f65_4b16_7c9e_eb00), 0x4341_fc91_497f_ccf2, 0x4341_fdca_ad94_65e9),
            (f64::from_bits(0x3fd5_8281_8955_c26e), 0x4402_5663_9601_09c8, 0x4402_57a3_0b0a_6480),
            (f64::from_bits(0x3fda_a23c_4870_5910), 0x449c_ec8e_37ce_60c0, 0x449c_ee86_1aa8_cf0d),
            (f64::from_bits(0x3f76_4ee1_b26e_3a80), 0x43e5_4ccd_792a_4678, 0x43e5_4e40_8adf_a4db),
            (f64::from_bits(0x3fb8_d71b_e79b_41d8), 0x4397_13b9_1ee5_b028, 0x4397_154b_2684_5777),
            (f64::from_bits(0x3fc8_a5d5_ba5d_f914), 0x4448_9108_2ade_fcc8, 0x4448_92b4_2430_8042),
            (f64::from_bits(0x3fd8_cdc7_eddc_570a), 0x43d1_3b91_ce4c_21a6, 0x43d1_3cbe_045b_5c58),
            (f64::from_bits(0x3fda_b73e_2bf5_86bc), 0x43e4_246f_50ec_f36c, 0x43e4_25ce_3744_d962),
            (f64::from_bits(0x3fea_2137_c1fa_f303), 0x4387_4e58_7cfc_85d8, 0x4387_4fee_7fad_833c),
            (f64::from_bits(0x3fe0_4834_c6d4_b9b8), 0x4234_a8a1_cbb6_2890, 0x4234_aa09_8caa_8d7b),
            (f64::from_bits(0x3fef_b953_ca59_65e5), 0x4235_68a8_776c_72db, 0x4235_6a0d_d880_08e5),
            (f64::from_bits(0x3fef_5a9c_3e49_81cf), 0x4238_dc09_67c2_e0f9, 0x4238_ddac_06f4_acef),
            (f64::from_bits(0x3fad_8482_bc0e_5cb1), 0x4243_1017_ae29_a0e8, 0x4243_116a_a6c6_5319),
            (f64::from_bits(0x3fa3_86c6_b5fc_af84), 0x425e_a996_05ef_015c, 0x425e_abb3_22dd_c316),
            (f64::from_bits(0x3f6c_5f21_fafb_ae3c), 0x42b1_a6f4_4253_98de, 0x42b1_a828_c7d6_9637),
            (f64::from_bits(0x3fbb_4143_b3a2_169c), 0x4376_410b_84d9_c2fa, 0x4376_428f_3738_0314),
        ] {
            let df = f64::from_bits(df);
            assert_eq!(chisq_inv_rt_kernel(p, df).unwrap().to_bits(), excel, "CHISQ.INV.RT({p}, {df})");
        }
    }

    /// FDIST and F.DIST on either side of `d1 f = d2`, in the series and recurrence arms and with
    /// a unit shape: Excel 16.0.20527's answers (the Excel answer service, 2026-11-12; dc-6332.8).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn f_tails_match_excel() {
        let rows = [
            (
                1.3,
                5.0,
                6.0,
                0x3fd7f771c9bb0b34_u64,
                0x3fe404471b227a66_u64,
            ),
            (0.7, 5.0, 6.0, 0x3fe49996497df1d1, 0x3fd6ccd36d041c5e),
            (2.5, 4.0, 9.0, 0x3fbde107cdec13bc, 0x3fec43df06427d88),
            (0.9, 3.0, 5.0, 0x3fe01749ebfa7d62, 0x3fdfd16c280b053b),
            (1.0, 6.0, 8.0, 0x3fdf0e2c0bbdd318, 0x3fe078e9fa211674),
            (0.5, 7.0, 2.0, 0x3fe96bef85edc1ae, 0x3fca5041e848f949),
            (3.0, 2.0, 7.0, 0x3fbd53f30c889855, 0x3fec55819e6eecf6),
            (1.1, 4.0, 4.0, 0x3fddb74ca74b539e, 0x3fe12459ac5a5631),
        ];
        for (f, d1, d2, right, left) in rows {
            let rt = f_dist_rt_kernel(f, d1, d2).unwrap();
            assert_eq!(rt.to_bits(), right, "FDIST({f}, {d1}, {d2})");
            let cdf = f_dist_kernel(f, d1, d2, true).unwrap();
            assert_eq!(cdf.to_bits(), left, "F.DIST({f}, {d1}, {d2}, TRUE)");
        }
    }
    use crate::value::WorksheetErrorCode;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn chisq_df1_matches_published_erf_erfc_sqrt_half_x() {
        use crate::ported::special_dist_family::{erf_of_sqrt_half_x, erfc_of_sqrt_half_x};
        for x in [0.0, 0.25, 0.5, 1.0, 2.0, 3.841458820694124, 10.0, 20.0] {
            let rt = chisq_dist_rt_kernel(x, 1.0).unwrap();
            let cdf = chisq_dist_kernel(x, 1.0, true).unwrap();
            assert_eq!(
                rt.to_bits(),
                erfc_of_sqrt_half_x(x).unwrap().to_bits(),
                "rt x={x}"
            );
            assert_eq!(
                cdf.to_bits(),
                erf_of_sqrt_half_x(x).unwrap().to_bits(),
                "cdf x={x}"
            );
        }
        // Exact publication at zero does not depend on the ERFC body.
        assert_eq!(
            chisq_dist_rt_kernel(0.0, 1.0).unwrap().to_bits(),
            0x3ff0000000000000
        );
        // Live Excel 16.0 b20228 CHIDIST(1,1) is 0x3fd44ed0bb7cb209.
        // The current ERFC.PRECISE body is still one ULP off that witness;
        // the df=1 dispatch only claims the composition, not the ERFC body.
        for x in [0.0, 0.125, 0.5, 1.0, 2.0, 8.0, 16.0, 40.0] {
            let rt = chisq_dist_rt_kernel(x, 2.0).unwrap();
            let exp = crate::ported::excel_numeric::excel_exp(-(x / 2.0));
            assert_eq!(rt.to_bits(), exp.to_bits(), "df=2 rt x={x}");
            let cdf = chisq_dist_kernel(x, 2.0, true).unwrap();
            let expon =
                crate::ported::discrete_dist::expon_dist_kernel(x / 2.0, 1.0, true).unwrap();
            assert_eq!(cdf.to_bits(), expon.to_bits(), "df=2 cdf x={x}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn even_df_chidist_matches_poisson_cdf_identity() {
        // Live Excel 16.0 b20228: CHIDIST(x, 2(k+1)) == POISSON.DIST(k, x/2, TRUE).
        // After the Poisson CDF dispatch, OxFunc reproduces that identity.
        for i in 1..40 {
            let x = 0.5 * f64::from(i);
            let chi4 = chisq_dist_rt_kernel(x, 4.0).unwrap();
            let pois1 =
                crate::ported::discrete_dist::poisson_dist_kernel(1.0, x / 2.0, true).unwrap();
            assert_eq!(chi4.to_bits(), pois1.to_bits(), "df=4 x={x}");
            let chi6 = chisq_dist_rt_kernel(x, 6.0).unwrap();
            let pois2 =
                crate::ported::discrete_dist::poisson_dist_kernel(2.0, x / 2.0, true).unwrap();
            assert_eq!(chi6.to_bits(), pois2.to_bits(), "df=6 x={x}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn chisq_cdf_matches_gamma_scale_two_identity() {
        for df in [1.0, 2.0, 3.0, 4.0, 5.0, 10.0] {
            for x in [0.0, 0.5, 1.0, 2.0, 8.0, 20.0] {
                let chi = chisq_dist_kernel(x, df, true).unwrap();
                let gam = crate::ported::beta_gamma_stats_family::gamma_dist_kernel(
                    x,
                    df / 2.0,
                    2.0,
                    true,
                )
                .unwrap();
                assert_eq!(chi.to_bits(), gam.to_bits(), "df={df} x={x}");
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn chisq_inv_matches_gamma_inv_scale_two_identity() {
        for df in [1.0, 2.0, 3.0, 4.0, 5.0, 10.0] {
            for p in [0.01, 0.05, 0.25, 0.5, 0.9, 0.99] {
                let chi = chisq_inv_kernel(p, df).unwrap();
                let gam =
                    crate::ported::beta_gamma_stats_family::gamma_inv_kernel(p, df / 2.0, 2.0)
                        .unwrap();
                assert_eq!(chi.to_bits(), gam.to_bits(), "df={df} p={p}");
            }
        }
        assert_eq!(chisq_inv_kernel(0.0, 2.0), Ok(0.0));
        assert_eq!(chisq_inv_kernel(1.0, 2.0), Err(WorksheetErrorCode::Num));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn chisq_dist_rt_df2_matches_live_excel_exp_pins() {
        // Live Excel 16.0 b20326: CHISQ.DIST.RT(x,2)=EXP(-x/2) 7/7.
        assert_eq!(
            chisq_dist_rt_kernel(1.0, 2.0).unwrap().to_bits(),
            0x3fe368b2fc6f960a
        );
        assert_eq!(
            chisq_dist_rt_kernel(2.0, 2.0).unwrap().to_bits(),
            0x3fd78b56362cef38
        );
        assert_eq!(
            chisq_dist_rt_kernel(10.0, 2.0).unwrap().to_bits(),
            0x3f7b993fe00d5376
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn t_inv_2t_df1_matches_live_excel_tan_pins() {
        // Live Excel 16.0 build 20326 / CV2, Value2 cell-ref injection.
        // T.INV.2T(p,1)=1/TAN(PI()*p/2), with p=0.5 published as exact 1.
        let rows: [(f64, u64); 9] = [
            (0.001_f64, 0x4083e4f438b2cde9),
            (0.01_f64, 0x404fd410182c3c38),
            (0.02_f64, 0x403fd20d55634e2e),
            (0.05_f64, 0x40296993aacc4d25),
            (0.1_f64, 0x4019414813ba662c),
            (0.2_f64, 0x40089f188bdcd7b0),
            (0.3_f64, 0x3fff66da45fee3f1),
            (0.4_f64, 0x3ff605a90c73ab79),
            (0.5_f64, 0x3ff0000000000000),
        ];
        for (p, bits) in rows {
            assert_eq!(
                t_inv_2t_kernel(p, 1.0).unwrap().to_bits(),
                bits,
                "T.INV.2T({p},1)"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn t_inv_df1_matches_two_tail_double_prob() {
        for p in [0.6, 0.75, 0.9, 0.95, 0.99, 0.995] {
            let one = t_inv_kernel(p, 1.0).unwrap();
            let two = t_inv_2t_kernel(2.0 * (1.0 - p), 1.0).unwrap();
            assert_eq!(one.to_bits(), two.to_bits(), "T.INV({p},1) vs 2T");
            let neg = t_inv_kernel(1.0 - p, 1.0).unwrap();
            assert_eq!(one.to_bits(), (-neg).to_bits(), "T.INV oddness p={p}");
        }
        // Live Excel 16.0 b20326 Value2 T.INV(p,1)
        let pins: [(f64, u64); 8] = [
            (0.55, 0x3fc445f0fbb1cf9c),
            (0.6, 0x3fd4cb7bfb4961b0),
            (0.75, 0x3ff0000000000000),
            (0.9, 0x40089f188bdcd7b1),
            (0.95, 0x4019414813ba6625),
            (0.975, 0x40296993aacc4d1e),
            (0.99, 0x403fd20d55634e26),
            (0.995, 0x404fd410182c3c30),
        ];
        for (p, bits) in pins {
            assert_eq!(
                t_inv_kernel(p, 1.0).unwrap().to_bits(),
                bits,
                "T.INV({p},1)"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn chisq_family_matches_known_rows() {
        assert!((chisq_dist_kernel(0.5, 1.0, true).unwrap() - 0.520_499_88).abs() < 1e-8);
        assert!(
            (chisq_dist_kernel(2.0, 3.0, false).unwrap() - 0.207_553_748_710_297).abs() < 1e-10
        );
        assert!((chisq_dist_rt_kernel(18.307, 10.0).unwrap() - 0.050_000_589_130_671).abs() < 1e-8);
        assert!((chisq_inv_rt_kernel(0.0500006, 10.0).unwrap() - 18.307).abs() < 1e-4);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn f_family_matches_known_rows() {
        assert!((f_dist_kernel(15.2069, 6.0, 4.0, true).unwrap() - 0.99).abs() < 5e-5);
        assert!((f_dist_kernel(15.2069, 6.0, 4.0, false).unwrap() - 0.001_223_8).abs() < 5e-7);
        assert!((f_inv_rt_kernel(0.01, 6.0, 4.0).unwrap() - 15.20686).abs() < 1e-4);
        assert!((f_inv_kernel(0.99, 6.0, 4.0).unwrap() - 15.20686).abs() < 1e-4);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn t_dist_df2_pdf_matches_live_excel_pow_chain_pins() {
        let pins = [
            (-8.0_f64, 0x3f5e8e78c21b35f0u64),
            (-4.0, 0x3f8ad1536fff177d),
            (-2.0, 0x3fb16b28f55d72d3),
            (-1.0, 0x3fc8a2345cc04426),
            (-0.5, 0x3fd2f684bda12f68),
            (0.0, 0x3fd6a09e667f3bcd),
            (0.5, 0x3fd2f684bda12f68),
            (1.0, 0x3fc8a2345cc04426),
            (2.0, 0x3fb16b28f55d72d3),
            (4.0, 0x3f8ad1536fff177d),
            (8.0, 0x3f5e8e78c21b35f0),
            (0.1, 0x3fd67571ae3ae506),
            (3.0, 0x3f9c11662a4daec1),
            (10.0, 0x3f4fcf18ff68ff16),
            (0.25, 0x3fd59b52a2911afa),
        ];
        for (x, bits) in pins {
            assert_eq!(
                t_dist_kernel(x, 2.0, false).unwrap().to_bits(),
                bits,
                "x={x}"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn t_dist_df1_pdf_matches_live_excel_pins() {
        assert_eq!(
            t_dist_kernel(0.0, 1.0, false).unwrap().to_bits(),
            0x3fd45f306dc9c883
        );
        assert_eq!(
            t_dist_kernel(1.0, 1.0, false).unwrap().to_bits(),
            0x3fc45f306dc9c883
        );
        assert_eq!(
            t_dist_kernel(0.5, 1.0, false).unwrap().to_bits(),
            0x3fd04c26be3b06cf
        );
        assert_eq!(
            t_dist_kernel(-2.0, 1.0, false).unwrap().to_bits(),
            0x3fb04c26be3b06cf
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn t_family_matches_known_rows() {
        assert!((t_dist_kernel(60.0, 1.0, true).unwrap() - 0.994_695_33).abs() < 1e-8);
        assert!((t_dist_kernel(8.0, 3.0, false).unwrap() - 0.000_736_91).abs() < 1e-8);
        assert!((t_dist_2t_kernel(1.959_999_998, 60.0).unwrap() - 0.054_644_93).abs() < 1e-7);
        assert!((t_dist_rt_kernel(1.959_999_998, 60.0).unwrap() - 0.027_322_465).abs() < 1e-8);
        assert!((t_inv_2t_kernel(0.054_644_93, 60.0).unwrap() - 1.959_999_998).abs() < 1e-6);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn compatibility_aliases_follow_modern_functions() {
        assert!((chisq_dist_rt_kernel(18.307, 10.0).unwrap() - 0.050_000_589_130_671).abs() < 1e-8);
        assert!((f_dist_rt_kernel(15.2069, 6.0, 4.0).unwrap() - 0.01).abs() < 5e-5);
        assert!((t_dist_2t_kernel(1.959_999_998, 60.0).unwrap() - 0.054_644_93).abs() < 1e-7);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn invalid_domains_return_num_errors() {
        assert_eq!(
            chisq_dist_kernel(-1.0, 1.0, true),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            f_dist_kernel(-1.0, 1.0, 1.0, true),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(t_dist_2t_kernel(-1.0, 10.0), Err(WorksheetErrorCode::Num));
        assert_eq!(
            t_dist_rt_kernel(-1.0, 10.0).unwrap().to_bits(),
            0x3fea8bb3a4954e72
        );
    }

    // BUG-FUNC-039 item 1: x=0 divergence published as #NUM!, not +inf.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn chisq_pdf_x0_df1_returns_num_not_inf() {
        assert_eq!(chisq_pdf_kernel(0.0, 1.0), Err(WorksheetErrorCode::Num));
        // df=2 at x=0 is finite (0.5); must not be broken by the fix.
        assert!((chisq_pdf_kernel(0.0, 2.0).unwrap() - 0.5).abs() < 1e-15);
        // df=3 at x=0 is 0.0.
        assert_eq!(chisq_pdf_kernel(0.0, 3.0), Ok(0.0));
    }

    // BUG-FUNC-039 item 1: F.DIST(0, d1<2, d2, FALSE) -> #NUM!.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn f_pdf_x0_d1lt2_returns_num_not_inf() {
        assert_eq!(f_pdf_kernel(0.0, 1.0, 5.0), Err(WorksheetErrorCode::Num));
        // d1=2 at x=0 is finite (1.0); must not be broken.
        assert!((f_pdf_kernel(0.0, 2.0, 5.0).unwrap() - 1.0).abs() < 1e-15);
        // d1=3 at x=0 is 0.0.
        assert_eq!(f_pdf_kernel(0.0, 3.0, 5.0), Ok(0.0));
    }

    /// Native and browser: a density whose gamma products overflow (half a degree of freedom
    /// past 171.6) is Excel's value, not #NUM!. Excel 16.0.20527 answers, full bank
    /// (bank2-answers-f.dist-scalar and -more); approximate to 1e-11 relative.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn f_pdf_with_overflowing_gamma_products_is_excel_s_value() {
        for (x, d1, d2, excel) in [
            (0.5, 365.0, 99.99999999999999, 0.0001292901188392102),
            (0.2013488237563844, 365.0, 95.9095690893189, 2.376064077949561e-26),
            (0.8522290673604944, 365.0, 9297.0, 0.7309842359669733),
            (6.112281326421112, 1.0, 449.4636342633071, 0.007696945090318107),
            (25.36225418055301, 365.0, 1.7203751039703308, 0.0030601731998629957),
            (515.6770269176984, 365.0, 88.00000000000001, 3.3021894813670246e-101),
            (2343.0, 365.0, 75.99999999999999, 2.863476397231786e-112),
            (1.0, 365.0, 947.0, 4.576463474759765),
        ] {
            let got = f_pdf_kernel(x, d1, d2).expect("a finite density");
            assert!(((got - excel) / excel).abs() < 1e-11, "F.DIST({x},{d1},{d2},0) = {got}, Excel {excel}");
        }
    }

    // BUG-FUNC-039 item 2: df exactly 1e10 must be admitted; df > 1e10 rejected.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn df_boundary_at_1e10_is_admitted() {
        // df = 1e10 exactly: should succeed (just needs to parse without error).
        assert!(chisq_dist_kernel(1.0, 1e10, false).is_ok());
        // df = 1e10 + 1 (next representable float above 1e10): must be rejected.
        let above = 1e10 + 1.0;
        assert_eq!(
            chisq_dist_kernel(1.0, above, false),
            Err(WorksheetErrorCode::Num)
        );
        // F family too.
        assert!(f_dist_kernel(1.0, 1e10, 1e10, false).is_ok());
        assert_eq!(
            f_dist_kernel(1.0, above, 1e10, false),
            Err(WorksheetErrorCode::Num)
        );
    }

    /// Excel 16.0.20527 (cb6, dc-j9k.60): df == 1 `erfinv(comp(q))^2` closed form,
    /// strict `q > 0.9999` complement switch, tiny upper `q` (`z = -NORM.S.INV(q)`),
    /// and `CHISQ.INV` holdout witnesses.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cb6_chisq_inv_rt_df1_complement_switch_and_tiny_upper_q() {
        let rt_rows: [(u64, u64, u64); 11] = [
            // df == 1 near-1 fold + fractional-df truncation holdouts.
            (0x3feffffffffffffe, 0x3ff0000000000000, 0x397921fb54442d19),
            (0x3fefffffffffffff, 0x3ff0000000000000, 0x395921fb54442d19),
            (0x3fefffffffffd7c9, 0x3ff3a152dd339d50, 0x3b03d8b07b1da78c),
            (0x3fefffffffffff70, 0x3ffbc7cce19b1d35, 0x3a3fcf0216a64913),
            // Strict q > 0.9999 complement switch: df = 4 across 0.9999 / 0.9999+1u and df = 2000 (Temme).
            (0x3fefff2e48e8a71e, 0x4010000000000000, 0x3f9d19bb9717c1b8),
            (0x3fefff2e48e8a71f, 0x4010000000000000, 0x3f9d19bb9717bc64),
            (0x3fefff2e48e8a71f, 0x409f400000000000, 0x409bb52fa3ae47dd),
            // Tiny upper q (where 1 - q == 1.0) and df >= 3 frozen holdouts.
            (0x38bb38f0ef960dc8, 0x4034000000000000, 0x406b5a6d411f57e1),
            (0x3d35d7924095522a, 0x4030000000000000, 0x4058883618de595b),
            (0x3fef13614fc4170d, 0x40435dce4a74a3be, 0x4037445ebe498e44),
            (0x3fef595e5de11e6c, 0x4046800000000000, 0x403bc4a1ff79a362),
        ];
        for (q, df, expected) in rt_rows {
            let got = chisq_inv_rt_kernel(f64::from_bits(q), f64::from_bits(df)).unwrap();
            assert_eq!(got.to_bits(), expected, "CHISQ.INV.RT({q:#x}, {df:#x})");
        }
        // Excel 16.0.20527 returns 0x4050d163896118e4 for CHISQ.INV.RT(0x3fefffffe9457792, 139);
        // with the Excel-proven |next - x| <= f64::EPSILON * x stop (dc-j9k.64), a +1 ULP forward
        // residual in gratio_series(69.5, 33.63585026673757) nudges iteration 1 across the half-ULP
        // rounding boundary, leaving this row at -1 ULP (0x4050d163896118e3; routed to dc-j9k.59).
        let got_139 = chisq_inv_rt_kernel(
            f64::from_bits(0x3fefffffe9457792),
            f64::from_bits(0x4061600000000000),
        )
        .unwrap();
        assert!(
            got_139.to_bits().abs_diff(0x4050d163896118e4) <= 1,
            "CHISQ.INV.RT(0x3fefffffe9457792, 139) got {:#018x}, Excel 0x4050d163896118e4",
            got_139.to_bits()
        );
        // df == 1 where comp(q) rounds to 1.0 -> #NUM!.
        assert_eq!(chisq_inv_rt_kernel(1e-20, 1.0), Err(WorksheetErrorCode::Num));

        let inv_rows: [(u64, u64, u64); 4] = [
            (0x3472bafced8d0a8e, 0x3ff174d95c369aae, 0x290138a4bd28394a),
            (0x3fe25565b97f7e19, 0x403e4b07dbe00c39, 0x403ec3943d55c9ed),
            (0x3feef9d11402353a, 0x404d800000000000, 0x40542ac639a8a117),
            (0x3fefffffff18b680, 0x406d000000000000, 0x4077ec71d6fe5247),
        ];
        for (p, df, expected) in inv_rows {
            let got = chisq_inv_kernel(f64::from_bits(p), f64::from_bits(df)).unwrap();
            assert_eq!(got.to_bits(), expected, "CHISQ.INV({p:#x}, {df:#x})");
        }
    }

    /// Excel 16.0.20527.20196 frozen unseen holdouts (Carbon cb13, dc-j9k.83):
    /// saddle-point `f_pdf_kernel` (large-df sawtooth, df > 1_048_576, general grid,
    /// exact x=1 symmetry) and signed `lam` in `excel_beta_fraction` (`sm + lg = 1 + 2^-52`).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cb13_f_pdf_saddle_point_and_bfrac_signed_lam_match_excel() {
        let pdf_rows: [(u64, u64, u64, u64); 14] = [
            // holdout_fpdf_sawtooth_large (d1=31247, d2=9413; baseline off 133..9,190 ULP)
            (0x3fe8b86bcf8e4edf, 0x40de83c000000000, 0x40c2628000000000, 0x34e27fdbf453f313),
            (0x3fe8b86bcf8e4ee1, 0x40de83c000000000, 0x40c2628000000000, 0x34e27fdbf453d752),
            (0x3fe8b86bcf8e4ee2, 0x40de83c000000000, 0x40c2628000000000, 0x34e27fdbf4540ed1),
            (0x3fe8b86bcf8e4ee3, 0x40de83c000000000, 0x40c2628000000000, 0x34e27fdbf45400f0),
            // holdout_fpdf_huge_df_num (df in 2.5e6..1e9; baseline returned #NUM!)
            (0x3ff5eb851eb851ec, 0x414312d000000000, 0x4014000000000000, 0x3fd97d5769dc73a7),
            (0x3ff1eb851eb851ec, 0x415312d000000000, 0x4028000000000000, 0x3fea88393db6be01),
            (0x3fe3d70a3d70a3d7, 0x4008000000000000, 0x4187d78400000000, 0x3fe49bbee997445d),
            (0x3ff9c28f5c28f5c3, 0x4187d78400000000, 0x4008000000000000, 0x3fcfc6f3b5e9b73b),
            // holdout_fpdf_general_grid (small/medium df; baseline off 1..49 ULP)
            (0x3fc0a3d70a3d70a4, 0x3ff0000000000000, 0x3ff0000000000000, 0x3fe90025a3f21faf),
            (0x3fd7ae147ae147ae, 0x3ff0000000000000, 0x4014000000000000, 0x3fe01ec4b4f897e4),
            (0x3ff30a3d70a3d70a, 0x3ff0000000000000, 0x4031000000000000, 0x3fc9172bdb51c900),
            (0x400dae147ae147ae, 0x3ff0000000000000, 0x4050400000000000, 0x3fa0ea9249b8b347),
            // holdout_fpdf_sym_and_recip (exact x=1 symmetry; baseline off 26..27 ULP)
            (0x3ff0000000000000, 0x4008000000000000, 0x401c000000000000, 0x3fd89791e889395b),
            (0x3ff0000000000000, 0x401c000000000000, 0x4008000000000000, 0x3fd89791e889395b),
        ];
        for (x, d1, d2, expected) in pdf_rows {
            let got = f_dist_kernel(f64::from_bits(x), f64::from_bits(d1), f64::from_bits(d2), false)
                .unwrap();
            assert_eq!(
                got.to_bits(),
                expected,
                "F.DIST({x:#018x}, {d1:#018x}, {d2:#018x}, FALSE)"
            );
        }

        // holdout_bfrac_signed_lam (sm + lg = 1 + 2^-52; baseline off 20..40 ULP)
        assert_eq!(
            f_dist_kernel(
                f64::from_bits(0x3ff0000000000001),
                f64::from_bits(0x4064c00000000000),
                f64::from_bits(0x4068a00000000000),
                true,
            )
            .unwrap()
            .to_bits(),
            0x3fe00ddf7fa2d126
        );
        assert_eq!(
            f_dist_kernel(
                f64::from_bits(0x3fefffffffffffff),
                f64::from_bits(0x407cf00000000000),
                f64::from_bits(0x407ee00000000000),
                true,
            )
            .unwrap()
            .to_bits(),
            0x3fe0033aa6341574
        );
        assert_eq!(
            f_dist_rt_kernel(
                f64::from_bits(0x3fefffffffffffff),
                f64::from_bits(0x407cf00000000000),
                f64::from_bits(0x407ee00000000000),
            )
            .unwrap()
            .to_bits(),
            0x3fdff98ab397d517
        );
    }

    /// Excel 16.0.20527 (dc-iuwct.53 / ww2): F.DIST(..., FALSE) admits degrees through 1e10
    /// (returning #NUM! at 1e10 + 1) and matches Excel's saddle-point density across the
    /// formerly structural degree controls and gross mass rows without gross/structural errors.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_iuwct_53_f_dist_pdf_matches_excel_across_degrees_and_masses() {
        let rows: [(u64, u64, u64, u64); 11] = [
            // Large d2 > 1_048_576 (formerly #NUM!) through d2 = 1e10 and swapped d1 > 1_048_576.
            (0x3fe749edb3026b93, 0x4014000000000000, 0x41a8f3fad2000000, 0x3fe7f18a473e4dd9),
            (0x3fe749edb3026b93, 0x4014000000000000, 0x4130000100000000, 0x3fe7f1873eda1937),
            (0x3fe749edb3026b93, 0x4014000000000000, 0x4202a05f20000000, 0x3fe7f18a4b12067c),
            (0x3fe749edb3026b93, 0x41a8f3fad2000000, 0x4014000000000000, 0x3fe74f02dab6e564),
            // Small-gamma gross row ($[2812], d1=2, d2=329) and large-degree mass rows.
            (0x3fb1c35a0f314f38, 0x4000000000000000, 0x4074900000000000, 0x3fedd7be14e7951e),
            (0x3feae517bf442825, 0x40d8f84000000000, 0x40c1428000000000, 0x3ba2cb2c8ee6d2a7),
            (0x3fdb9af843d2bf10, 0x40c0948000000000, 0x40b8f00000000000, 0x059300786b2a69e0),
            (0x3fdfecdefeed1315, 0x40bbec0000000000, 0x40b9250000000000, 0x1b9b459c13b59e94),
            (0x3fe66c65ade4194a, 0x40d8f84000000000, 0x40b4610000000000, 0x32e44803a69ef6c6),
            // Adjacent-x discriminant controls (r < 0.375 vs r >= 0.375).
            (0x3fefffffffffffff, 0x4076d00000000000, 0x40e5f90000000000, 0x40157600e86856ab),
            (0x3fdb9af843d2bf10, 0x40c0950000000000, 0x40b8f00000000000, 0x0592315d8d4f2157),
        ];
        for (x_bits, d1_bits, d2_bits, excel) in rows {
            let got = f_dist_kernel(
                f64::from_bits(x_bits),
                f64::from_bits(d1_bits),
                f64::from_bits(d2_bits),
                false,
            )
            .expect("finite F.DIST PDF");
            let diff = got.to_bits().abs_diff(excel);
            assert!(
                diff <= 7,
                "F.DIST({x_bits:#018x}, {d1_bits:#018x}, {d2_bits:#018x}, FALSE) got {:#018x}, Excel {excel:#018x} ({diff} ULP)",
                got.to_bits()
            );
        }
        // d2 = 1e10 + 1 -> #NUM! (ww2 confirmed all 26 degree controls return #NUM! above 1e10).
        assert_eq!(
            f_dist_kernel(f64::from_bits(0x3fe749edb3026b93), 5.0, 1e10 + 1.0, false),
            Err(WorksheetErrorCode::Num)
        );
    }
}
