//! The probability distribution family fragment ported from OxFunc (functions/ at 6fca3518):
//! the normal and log-normal, chi-squared, F, t, beta, gamma, Weibull, exponential, binomial,
//! negative binomial, hypergeometric and Poisson distributions and their inverses, both the
//! current spellings and the legacy ones (NORMDIST, CHIDIST, TDIST and the like), GAMMA, GAMMALN,
//! GAUSS, PHI, the confidence intervals and the tests (CHISQ.TEST, F.TEST, T.TEST, Z.TEST). The
//! numeric kernels are OxFunc's ([`crate::ported`]); every argument is read by worksheet number
//! coercion, and a cumulative flag uses worksheet logical coercion.
//!
//! The error function is in this family too (ERF, ERF.PRECISE, ERFC, ERFC.PRECISE), since its
//! kernels are the distributions' own.

use super::aggregate::Held;
use super::kit::{
    fail, flag, given, grid, num, num_with_subnormal, numbers, out, out_with_subnormal,
    reject_logical,
};
use crate::call::EvalServices;
use crate::kernel::{Site, Step};
use crate::ported::{self as k};
use crate::ported::{
    beta_gamma_stats_family as bg, chi_f_t_family as cft, confidence_test_family as ct,
    discrete_dist as dd, normal_log_family as nl, special_dist_family as sd,
    statistical_tests_family as st,
};
use crate::value::{CalcValue, CoreRef, WorksheetErrorCode};

use calc_catalogue::ported::distributions::{CODE, ENTRIES as DECLARATIONS};

/// The first `count` arguments as numbers.
fn nums<const N: usize>(site: &Site<'_>, a: &[CalcValue]) -> Step<[f64; N]> {
    let mut found = [0.0; N];
    for (index, slot) in found.iter_mut().enumerate() {
        *slot = num(site, a, index)?;
    }
    Ok(found)
}

/// Argument `index`'s numbers by the aggregate rule.
fn sample(
    site: &Site<'_>,
    a: &[CalcValue],
    index: usize,
    sv: &mut EvalServices<'_>,
) -> Step<Vec<f64>> {
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

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
ported_shared_family! {
    code: CODE;
    declarations: DECLARATIONS;
    // The normal and log-normal distributions.
    1 norm_dist |s, a, sv| {
        let [x, m, sd_] = nums::<3>(s, a)?;
        out(s, nl::norm_dist_kernel(x, m, sd_, flag(s, a, 3)?))
    }
    2 normdist |s, a, sv| {
        let [x, m, sd_] = nums::<3>(s, a)?;
        out(s, nl::norm_dist_kernel(x, m, sd_, flag(s, a, 3)?))
    }
    3 norm_inv |s, a, sv| {
        let [p, m, sd_] = nums::<3>(s, a)?;
        out(s, nl::norm_inv_kernel(p, m, sd_))
    }
    4 norminv |s, a, sv| {
        let [p, m, sd_] = nums::<3>(s, a)?;
        out(s, nl::norm_inv_kernel(p, m, sd_))
    }
    5 norm_s_dist |s, a, sv| {
        let z = num(s, a, 0)?;
        out(s, nl::norm_s_dist_kernel(z, flag(s, a, 1)?))
    }
    6 normsdist |s, a, sv| {
        out(s, nl::norm_s_dist_kernel(num(s, a, 0)?, true))
    }
    7 norm_s_inv |s, a, sv| {
        out(s, nl::norm_s_inv_kernel(num(s, a, 0)?))
    }
    8 normsinv |s, a, sv| {
        out(s, nl::norm_s_inv_kernel(num(s, a, 0)?))
    }
    9 lognorm_dist |s, a, sv| {
        let [x, m, sd_] = nums::<3>(s, a)?;
        out(s, nl::lognorm_dist_kernel(x, m, sd_, flag(s, a, 3)?))
    }
    10 lognormdist |s, a, sv| {
        let [x, m, sd_] = nums::<3>(s, a)?;
        out(s, nl::lognorm_dist_kernel(x, m, sd_, true))
    }
    11 lognorm_inv |s, a, sv| {
        let [p, m, sd_] = nums::<3>(s, a)?;
        out_with_subnormal(s, nl::lognorm_inv_kernel(p, m, sd_))
    }
    12 loginv |s, a, sv| {
        let [p, m, sd_] = nums::<3>(s, a)?;
        out_with_subnormal(s, nl::lognorm_inv_kernel(p, m, sd_))
    }
    13 confidence |s, a, sv| {
        let [alpha, sd_, n] = nums::<3>(s, a)?;
        out(s, nl::confidence_norm_kernel(alpha, sd_, n))
    }
    14 confidence_norm |s, a, sv| {
        let [alpha, sd_, n] = nums::<3>(s, a)?;
        out(s, nl::confidence_norm_kernel(alpha, sd_, n))
    }
    15 confidence_t |s, a, sv| {
        let [alpha, sd_, n] = nums::<3>(s, a)?;
        out(s, ct::confidence_t_kernel(alpha, sd_, n))
    }
    16 gauss |s, a, sv| { out(s, k::gauss_fn::gauss_kernel(num_with_subnormal(s, a, 0)?)) }
    17 phi |s, a, sv| { out(s, k::phi_fn::phi_kernel(num(s, a, 0)?)) }
    // Chi-squared.
    20 chisq_dist |s, a, sv| {
        let [x, df] = nums::<2>(s, a)?;
        out(s, cft::chisq_dist_kernel(x, df, flag(s, a, 2)?))
    }
    21 chisq_dist_rt |s, a, sv| {
        let [x, df] = nums::<2>(s, a)?;
        out(s, cft::chisq_dist_rt_kernel(x, df))
    }
    22 chidist |s, a, sv| {
        let [x, df] = nums::<2>(s, a)?;
        out(s, cft::chisq_dist_rt_kernel(x, df))
    }
    23 chisq_inv |s, a, sv| {
        let [p, df] = nums::<2>(s, a)?;
        out(s, cft::chisq_inv_kernel(p, df))
    }
    24 chisq_inv_rt |s, a, sv| {
        let [p, df] = nums::<2>(s, a)?;
        out(s, cft::chisq_inv_rt_kernel(p, df))
    }
    25 chiinv |s, a, sv| {
        let [p, df] = nums::<2>(s, a)?;
        out(s, cft::chisq_inv_rt_kernel(p, df))
    }
    // F.
    30 f_dist |s, a, sv| {
        let [x, d1, d2] = nums::<3>(s, a)?;
        out(s, cft::f_dist_kernel(x, d1, d2, flag(s, a, 3)?))
    }
    31 f_dist_rt |s, a, sv| {
        let [x, d1, d2] = nums::<3>(s, a)?;
        out(s, cft::f_dist_rt_kernel(x, d1, d2))
    }
    32 fdist |s, a, sv| {
        let [x, d1, d2] = nums::<3>(s, a)?;
        out(s, cft::f_dist_rt_kernel(x, d1, d2))
    }
    33 f_inv |s, a, sv| {
        let [p, d1, d2] = nums::<3>(s, a)?;
        out(s, cft::f_inv_kernel(p, d1, d2))
    }
    34 f_inv_rt |s, a, sv| {
        let [p, d1, d2] = nums::<3>(s, a)?;
        out(s, cft::f_inv_rt_kernel(p, d1, d2))
    }
    35 finv |s, a, sv| {
        let [p, d1, d2] = nums::<3>(s, a)?;
        out(s, cft::f_inv_rt_kernel(p, d1, d2))
    }
    // t.
    40 t_dist |s, a, sv| {
        let [x, df] = nums::<2>(s, a)?;
        out(s, cft::t_dist_kernel(x, df, flag(s, a, 2)?))
    }
    41 t_dist_rt |s, a, sv| {
        let [x, df] = nums::<2>(s, a)?;
        out(s, cft::t_dist_rt_kernel(x, df))
    }
    42 t_dist_2t |s, a, sv| {
        let [x, df] = nums::<2>(s, a)?;
        out(s, cft::t_dist_2t_kernel(x, df))
    }
    43 tdist |s, a, sv| {
        let [x, df, tails] = nums::<3>(s, a)?;
        out(s, cft::tdist_kernel(x, df, tails))
    }
    44 t_inv |s, a, sv| {
        let [p, df] = nums::<2>(s, a)?;
        out(s, cft::t_inv_kernel(p, df))
    }
    45 t_inv_2t |s, a, sv| {
        let [p, df] = nums::<2>(s, a)?;
        out(s, cft::t_inv_2t_kernel(p, df))
    }
    46 tinv |s, a, sv| {
        let [p, df] = nums::<2>(s, a)?;
        out(s, cft::t_inv_2t_kernel(p, df))
    }
    // Beta and gamma.
    50 beta_dist |s, a, sv| {
        let [x, alpha, beta] = nums::<3>(s, a)?;
        let cumulative = flag(s, a, 3)?;
        let lower = if given(a, 4) { num(s, a, 4)? } else { 0.0 };
        let upper = if given(a, 5) { num(s, a, 5)? } else { 1.0 };
        out(s, bg::beta_dist_kernel(x, alpha, beta, cumulative, lower, upper))
    }
    51 betadist |s, a, sv| {
        let [x, alpha, beta] = nums::<3>(s, a)?;
        let lower = if given(a, 3) { num(s, a, 3)? } else { 0.0 };
        let upper = if given(a, 4) { num(s, a, 4)? } else { 1.0 };
        out(s, bg::beta_dist_kernel(x, alpha, beta, true, lower, upper))
    }
    52 beta_inv |s, a, sv| {
        let [p, alpha, beta] = nums::<3>(s, a)?;
        let lower = if given(a, 3) { num(s, a, 3)? } else { 0.0 };
        let upper = if given(a, 4) { num(s, a, 4)? } else { 1.0 };
        out(s, bg::beta_inv_kernel(p, alpha, beta, lower, upper))
    }
    53 betainv |s, a, sv| {
        let [p, alpha, beta] = nums::<3>(s, a)?;
        let lower = if given(a, 3) { num(s, a, 3)? } else { 0.0 };
        let upper = if given(a, 4) { num(s, a, 4)? } else { 1.0 };
        out(s, bg::beta_inv_kernel(p, alpha, beta, lower, upper))
    }
    54 gamma_dist |s, a, sv| {
        let [x, alpha, beta] = nums::<3>(s, a)?;
        out(s, bg::gamma_dist_kernel(x, alpha, beta, flag(s, a, 3)?))
    }
    55 gammadist |s, a, sv| {
        let [x, alpha, beta] = nums::<3>(s, a)?;
        out(s, bg::gamma_dist_kernel(x, alpha, beta, flag(s, a, 3)?))
    }
    56 gamma_inv |s, a, sv| {
        let [p, alpha, beta] = nums::<3>(s, a)?;
        out(s, bg::gamma_inv_kernel(p, alpha, beta))
    }
    57 gammainv |s, a, sv| {
        let [p, alpha, beta] = nums::<3>(s, a)?;
        out(s, bg::gamma_inv_kernel(p, alpha, beta))
    }
    58 gamma |s, a, sv| { out(s, sd::gamma_kernel(num(s, a, 0)?)) }
    59 gammaln |s, a, sv| { out(s, sd::gammaln_kernel(num_with_subnormal(s, a, 0)?)) }
    60 gammaln_precise |s, a, sv| {
        out(s, sd::gammaln_precise_kernel(num_with_subnormal(s, a, 0)?))
    }
    61 weibull_dist |s, a, sv| {
        let [x, alpha, beta] = nums::<3>(s, a)?;
        out(s, sd::weibull_dist_kernel(x, alpha, beta, flag(s, a, 3)?))
    }
    62 weibull |s, a, sv| {
        let [x, alpha, beta] = nums::<3>(s, a)?;
        out(s, sd::weibull_kernel(x, alpha, beta, flag(s, a, 3)?))
    }
    63 expon_dist |s, a, sv| {
        let [x, lambda] = nums::<2>(s, a)?;
        out(s, dd::expon_dist_kernel(x, lambda, flag(s, a, 2)?))
    }
    64 expondist |s, a, sv| {
        let [x, lambda] = nums::<2>(s, a)?;
        out(s, dd::expon_dist_kernel(x, lambda, flag(s, a, 2)?))
    }
    // The discrete distributions.
    70 binom_dist |s, a, sv| {
        let [n, t, p] = nums::<3>(s, a)?;
        out(s, dd::binom_dist_kernel(n, t, p, flag(s, a, 3)?))
    }
    71 binomdist |s, a, sv| {
        let [n, t, p] = nums::<3>(s, a)?;
        out(s, dd::binom_dist_kernel(n, t, p, flag(s, a, 3)?))
    }
    72 binom_dist_range |s, a, sv| {
        let [t, p, n] = nums::<3>(s, a)?;
        let n2 = if given(a, 3) { Some(num(s, a, 3)?) } else { None };
        out(s, dd::binom_dist_range_kernel(t, p, n, n2))
    }
    73 binom_inv |s, a, sv| {
        let [t, p, alpha] = nums::<3>(s, a)?;
        out(s, dd::binom_inv_kernel(t, p, alpha))
    }
    74 critbinom |s, a, sv| {
        let [t, p, alpha] = nums::<3>(s, a)?;
        out(s, dd::binom_inv_kernel(t, p, alpha))
    }
    75 poisson_dist |s, a, sv| {
        let [x, m] = nums::<2>(s, a)?;
        out_with_subnormal(s, dd::poisson_dist_kernel(x, m, flag(s, a, 2)?))
    }
    76 poisson |s, a, sv| {
        let [x, m] = nums::<2>(s, a)?;
        out_with_subnormal(s, dd::poisson_dist_kernel(x, m, flag(s, a, 2)?))
    }
    77 hypgeom_dist |s, a, sv| {
        let [ss, ns, ps, np] = nums::<4>(s, a)?;
        out(s, dd::hypergeom_dist_kernel(ss, ns, ps, np, flag(s, a, 4)?))
    }
    78 hypgeomdist |s, a, sv| {
        let [ss, ns, ps, np] = nums::<4>(s, a)?;
        out(s, dd::hypergeom_dist_kernel(ss, ns, ps, np, false))
    }
    79 negbinom_dist |s, a, sv| {
        let [f, n, p] = nums::<3>(s, a)?;
        out(s, dd::negbinom_dist_kernel(f, n, p, flag(s, a, 3)?))
    }
    80 negbinomdist |s, a, sv| {
        let [f, n, p] = nums::<3>(s, a)?;
        out(s, dd::negbinom_dist_kernel(f, n, p, false))
    }
    // The error function.
    90 erf |s, a, sv| {
        reject_logical(&a[0])?;
        let lower = num(s, a, 0)?;
        let upper = if a.len() > 1 { Some(super::kit::num_without_logical(s, a, 1)?) } else { None };
        out(s, sd::erf_kernel(lower, upper))
    }
    91 erf_precise |s, a, sv| {
        reject_logical(&a[0])?;
        out(s, sd::erf_precise_kernel(num(s, a, 0)?))
    }
    92 erfc |s, a, sv| {
        reject_logical(&a[0])?;
        out(s, sd::erfc_kernel(num(s, a, 0)?))
    }
    93 erfc_precise |s, a, sv| {
        reject_logical(&a[0])?;
        out(s, sd::erfc_precise_kernel(num(s, a, 0)?))
    }
    // The tests.
    100 chisq_test |s, a, sv| {
        chisq_test_of(s, a, sv)
    }
    101 chitest |s, a, sv| {
        chisq_test_of(s, a, sv)
    }
    102 f_test |s, a, sv| {
        let x = sample(s, a, 0, sv)?;
        let y = sample(s, a, 1, sv)?;
        out(s, st::f_test_kernel(&x, &y))
    }
    103 ftest |s, a, sv| {
        let x = sample(s, a, 0, sv)?;
        let y = sample(s, a, 1, sv)?;
        out(s, st::f_test_kernel(&x, &y))
    }
    104 t_test |s, a, sv| {
        t_test_of(s, a, sv)
    }
    105 ttest |s, a, sv| {
        t_test_of(s, a, sv)
    }
    106 z_test |s, a, sv| { z_test_of(s, a, sv) }
    107 ztest |s, a, sv| { z_test_of(s, a, sv) }
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
            let pairs = super::kit::pairs(site, a, 0, 1, super::kit::PairScalars::Coerce, sv)?;
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
    // Literal text is coerced while binding the first argument; references and
    // arrays are read only after the scalar arguments have been validated.
    let literal_sample = if matches!(a[0].core_ref(), CoreRef::Text(_)) {
        Some(sample(site, a, 0, sv)?)
    } else {
        None
    };
    let x = num(site, a, 1)?;
    let sigma = if given(a, 2) {
        Some(num(site, a, 2)?)
    } else {
        None
    };
    let values = match literal_sample {
        Some(values) => values,
        None => sample(site, a, 0, sv)?,
    };
    out(site, ct::z_test_kernel(&values, x, sigma))
}
