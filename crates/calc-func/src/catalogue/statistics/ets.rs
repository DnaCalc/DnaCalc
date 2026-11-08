//! ETS's measured numeric-vector surface (dc-j9k, Excel20527, R38).
//!
//! Timeline preparation, option errors, step statistics and the exact, nonseasonal
//! zero-residual model are implemented. Parameter optimisation, nonzero residuals,
//! seasonal states and calendar inference still belong to dc-j9k. In particular,
//! bit-identical input values do NOT justify returning that value: adjacent-load
//! measurements expose rounding and changed optimisation even on constant data.

use super::super::kit::{fail, grid, num, opt};
use crate::call::{EvalServices, FunctionFault};
use crate::kernel::{Exit, Site, Step};
use crate::value::{CalcValue, CoreRef, WorksheetErrorCode};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Output { Forecast, Confidence, Seasonality, Statistic }

fn unfinished(site: &Site<'_>) -> Exit {
    Exit::Fault(FunctionFault::NotHeld { id: site.id(), owner: "dc-j9k" })
}

fn vector(site: &Site<'_>, args: &[CalcValue], i: usize, sv: &mut EvalServices<'_>) -> Step<Vec<f64>> {
    let g = grid(site, args, i, sv)?;
    if g.rows > 1 && g.cols > 1 { return Err(unfinished(site)); }
    g.cells.iter().map(|v| match v.core_ref() {
        CoreRef::Number(v) => Ok(v),
        CoreRef::Error(e) => fail(e),
        _ => Err(unfinished(site)),
    }).collect()
}

struct Series { values: Vec<f64>, first: f64, last: f64, step: f64, ordered: bool, preparation_exact: bool }

fn exact_ratio(a: f64, b: f64) -> Option<f64> {
    let q = a / b;
    (q.is_finite() && q.mul_add(b, -a) == 0.0).then_some(q)
}

/// Every subset sum fits the binary64 integer lattice. This is sufficient
/// regardless of the private duplicate reduction's order or working precision.
fn exact_total(values: &[f64]) -> Option<f64> {
    let minimum = quantum_exponent(values);
    if !(-900..=900).contains(&minimum) { return None; }
    let quantum = 2.0_f64.powi(minimum);
    let mut absolute = 0u128;
    for &v in values {
        let scaled = (v / quantum).abs();
        if !scaled.is_finite() || scaled.fract() != 0.0 || scaled > (1u64 << 52) as f64 { return None; }
        absolute += scaled as u128;
    }
    (absolute <= 1u128 << 52).then(|| values.iter().sum())
}

fn prepare(site: &Site<'_>, y: Vec<f64>, t: Vec<f64>, completion: f64, aggregation: f64) -> Step<Series> {
    if y.len() != t.len() { return fail(WorksheetErrorCode::NA); }
    if y.len() < 2 { return fail(WorksheetErrorCode::Div0); }
    let ordered = t.windows(2).all(|p| p[0] <= p[1]);
    let mut pairs: Vec<_> = t.into_iter().zip(y).collect();
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut times = Vec::new();
    let mut values = Vec::new();
    let mut preparation_exact = true;
    let mut i = 0;
    while i < pairs.len() {
        let start = i;
        let time = pairs[i].0;
        while i < pairs.len() && pairs[i].0 == time { i += 1; }
        let mut group: Vec<_> = pairs[start..i].iter().map(|p| p.1).collect();
        if group.len() > 1 {
            let exact = match aggregation as u8 {
                1 => exact_total(&group).and_then(|s| exact_ratio(s, group.len() as f64)).is_some(),
                7 => exact_total(&group).is_some(),
                _ => true,
            };
            preparation_exact &= exact;
        }
        let value = match aggregation as u8 {
            1 => group.iter().sum::<f64>() / group.len() as f64,
            2 | 3 => group.len() as f64,
            4 => group.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            5 => {
                group.sort_by(f64::total_cmp);
                let m = group.len() / 2;
                if group.len() % 2 == 0 {
                    preparation_exact &= exact_total(&group[m - 1..=m])
                        .and_then(|s| exact_ratio(s, 2.0)).is_some();
                    (group[m - 1] + group[m]) / 2.0
                } else { group[m] }
            }
            6 => group.iter().copied().fold(f64::INFINITY, f64::min),
            7 => group.iter().sum(),
            _ => unreachable!(),
        };
        times.push(time);
        values.push(value);
    }
    if values.iter().any(|v| !v.is_finite()) { return Err(unfinished(site)); }
    if times.len() < 2 { return fail(WorksheetErrorCode::Num); }
    let step = times.windows(2).map(|p| p[1] - p[0]).fold(f64::INFINITY, f64::min);
    if !step.is_finite() || step <= 0.0 { return Err(unfinished(site)); }
    let mut gaps = Vec::new();
    let mut count = 1usize;
    for pair in times.windows(2) {
        let gap = (pair[1] - pair[0]) / step;
        // Calendar-month inference and almost-integral timestamps need separate
        // measurements. Never turn an unidentified timeline into a worksheet error.
        if gap.fract() != 0.0 || gap < 1.0 || gap > 1_000_000.0 { return Err(unfinished(site)); }
        count = count.checked_add(gap as usize).ok_or_else(|| unfinished(site))?;
        gaps.push(gap as usize);
    }
    if count > 128 { return Err(unfinished(site)); }
    if (count - times.len()) * 10 > count * 3 { return fail(WorksheetErrorCode::Num); }
    let mut completed = Vec::with_capacity(count);
    completed.push(values[0]);
    for (i, &gap) in gaps.iter().enumerate() {
        for j in 1..gap {
            if completion != 0.0 {
                let exact = exact_sum(values[i + 1], -values[i]).and_then(|difference| {
                    let fraction = exact_ratio(j as f64, gap as f64)?;
                    let product = difference * fraction;
                    if difference.mul_add(fraction, -product) != 0.0 { return None; }
                    exact_sum(values[i], product)
                });
                preparation_exact &= exact.is_some();
            }
            completed.push(if completion == 0.0 { 0.0 } else {
                values[i] + (values[i + 1] - values[i]) * (j as f64 / gap as f64)
            });
        }
        completed.push(values[i + 1]);
    }
    if completed.iter().any(|v| !v.is_finite()) { return Err(unfinished(site)); }
    Ok(Series { values: completed, first: times[0], last: *times.last().unwrap(), step, ordered, preparation_exact })
}

/// Sufficient exact-arithmetic region for the initial ordinary least-squares
/// trend and its zero innovations. The bound follows the moment sums' n^3
/// growth and binary64's 53 bits; it is not a cutoff fitted to answer counts.
fn quantum_exponent(y: &[f64]) -> i32 {
    y.iter().filter(|&&v| v != 0.0).map(|v| {
        let raw = v.to_bits() & 0x7fff_ffff_ffff_ffff;
        let exp = ((raw >> 52) & 0x7ff) as i32;
        let mantissa = (raw & ((1u64 << 52) - 1)) | (1u64 << 52);
        exp - 1023 - 52 + mantissa.trailing_zeros() as i32
    }).min().unwrap_or(0)
}

fn exact_trend(y: &[f64]) -> Option<f64> {
    let n = y.len();
    let minimum = quantum_exponent(y);
    if !(-900..=900).contains(&minimum) { return None; }
    let quantum = 2.0_f64.powi(minimum);
    let bound = (1u64 << 50) as f64 / (n as f64).powi(3);
    let scaled: Vec<_> = y.iter().map(|v| v / quantum).collect();
    if scaled.iter().any(|v| !v.is_finite() || v.fract() != 0.0 || v.abs() > bound) { return None; }
    let difference = scaled[1] - scaled[0];
    if scaled.iter().enumerate().any(|(i, &v)| v != scaled[0] + i as f64 * difference) { return None; }
    let trend = difference * quantum;
    // The observed nonseasonal default is alpha .9, beta .001, gamma 2^-52.
    // Require the actual state updates to retain zero innovations as well.
    let mut level = y[0];
    for &v in &y[1..] {
        let prediction = level + trend;
        if prediction != v { return None; }
        level = 0.9 * v + (1.0 - 0.9) * prediction;
        if level != v { return None; }
    }
    Some(trend)
}

/// TwoSum retains the addition's rounding residue. Final extrapolation is
/// admitted only when its product and sum are exact; private rounded expression
/// order outside this region has not been identified.
fn exact_sum(a: f64, b: f64) -> Option<f64> {
    let sum = a + b;
    let part = sum - a;
    let residue = (a - (sum - part)) + (b - part);
    (sum.is_finite() && residue == 0.0).then_some(sum)
}

pub(super) fn evaluate(site: &Site<'_>, args: &[CalcValue], sv: &mut EvalServices<'_>, output: Output) -> Step<CalcValue> {
    let offset = usize::from(matches!(output, Output::Forecast | Output::Confidence));
    let y = vector(site, args, offset, sv)?;
    let t = vector(site, args, offset + 1, sv)?;
    // Excel checks paired vector lengths before the scalar option domains.
    if y.len() != t.len() { return fail(WorksheetErrorCode::NA); }
    let statistic = if output == Output::Statistic { num(site, args, 2)? } else { 8.0 };
    let confidence = if output == Output::Confidence { opt(site, args, 3, 0.95)? } else { 0.0 };
    let option_start = match output { Output::Forecast | Output::Statistic => 3, Output::Confidence => 4, Output::Seasonality => 2 };
    let season = if output == Output::Seasonality { 1.0 } else { opt(site, args, option_start, 1.0)? };
    let completion_at = option_start + usize::from(output != Output::Seasonality);
    let completion = opt(site, args, completion_at, 1.0)?;
    let aggregation = opt(site, args, completion_at + 1, 1.0)?;
    if statistic.fract() != 0.0 || !(1.0..=8.0).contains(&statistic)
        || !(0.0..1.0).contains(&confidence)
        // Bisection locates the last admitted integer period at 8784; the
        // error applies to step statistics and confidence zero as well.
        || season < 0.0 || season > 8_784.0 || season.fract() != 0.0
        || (completion != 0.0 && completion != 1.0)
        || aggregation.fract() != 0.0 || !(1.0..=7.0).contains(&aggregation) {
        return fail(WorksheetErrorCode::Num);
    }
    let series = prepare(site, y, t, completion, aggregation)?;
    let target = if offset == 1 { num(site, args, 0)? } else { series.last };
    if target < series.first {
        // A reversed input timeline can admit extrapolation before its minimum
        // where an ascending one returns NUM. Its admission rule is still open.
        return if series.ordered { fail(WorksheetErrorCode::Num) } else { Err(unfinished(site)) };
    }
    if output == Output::Confidence && target <= series.last {
        return fail(WorksheetErrorCode::Num);
    }
    let horizon = if offset == 1 {
        let distance = exact_sum(target, -series.last).ok_or_else(|| unfinished(site))?;
        let h = distance / series.step;
        if h.fract() != 0.0 || h * series.step != distance { return Err(unfinished(site)); }
        // Both target functions share this signed-32-bit future-index limit,
        // including the confidence-zero case (three translated/scaled timelines).
        if h > i32::MAX as f64 { return fail(WorksheetErrorCode::Num); }
        h
    } else { 0.0 };
    if output == Output::Statistic && statistic == 8.0 { return site.number(series.step); }
    if output == Output::Confidence && confidence == 0.0 { return site.number(0.0); }
    if !series.preparation_exact { return Err(unfinished(site)); }
    let trend = exact_trend(&series.values).ok_or_else(|| unfinished(site))?;
    if output == Output::Seasonality { return site.number(0.0); }
    // An explicit period longer than half the data falls back to the
    // nonseasonal model. Otherwise seasonal states remain unimplemented.
    if season > 1.0 && season <= (series.values.len() / 2) as f64 { return Err(unfinished(site)); }
    match output {
        Output::Forecast => {
            let product = horizon * trend;
            // The trend's quantum is at least 2^-900 and h is an integer, so
            // a nonzero multiplication residue cannot underflow here.
            if !product.is_finite() || horizon.mul_add(trend, -product) != 0.0 {
                return Err(unfinished(site));
            }
            site.number(exact_sum(*series.values.last().unwrap(), product)
                .ok_or_else(|| unfinished(site))?)
        }
        Output::Confidence => site.number(0.0),
        Output::Statistic => site.number(match statistic as u8 {
            1 => 0.9,
            2 => 0.001,
            3 => f64::EPSILON,
            _ => 0.0,
        }),
        Output::Seasonality => unreachable!(),
    }
}
