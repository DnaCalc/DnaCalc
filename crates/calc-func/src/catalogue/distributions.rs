//! The probability distribution family fragment ported from OxFunc (functions/ at 6fca3518):
//! the normal and log-normal, chi-squared, F, t, beta, gamma, Weibull, exponential, binomial,
//! negative binomial, hypergeometric and Poisson distributions and their inverses, both the
//! current spellings and the legacy ones (NORMDIST, CHIDIST, TDIST and the like), GAMMA, GAMMALN,
//! GAUSS, PHI, the confidence intervals and the tests (CHISQ.TEST, F.TEST, T.TEST, Z.TEST). The
//! numeric kernels are OxFunc's ([`crate::ported`]); every argument is read by worksheet number
//! coercion, and a cumulative flag is a number read as TRUE when it is not 0.
//!
//! The error function is in this family too (ERF, ERF.PRECISE, ERFC, ERFC.PRECISE), since its
//! kernels are the distributions' own.

use super::aggregate::Held;
use super::kit::{fail, given, grid, num, numbers, out};
use crate::call::EvalServices;
use crate::kernel::{Site, Step};
use crate::ported::{
    beta_gamma_stats_family as bg, chi_f_t_family as cft, confidence_test_family as ct,
    discrete_dist_family as dd, normal_log_family as nl, special_dist_family as sd,
    statistical_tests_family as st,
};
use crate::ported::{self as k};
use crate::value::{CalcValue, CoreRef, WorksheetErrorCode};

const CODE: u16 = 104;

/// Argument `index` as a cumulative flag: a number, TRUE unless 0.
fn flag(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<bool> {
    Ok(num(site, a, index)? != 0.0)
}

/// The first `count` arguments as numbers.
fn nums<const N: usize>(site: &Site<'_>, a: &[CalcValue]) -> Step<[f64; N]> {
    let mut found = [0.0; N];
    for (index, slot) in found.iter_mut().enumerate() {
        *slot = num(site, a, index)?;
    }
    Ok(found)
}

/// Argument `index`'s numbers by the aggregate rule.
fn sample(site: &Site<'_>, a: &[CalcValue], index: usize, sv: &mut EvalServices<'_>) -> Step<Vec<f64>> {
    numbers(site, a, index, index + 1, sv, Held::Numbers)
}

/// The tests' tails or type argument: a number, truncated.
fn whole(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<i32> {
    let n = num(site, a, index)?;
    if !n.is_finite() {
        return fail(WorksheetErrorCode::Num);
    }
    Ok(n.trunc() as i32)
}

ported_family! {
    code: CODE;
    // The normal and log-normal distributions.
    1 norm_dist "NORM.DIST" (4, 4) [V "x", V "mean", V "standard_dev", V "cumulative"] |s, a, sv| {
        let [x, m, sd_] = nums::<3>(s, a)?;
        out(s, nl::norm_dist_kernel(x, m, sd_, flag(s, a, 3)?))
    }
    2 normdist "NORMDIST" (4, 4) [V "x", V "mean", V "standard_dev", V "cumulative"] |s, a, sv| {
        let [x, m, sd_] = nums::<3>(s, a)?;
        out(s, nl::norm_dist_kernel(x, m, sd_, flag(s, a, 3)?))
    }
    3 norm_inv "NORM.INV" (3, 3) [V "probability", V "mean", V "standard_dev"] |s, a, sv| {
        let [p, m, sd_] = nums::<3>(s, a)?;
        out(s, nl::norm_inv_kernel(p, m, sd_))
    }
    4 norminv "NORMINV" (3, 3) [V "probability", V "mean", V "standard_dev"] |s, a, sv| {
        let [p, m, sd_] = nums::<3>(s, a)?;
        out(s, nl::norm_inv_kernel(p, m, sd_))
    }
    5 norm_s_dist "NORM.S.DIST" (2, 2) [V "z", V "cumulative"] |s, a, sv| {
        let z = num(s, a, 0)?;
        out(s, nl::norm_s_dist_kernel(z, flag(s, a, 1)?))
    }
    6 normsdist "NORMSDIST" (1, 1) [V "z"] |s, a, sv| {
        out(s, nl::norm_s_dist_kernel(num(s, a, 0)?, true))
    }
    7 norm_s_inv "NORM.S.INV" (1, 1) [V "probability"] |s, a, sv| {
        out(s, nl::norm_s_inv_kernel(num(s, a, 0)?))
    }
    8 normsinv "NORMSINV" (1, 1) [V "probability"] |s, a, sv| {
        out(s, nl::norm_s_inv_kernel(num(s, a, 0)?))
    }
    9 lognorm_dist "LOGNORM.DIST" (4, 4) [V "x", V "mean", V "standard_dev", V "cumulative"] |s, a, sv| {
        let [x, m, sd_] = nums::<3>(s, a)?;
        out(s, nl::lognorm_dist_kernel(x, m, sd_, flag(s, a, 3)?))
    }
    10 lognormdist "LOGNORMDIST" (3, 3) [V "x", V "mean", V "standard_dev"] |s, a, sv| {
        let [x, m, sd_] = nums::<3>(s, a)?;
        out(s, nl::lognorm_dist_kernel(x, m, sd_, true))
    }
    11 lognorm_inv "LOGNORM.INV" (3, 3) [V "probability", V "mean", V "standard_dev"] |s, a, sv| {
        let [p, m, sd_] = nums::<3>(s, a)?;
        out(s, nl::lognorm_inv_kernel(p, m, sd_))
    }
    12 loginv "LOGINV" (3, 3) [V "probability", V "mean", V "standard_dev"] |s, a, sv| {
        let [p, m, sd_] = nums::<3>(s, a)?;
        out(s, nl::lognorm_inv_kernel(p, m, sd_))
    }
    13 confidence "CONFIDENCE" (3, 3) [V "alpha", V "standard_dev", V "size"] |s, a, sv| {
        let [alpha, sd_, n] = nums::<3>(s, a)?;
        out(s, nl::confidence_norm_kernel(alpha, sd_, n))
    }
    14 confidence_norm "CONFIDENCE.NORM" (3, 3) [V "alpha", V "standard_dev", V "size"] |s, a, sv| {
        let [alpha, sd_, n] = nums::<3>(s, a)?;
        out(s, nl::confidence_norm_kernel(alpha, sd_, n))
    }
    15 confidence_t "CONFIDENCE.T" (3, 3) [V "alpha", V "standard_dev", V "size"] |s, a, sv| {
        let [alpha, sd_, n] = nums::<3>(s, a)?;
        out(s, ct::confidence_t_kernel(alpha, sd_, n))
    }
    16 gauss "GAUSS" (1, 1) [V "z"] |s, a, sv| { out(s, k::gauss_fn::gauss_kernel(num(s, a, 0)?)) }
    17 phi "PHI" (1, 1) [V "x"] |s, a, sv| { out(s, k::phi_fn::phi_kernel(num(s, a, 0)?)) }
    // Chi-squared.
    20 chisq_dist "CHISQ.DIST" (3, 3) [V "x", V "deg_freedom", V "cumulative"] |s, a, sv| {
        let [x, df] = nums::<2>(s, a)?;
        out(s, cft::chisq_dist_kernel(x, df, flag(s, a, 2)?))
    }
    21 chisq_dist_rt "CHISQ.DIST.RT" (2, 2) [V "x", V "deg_freedom"] |s, a, sv| {
        let [x, df] = nums::<2>(s, a)?;
        out(s, cft::chisq_dist_rt_kernel(x, df))
    }
    22 chidist "CHIDIST" (2, 2) [V "x", V "deg_freedom"] |s, a, sv| {
        let [x, df] = nums::<2>(s, a)?;
        out(s, cft::chisq_dist_rt_kernel(x, df))
    }
    23 chisq_inv "CHISQ.INV" (2, 2) [V "probability", V "deg_freedom"] |s, a, sv| {
        let [p, df] = nums::<2>(s, a)?;
        out(s, cft::chisq_inv_kernel(p, df))
    }
    24 chisq_inv_rt "CHISQ.INV.RT" (2, 2) [V "probability", V "deg_freedom"] |s, a, sv| {
        let [p, df] = nums::<2>(s, a)?;
        out(s, cft::chisq_inv_rt_kernel(p, df))
    }
    25 chiinv "CHIINV" (2, 2) [V "probability", V "deg_freedom"] |s, a, sv| {
        let [p, df] = nums::<2>(s, a)?;
        out(s, cft::chisq_inv_rt_kernel(p, df))
    }
    // F.
    30 f_dist "F.DIST" (4, 4) [V "x", V "deg_freedom1", V "deg_freedom2", V "cumulative"] |s, a, sv| {
        let [x, d1, d2] = nums::<3>(s, a)?;
        out(s, cft::f_dist_kernel(x, d1, d2, flag(s, a, 3)?))
    }
    31 f_dist_rt "F.DIST.RT" (3, 3) [V "x", V "deg_freedom1", V "deg_freedom2"] |s, a, sv| {
        let [x, d1, d2] = nums::<3>(s, a)?;
        out(s, cft::f_dist_rt_kernel(x, d1, d2))
    }
    32 fdist "FDIST" (3, 3) [V "x", V "deg_freedom1", V "deg_freedom2"] |s, a, sv| {
        let [x, d1, d2] = nums::<3>(s, a)?;
        out(s, cft::f_dist_rt_kernel(x, d1, d2))
    }
    33 f_inv "F.INV" (3, 3) [V "probability", V "deg_freedom1", V "deg_freedom2"] |s, a, sv| {
        let [p, d1, d2] = nums::<3>(s, a)?;
        out(s, cft::f_inv_kernel(p, d1, d2))
    }
    34 f_inv_rt "F.INV.RT" (3, 3) [V "probability", V "deg_freedom1", V "deg_freedom2"] |s, a, sv| {
        let [p, d1, d2] = nums::<3>(s, a)?;
        out(s, cft::f_inv_rt_kernel(p, d1, d2))
    }
    35 finv "FINV" (3, 3) [V "probability", V "deg_freedom1", V "deg_freedom2"] |s, a, sv| {
        let [p, d1, d2] = nums::<3>(s, a)?;
        out(s, cft::f_inv_rt_kernel(p, d1, d2))
    }
    // t.
    40 t_dist "T.DIST" (3, 3) [V "x", V "deg_freedom", V "cumulative"] |s, a, sv| {
        let [x, df] = nums::<2>(s, a)?;
        out(s, cft::t_dist_kernel(x, df, flag(s, a, 2)?))
    }
    41 t_dist_rt "T.DIST.RT" (2, 2) [V "x", V "deg_freedom"] |s, a, sv| {
        let [x, df] = nums::<2>(s, a)?;
        out(s, cft::t_dist_rt_kernel(x, df))
    }
    42 t_dist_2t "T.DIST.2T" (2, 2) [V "x", V "deg_freedom"] |s, a, sv| {
        let [x, df] = nums::<2>(s, a)?;
        out(s, cft::t_dist_2t_kernel(x, df))
    }
    43 tdist "TDIST" (3, 3) [V "x", V "deg_freedom", V "tails"] |s, a, sv| {
        let [x, df, tails] = nums::<3>(s, a)?;
        out(
            s,
            match tails.trunc() as i32 {
                1 => cft::t_dist_rt_kernel(x, df),
                2 => cft::t_dist_2t_kernel(x, df),
                _ => Err(WorksheetErrorCode::Num),
            },
        )
    }
    44 t_inv "T.INV" (2, 2) [V "probability", V "deg_freedom"] |s, a, sv| {
        let [p, df] = nums::<2>(s, a)?;
        out(s, cft::t_inv_kernel(p, df))
    }
    45 t_inv_2t "T.INV.2T" (2, 2) [V "probability", V "deg_freedom"] |s, a, sv| {
        let [p, df] = nums::<2>(s, a)?;
        out(s, cft::t_inv_2t_kernel(p, df))
    }
    46 tinv "TINV" (2, 2) [V "probability", V "deg_freedom"] |s, a, sv| {
        let [p, df] = nums::<2>(s, a)?;
        out(s, cft::t_inv_2t_kernel(p, df))
    }
    // Beta and gamma.
    50 beta_dist "BETA.DIST" (4, 6) [V "x", V "alpha", V "beta", V "cumulative", V "A", V "B"] |s, a, sv| {
        let [x, alpha, beta] = nums::<3>(s, a)?;
        let cumulative = flag(s, a, 3)?;
        let lower = if given(a, 4) { num(s, a, 4)? } else { 0.0 };
        let upper = if given(a, 5) { num(s, a, 5)? } else { 1.0 };
        out(s, bg::beta_dist_kernel(x, alpha, beta, cumulative, lower, upper))
    }
    51 betadist "BETADIST" (3, 5) [V "x", V "alpha", V "beta", V "A", V "B"] |s, a, sv| {
        let [x, alpha, beta] = nums::<3>(s, a)?;
        let lower = if given(a, 3) { num(s, a, 3)? } else { 0.0 };
        let upper = if given(a, 4) { num(s, a, 4)? } else { 1.0 };
        out(s, bg::beta_dist_kernel(x, alpha, beta, true, lower, upper))
    }
    52 beta_inv "BETA.INV" (3, 5) [V "probability", V "alpha", V "beta", V "A", V "B"] |s, a, sv| {
        let [p, alpha, beta] = nums::<3>(s, a)?;
        let lower = if given(a, 3) { num(s, a, 3)? } else { 0.0 };
        let upper = if given(a, 4) { num(s, a, 4)? } else { 1.0 };
        out(s, bg::beta_inv_kernel(p, alpha, beta, lower, upper))
    }
    53 betainv "BETAINV" (3, 5) [V "probability", V "alpha", V "beta", V "A", V "B"] |s, a, sv| {
        let [p, alpha, beta] = nums::<3>(s, a)?;
        let lower = if given(a, 3) { num(s, a, 3)? } else { 0.0 };
        let upper = if given(a, 4) { num(s, a, 4)? } else { 1.0 };
        out(s, bg::beta_inv_kernel(p, alpha, beta, lower, upper))
    }
    54 gamma_dist "GAMMA.DIST" (4, 4) [V "x", V "alpha", V "beta", V "cumulative"] |s, a, sv| {
        let [x, alpha, beta] = nums::<3>(s, a)?;
        out(s, bg::gamma_dist_kernel(x, alpha, beta, flag(s, a, 3)?))
    }
    55 gammadist "GAMMADIST" (4, 4) [V "x", V "alpha", V "beta", V "cumulative"] |s, a, sv| {
        let [x, alpha, beta] = nums::<3>(s, a)?;
        out(s, bg::gamma_dist_kernel(x, alpha, beta, flag(s, a, 3)?))
    }
    56 gamma_inv "GAMMA.INV" (3, 3) [V "probability", V "alpha", V "beta"] |s, a, sv| {
        let [p, alpha, beta] = nums::<3>(s, a)?;
        out(s, bg::gamma_inv_kernel(p, alpha, beta))
    }
    57 gammainv "GAMMAINV" (3, 3) [V "probability", V "alpha", V "beta"] |s, a, sv| {
        let [p, alpha, beta] = nums::<3>(s, a)?;
        out(s, bg::gamma_inv_kernel(p, alpha, beta))
    }
    58 gamma "GAMMA" (1, 1) [V "number"] |s, a, sv| { out(s, sd::gamma_kernel(num(s, a, 0)?)) }
    59 gammaln "GAMMALN" (1, 1) [V "x"] |s, a, sv| { out(s, sd::gammaln_kernel(num(s, a, 0)?)) }
    60 gammaln_precise "GAMMALN.PRECISE" (1, 1) [V "x"] |s, a, sv| {
        out(s, sd::gammaln_precise_kernel(num(s, a, 0)?))
    }
    61 weibull_dist "WEIBULL.DIST" (4, 4) [V "x", V "alpha", V "beta", V "cumulative"] |s, a, sv| {
        let [x, alpha, beta] = nums::<3>(s, a)?;
        out(s, sd::weibull_dist_kernel(x, alpha, beta, flag(s, a, 3)?))
    }
    62 weibull "WEIBULL" (4, 4) [V "x", V "alpha", V "beta", V "cumulative"] |s, a, sv| {
        let [x, alpha, beta] = nums::<3>(s, a)?;
        out(s, sd::weibull_kernel(x, alpha, beta, flag(s, a, 3)?))
    }
    63 expon_dist "EXPON.DIST" (3, 3) [V "x", V "lambda", V "cumulative"] |s, a, sv| {
        let [x, lambda] = nums::<2>(s, a)?;
        out(s, dd::expon_dist_kernel(x, lambda, flag(s, a, 2)?))
    }
    64 expondist "EXPONDIST" (3, 3) [V "x", V "lambda", V "cumulative"] |s, a, sv| {
        let [x, lambda] = nums::<2>(s, a)?;
        out(s, dd::expon_dist_kernel(x, lambda, flag(s, a, 2)?))
    }
    // The discrete distributions.
    70 binom_dist "BINOM.DIST" (4, 4) [V "number_s", V "trials", V "probability_s", V "cumulative"] |s, a, sv| {
        let [n, t, p] = nums::<3>(s, a)?;
        out(s, dd::binom_dist_kernel(n, t, p, flag(s, a, 3)?))
    }
    71 binomdist "BINOMDIST" (4, 4) [V "number_s", V "trials", V "probability_s", V "cumulative"] |s, a, sv| {
        let [n, t, p] = nums::<3>(s, a)?;
        out(s, dd::binom_dist_kernel(n, t, p, flag(s, a, 3)?))
    }
    72 binom_dist_range "BINOM.DIST.RANGE" (3, 4) [V "trials", V "probability_s", V "number_s", V "number_s2"] |s, a, sv| {
        let [t, p, n] = nums::<3>(s, a)?;
        let n2 = if given(a, 3) { Some(num(s, a, 3)?) } else { None };
        out(s, dd::binom_dist_range_kernel(t, p, n, n2))
    }
    73 binom_inv "BINOM.INV" (3, 3) [V "trials", V "probability_s", V "alpha"] |s, a, sv| {
        let [t, p, alpha] = nums::<3>(s, a)?;
        out(s, dd::binom_inv_kernel(t, p, alpha))
    }
    74 critbinom "CRITBINOM" (3, 3) [V "trials", V "probability_s", V "alpha"] |s, a, sv| {
        let [t, p, alpha] = nums::<3>(s, a)?;
        out(s, dd::binom_inv_kernel(t, p, alpha))
    }
    75 poisson_dist "POISSON.DIST" (3, 3) [V "x", V "mean", V "cumulative"] |s, a, sv| {
        let [x, m] = nums::<2>(s, a)?;
        out(s, dd::poisson_dist_kernel(x, m, flag(s, a, 2)?))
    }
    76 poisson "POISSON" (3, 3) [V "x", V "mean", V "cumulative"] |s, a, sv| {
        let [x, m] = nums::<2>(s, a)?;
        out(s, dd::poisson_dist_kernel(x, m, flag(s, a, 2)?))
    }
    77 hypgeom_dist "HYPGEOM.DIST" (5, 5) [V "sample_s", V "number_sample", V "population_s", V "number_pop", V "cumulative"] |s, a, sv| {
        let [ss, ns, ps, np] = nums::<4>(s, a)?;
        out(s, dd::hypergeom_dist_kernel(ss, ns, ps, np, flag(s, a, 4)?))
    }
    78 hypgeomdist "HYPGEOMDIST" (4, 4) [V "sample_s", V "number_sample", V "population_s", V "number_pop"] |s, a, sv| {
        let [ss, ns, ps, np] = nums::<4>(s, a)?;
        out(s, dd::hypergeom_dist_kernel(ss, ns, ps, np, false))
    }
    79 negbinom_dist "NEGBINOM.DIST" (4, 4) [V "number_f", V "number_s", V "probability_s", V "cumulative"] |s, a, sv| {
        let [f, n, p] = nums::<3>(s, a)?;
        out(s, dd::negbinom_dist_kernel(f, n, p, flag(s, a, 3)?))
    }
    80 negbinomdist "NEGBINOMDIST" (3, 3) [V "number_f", V "number_s", V "probability_s"] |s, a, sv| {
        let [f, n, p] = nums::<3>(s, a)?;
        out(s, dd::negbinom_dist_kernel(f, n, p, false))
    }
    // The error function.
    90 erf "ERF" (1, 2) [V "lower_limit", V "upper_limit"] |s, a, sv| {
        let lower = num(s, a, 0)?;
        let upper = if a.len() > 1 { Some(num(s, a, 1)?) } else { None };
        out(s, sd::erf_kernel(lower, upper))
    }
    91 erf_precise "ERF.PRECISE" (1, 1) [V "x"] |s, a, sv| {
        out(s, sd::erf_precise_kernel(num(s, a, 0)?))
    }
    92 erfc "ERFC" (1, 1) [V "x"] |s, a, sv| { out(s, sd::erfc_kernel(num(s, a, 0)?)) }
    93 erfc_precise "ERFC.PRECISE" (1, 1) [V "x"] |s, a, sv| {
        out(s, sd::erfc_precise_kernel(num(s, a, 0)?))
    }
    // The tests.
    100 chisq_test "CHISQ.TEST" (2, 2) [R "actual_range", R "expected_range"] |s, a, sv| {
        chisq_test_of(s, a, sv)
    }
    101 chitest "CHITEST" (2, 2) [R "actual_range", R "expected_range"] |s, a, sv| {
        chisq_test_of(s, a, sv)
    }
    102 f_test "F.TEST" (2, 2) [R "array1", R "array2"] |s, a, sv| {
        let x = sample(s, a, 0, sv)?;
        let y = sample(s, a, 1, sv)?;
        out(s, st::f_test_kernel(&x, &y))
    }
    103 ftest "FTEST" (2, 2) [R "array1", R "array2"] |s, a, sv| {
        let x = sample(s, a, 0, sv)?;
        let y = sample(s, a, 1, sv)?;
        out(s, st::f_test_kernel(&x, &y))
    }
    104 t_test "T.TEST" (4, 4) [R "array1", R "array2", V "tails", V "type"] |s, a, sv| {
        t_test_of(s, a, sv)
    }
    105 ttest "TTEST" (4, 4) [R "array1", R "array2", V "tails", V "type"] |s, a, sv| {
        t_test_of(s, a, sv)
    }
    106 z_test "Z.TEST" (2, 3) [R "array", V "x", V "sigma"] |s, a, sv| { z_test_of(s, a, sv) }
    107 ztest "ZTEST" (2, 3) [R "array", V "x", V "sigma"] |s, a, sv| { z_test_of(s, a, sv) }
}

fn chisq_test_of(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    // An argument that is an error value is the result before the shapes are compared (Excel
    // 16.0.20430, 2026-10-25, dc-6au: `=CHISQ.TEST(#REF!,1)`, `=CHISQ.TEST(1,#REF!)` and
    // `=CHISQ.TEST(A1:A2,#REF!)` are #REF!, `=CHISQ.TEST(#DIV/0!,A1:A2)` #DIV/0!).
    for argument in &a[..2] {
        if let CoreRef::Error(code) = argument.core_ref() {
            return fail(code);
        }
    }
    let actual = grid(site, a, 0, sv)?;
    let expected = grid(site, a, 1, sv)?;
    if (actual.rows, actual.cols) != (expected.rows, expected.cols) {
        return fail(WorksheetErrorCode::NA);
    }
    let df = if actual.rows > 1 && actual.cols > 1 {
        (actual.rows - 1) * (actual.cols - 1)
    } else if actual.rows == 1 {
        actual.cols - 1
    } else {
        actual.rows - 1
    };
    if df == 0 {
        return fail(WorksheetErrorCode::NA);
    }
    let mut statistic = 0.0;
    for (x, e) in actual.cells.iter().zip(&expected.cells) {
        let (x, e) = match (x.core_ref(), e.core_ref()) {
            (CoreRef::Error(code), _) | (_, CoreRef::Error(code)) => return fail(code),
            (CoreRef::Number(x), CoreRef::Number(e)) => (x, e),
            _ => return fail(WorksheetErrorCode::Value),
        };
        if e <= 0.0 {
            return fail(WorksheetErrorCode::Num);
        }
        let delta = x - e;
        statistic += delta * delta / e;
    }
    out(site, cft::chisq_dist_rt_kernel(statistic, df as f64))
}

fn t_test_of(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let tails = whole(site, a, 2)?;
    let kind = whole(site, a, 3)?;
    let result = match kind {
        1 => {
            let pairs = super::kit::pairs(site, a, 0, 1, sv)?;
            st::t_test_paired_kernel(&pairs, tails)
        }
        2 | 3 => {
            let x = sample(site, a, 0, sv)?;
            let y = sample(site, a, 1, sv)?;
            if kind == 2 {
                st::t_test_equal_variance_kernel(&x, &y, tails)
            } else {
                st::t_test_unequal_variance_kernel(&x, &y, tails)
            }
        }
        _ => Err(WorksheetErrorCode::Num),
    };
    out(site, result)
}

fn z_test_of(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let values = sample(site, a, 0, sv)?;
    let x = num(site, a, 1)?;
    let sigma = if given(a, 2) { Some(num(site, a, 2)?) } else { None };
    out(site, ct::z_test_kernel(&values, x, sigma))
}
