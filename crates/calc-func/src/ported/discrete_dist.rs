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
//! - BINOM.INV advances past the equal cumulative boundaries recorded in the W111 bank.
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
//! - The Stirling error: a table to 6 cut to 16 decimals, ten terms of its series from 7. A shape
//!   that is not whole takes the series from 6 and comes down the recurrence (`excel_stirling_step`).
//! - The deviance outside its series range: `x/np`, its logarithm, the product with `x` and `np - x`
//!   each stored, then summed.
//! - BINOM.DIST's exponent: `s(n) - s(x) - d(x) - s(n-x) - d(n-x)`, stored after each subtraction.
//! - BINOM.DIST's normalizer and POISSON.DIST's first route: the exponential of a stored difference
//!   with what the stores dropped put back as a factor (`excel_exp_running`,
//!   `excel_exp_compensated`).
//! - The deviance inside its series range: `excel_deviance_series`.
//!
//! - The zero-success mass: EXP of the stored `n ln(1 - p)`, with Excel's internal logarithm of
//!   `1 + u` (`excel_log1p_times`), all 11,901 rows of the bank.

use crate::ported::excel_numeric::{
    excel_deviance_in_series, excel_deviance_series, excel_exp, excel_exp_compensated,
    excel_exp_running, excel_expm1_spilled, excel_log, excel_log1p_times, excel_stirling_series,
    excel_stirling_step, excel_x87_add, excel_x87_div, excel_x87_mul, excel_x87_sub, gammaln_excel,
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

///
/// A count that is not whole reaches here from GAMMA.DIST: the series at `n + k` in `[6, 7)`, each
/// step stored into the running value. At `x` next to `a` the exponent is `-s(a)` alone, and that
/// pins `s(a)` to a double: of 2,430 shapes from 1.1 to 7, 2,392 hold within two of 40 rows.
/// The ten-term series at 6 is what puts the error of -3.6 units of 2^-53 just above each integer.
pub(crate) fn stirling_error(n: f64) -> f64 {
    if n == n.floor() && n < 7.0 {
        return STIRLING_ERROR[n as usize];
    }
    let steps = (6.0 - n.floor()).max(0.0);
    let mut s = excel_stirling_series(n + steps);
    let mut k = steps - 1.0;
    while k >= 0.0 {
        s = excel_x87_add(s, excel_stirling_step(n + k));
        k -= 1.0;
    }
    s
}

/// Loader's deviance `x ln(x/np) + np - x`. Measured on POISSON.DIST: the series range is
/// `|x - np| < 0.2 (x + np)`, that is `x/np` inside `(2/3, 3/2)`, where Loader has 0.1. Outside it
/// the quotient, its logarithm, the product and `np - x` are each stored and then summed.
fn deviance(x: f64, np: f64) -> f64 {
    if excel_deviance_in_series(x, np) {
        return excel_deviance_series(x, np);
    }
    excel_x87_add(
        excel_x87_mul(x, excel_log(excel_x87_div(x, np))),
        excel_x87_sub(np, x),
    )
}

/// The binomial mass of `x` successes in `n` trials, `0 < p < 1` (Loader's saddle-point form).
fn binom_mass(x: f64, n: f64, p: f64, q: f64) -> f64 {
    if x == 0.0 {
        if n == 0.0 {
            return 1.0;
        }
        return excel_exp(excel_log1p_times(n, -p));
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
    excel_exp_running(binom_exponent(x, n, p, q), &binom_normalizer(x, n))
}

/// The stored exponent of the binomial mass, `0 < x < n`. The stored order is read from the
/// exponent's grid: 100% of 13,000 rows of six pairs `(n, x)`.
fn binom_exponent(x: f64, n: f64, p: f64, q: f64) -> f64 {
    let lc = excel_x87_sub(stirling_error(n), stirling_error(x));
    let lc = excel_x87_sub(lc, deviance(x, excel_x87_mul(n, p)));
    let lc = excel_x87_sub(lc, stirling_error(n - x));
    excel_x87_sub(lc, deviance(n - x, excel_x87_mul(n, q)))
}

/// `sqrt(n / (2 pi x (n - x)))` as halves of stored logarithms, in this order: outside the series
/// range 41,430 of 41,437 bank and live rows.
fn binom_normalizer(x: f64, n: f64) -> [f64; 4] {
    [
        -excel_log(n) / 2.0,
        excel_log(x) / 2.0,
        excel_log(n - x) / 2.0,
        LN_SQRT_2PI,
    ]
}

/// The Poisson mass at `x` for `lambda > 0`, with `x` not necessarily whole: it is also GAMMA.DIST's
/// prefactor `y^a exp(-y) / Gamma(a + 1)`, in the cumulative's arms and in the density, which is the
/// mass times the stored `a/y`. At `a = 2` the error of GAMMA.DIST jumps at `y = 1.1` (5,000 live
/// answers around it), and with this route the bank's failures fall from 29,872 to 594.
///
/// Two routes, split at `lambda = 1.1` (bisected on live answers at `x = 2, 3, 7`) with `x <= 1`
/// always on the first:
///
/// - `exp(x ln lambda - lambda - GAMMALN(x + 1))`.
///   `ln lambda` is stored (the answers sit on its grid) and so is the product with `x`; the stored
///   GAMMALN is why the bank's constants carry its low bits (+1 unit at `x = 2, 8`). The two
///   subtractions are compensated (`excel_exp_compensated`).
/// - Loader's saddle point `exp(-stirling_error(x) - deviance(x, lambda)) / sqrt(2 pi x)`, with the
///   exponent and the exponential each stored: the answers sit on the exponent's binary64 grid
///   once the normalizer is removed, from `lambda` near 4 to 1000 and `x` up to 100.
pub(crate) fn poisson_mass(x: f64, lambda: f64) -> f64 {
    if x == 0.0 {
        return excel_exp(-lambda);
    }
    if x <= 1.0 || lambda < 1.1 {
        let t = excel_x87_mul(x, excel_log(lambda));
        return excel_exp_compensated(t, &[lambda, gammaln_excel(x + 1.0)]);
    }
    let exponent = excel_x87_sub(-stirling_error(x), deviance(x, lambda));
    excel_x87_div(
        excel_exp(exponent),
        (2.0 * core::f64::consts::PI * x).sqrt(),
    )
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
    finite(if cumulative {
        -excel_expm1_spilled(-t)
    } else {
        excel_x87_mul(lambda, excel_exp(-t))
    })
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
    finite(binom_cumulative(k, n, p, q))
}

/// The binomial's lower tail: the mass at 0, one at `n`, and between them the regularized beta at
/// `q`, the same bits as `BETA.DIST(q, n - k, k + 1, TRUE)` on every row of 128 live ones at `n` =
/// 30 and 100. Not a sum of the masses: that sum misses nine rows in ten by up to 188 units.
fn binom_cumulative(k: f64, n: f64, p: f64, q: f64) -> f64 {
    if k == n {
        1.0
    } else if k == 0.0 {
        binom_mass(k, n, p, q)
    } else if n - k == 1.0 && q < 0.1 {
        // One short of n with q below 0.1: -expm1(n * log1p(-q)) with the product stored in
        // binary64, not BETA.DIST's x87 chain at a = 1 (R43: Excel's BETA.DIST(q, 1, n) and
        // BINOM.DIST(n - 1, n, p, TRUE) differ on 91 of 500 fresh calls; this matches 499 of
        // the binomial's). Which EXPM1 graph runs is undetermined: none of the asked
        // arguments separates the internal and spilled ones.
        use crate::ported::excel_numeric::{excel_expm1_internal, excel_log1p};
        -excel_expm1_internal(n * excel_log1p(-q))
    } else {
        crate::ported::special_math_common::regularized_beta(q, n - k, k + 1.0)
    }
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
    if s == s2 || p == 0.0 || p == 1.0 {
        return finite(if s == s2 {
            mass(s)
        } else {
            let (first, last) = if p == 0.0 { (0.0, 0.0) } else { (n, n) };
            if s <= first && first <= s2 && last <= s2 {
                1.0
            } else {
                0.0
            }
        });
    }
    // Live: the difference of the two tails on 383 of 421 rows, against 20 for the sum of masses.
    let q = 1.0 - p;
    let upper = binom_cumulative(s2, n, p, q);
    finite(if s == 0.0 {
        upper
    } else {
        excel_x87_sub(upper, binom_cumulative(s - 1.0, n, p, q))
    })
}

/// `BINOM.INV(trials, probability_s, alpha)`: the smallest count whose public cumulative
/// probability reaches alpha. Excel compares the mass at zero strictly and every later
/// cumulative probability inclusively (dc-j9k, Excel 16.0.20527): on 252 dyadic boundary
/// designs, each asked at the exact lower tail and three binary64 neighbours either side, the
/// 200 with a count above zero meet alpha exactly when BINOM.DIST(k, n, p, TRUE) >= alpha, and
/// all 52 at zero only when BINOM.DIST(0, n, p, TRUE) > alpha. A forward mass recurrence locates
/// the count; the public tails then settle it.
pub fn binom_inv_kernel(trials: f64, p: f64, alpha: f64) -> R {
    let n = trials.trunc();
    if n < 0.0 || !(p > 0.0 && p < 1.0) || !(alpha > 0.0 && alpha < 1.0) {
        return Err(NUM);
    }
    let q = 1.0 - p;
    let reaches = |k: f64| {
        if k == 0.0 {
            binom_cumulative(0.0, n, p, q) > alpha
        } else {
            binom_cumulative(k, n, p, q) >= alpha
        }
    };
    let mut k = binom_inv_estimate(n, p, q, alpha);
    while k > 0.0 && reaches(k - 1.0) {
        k -= 1.0;
    }
    while k < n && !reaches(k) {
        k += 1.0;
    }
    Ok(k)
}

/// The count a forward mass recurrence reaches, used only to start the search near the answer.
fn binom_inv_estimate(n: f64, p: f64, q: f64, alpha: f64) -> f64 {
    let mut sum = 0.0;
    let mut k = 0.0;
    // The forward recurrence starts at the larger-probability endpoint; use the existing
    // mass calculation when that endpoint is the upper tail or its starting mass underflows.
    let mut mass = if p <= q {
        crate::ported::excel_numeric::crt_pow(q, n)
    } else {
        0.0
    };
    while k < n {
        sum += if mass == 0.0 {
            binom_mass(k, n, p, q)
        } else {
            mass
        };
        if sum > alpha {
            return k;
        }
        k += 1.0;
        mass *= (n - k + 1.0) / k * p / q;
    }
    n
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
    finite(crate::ported::special_math_common::regularized_gamma_q(
        x + 1.0,
        mean,
    ))
}

/// The hypergeometric mass: `s` successes in a sample of `n` from `big_n` holding `k` successes,
/// as `b(s; k, n/N) b(n - s; N - k, n/N) / b(n; N, n/N)`.
fn hypergeom_mass(s: f64, n: f64, k: f64, big_n: f64) -> f64 {
    if s < 0.0 || s > n || s > k || n - s > big_n - k {
        return 0.0;
    }
    if n == 0.0 || n == big_n {
        return if s == (if n == 0.0 { 0.0 } else { k }) {
            1.0
        } else {
            0.0
        };
    }
    let p = n / big_n;
    let q = (big_n - n) / big_n;
    let a = binom_mass(s, k, p, q);
    let b = binom_mass(n - s, big_n - k, p, q);
    let c = binom_mass(n, big_n, p, q);
    excel_x87_mul(excel_x87_div(a, c), b)
}

/// `HYPGEOM.DIST(sample_s, number_sample, population_s, number_pop, cumulative)`.
pub fn hypergeom_dist_kernel(
    sample_s: f64,
    number_sample: f64,
    population_s: f64,
    number_pop: f64,
    cumulative: bool,
) -> R {
    // W111 fractional-support controls validate the original signs. Truncating
    // first would turn a negative count between -1 and 0 into a valid zero.
    if sample_s < 0.0 || number_sample < 0.0 || population_s < 0.0 || number_pop < 0.0 {
        return Err(NUM);
    }
    let (s, n, k, big_n) = (
        sample_s.trunc(),
        number_sample.trunc(),
        population_s.trunc(),
        number_pop.trunc(),
    );
    if n > big_n || k > big_n {
        return Err(NUM);
    }
    if !cumulative {
        return finite(hypergeom_mass(s, n, k, big_n));
    }
    // From the largest possible count the cumulative is 1 without a sum, except when that count is
    // 0: the cumulative is then its one mass, which Excel 16.0.20527 gives as 0.9999999999999999
    // for (0,1,0,2) and (0,12,0,13), like the mass (and 1 for (0,5,0,10) and (0,7,0,13), whose
    // mass is 1). Beyond that count it is 1 for every probe: (1,1,0,2), (5,1,0,2), (3,2,1,5).
    if s > n.min(k) || (s == n.min(k) && s > 0.0) {
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

/// The negative binomial mass of `f` failures before the `s`-th success, with `n = s + f`: the
/// binomial mass of `s - 1` successes in `n - 1` trials times `n` inside the exponential, then the
/// stored `p / n` as a stored multiply.
///
/// Read from the bank: `p b(s - 1; n - 1)` shares its deviance noise with Excel's answers (spread of
/// one unit, where `s/n b(s; n)` spreads by up to 30), and its mean error over the 60 rows of each
/// `(f, s)` is the rounding error of `LN(n)` to a coefficient of -1.05, on `n` alone, for the
/// zero-success route too. So `ln n` sits in the exponent as the last running subtraction and
/// `n` comes back out through `p / n`: 60 of 60 at each `s = 1`.
fn negbinom_mass(f: f64, s: f64, p: f64) -> f64 {
    let q = 1.0 - p;
    if f == 0.0 {
        return excel_exp(s * excel_log(p));
    }
    let n = s + f;
    let (x, trials) = (s - 1.0, n - 1.0);
    let ln_n = excel_log(n);
    let body = if x == 0.0 {
        excel_exp_running(excel_log1p_times(trials, -p), &[-ln_n])
    } else {
        let [a, b, c, d] = binom_normalizer(x, trials);
        excel_exp_running(binom_exponent(x, trials, p, q), &[a, b, c, d, -ln_n])
    };
    // `p / n` below the normal range is gone before the product (NEGBINOM.DIST(1, 1, DBL_MIN, 0) = 0).
    let rate = excel_x87_div(p, n);
    if rate.is_subnormal() {
        return 0.0;
    }
    excel_x87_mul(body, rate)
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
    if f == 0.0 {
        return finite(negbinom_mass(0.0, s, p));
    }
    // Excel's cumulative is not the sum of its masses: at dyadic `p` it can sit on the exact dyadic
    // sum (`fresh-negbinom.dist-00113`) where masses through EXP cannot, and at `p = 1/8` it stands
    // 13 units off it. `I_p(s, f + 1)` holds the exact row and more of the 141 rows than the sum
    // (73 against 70). The stages belong to the incomplete beta, still open.
    finite(crate::ported::special_math_common::regularized_beta(
        p,
        s,
        f + 1.0,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// dc-j9k's D180: the Temme arm keeps Excel's truncated fraction.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn poisson_tail_keeps_the_truncated_fraction() {
        let mean = (128159288.0 * 67108864.0 + 59652325.0) / 70368744177664.0;
        assert_eq!(
            poisson_dist_kernel(100.0, mean, true).unwrap().to_bits(),
            0x3f96_a06a_3c48_3193
        );
    }

    /// BINOM.DIST(n - 1, n, p, TRUE) with 1 - p below 0.1 is -expm1(n * log1p(p - 1)) with a
    /// binary64 product, not BETA.DIST(1 - p, 1, n): at n = 10, p = 0.99 the two Excel answers
    /// differ by five units. Excel 16.0 build 20527, the question service, 2026-11-09 (R43).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn binomial_cdf_one_short_of_n_matches_excel() {
        let rows: [(f64, f64, f64, u64); 6] = [
            (19.0, 20.0, 0.95, 0x3fe4874887e5b9b4),
            (99.0, 100.0, 0.999, 0x3fb85f8ab6781056),
            (9.0, 10.0, 0.99, 0x3fb87a6a94bfa208),
            (49.0, 50.0, 0.97, 0x3fe9059bc30fc602),
            (4.0, 5.0, 0.93, 0x3fd379d77d69880c),
            (29.0, 30.0, 0.995, 0x3fc1deee48dd912d),
        ];
        for (k, n, p, expected) in rows {
            let got = binom_dist_kernel(k, n, p, true).unwrap();
            assert_eq!(got.to_bits(), expected, "BINOM.DIST({k},{n},{p},TRUE)");
        }
    }

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
