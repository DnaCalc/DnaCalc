//! The descriptive statistics family fragment ported from OxFunc (functions/ at 6fca3518):
//! deviations and means, the order statistics (MEDIAN, MODE, LARGE, SMALL, RANK, PERCENTILE,
//! QUARTILE, PERCENTRANK), variances and standard deviations, the paired statistics (COVAR,
//! CORREL, SLOPE, INTERCEPT, STEYX, FORECAST and the like), the moments, TRIMMEAN, STANDARDIZE,
//! FISHER, PROB, FREQUENCY, and LINEST, LOGEST, TREND and GROWTH. The numeric kernels are OxFunc's ([`crate::ported`]).
//!
//! The lists are read by the aggregate family's rule ([`super::aggregate`]): a direct argument
//! through number coercion, a held value (a cell of a reference, an element of an array) as a
//! number only, text, logicals and blank cells skipped; the A-suffixed functions read held text
//! as 0 and held logicals as 1 and 0. MODE also skips direct text and logicals.
//! The paired statistics skip a position where either list
//! holds no number, and lists of different sizes are #N/A.

use super::aggregate::Held;
use super::kit::{fail, given, grid, num, numbers, opt, out, out_matrix, out_with_subnormal, pairs, PairScalars};
use crate::call::EvalServices;
use crate::kernel::{Exit, Site, Step};
use crate::ported::paired_stats_common::{self as paired, CovarianceDivisor};
use crate::ported::percentrank_common::{PercentRankMode, percentrank};
use crate::ported::rank_common::{RankOrder, rank_avg, rank_eq};
use crate::ported::variance_common::{VarianceDivisor, stdev_from_values, variance_from_values};
use crate::ported::{self as k};
use crate::value::{CalcArray, CalcValue, CoreRef, WorksheetErrorCode};
use std::collections::BTreeMap;
use crate::ported::excel_numeric::{crt_exp, crt_log};

const CODE: u16 = 103;

mod ets;

/// Every argument's numbers by the aggregate rule `held`.
fn all(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>, held: Held) -> Step<Vec<f64>> {
    numbers(site, a, 0, a.len(), sv, held)
}

/// Argument `index`'s numbers by the aggregate rule for numbers.
fn list(site: &Site<'_>, a: &[CalcValue], index: usize, sv: &mut EvalServices<'_>) -> Step<Vec<f64>> {
    numbers(site, a, index, index + 1, sv, Held::Numbers)
}

/// Reads the direct arguments through number coercion first, so that their error comes before
/// any held error, as MEDIAN, DEVSQ and HARMEAN order them.
fn direct_first(site: &Site<'_>, a: &[CalcValue]) -> Step<()> {
    for (index, argument) in a.iter().enumerate() {
        if !matches!(argument.core_ref(), CoreRef::Reference(_) | CoreRef::Array(_)) {
            site.number_argument(index, argument)?;
        }
    }
    Ok(())
}

fn sorted(mut values: Vec<f64>) -> Vec<f64> {
    values.sort_by(f64::total_cmp);
    values
}

/// The k-th largest (`largest`) or smallest number of argument 0.
fn nth(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>, largest: bool) -> Step<CalcValue> {
    let values = list(site, a, 0, sv)?;
    let k = num(site, a, 1)?.trunc();
    if k < 1.0 || k > values.len() as f64 {
        return fail(WorksheetErrorCode::Num);
    }
    let mut values = sorted(values);
    if largest {
        values.reverse();
    }
    site.number(values[k as usize - 1])
}

/// RANK's number: a number as it is, an error as itself, anything else #N/A.
fn rank_number(a: &[CalcValue]) -> Step<f64> {
    match a[0].core_ref() {
        CoreRef::Number(n) => Ok(n),
        CoreRef::Error(code) => fail(code),
        _ => fail(WorksheetErrorCode::NA),
    }
}

fn rank(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>, average: bool) -> Step<CalcValue> {
    let number = rank_number(a)?;
    let values = list(site, a, 1, sv)?;
    let order = if given(a, 2) && num(site, a, 2)?.trunc() != 0.0 {
        RankOrder::Ascending
    } else {
        RankOrder::Descending
    };
    let r = if average {
        rank_avg(number, &values, order)
    } else {
        rank_eq(number, &values, order)
    };
    out(site, r)
}

fn percentile(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>, exclusive: bool) -> Step<CalcValue> {
    let mut values = list(site, a, 0, sv)?;
    let p = num(site, a, 1)?;
    out(
        site,
        if exclusive {
            k::percentile_common::percentile_exc_kernel(&mut values, p)
        } else {
            k::percentile_common::percentile_inc_kernel(&mut values, p)
        },
    )
}

fn quartile(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>, exclusive: bool) -> Step<CalcValue> {
    let mut values = list(site, a, 0, sv)?;
    let q = num(site, a, 1)?.trunc();
    let admitted = if exclusive { 1.0..=3.0 } else { 0.0..=4.0 };
    if !admitted.contains(&q) {
        return fail(WorksheetErrorCode::Num);
    }
    out(
        site,
        if exclusive {
            k::percentile_common::percentile_exc_kernel(&mut values, q / 4.0)
        } else {
            k::percentile_common::percentile_inc_kernel(&mut values, q / 4.0)
        },
    )
}

fn percent_rank(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>, mode: PercentRankMode) -> Step<CalcValue> {
    let mut values = list(site, a, 0, sv)?;
    let x = num(site, a, 1)?;
    let significance = opt(site, a, 2, 3.0)?.trunc() as i64;
    out(site, percentrank(&mut values, x, significance, mode))
}

fn variance(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>, held: Held, divisor: VarianceDivisor, root: bool) -> Step<CalcValue> {
    let values = all(site, a, sv, held)?;
    out(
        site,
        if root {
            stdev_from_values(&values, divisor)
        } else {
            variance_from_values(&values, divisor)
        },
    )
}

/// The most frequent numbers, each with its count, in order of first appearance.
fn modes(values: &[f64]) -> Vec<f64> {
    let mut counts: BTreeMap<u64, (usize, usize)> = BTreeMap::new();
    for (position, value) in values.iter().enumerate() {
        let key = (if *value == 0.0 { 0.0 } else { *value }).to_bits();
        counts.entry(key).or_insert((position, 0)).1 += 1;
    }
    let best = counts.values().map(|(_, c)| *c).max().unwrap_or(0);
    if best < 2 {
        return Vec::new();
    }
    let mut found: Vec<(usize, f64)> = counts
        .iter()
        .filter(|(_, (_, c))| *c == best)
        .map(|(key, (first, _))| (*first, f64::from_bits(*key)))
        .collect();
    found.sort_by_key(|(first, _)| *first);
    found.into_iter().map(|(_, v)| v).collect()
}

/// FORECAST's line through the pairs (x, y), evaluated at `x`.
fn forecast(site: &Site<'_>, x: f64, pairs: &[(f64, f64)]) -> Step<CalcValue> {
    let n = pairs.len();
    if n == 0 {
        return fail(WorksheetErrorCode::Div0);
    }
    let nf = n as f64;
    let mean_x = pairs.iter().map(|(x, _)| *x).sum::<f64>() / nf;
    let mean_y = pairs.iter().map(|(_, y)| *y).sum::<f64>() / nf;
    let (mut num, mut den) = (0.0, 0.0);
    for (xv, yv) in pairs {
        let dx = xv - mean_x;
        num += dx * (yv - mean_y);
        den += dx * dx;
    }
    if den == 0.0 {
        return fail(WorksheetErrorCode::Div0);
    }
    let b = num / den;
    let a = mean_y - b * mean_x;
    site.number(a + b * x)
}

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
ported_family! {
    code: CODE;
    1 avedev "AVEDEV" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        let values = all(s, a, sv, Held::Numbers)?;
        if values.is_empty() {
            return fail(WorksheetErrorCode::Num);
        }
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let avedev = values.iter().map(|v| (v - mean).abs()).sum::<f64>() / values.len() as f64;
        s.number(avedev)
    }
    2 devsq "DEVSQ" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        direct_first(s, a)?;
        let values = all(s, a, sv, Held::Numbers)?;
        if values.is_empty() {
            return fail(WorksheetErrorCode::Num);
        }
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let devsq = values
            .iter()
            .map(|v| {
                let delta = v - mean;
                let square = k::excel_numeric::excel_x87_mul(delta, delta);
                if square < f64::MIN_POSITIVE { 0.0 } else { square }
            })
            .sum::<f64>();
        s.number(devsq)
    }
    3 geomean "GEOMEAN" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        let values = all(s, a, sv, Held::Numbers)?;
        if values.is_empty() || values.iter().any(|v| *v <= 0.0) {
            return fail(WorksheetErrorCode::Num);
        }
        // The equal-positive identity also avoids product underflow followed
        // by a log/exp round trip across the minimum-normal boundary.
        if values.iter().all(|value| *value == values[0]) {
            return s.number(values[0]);
        }
        // R26: each product assignment stores from x87 PC64. Independent
        // GEOMEAN(product,1,1) readings distinguish this loop from plain
        // binary64 without assuming the private root implementation.
        // The current root reconstruction still has recorded mismatches.
        // The overflow/underflow fallback remains a separate open question.
        let n = values.len() as f64;
        let product = values.iter().fold(1.0, |p, &x| k::excel_numeric::excel_x87_mul(p, x));
        if product.is_normal() {
            s.number(k::excel_numeric::excel_root_x87(product, values.len()))
        } else {
            let ln = values.iter().map(|v| crt_log(*v)).sum::<f64>();
            s.number(crt_exp(ln / n))
        }
    }
    4 harmean "HARMEAN" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        direct_first(s, a)?;
        let values = all(s, a, sv, Held::Numbers)?;
        let mut reciprocal_sum = 0.0;
        for v in &values {
            if *v <= 0.0 {
                return fail(WorksheetErrorCode::Num);
            }
            reciprocal_sum = k::excel_numeric::excel_x87_add(
                reciprocal_sum,
                k::excel_numeric::excel_x87_recip(*v),
            );
        }
        if values.is_empty() {
            return fail(WorksheetErrorCode::NA);
        }
        if !reciprocal_sum.is_finite() {
            return fail(WorksheetErrorCode::Num);
        }
        let average = k::excel_numeric::excel_x87_div(reciprocal_sum, values.len() as f64);
        s.number(k::excel_numeric::excel_x87_recip(average))
    }
    5 median "MEDIAN" (1, 255) [R "number1" ~ AggregateUnderArray, R "number2" ~ AggregateUnderArrayRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        direct_first(s, a)?;
        let values = sorted(all(s, a, sv, Held::Numbers)?);
        if values.is_empty() {
            return fail(WorksheetErrorCode::Num);
        }
        let mid = values.len() / 2;
        let result = if values.len() % 2 == 1 {
            values[mid]
        } else {
            let half = (values[mid] - values[mid - 1]) / 2.0;
            values[mid - 1] + if half < f64::MIN_POSITIVE { 0.0 } else { half }
        };
        // W111's cancellation pairs have normal inputs and retain subnormal
        // results. Their input coercion and middle-value arithmetic stay the same.
        out_with_subnormal(s, Ok(result))
    }
    6 mode "MODE" (1, 255) [R "number1" ~ ArrayAtRoot, R "number2" ~ ArrayAtRootRepeated] three_d (None, None, Some(&[])); |s, a, sv| {
        match modes(&all(s, a, sv, Held::ModeNumbers)?).first() {
            Some(m) => s.number(*m),
            None => fail(WorksheetErrorCode::NA),
        }
    }
    7 mode_sngl "MODE.SNGL" (1, 255) [R "number1" ~ ArrayAtRoot, R "number2" ~ ArrayAtRootRepeated] three_d (None, None, Some(&[])); |s, a, sv| {
        match modes(&all(s, a, sv, Held::ModeNumbers)?).first() {
            Some(m) => s.number(*m),
            None => fail(WorksheetErrorCode::NA),
        }
    }
    8 mode_mult "MODE.MULT" (1, 255) [R "number1" ~ ArrayAtRoot, R "number2" ~ ArrayAtRootRepeated] three_d (None, None, Some(&[])); |s, a, sv| {
        let found = modes(&all(s, a, sv, Held::ModeNumbers)?);
        if found.is_empty() {
            return fail(WorksheetErrorCode::NA);
        }
        let rows: Vec<Vec<f64>> = found.into_iter().map(|m| vec![m]).collect();
        out_matrix(s, &rows)
    }
    9 large "LARGE" (2, 2) [R "array" ~ AggregateAtRoot, V "k"] three_d (Some(&[0]), None, Some(&[1])); |s, a, sv| { nth(s, a, sv, true) }
    10 small "SMALL" (2, 2) [R "array" ~ AggregateAtRoot, V "k"] three_d (Some(&[0]), None, Some(&[1])); |s, a, sv| { nth(s, a, sv, false) }
    // The ref of RANK, RANK.EQ and RANK.AVG takes only an operand that may give a reference
    // (dc-mx3x, Excel 16.0.20527, 2026-11-07, Formula2 and Formula alike): `=RANK(1,2)`,
    // `=RANK(1,{1,2})` and `=RANK(1,1+1)` are refused, `=RANK(1,B1:B3)` and
    // `=RANK(1,INDEX(B1:B3,0))` enter.
    11 rank_legacy "RANK" (2, 3) [V "number", F "ref" @Reference, V "order"] three_d (Some(&[1]), None, Some(&[0, 2])); |s, a, sv| { rank(s, a, sv, false) }
    12 rank_eq_fn "RANK.EQ" (2, 3) [V "number", F "ref" @Reference, V "order"] three_d (Some(&[1]), None, Some(&[0, 2])); |s, a, sv| { rank(s, a, sv, false) }
    13 rank_avg_fn "RANK.AVG" (2, 3) [V "number", F "ref" @Reference, V "order"] three_d (Some(&[1]), None, Some(&[0, 2])); |s, a, sv| { rank(s, a, sv, true) }
    14 percentile_legacy "PERCENTILE" (2, 2) [R "array" ~ AggregateAtRoot, V "k"] three_d (Some(&[0]), None, Some(&[1])); |s, a, sv| { percentile(s, a, sv, false) }
    15 percentile_inc "PERCENTILE.INC" (2, 2) [R "array" ~ AggregateAtRoot, V "k"] three_d (Some(&[0]), None, Some(&[1])); |s, a, sv| { percentile(s, a, sv, false) }
    16 percentile_exc "PERCENTILE.EXC" (2, 2) [R "array" ~ AggregateAtRoot, V "k"] three_d (Some(&[0]), None, Some(&[1])); |s, a, sv| { percentile(s, a, sv, true) }
    17 quartile_legacy "QUARTILE" (2, 2) [R "array" ~ AggregateAtRoot, V "quart"] three_d (Some(&[0]), None, Some(&[1])); |s, a, sv| { quartile(s, a, sv, false) }
    18 quartile_inc "QUARTILE.INC" (2, 2) [R "array" ~ AggregateAtRoot, V "quart"] three_d (Some(&[0]), None, Some(&[1])); |s, a, sv| { quartile(s, a, sv, false) }
    19 quartile_exc "QUARTILE.EXC" (2, 2) [R "array" ~ AggregateAtRoot, V "quart"] three_d (Some(&[0]), None, Some(&[1])); |s, a, sv| { quartile(s, a, sv, true) }
    20 percentrank_legacy "PERCENTRANK" (2, 3) [R "array" ~ AggregateAtRoot, V "x", V "significance"] three_d (Some(&[0]), None, Some(&[1, 2])); |s, a, sv| {
        percent_rank(s, a, sv, PercentRankMode::Inclusive)
    }
    21 percentrank_inc "PERCENTRANK.INC" (2, 3) [R "array" ~ AggregateAtRoot, V "x", V "significance"] three_d (Some(&[0]), None, Some(&[1, 2])); |s, a, sv| {
        percent_rank(s, a, sv, PercentRankMode::Inclusive)
    }
    22 percentrank_exc "PERCENTRANK.EXC" (2, 3) [R "array" ~ AggregateAtRoot, V "x", V "significance"] three_d (Some(&[0]), None, Some(&[1, 2])); |s, a, sv| {
        percent_rank(s, a, sv, PercentRankMode::Exclusive)
    }
    23 stdev "STDEV" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        variance(s, a, sv, Held::Numbers, VarianceDivisor::Sample, true)
    }
    24 stdev_s "STDEV.S" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        variance(s, a, sv, Held::Numbers, VarianceDivisor::Sample, true)
    }
    25 stdev_p "STDEV.P" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        variance(s, a, sv, Held::Numbers, VarianceDivisor::Population, true)
    }
    26 stdeva "STDEVA" (1, 255) [R "value1" ~ AggregateAtRoot, R "value2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        variance(s, a, sv, Held::Values, VarianceDivisor::Sample, true)
    }
    27 stdevp "STDEVP" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        variance(s, a, sv, Held::Numbers, VarianceDivisor::Population, true)
    }
    28 stdevpa "STDEVPA" (1, 255) [R "value1" ~ AggregateAtRoot, R "value2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        variance(s, a, sv, Held::Values, VarianceDivisor::Population, true)
    }
    29 var "VAR" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        variance(s, a, sv, Held::Numbers, VarianceDivisor::Sample, false)
    }
    30 var_s "VAR.S" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        variance(s, a, sv, Held::Numbers, VarianceDivisor::Sample, false)
    }
    31 var_p "VAR.P" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        variance(s, a, sv, Held::Numbers, VarianceDivisor::Population, false)
    }
    32 vara "VARA" (1, 255) [R "value1" ~ AggregateAtRoot, R "value2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        variance(s, a, sv, Held::Values, VarianceDivisor::Sample, false)
    }
    33 varp "VARP" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        variance(s, a, sv, Held::Numbers, VarianceDivisor::Population, false)
    }
    34 varpa "VARPA" (1, 255) [R "value1" ~ AggregateAtRoot, R "value2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        variance(s, a, sv, Held::Values, VarianceDivisor::Population, false)
    }
    35 covar "COVAR" (2, 2) [R "array1" ~ ArrayAtRoot, R "array2" ~ ArrayAtRoot] three_d (None, None, Some(&[])); |s, a, sv| {
        out(s, paired::covariance_from_pairs(&pairs(s, a, 0, 1, PairScalars::SkipNonnumeric, sv)?, CovarianceDivisor::Population))
    }
    36 covariance_p "COVARIANCE.P" (2, 2) [R "array1" ~ ArrayAtRoot, R "array2" ~ ArrayAtRoot] three_d (None, None, Some(&[])); |s, a, sv| {
        out(s, paired::covariance_from_pairs(&pairs(s, a, 0, 1, PairScalars::SkipNonnumeric, sv)?, CovarianceDivisor::Population))
    }
    37 covariance_s "COVARIANCE.S" (2, 2) [R "array1" ~ ArrayAtRoot, R "array2" ~ ArrayAtRoot] three_d (None, None, Some(&[])); |s, a, sv| {
        out(s, paired::covariance_from_pairs(&pairs(s, a, 0, 1, PairScalars::SkipNonnumeric, sv)?, CovarianceDivisor::Sample))
    }
    38 correl "CORREL" (2, 2) [R "array1" ~ ArrayAtRoot, R "array2" ~ ArrayAtRoot] three_d (None, None, Some(&[])); |s, a, sv| {
        out(s, paired::correlation_from_pairs(&pairs(s, a, 0, 1, PairScalars::SkipNonnumeric, sv)?))
    }
    39 pearson "PEARSON" (2, 2) [R "array1" ~ ArrayAtRoot, R "array2" ~ ArrayAtRoot] three_d (None, None, Some(&[])); |s, a, sv| {
        out(s, paired::correlation_from_pairs(&pairs(s, a, 0, 1, PairScalars::SkipNonnumeric, sv)?))
    }
    40 rsq "RSQ" (2, 2) [R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAtRoot] three_d (None, None, Some(&[])); |s, a, sv| {
        out(s, paired::rsq_from_pairs(&pairs(s, a, 0, 1, PairScalars::SkipNonnumeric, sv)?))
    }
    41 slope "SLOPE" (2, 2) [R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAtRoot] three_d (None, None, Some(&[])); |s, a, sv| {
        out(s, paired::slope_from_pairs(&pairs(s, a, 1, 0, PairScalars::SkipNonnumeric, sv)?))
    }
    42 intercept "INTERCEPT" (2, 2) [R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAtRoot] three_d (None, None, Some(&[])); |s, a, sv| {
        out(s, paired::intercept_from_pairs(&pairs(s, a, 1, 0, PairScalars::SkipNonnumeric, sv)?))
    }
    43 steyx "STEYX" (2, 2) [R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAtRoot] three_d (None, None, Some(&[])); |s, a, sv| {
        out(s, k::moment_stats_family::steyx_kernel(&pairs(s, a, 1, 0, PairScalars::SkipNonnumeric, sv)?))
    }
    44 forecast_legacy "FORECAST" (3, 3) [V "x", R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAtRoot] three_d (None, None, Some(&[])); |s, a, sv| {
        let x = num(s, a, 0)?;
        forecast(s, x, &pairs(s, a, 2, 1, PairScalars::Coerce, sv)?)
    }
    45 forecast_linear "FORECAST.LINEAR" (3, 3) [V "x", R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAtRoot] three_d (None, None, Some(&[])); |s, a, sv| {
        let x = num(s, a, 0)?;
        forecast(s, x, &pairs(s, a, 2, 1, PairScalars::Coerce, sv)?)
    }
    46 kurt "KURT" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        out(s, k::moment_stats_family::kurt_kernel(&all(s, a, sv, Held::Numbers)?))
    }
    47 skew "SKEW" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        out(s, k::moment_stats_family::skew_kernel(&all(s, a, sv, Held::Numbers)?))
    }
    48 skew_p "SKEW.P" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); |s, a, sv| {
        out(s, k::moment_stats_family::skew_p_kernel(&all(s, a, sv, Held::Numbers)?))
    }
    49 trimmean "TRIMMEAN" (2, 2) [R "array" ~ AggregateAtRoot, V "percent"] three_d (Some(&[0]), None, Some(&[1])); |s, a, sv| {
        let mut values = list(s, a, 0, sv)?;
        let percent = num(s, a, 1)?;
        out(s, k::moment_stats_family::trimmean_kernel(&mut values, percent))
    }
    50 standardize "STANDARDIZE" (3, 3) [V "x", V "mean", V "standard_dev"] three_d (None, None, Some(&[])); |s, a, sv| {
        let (x, mean, sd) = (num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?);
        out(s, k::standardize_fn::standardize_kernel(x, mean, sd))
    }
    51 fisher "FISHER" (1, 1) [V "x"] three_d (None, None, Some(&[])); |s, a, sv| {
        out(s, k::fisher_fn::fisher_kernel(num(s, a, 0)?))
    }
    52 fisherinv "FISHERINV" (1, 1) [V "y"] three_d (None, None, Some(&[])); |s, a, sv| {
        out(s, k::fisherinv_fn::fisherinv_kernel(num(s, a, 0)?))
    }
    53 prob "PROB" (3, 4) [R "x_range" ~ ArrayAtRoot, R "prob_range" ~ ArrayAtRoot, V "lower_limit", V "upper_limit"] three_d (None, None, Some(&[])); |s, a, sv| {
        let xs = strict_numbers(s, a, 0, sv)?;
        let ps = strict_numbers(s, a, 1, sv)?;
        let lower = num(s, a, 2)?;
        let upper = if given(a, 3) { Some(num(s, a, 3)?) } else { None };
        out(s, k::lookup_prob_frequency_family::prob_kernel(&xs, &ps, lower, upper))
    }
    54 frequency "FREQUENCY" (2, 2) [R "data_array" ~ ArrayAtRoot, R "bins_array" ~ ArrayAtRoot] three_d (Some(&[0, 1]), None, None); |s, a, sv| {
        let data = list(s, a, 0, sv)?;
        let bins = list(s, a, 1, sv)?;
        out_matrix(s, &frequency_counts(&data, &bins))
    }
    55 trend "TREND" (1, 4) [R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAtRoot, R "new_xs", V "const"] three_d (None, Some(&[]), None); |s, a, sv| {
        regression(s, a, sv, k::regression_forecast_family::eval_trend_surface, RegressionNumbers::Standard)
    }
    56 growth "GROWTH" (1, 4) [R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAtRoot, R "new_xs", V "const"] three_d (None, Some(&[]), None); |s, a, sv| {
        regression(s, a, sv, k::regression_forecast_family::eval_growth_surface, RegressionNumbers::Forecast)
    }
    // R25 legacy INDEX captures retain both predictor columns below LINEST and LOGEST.
    57 linest "LINEST" (1, 4) [R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAlways, V "const", V "stats"] three_d (None, Some(&[]), None); |s, a, sv| {
        regression(s, a, sv, k::regression_forecast_family::eval_linest_surface, RegressionNumbers::Standard)
    }
    58 logest "LOGEST" (1, 4) [R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAlways, V "const", V "stats"] three_d (None, Some(&[]), None); |s, a, sv| {
        regression(s, a, sv, k::regression_forecast_family::eval_logest_surface, RegressionNumbers::Coefficients)
    }
    59 forecast_ets "FORECAST.ETS" (3, 6) [V "target_date", R "values" ~ ArrayAlways, R "timeline" ~ ArrayAlways, V "seasonality", V "data_completion", V "aggregation"] three_d (None, None, Some(&[])); |s, a, sv| {
        ets::evaluate(s, a, sv, ets::Output::Forecast)
    }
    60 forecast_ets_confint "FORECAST.ETS.CONFINT" (3, 7) [V "target_date", R "values" ~ ArrayAlways, R "timeline" ~ ArrayAlways, V "confidence_level", V "seasonality", V "data_completion", V "aggregation"] three_d (None, None, Some(&[])); |s, a, sv| {
        ets::evaluate(s, a, sv, ets::Output::Confidence)
    }
    61 forecast_ets_seasonality "FORECAST.ETS.SEASONALITY" (2, 4) [R "values" ~ ArrayAlways, R "timeline" ~ ArrayAlways, V "data_completion", V "aggregation"] three_d (None, None, Some(&[])); |s, a, sv| {
        ets::evaluate(s, a, sv, ets::Output::Seasonality)
    }
    62 forecast_ets_stat "FORECAST.ETS.STAT" (3, 6) [R "values" ~ ArrayAlways, R "timeline" ~ ArrayAlways, V "statistic_type", V "seasonality", V "data_completion", V "aggregation"] three_d (None, None, Some(&[])); |s, a, sv| {
        ets::evaluate(s, a, sv, ets::Output::Statistic)
    }
}

#[derive(Clone, Copy)]
enum RegressionNumbers {
    Standard,
    Forecast,
    Coefficients,
}

/// LINEST, LOGEST, TREND and GROWTH: OxFunc's evaluation over the arguments with each reference
/// read into an array (a blank cell is Empty, which those functions refuse with #VALUE!), its
/// number results published by the number rule.
fn regression(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
    evaluate: fn(&[CalcValue]) -> Result<CalcValue, WorksheetErrorCode>,
    publication: RegressionNumbers,
) -> Step<CalcValue> {
    let mut read = Vec::with_capacity(a.len());
    for (index, argument) in a.iter().enumerate() {
        read.push(match argument.core_ref() {
            CoreRef::Reference(_) => {
                let g = grid(site, a, index, sv)?;
                let rows: Vec<Vec<CalcValue>> =
                    g.cells.chunks(g.cols).map(<[CalcValue]>::to_vec).collect();
                CalcValue::array(
                    CalcArray::from_rows(rows).ok_or_else(|| site.not_modelled(Some(index)))?,
                )
            }
            _ => argument.clone(),
        });
    }
    let result = evaluate(&read).map_err(Exit::Error)?;
    // The retained coefficient-publication observations distinguish LOGEST's
    // subnormal coefficient row from GROWTH's zero forecasts. Statistics rows
    // keep the existing number rule; normal and nonfinite outputs do too.
    let number = |n: f64, first_row: bool| match publication {
        RegressionNumbers::Coefficients if first_row && n.is_subnormal() => out_with_subnormal(site, Ok(n)),
        RegressionNumbers::Forecast if n.is_subnormal() => site.number(0.0),
        _ => site.number(n),
    };
    match result.core_ref() {
        CoreRef::Number(n) => number(n, true),
        CoreRef::Array(array) => {
            let mut rows = Vec::with_capacity(array.shape().rows);
            for r in 0..array.shape().rows {
                let mut row = Vec::with_capacity(array.shape().cols);
                for cell in array.row_slice(r).unwrap_or(&[]) {
                    row.push(match cell.core_ref() {
                        CoreRef::Number(n) => match number(n, r == 0) {
                            Ok(v) => v,
                            Err(Exit::Error(code)) => CalcValue::error(code),
                            Err(fault) => return Err(fault),
                        },
                        _ => cell.clone(),
                    });
                }
                rows.push(row);
            }
            CalcArray::from_rows(rows)
                .map(CalcValue::array)
                .ok_or_else(|| site.not_modelled(None))
        }
        _ => Ok(result),
    }
}

/// PROB's lists: every cell must hold a number (an error cell is the result, anything else
/// #VALUE!).
fn strict_numbers(site: &Site<'_>, a: &[CalcValue], index: usize, sv: &mut EvalServices<'_>) -> Step<Vec<f64>> {
    let g = grid(site, a, index, sv)?;
    let mut found = Vec::with_capacity(g.cells.len());
    for cell in &g.cells {
        match cell.core_ref() {
            CoreRef::Number(n) => found.push(n),
            CoreRef::Error(code) => return fail(code),
            _ => return fail(WorksheetErrorCode::Value),
        }
    }
    Ok(found)
}

/// FREQUENCY's counts, one per bin in the bins' order and one more for the numbers above every
/// bin: a bin counts the numbers above the next smaller bin and at most itself; a repeated bin
/// counts in its first place and 0 in the others.
fn frequency_counts(data: &[f64], bins: &[f64]) -> Vec<Vec<f64>> {
    let mut counts = vec![0.0; bins.len() + 1];
    for value in data {
        let mut best: Option<usize> = None;
        for (index, bin) in bins.iter().enumerate() {
            if value <= bin && best.is_none_or(|b| *bin < bins[b]) {
                best = Some(index);
            }
        }
        counts[best.unwrap_or(bins.len())] += 1.0;
    }
    counts.into_iter().map(|c| vec![c]).collect()
}
