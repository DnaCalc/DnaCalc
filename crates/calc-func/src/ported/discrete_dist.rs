//! EXPON.DIST, BINOM.DIST, BINOM.DIST.RANGE, BINOM.INV, POISSON.DIST, HYPGEOM.DIST and
//! NEGBINOM.DIST: a clean-room implementation (dc-96l.3), written without reading the code it
//! replaces, from the published method and from measurements of Excel.
//!
//! # Sources
//!
//! - C. Loader, "Fast and Accurate Computation of Binomial Probabilities" (2000): the saddle-point
//!   form of the binomial mass, `exp(lc) / sqrt(2 pi x (n - x) / n)` with
//!   `lc = stirling_error(n) - stirling_error(x) - stirling_error(n - x) - deviance(x, np) - deviance(n - x, nq)`; the
//!   deviance term `deviance(x, np) = x ln(x/np) + np - x`, summed as its series in
//!   `v = (x - np)/(x + np)` when `|x - np| <= 0.1 (x + np)` at positive finite boundaries; the Stirling error
//!   `stirling_error(n) = ln n! - (n + 1/2) ln n + n - ln sqrt(2 pi)`, for `n > 15` from its asymptotic
//!   series `1/(12n) - 1/(360n^3) + 1/(1260n^5) - 1/(1680n^7) + 1/(1188n^9)` cut at 35, 80 and
//!   500, and for the half-integers to 15 from its value (computed here to binary64 from the
//!   definition with a high-precision log-gamma); the Poisson mass
//!   `exp(-stirling_error(x) - deviance(x, lambda)) / sqrt(2 pi x)`; the hypergeometric mass as a ratio of
//!   three binomial masses, and the negative binomial as `s/(s+f)` times a binomial mass.
//! - The cumulative functions sum the masses; BINOM.INV is the smallest count whose cumulative
//!   probability reaches alpha.
//!
//! # What was measured in Excel (16.0, 64-bit, 2026-10-30)
//!
//! The argument rules (truncation of counts, which out-of-range arguments are #NUM! and which give
//! 0 or 1), the special cases at p = 0 and p = 1 (BINOM.DIST(0,10,1,FALSE) is #NUM!), the use of
//! Excel's EXP and LN chains, the exponential distribution's cumulative form `-expm1(-lambda x)`,
//! and the binomial mass's final step: `lc - lf/2` formed in extended precision before the
//! exponential, `lf = ln(2 pi x (n - x) / n)`, and `n ln p` at x = n. Excel's masses agree with these to within a few
//! units in the last place; about half are bit-identical (the rest are dc-j9k's to find).

use crate::ported::excel_numeric::{crt_log, excel_binom_pmf_exp, excel_exp, excel_expm1_spilled, excel_log};
use crate::value::WorksheetErrorCode;

type R = Result<f64, WorksheetErrorCode>;
const NUM: WorksheetErrorCode = WorksheetErrorCode::Num;

/// `ln(sqrt(2 pi))`, rounded to binary64.
const LN_SQRT_2PI: f64 = 0.9189385332046728;

/// `stirling_error(k / 2)` for k = 0..30, rounded to binary64 from the definition.
const STIRLING_ERROR_HALVES: [f64; 31] = [
    0.0,
    0.15342640972002736,
    0.08106146679532726,
    0.05481412105191765,
    0.0413406959554093,
    0.03316287351993629,
    0.02767792568499834,
    0.023746163656297496,
    0.020790672103765093,
    0.018488450532673187,
    0.016644691189821193,
    0.015134973221917378,
    0.013876128823070748,
    0.012810465242920227,
    0.01189670994589177,
    0.011104559758206917,
    0.010411265261972096,
    0.009799416126158804,
    0.009255462182712733,
    0.008768700134139386,
    0.00833056343336287,
    0.00793411456431402,
    0.007573675487951841,
    0.007244554301320383,
    0.00694284010720953,
    0.006665247032707682,
    0.006408994188004207,
    0.006171712263039458,
    0.0059513701127588475,
    0.0057462165130101155,
    0.005554733551962801,
];

fn stirling_error(n: f64) -> f64 {
    // The 15-digit leading coefficient also used by the retained NSWC Stirling
    // expansions distinguishes recorded interior mass oc16921 from RN(1/12).
    // This is a finite-precision model parameter, not the exact Bernoulli value.
    const S0: f64 = 0.0833333333333333;
    const S1: f64 = 1.0 / 360.0;
    const S2: f64 = 1.0 / 1260.0;
    const S3: f64 = 1.0 / 1680.0;
    const S4: f64 = 1.0 / 1188.0;
    if n <= 15.0 {
        let nn = n + n;
        if nn == nn.trunc() {
            return STIRLING_ERROR_HALVES[nn as usize];
        }
        return crate::ported::excel_numeric::gammaln_excel(n + 1.0) - (n + 0.5) * excel_log(n) + n - LN_SQRT_2PI;
    }
    let nn = n * n;
    if n > 500.0 {
        return (S0 - S1 / nn) / n;
    }
    if n > 80.0 {
        return (S0 - (S1 - S2 / nn) / nn) / n;
    }
    if n > 35.0 {
        return (S0 - (S1 - (S2 - S3 / nn) / nn) / nn) / n;
    }
    (S0 - (S1 - (S2 - (S3 - S4 / nn) / nn) / nn) / nn) / n
}

fn deviance(x: f64, np: f64) -> f64 {
    // The recorded negative-binomial CDF boundary is admitted to this series.
    let difference = (x - np).abs();
    let boundary = 0.1 * (x + np);
    if difference < boundary
        || (difference == boundary && difference > 0.0 && difference.is_finite())
    {
        let mut v = (x - np) / (x + np);
        // Preserve the PC64 product before the initial series sum is stored.
        let mut s = crate::ported::excel_numeric::excel_x87_mul(x - np, v);
        let mut ej = 2.0 * x * v;
        v *= v;
        let mut j = 1.0;
        loop {
            ej *= v;
            let s1 = s + ej / (2.0 * j + 1.0);
            if s1 == s {
                return s1;
            }
            s = s1;
            j += 1.0;
        }
    }
    x * excel_log(x / np) + np - x
}

/// The binomial mass of `x` successes in `n` trials, `0 < p < 1` (Loader's saddle-point form).
fn binom_mass(x: f64, n: f64, p: f64, q: f64) -> f64 {
    if x == 0.0 {
        if n == 0.0 {
            return 1.0;
        }
        let lc = if p < 0.1 { -deviance(n, n * q) - n * p } else { n * excel_log(q) };
        return excel_exp(lc);
    }
    if x == n {
        // Measured: Excel takes n ln p here for every p (2,809 of 2,811 oracle rows with p > 0.9),
        // where Loader's deviance form is used only at x = 0.
        return excel_exp(n * excel_log(p));
    }
    if x < 0.0 || x > n {
        return 0.0;
    }
    let lc = stirling_error(n) - stirling_error(x) - stirling_error(n - x) - deviance(x, n * p) - deviance(n - x, n * q);
    // The recorded interior masses distinguish the internal CRT logarithm here
    // from the worksheet logarithm used in the deviance and endpoint branches.
    let lf = crt_log(2.0 * core::f64::consts::PI * x * (n - x) / n);
    excel_binom_pmf_exp(lc, lf)
}

/// The Poisson mass at `x` for `lambda > 0`.
fn poisson_mass(x: f64, lambda: f64) -> f64 {
    if x == 0.0 {
        return excel_exp(-lambda);
    }
    excel_exp(-stirling_error(x) - deviance(x, lambda)) / (2.0 * core::f64::consts::PI * x).sqrt()
}

/// A finite result, with a subnormal one flushed to zero as Excel computes (EXPON.DIST(745,1,FALSE)
/// is 0); otherwise #NUM!.
fn finite(v: f64) -> R {
    if v.is_subnormal() {
        Ok(0.0)
    } else if v.is_finite() {
        Ok(v)
    } else {
        Err(NUM)
    }
}

/// `EXPON.DIST(x, lambda, cumulative)`.
pub fn expon_dist_kernel(x: f64, lambda: f64, cumulative: bool) -> R {
    use crate::ported::excel_numeric::excel_x87_mul;
    if x < 0.0 || lambda <= 0.0 {
        return Err(NUM);
    }
    let mut t = excel_x87_mul(lambda, x);
    if !t.is_finite() {
        return Err(NUM);
    }
    if t.is_subnormal() {
        t = 0.0;
    }
    finite(if cumulative { -excel_expm1_spilled(-t) } else { excel_x87_mul(lambda, excel_exp(-t)) })
}

/// `BINOM.DIST(number_s, trials, probability_s, cumulative)`.
pub fn binom_dist_kernel(number_s: f64, trials: f64, p: f64, cumulative: bool) -> R {
    let (k, n) = (number_s.trunc(), trials.trunc());
    if n < 0.0 || k < 0.0 || k > n || !(0.0..=1.0).contains(&p) {
        return Err(NUM);
    }
    if p == 0.0 {
        return Ok(if cumulative || k == 0.0 { 1.0 } else { 0.0 });
    }
    if p == 1.0 {
        return if k == n {
            Ok(1.0)
        } else if cumulative {
            Ok(0.0)
        } else if k == 0.0 {
            Err(NUM)
        } else {
            Ok(0.0)
        };
    }
    let q = 1.0 - p;
    if !cumulative {
        return finite(binom_mass(k, n, p, q));
    }
    if k == n {
        return Ok(1.0);
    }
    let mut sum = 0.0;
    let mut i = 0.0;
    while i <= k {
        sum += binom_mass(i, n, p, q);
        i += 1.0;
    }
    finite(sum.min(1.0))
}

/// `BINOM.DIST.RANGE(trials, probability_s, number_s, [number_s2])`.
pub fn binom_dist_range_kernel(trials: f64, p: f64, number_s: f64, number_s2: Option<f64>) -> R {
    let n = trials.trunc();
    let s = number_s.trunc();
    let s2 = number_s2.map_or(s, f64::trunc);
    if n < 0.0 || !(0.0..=1.0).contains(&p) || s < 0.0 || s > n || s2 < s || s2 > n {
        return Err(NUM);
    }
    let mass = |i: f64| {
        if p == 0.0 {
            if i == 0.0 { 1.0 } else { 0.0 }
        } else if p == 1.0 {
            if i == n { 1.0 } else { 0.0 }
        } else {
            binom_mass(i, n, p, 1.0 - p)
        }
    };
    let mut sum = 0.0;
    let mut i = s;
    while i <= s2 {
        sum += mass(i);
        i += 1.0;
    }
    finite(sum.min(1.0))
}

/// `BINOM.INV(trials, probability_s, alpha)`: the smallest count whose cumulative binomial
/// probability is at least alpha.
pub fn binom_inv_kernel(trials: f64, p: f64, alpha: f64) -> R {
    let n = trials.trunc();
    if n < 0.0 || !(p > 0.0 && p < 1.0) || !(alpha > 0.0 && alpha < 1.0) {
        return Err(NUM);
    }
    let q = 1.0 - p;
    let mut sum = 0.0;
    let mut k = 0.0;
    while k < n {
        sum += binom_mass(k, n, p, q);
        if sum >= alpha {
            return Ok(k);
        }
        k += 1.0;
    }
    Ok(n)
}

/// `POISSON.DIST(x, mean, cumulative)`.
pub fn poisson_dist_kernel(x: f64, mean: f64, cumulative: bool) -> R {
    let x = x.trunc();
    if x < 0.0 || mean < 0.0 {
        return Err(NUM);
    }
    if mean == 0.0 {
        return Ok(if cumulative || x == 0.0 { 1.0 } else { 0.0 });
    }
    // P(X=0) = P(X<=0) = exp(-mean). The recorded zero-count branch retains
    // its subnormal exponential, while the nonzero-count branches flush below normal.
    if x == 0.0 {
        return Ok(excel_exp(-mean));
    }
    if !cumulative {
        return finite(poisson_mass(x, mean));
    }
    // P(X <= x) = Q(x + 1, mean), the regularized upper incomplete gamma function (A&S 6.5.13,
    // 26.4.21): measured, Excel's POISSON.DIST(k, x/2, TRUE) equals its CHIDIST(x, 2(k+1)).
    finite(crate::ported::special_math_common::regularized_gamma_q(x + 1.0, mean))
}

/// The hypergeometric mass: `s` successes in a sample of `n` from `big_n` holding `k` successes,
/// as `b(s; k, n/N) b(n - s; N - k, n/N) / b(n; N, n/N)`.
fn hypergeom_mass(s: f64, n: f64, k: f64, big_n: f64) -> f64 {
    if s < 0.0 || s > n || s > k || n - s > big_n - k {
        return 0.0;
    }
    if n == 0.0 || n == big_n {
        return if s == (if n == 0.0 { 0.0 } else { k }) { 1.0 } else { 0.0 };
    }
    let p = n / big_n;
    let q = (big_n - n) / big_n;
    let a = binom_mass(s, k, p, q);
    let b = binom_mass(n - s, big_n - k, p, q);
    let c = binom_mass(n, big_n, p, q);
    a * b / c
}

/// `HYPGEOM.DIST(sample_s, number_sample, population_s, number_pop, cumulative)`.
pub fn hypergeom_dist_kernel(sample_s: f64, number_sample: f64, population_s: f64, number_pop: f64, cumulative: bool) -> R {
    let (s, n, k, big_n) = (sample_s.trunc(), number_sample.trunc(), population_s.trunc(), number_pop.trunc());
    if s < 0.0 || n < 0.0 || k < 0.0 || big_n <= 0.0 || n > big_n || k > big_n {
        return Err(NUM);
    }
    if !cumulative {
        return finite(hypergeom_mass(s, n, k, big_n));
    }
    if s >= n.min(k) {
        return Ok(1.0);
    }
    let mut sum = 0.0;
    let mut i = 0.0;
    while i <= s {
        sum += hypergeom_mass(i, n, k, big_n);
        i += 1.0;
    }
    finite(sum.min(1.0))
}

/// The negative binomial mass of `f` failures before the `s`-th success: `s/(s+f)` times the
/// binomial mass of `s` successes in `s + f` trials.
fn negbinom_mass(f: f64, s: f64, p: f64) -> f64 {
    let q = 1.0 - p;
    if f == 0.0 {
        return excel_exp(s * excel_log(p));
    }
    s / (s + f) * binom_mass(s, s + f, p, q)
}

/// `NEGBINOM.DIST(number_f, number_s, probability_s, cumulative)`.
pub fn negbinom_dist_kernel(number_f: f64, number_s: f64, p: f64, cumulative: bool) -> R {
    let (f, s) = (number_f.trunc(), number_s.trunc());
    if f < 0.0 || s < 1.0 || !(p > 0.0 && p < 1.0) {
        return Err(NUM);
    }
    if !cumulative {
        return finite(negbinom_mass(f, s, p));
    }
    let mut sum = 0.0;
    let mut i = 0.0;
    while i <= f {
        sum += negbinom_mass(i, s, p);
        i += 1.0;
    }
    finite(sum.min(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn argument_rules_match_excel() {
        assert_eq!(binom_dist_kernel(0.0, 10.0, 1.0, false), Err(NUM));
        assert_eq!(binom_dist_kernel(6.0, 10.0, 1.0, false), Ok(0.0));
        assert_eq!(binom_dist_kernel(6.0, 10.0, 0.0, true), Ok(1.0));
        assert_eq!(binom_dist_kernel(11.0, 10.0, 0.5, true), Err(NUM));
        assert_eq!(binom_inv_kernel(10.0, 1.0, 0.5), Err(NUM));
        assert_eq!(binom_inv_kernel(6.0, 0.5, 0.75), Ok(4.0));
        assert_eq!(poisson_dist_kernel(2.0, 0.0, false), Ok(0.0));
        assert_eq!(expon_dist_kernel(1e-300, 1e-10, true), Ok(0.0));
        assert_eq!(hypergeom_dist_kernel(5.0, 4.0, 8.0, 20.0, true), Ok(1.0));
        assert_eq!(negbinom_dist_kernel(10.0, 5.0, 1.0, false), Err(NUM));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn stirling_table_is_the_definition() {
        // stirling_error(1) = 1 - ln sqrt(2 pi); stirling_error(1/2) = ln sqrt(pi) - 1/2 ln(1/2) + 1/2 - ln sqrt(2 pi)
        assert!((stirling_error(1.0) - (1.0 - LN_SQRT_2PI)).abs() < 1e-16);
        // The asymptotic arm uses the recorded finite-precision coefficient;
        // the application-route oc16921 control distinguishes it from RN(1/12).
        assert_eq!(stirling_error(16.0), (0.0833333333333333 - (1.0 / 360.0 - (1.0 / 1260.0 - (1.0 / 1680.0 - (1.0 / 1188.0) / 256.0) / 256.0) / 256.0) / 256.0) / 16.0);
    }
}
