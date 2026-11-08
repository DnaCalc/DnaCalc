//! ETS's measured numeric-vector surface (dc-j9k, Excel20527, R39).
//!
//! Timeline preparation, option errors, step statistics and the exact, nonseasonal
//! zero-residual models and bounded nonseasonal fits are implemented. Short-series
//! initialisation, rounded constant initialisation, the minimum-alpha forecast
//! reset, nonzero seasonal fits, incomplete cycles and calendar inference still
//! belong to dc-j9k. In particular,
//! bit-identical input values do NOT justify returning that value: adjacent-load
//! measurements expose rounding and changed optimisation even on constant data.
//! The corpus counts the remaining nonseasonal last-bit differences as mismatches.

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

// R39's fixed parameter family, measured before the independent random fits.
// The later trend correction can report beta .001 for other alpha values;
// those corrected pairs must not become additional optimisation candidates.
const PARAMETERS: &[(f64, f64)] = &[
    (0.9, 0.001), (0.9, 0.899), (0.833333, 0.75), (0.75, 0.001), (0.75, 0.749),
    (0.666667, 0.166667), (0.666667, 0.5), (0.5, 0.001), (0.5, 0.25), (0.5, 0.499),
    (0.333333, 0.166667), (0.25, 0.001), (0.25, 0.249), (0.166667, 0.0833333),
    (0.126, 0.001), (0.126, 0.125), (0.1, 0.001), (0.1, 0.099), (0.002, 0.001),
    (0.998, 0.099), (0.998, 0.499), (0.998, 0.749), (0.998, 0.899),
];

fn initial_slope(y: &[f64]) -> f64 {
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut sxx = 0.0;
    let mut sxy = 0.0;
    for (i, &v) in y.iter().enumerate() {
        let x = (i + 1) as f64;
        sx += x;
        sy += v;
        sxx += x * x;
        sxy += x * v;
    }
    let n = y.len() as f64;
    (n * sxy - sx * sy) / (n * sxx - sx * sx)
}

fn statistic_window(n: usize) -> usize {
    if n <= 10 { n } else { ((n as f64 * 0.4) as usize).min(10) }
}

struct Model {
    level: f64,
    trend: f64,
    seasons: [f64; 8],
    period: usize,
    errors: Vec<f64>,
    predictions: Vec<f64>,
    trends: Vec<f64>,
}

struct Initial {
    level: f64,
    trend: f64,
    seasons: [f64; 8],
    period: usize,
    gamma: f64,
}

impl Model {
    /// The application and parameter search use this same recurrence. During
    /// validation only the transition runs; observations no longer update it.
    fn run(y: &[f64], alpha: f64, beta: f64, validation: Option<usize>) -> Self {
        Self::run_initialized(y, alpha, beta, validation, Initial {
            level: y[0], trend: initial_slope(y), seasons: [0.0; 8], period: 2, gamma: f64::EPSILON,
        })
    }

    fn run_initialized(y: &[f64], alpha: f64, beta: f64, validation: Option<usize>, initial: Initial) -> Self {
        let mut model = Self { level: initial.level, trend: initial.trend,
            seasons: initial.seasons, period: initial.period,
            errors: vec![0.0], predictions: vec![y[0]], trends: Vec::with_capacity(y.len() - 1) };
        for (j, &v) in y.iter().enumerate().skip(1) {
            let q = j % model.period;
            let prediction = (model.level + model.trend) + model.seasons[q];
            let error = v - prediction;
            model.errors.push(error);
            model.predictions.push(prediction);
            if validation.is_some_and(|start| j >= start) {
                model.level += model.trend;
            } else {
                model.level += model.trend + alpha * error;
                model.trend += beta * error;
                model.seasons[q] += initial.gamma * error;
            }
            model.trends.push(model.trend);
        }
        model
    }

    fn squared_errors(&self) -> f64 {
        self.errors.iter().fold(0.0, |sum, &v| sum + v * v)
    }

    fn finish(&mut self, y: &[f64], beta: &mut f64) -> bool {
        // This window is independent of STAT's tail. It retains the transitions
        // within the final half of the observations, including its first state.
        let window = if y.len() <= 10 { y.len() } else { y.len().div_ceil(2) };
        let count = window - 1;
        let trends = &self.trends[self.trends.len() - count..];
        let mean = trends.iter().fold(0.0, |sum, &v| sum + v) / count as f64;
        let variance = trends.iter().fold(0.0, |sum, &v| sum + (v - mean) * (v - mean)) / count as f64;
        let reset = mean.abs() < variance.sqrt() * window as f64 / count as f64;
        if reset {
            self.level = *y.last().unwrap() - self.seasons[(y.len() - 1) % 2];
            self.trend = initial_slope(y);
            *beta = 0.001;
        }
        let center = (self.seasons[0] + self.seasons[1]) / 2.0;
        self.level += center;
        for season in &mut self.seasons[..self.period] { *season -= center; }
        reset
    }

    fn statistics(&self, y: &[f64]) -> [f64; 4] {
        let count = statistic_window(y.len());
        let start = y.len() - count;
        let mae = self.errors[start..].iter().fold(0.0, |sum, &v| sum + v.abs()) / count as f64;
        let rmse = (self.errors[start..].iter().fold(0.0, |sum, &v| sum + v * v) / count as f64).sqrt();
        let smape = (start..y.len()).fold(0.0, |sum, j| {
            let denominator = (y[j].abs() + self.predictions[j].abs()) / 2.0;
            sum + if denominator == 0.0 { 0.0 } else { self.errors[j].abs() / denominator }
        }) / count as f64;
        let prefix = if start == 0 { y.len() } else { start };
        let scale = y[..prefix].windows(2).fold(0.0, |sum, v| sum + (v[1] - v[0]).abs()) / (prefix - 1) as f64;
        [if scale == 0.0 { 0.0 } else { mae / scale }, smape, mae, rmse]
    }
}

/// Complete-cycle seasonal affine histories in the measured exact-arithmetic
/// region. Every innovation is calculated by the application's shared loop.
/// The zero objective retains the measured starting parameters; a nonzero
/// innovation needs the still unidentified seasonal initialisation/search.
fn exact_seasonal(site: &Site<'_>, y: &[f64], period: usize, output: Output, statistic: f64, horizon: f64, automatic: bool) -> Step<CalcValue> {
    if !(8..=32).contains(&y.len()) || ![2, 4, 8].contains(&period)
        || y.len() % period != 0 || y.len() < 2 * period
        || y.iter().any(|&v| v != 0.0 && !(2.0_f64.powi(-400)..=2.0_f64.powi(400)).contains(&v.abs())) {
        return Err(unfinished(site));
    }
    let differences: Vec<_> = (0..period).map(|j| exact_sum(y[j + period], -y[j]))
        .collect::<Option<_>>().ok_or_else(|| unfinished(site))?;
    let trend = exact_total(&differences).and_then(|v| exact_ratio(v, (period * period) as f64))
        .ok_or_else(|| unfinished(site))?;
    let mut adjusted = Vec::with_capacity(period);
    for (j, &v) in y.iter().take(period).enumerate() {
        let product = j as f64 * trend;
        if (j as f64).mul_add(trend, -product) != 0.0 { return Err(unfinished(site)); }
        adjusted.push(exact_sum(v, -product).ok_or_else(|| unfinished(site))?);
    }
    let level = exact_total(&adjusted).and_then(|v| exact_ratio(v, period as f64))
        .ok_or_else(|| unfinished(site))?;
    let mut seasons = [0.0; 8];
    for (target, &v) in seasons.iter_mut().zip(&adjusted) {
        *target = exact_sum(v, -level).ok_or_else(|| unfinished(site))?;
    }
    if exact_total(&seasons[..period]) != Some(0.0) { return Err(unfinished(site)); }
    let mut lattice = y.to_vec();
    lattice.extend_from_slice(&adjusted);
    lattice.extend_from_slice(&seasons[..period]);
    lattice.extend([level, trend]);
    let minimum = quantum_exponent(&lattice);
    if !(-900..=900).contains(&minimum) { return Err(unfinished(site)); }
    let quantum = 2.0_f64.powi(minimum);
    let bound = (1u64 << 50) as f64 / (y.len() as f64).powi(3);
    if lattice.iter().any(|v| (v / quantum).abs() > bound) { return Err(unfinished(site)); }
    if exact_sum(level, seasons[0]) != Some(y[0]) { return Err(unfinished(site)); }
    let model = Model::run_initialized(y, 0.751, 0.001, None,
        Initial { level, trend, seasons, period, gamma: 0.001 });
    if model.errors.iter().any(|&v| v != 0.0) { return Err(unfinished(site)); }
    // Fresh automatic profiles distinguish a nonzero alternating component
    // from the affine case. Other detector shapes remain unidentified.
    if automatic && (period != 2 || model.seasons[0] == model.seasons[1]) {
        return Err(unfinished(site));
    }
    match output {
        Output::Forecast => {
            if horizon < 0.0 { return Err(unfinished(site)); }
            let product = horizon * model.trend;
            if !product.is_finite() || horizon.mul_add(model.trend, -product) != 0.0 {
                return Err(unfinished(site));
            }
            let phase = ((y.len() - 1) as u64 + horizon as u64) % period as u64;
            let projected = exact_sum(model.level, product).ok_or_else(|| unfinished(site))?;
            site.number(exact_sum(projected, model.seasons[phase as usize])
                .ok_or_else(|| unfinished(site))?)
        }
        // The measured zero innovations imply zero forecast variance at every
        // admitted horizon. No general seasonal variance expression is assumed.
        Output::Confidence => site.number((model.squared_errors() / y.len() as f64).sqrt()),
        Output::Statistic => {
            let statistics = model.statistics(y);
            site.number(match statistic as u8 {
                1 => 0.751, 2 | 3 => 0.001, 4..=7 => statistics[statistic as usize - 4],
                _ => unreachable!(),
            })
        }
        Output::Seasonality => site.number(period as f64),
    }
}

fn nonseasonal(site: &Site<'_>, y: &[f64], output: Output, statistic: f64, horizon: f64, confidence: f64) -> Step<CalcValue> {
    if y.iter().any(|&v| v != 0.0 && !(2.0_f64.powi(-400)..=2.0_f64.powi(400)).contains(&v.abs())) {
        return Err(unfinished(site));
    }
    // Gamma does not require the unidentified initial states. Fresh short and
    // rounded-constant selections retain the same two epsilon seasonal states.
    if output == Output::Statistic && statistic == 3.0 && (3..=32).contains(&y.len()) {
        return site.number(f64::EPSILON);
    }
    // The service separates short initialisation from the ordinary regression
    // at this boundary. The seven-observation boundary still has last-bit losses.
    if !(8..=32).contains(&y.len()) {
        return Err(unfinished(site));
    }
    // The exact-affine route has already failed here. Rounded constant
    // initialisation changes the search parameters in retained adjacent probes.
    if y.iter().all(|&v| v == y[0]) { return Err(unfinished(site)); }
    let start = y.len() - statistic_window(y.len());
    let validation = (start != 0).then_some(start);
    let mut best = f64::INFINITY;
    let mut chosen = PARAMETERS[0];
    for &(alpha, beta) in PARAMETERS {
        let score = Model::run(y, alpha, beta, validation).squared_errors();
        if score < best { best = score; chosen = (alpha, beta); }
    }
    if !best.is_finite() { return Err(unfinished(site)); }
    let (alpha, mut beta) = chosen;
    let mut model = Model::run(y, alpha, beta, None);
    let squared_errors = model.squared_errors();
    let statistics = model.statistics(y);
    let reset = model.finish(y, &mut beta);
    match output {
        Output::Forecast => {
            if horizon < 0.0 { return Err(unfinished(site)); }
            // Validation at the raw minimum-alpha point has a separate postfit
            // reset decision. Symmetric fresh histories refute the ordinary
            // decision there; pre-reset errors and beta .001 remain identified.
            if alpha == 0.002 && y.len() > 10 && reset { return Err(unfinished(site)); }
            let phase = ((y.len() - 1) as u64 + horizon as u64) % 2;
            site.number((model.level + horizon * model.trend) + model.seasons[phase as usize])
        }
        Output::Statistic => site.number(match statistic as u8 {
            1 => alpha, 2 => beta, 3 => f64::EPSILON, 4..=7 => statistics[statistic as usize - 4],
            _ => unreachable!(),
        }),
        Output::Confidence => {
            let h = horizon;
            let k = ((h - 1.0) / 2.0).floor();
            let g = f64::EPSILON;
            let variance = 1.0 + (h - 1.0) * ((h * alpha * beta + h * (2.0 * h - 1.0) * beta * beta / 6.0) + alpha * alpha);
            let variance = variance + g * k * ((2.0 * alpha + g) + 2.0 * beta * (k + 1.0));
            let probability = (1.0 + confidence) / 2.0;
            // Valid confidence immediately below one rounds this probability
            // to one. Sixteen fresh selections identify the internal fallback.
            let z = if probability == 1.0 { 1.0 } else {
                crate::ported::normal_log_family::norm_s_inv_kernel(probability).map_err(Exit::Error)?
            };
            site.number(z * ((squared_errors / y.len() as f64) * variance).sqrt())
        }
        Output::Seasonality => Err(unfinished(site)),
    }
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
    if season == 1.0 {
        match exact_seasonal(site, &series.values, 2, output, statistic, horizon, true) {
            Ok(value) => return Ok(value),
            Err(Exit::Fault(FunctionFault::NotHeld { .. })) => {}
            Err(why) => return Err(why),
        }
    }
    if season > 1.0 && season <= (series.values.len() / 2) as f64 {
        return exact_seasonal(site, &series.values, season as usize, output, statistic, horizon, false);
    }
    let trend = match exact_trend(&series.values) {
        Some(trend) => trend,
        None => {
            if season == 0.0 || season > (series.values.len() / 2) as f64 {
                return nonseasonal(site, &series.values, output, statistic, horizon, confidence);
            }
            return Err(unfinished(site));
        }
    };
    if output == Output::Seasonality { return site.number(0.0); }
    // An explicit period longer than half the data uses the nonseasonal model.
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
