//! EXPON.DIST, BINOM.DIST, BINOM.DIST.RANGE, BINOM.INV, POISSON.DIST, HYPGEOM.DIST and
//! NEGBINOM.DIST: a clean-room implementation (dc-96l.3), written without reading the code it
//! replaces, from the published method and from measurements of Excel.
//!
//! # Sources
//!
//! - C. Loader, "Fast and Accurate Computation of Binomial Probabilities" (2000): the saddle-point
//!   form of the binomial and Poisson masses, the deviance `x ln(x/np) + np - x` and its series in
//!   `v = (x - np)/(x + np)`, the Stirling error `ln n! - (n + 1/2) ln n + n - ln sqrt(2 pi)`, the
//!   hypergeometric mass as a ratio of three binomial masses and the negative binomial as
//!   `s/(s+f)` times a binomial mass.
//! - The cumulative functions sum the masses; BINOM.INV is the smallest count whose cumulative
//!   probability reaches alpha.
//!
//! # What was measured in Excel (16.0, 64-bit)
//!
//! The argument rules (truncation of counts, which out-of-range arguments are #NUM! and which give
//! 0 or 1), the special cases at p = 0 and p = 1 (BINOM.DIST(0,10,1,FALSE) is #NUM!), the use of
//! Excel's EXP and LN chains, the exponential distribution's cumulative form `-expm1(-lambda x)`,
//! `n ln p` at x = n, and these stages of the masses, each read from Excel's answers (live windows
//! of adjacent doubles and the bank):
//!
//! - The stored exponent: with the normalizer removed, the answers sit on its binary64 grid.
//! - The Stirling error: a table to 6 cut to 16 decimals, ten terms of its series from 7.
//! - The deviance outside its series range: `x/np`, its logarithm, the product with `x` and `np - x`
//!   each stored, then summed.
//! - BINOM.DIST's exponent: `s(n) - s(x) - d(x) - s(n-x) - d(n-x)`, stored after each subtraction.
//! - BINOM.DIST's normalizer and POISSON.DIST's first route: the exponential of a stored difference
//!   with what the stores dropped put back as a factor (`excel_exp_running`,
//!   `excel_exp_compensated`).
//! - The deviance inside its series range: `excel_deviance_series`.
//!
//! The zero-success mass is not read: 100 rows of the bank, where `ln q` is not of the stored `q`.

use crate::ported::excel_numeric::{
    excel_deviance_series, excel_exp, excel_exp_compensated, excel_exp_running, excel_expm1_spilled, excel_log, excel_stirling_series, excel_x87_add, excel_x87_div,
    excel_x87_mul, excel_x87_sub, gammaln_excel,
};
use crate::value::WorksheetErrorCode;

type R = Result<f64, WorksheetErrorCode>;
const NUM: WorksheetErrorCode = WorksheetErrorCode::Num;

/// `ln(sqrt(2 pi))`, rounded to binary64.
const LN_SQRT_2PI: f64 = 0.9189385332046728;

/// `stirling_error(n)` for n = 0..6, each the exact value cut to 16 decimals, not rounded: with the
/// rest of the model in place the bank and live rows hold `n = 1, 2, 3, 5, 6` on these digits to
/// within a few units of 2^-60. `n = 4` reads one unit in the last place above its digits, which
/// is not explained and is not taken.
const STIRLING_ERROR: [f64; 7] = [
    0.0,
    0.0810614667953272,
    0.0413406959554092,
    0.0276779256849983,
    0.0207906721037650,
    0.0166446911898211,
    0.0138761288230707,
];

fn stirling_error(n: f64) -> f64 {
    if n < 7.0 {
        return STIRLING_ERROR[n as usize];
    }
    excel_stirling_series(n)
}

/// Loader's deviance `x ln(x/np) + np - x`. Measured on POISSON.DIST: the series range is
/// `|x - np| < 0.2 (x + np)`, that is `x/np` inside `(2/3, 3/2)`, where Loader has 0.1. Outside it
/// the quotient, its logarithm, the product and `np - x` are each stored and then summed.
fn deviance(x: f64, np: f64) -> f64 {
    if (x - np).abs() < 0.2 * (x + np) {
        return excel_deviance_series(x, np);
    }
    excel_x87_add(excel_x87_mul(x, excel_log(excel_x87_div(x, np))), excel_x87_sub(np, x))
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
        // With measured binary64 LN/EXP leaves, the R27 large-count readings
        // distinguish the stored PC64 product: all 300 numeric discriminants
        // and 256 controls match that program. Private leaf boundaries stay open;
        // 576 separately recorded #NUM domain answers remain unresolved.
        return excel_exp(excel_x87_mul(n, excel_log(p)));
    }
    if x < 0.0 || x > n {
        return 0.0;
    }
    // The stored order is read from the exponent's grid: 100% of 13,000 rows of six pairs `(n, x)`.
    let lc = excel_x87_sub(stirling_error(n), stirling_error(x));
    let lc = excel_x87_sub(lc, deviance(x, excel_x87_mul(n, p)));
    let lc = excel_x87_sub(lc, stirling_error(n - x));
    let lc = excel_x87_sub(lc, deviance(n - x, excel_x87_mul(n, q)));
    // `sqrt(n / (2 pi x (n - x)))` as halves of stored logarithms, in this order: outside the
    // series range 41,430 of 41,437 bank and live rows.
    excel_exp_running(lc, &[-excel_log(n) / 2.0, excel_log(x) / 2.0, excel_log(n - x) / 2.0, LN_SQRT_2PI])
}

/// The Poisson mass at `x` for `lambda > 0`. Two routes, split at `lambda = 1.1` (bisected on live
/// answers at `x = 2, 3, 7`) with `x = 1` always on the first:
///
/// - `exp(x ln lambda - lambda - GAMMALN(x + 1))`, the prefactor of GAMMA.DIST's series arm.
///   `ln lambda` is stored (the answers sit on its grid) and so is the product with `x`; the stored
///   GAMMALN is why the bank's constants carry its low bits (+1 unit at `x = 2, 8`). The two
///   subtractions are compensated (`excel_exp_compensated`).
/// - Loader's saddle point `exp(-stirling_error(x) - deviance(x, lambda)) / sqrt(2 pi x)`, with the
///   exponent and the exponential each stored: the answers sit on the exponent's binary64 grid
///   once the normalizer is removed, from `lambda` near 4 to 1000 and `x` up to 100.
fn poisson_mass(x: f64, lambda: f64) -> f64 {
    if x == 0.0 {
        return excel_exp(-lambda);
    }
    if x == 1.0 || lambda < 1.1 {
        let t = excel_x87_mul(x, excel_log(lambda));
        return excel_exp_compensated(t, &[lambda, gammaln_excel(x + 1.0)]);
    }
    let exponent = excel_x87_sub(-stirling_error(x), deviance(x, lambda));
    excel_x87_div(excel_exp(exponent), (2.0 * core::f64::consts::PI * x).sqrt())
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
    // R28: the adjacent count operands below/at signed32's positive sentinel
    // distinguish admission before PMF/CDF and p=0/1 shortcuts. Fractional
    // counts below the sentinel still truncate into the admitted range.
    if n < 0.0 || n >= i32::MAX as f64 || k < 0.0 || k > n || !(0.0..=1.0).contains(&p) {
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
    // Recorded W111 count-domain controls reject a negative fractional count
    // before truncation, but let a nonnegative count below one reach EXP even
    // with a negative mean. The zero-count branch precedes mean validation.
    if x < 0.0 {
        return Err(NUM);
    }
    let x = x.trunc();
    // P(X=0) = P(X<=0) = exp(-mean). The recorded zero-count branch retains
    // its subnormal exponential, while the nonzero-count branches flush below normal.
    if x == 0.0 {
        return Ok(excel_exp(-mean));
    }
    if mean < 0.0 {
        return Err(NUM);
    }
    if mean == 0.0 {
        return Ok(if cumulative { 1.0 } else { 0.0 });
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
    // W111 fractional-support controls validate the original signs. Truncating
    // first would turn a negative count between -1 and 0 into a valid zero.
    if sample_s < 0.0 || number_sample < 0.0 || population_s < 0.0 || number_pop < 0.0 {
        return Err(NUM);
    }
    let (s, n, k, big_n) = (sample_s.trunc(), number_sample.trunc(), population_s.trunc(), number_pop.trunc());
    if n > big_n || k > big_n {
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
        // stirling_error(1) = 1 - ln sqrt(2 pi), cut to 16 decimals.
        assert!((stirling_error(1.0) - (1.0 - LN_SQRT_2PI)).abs() < 1e-16);
        // The series arm against the definition's value at 8 and 100.
        assert!((stirling_error(8.0) - 0.010411265261972096).abs() < 1e-16);
        assert!((stirling_error(100.0) - 0.0008333305556349147).abs() < 1e-17);
    }
}
