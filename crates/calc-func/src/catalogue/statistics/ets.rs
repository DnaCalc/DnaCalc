//! ETS's measured numeric-vector surface (dc-j9k, Excel20527, R39).
//!
//! Timeline preparation, option errors, step statistics and the exact, nonseasonal
//! zero-residual models and bounded nonseasonal fits are implemented. Short fits
//! require exact moment regressions and phase extension. Rounded constant initialisation,
//! nonzero seasonal fits, incomplete cycles and calendar inference still
//! belong to dc-j9k. In particular,
//! bit-identical input values do NOT justify returning that value: adjacent-load
//! measurements expose rounding and changed optimisation even on constant data.
//! The corpus counts the remaining nonseasonal last-bit differences as mismatches.

use super::super::kit::{fail, grid, num, opt};
use crate::call::{EvalServices, FunctionFault};
use crate::kernel::{Exit, Site, Step};
use crate::value::{CalcValue, CoreRef, WorksheetErrorCode};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Output {
    Forecast,
    Confidence,
    Seasonality,
    Statistic,
}

fn unfinished(site: &Site<'_>) -> Exit {
    Exit::Fault(FunctionFault::NotHeld {
        id: site.id(),
        owner: "dc-j9k",
    })
}

fn vector(
    site: &Site<'_>,
    args: &[CalcValue],
    i: usize,
    sv: &mut EvalServices<'_>,
) -> Step<Vec<f64>> {
    let g = grid(site, args, i, sv)?;
    if g.rows > 1 && g.cols > 1 {
        return Err(unfinished(site));
    }
    g.cells
        .iter()
        .map(|v| match v.core_ref() {
            CoreRef::Number(v) => Ok(v),
            CoreRef::Error(e) => fail(e),
            _ => Err(unfinished(site)),
        })
        .collect()
}

struct Series {
    values: Vec<f64>,
    first: f64,
    last: f64,
    step: f64,
    ordered: bool,
    preparation_exact: bool,
    directly_observed: bool,
}

fn exact_ratio(a: f64, b: f64) -> Option<f64> {
    let q = a / b;
    (q.is_finite() && q.mul_add(b, -a) == 0.0).then_some(q)
}

/// Every subset sum fits the binary64 integer lattice. This is sufficient
/// regardless of the private duplicate reduction's order or working precision.
fn exact_total(values: &[f64]) -> Option<f64> {
    let minimum = quantum_exponent(values);
    if !(-900..=900).contains(&minimum) {
        return None;
    }
    let quantum = 2.0_f64.powi(minimum);
    let mut absolute = 0u128;
    for &v in values {
        let scaled = (v / quantum).abs();
        if !scaled.is_finite() || scaled.fract() != 0.0 || scaled > (1u64 << 52) as f64 {
            return None;
        }
        absolute += scaled as u128;
    }
    (absolute <= 1u128 << 52).then(|| values.iter().sum())
}

fn prepare(
    site: &Site<'_>,
    y: Vec<f64>,
    t: Vec<f64>,
    completion: f64,
    aggregation: f64,
) -> Step<Series> {
    if y.len() != t.len() {
        return fail(WorksheetErrorCode::NA);
    }
    if y.len() < 2 {
        return fail(WorksheetErrorCode::Div0);
    }
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
        while i < pairs.len() && pairs[i].0 == time {
            i += 1;
        }
        let mut group: Vec<_> = pairs[start..i].iter().map(|p| p.1).collect();
        if group.len() > 1 {
            let exact = match aggregation as u8 {
                1 => exact_total(&group)
                    .and_then(|s| exact_ratio(s, group.len() as f64))
                    .is_some(),
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
                        .and_then(|s| exact_ratio(s, 2.0))
                        .is_some();
                    (group[m - 1] + group[m]) / 2.0
                } else {
                    group[m]
                }
            }
            6 => group.iter().copied().fold(f64::INFINITY, f64::min),
            7 => group.iter().sum(),
            _ => unreachable!(),
        };
        times.push(time);
        values.push(value);
    }
    if values.iter().any(|v| !v.is_finite()) {
        return Err(unfinished(site));
    }
    if times.len() < 2 {
        return fail(WorksheetErrorCode::Num);
    }
    let min_step = times
        .windows(2)
        .map(|p| p[1] - p[0])
        .fold(f64::INFINITY, f64::min);
    if !min_step.is_finite() || min_step <= 0.0 {
        return Err(unfinished(site));
    }
    // Excel 20527 clusters consecutive intervals within 3e-7 and keeps the first
    // representative interval (Touchstone STAT(..., 8) bisection, dc-iuwct.54).
    // Admit binary64-rounded uniform/missing-step decimal grids while leaving
    // genuinely irregular or calendar-month timelines as typed refusals.
    let step = times
        .windows(2)
        .map(|p| p[1] - p[0])
        .find(|&d| (d - min_step).abs() <= 3e-7 && (d - min_step).abs() <= min_step * 1e-9)
        .unwrap_or(min_step);
    let mut gaps = Vec::new();
    let mut count = 1usize;
    for pair in times.windows(2) {
        let diff = pair[1] - pair[0];
        let gap_raw = diff / step;
        let gap = if gap_raw.fract() == 0.0 {
            gap_raw
        } else {
            let rounded = gap_raw.round();
            if rounded >= 1.0
                && rounded <= 1_000_000.0
                && (gap_raw - rounded).abs() <= 1e-9
                && (diff - rounded * step).abs() <= 3e-7
            {
                rounded
            } else {
                return Err(unfinished(site));
            }
        };
        if gap < 1.0 || gap > 1_000_000.0 {
            return Err(unfinished(site));
        }
        count = count
            .checked_add(gap as usize)
            .ok_or_else(|| unfinished(site))?;
        gaps.push(gap as usize);
    }
    if count > 128 {
        return Err(unfinished(site));
    }
    if (count - times.len()) * 10 > count * 3 {
        return fail(WorksheetErrorCode::Num);
    }
    let mut completed = Vec::with_capacity(count);
    completed.push(values[0]);
    for (i, &gap) in gaps.iter().enumerate() {
        for j in 1..gap {
            if completion != 0.0 {
                let exact = exact_sum(values[i + 1], -values[i]).and_then(|difference| {
                    let fraction = exact_ratio(j as f64, gap as f64)?;
                    let product = difference * fraction;
                    if difference.mul_add(fraction, -product) != 0.0 {
                        return None;
                    }
                    exact_sum(values[i], product)
                });
                preparation_exact &= exact.is_some();
            }
            completed.push(if completion == 0.0 {
                0.0
            } else {
                values[i] + (values[i + 1] - values[i]) * (j as f64 / gap as f64)
            });
        }
        completed.push(values[i + 1]);
    }
    if completed.iter().any(|v| !v.is_finite()) {
        return Err(unfinished(site));
    }
    Ok(Series {
        values: completed,
        first: times[0],
        last: *times.last().unwrap(),
        step,
        ordered,
        preparation_exact,
        directly_observed: pairs.len() == times.len() && count == times.len(),
    })
}

/// Sufficient exact-arithmetic region for the initial ordinary least-squares
/// trend and its zero innovations. The bound follows the moment sums' n^3
/// growth and binary64's 53 bits; it is not a cutoff fitted to answer counts.
fn quantum_exponent(y: &[f64]) -> i32 {
    y.iter()
        .filter(|&&v| v != 0.0)
        .map(|v| {
            let raw = v.to_bits() & 0x7fff_ffff_ffff_ffff;
            let exp = ((raw >> 52) & 0x7ff) as i32;
            let mantissa = (raw & ((1u64 << 52) - 1)) | (1u64 << 52);
            exp - 1023 - 52 + mantissa.trailing_zeros() as i32
        })
        .min()
        .unwrap_or(0)
}

fn exact_scaled_history(y: &[f64]) -> Option<(f64, Vec<f64>)> {
    let n = y.len();
    let minimum = quantum_exponent(y);
    if !(-900..=900).contains(&minimum) {
        return None;
    }
    let quantum = 2.0_f64.powi(minimum);
    let bound = (1u64 << 50) as f64 / (n as f64).powi(3);
    let scaled: Vec<_> = y.iter().map(|v| v / quantum).collect();
    if scaled
        .iter()
        .any(|v| !v.is_finite() || v.fract() != 0.0 || v.abs() > bound)
    {
        return None;
    }
    Some((quantum, scaled))
}

fn exact_trend(y: &[f64]) -> Option<f64> {
    let (quantum, scaled) = exact_scaled_history(y)?;
    let difference = scaled[1] - scaled[0];
    if scaled
        .iter()
        .enumerate()
        .any(|(i, &v)| v != scaled[0] + i as f64 * difference)
    {
        return None;
    }
    let trend = difference * quantum;
    // The observed nonseasonal default is alpha .9, beta .001, gamma 2^-52.
    // Require the actual state updates to retain zero innovations as well.
    let mut level = y[0];
    for &v in &y[1..] {
        let prediction = level + trend;
        if prediction != v {
            return None;
        }
        level = 0.9 * v + (1.0 - 0.9) * prediction;
        if level != v {
            return None;
        }
    }
    Some(trend)
}

/// TwoSum retains the addition's rounding residue. Final extrapolation is
/// admitted only when its product and sum are exact; private rounded expression
/// order outside this region has not been identified.
fn sum_and_error(a: f64, b: f64) -> Option<(f64, f64)> {
    let sum = a + b;
    let part = sum - a;
    let residue = (a - (sum - part)) + (b - part);
    sum.is_finite().then_some((sum, residue))
}

fn exact_sum(a: f64, b: f64) -> Option<f64> {
    let (sum, residue) = sum_and_error(a, b)?;
    (residue == 0.0).then_some(sum)
}

/// Verify the five-point regression against an exact centered numerator.
/// The division is correctly rounded; agreement proves the initial slope
/// did not depend on the ordinary moment sums' rounding.
fn exact_centered_five_slope(y: &[f64]) -> bool {
    if y.len() != 5 {
        return false;
    }
    let Some(outer) = exact_sum(y[4], -y[0]) else { return false };
    let Some(inner) = exact_sum(y[3], -y[1]) else { return false };
    let twice = 2.0 * outer;
    if !twice.is_finite() || 2.0_f64.mul_add(outer, -twice) != 0.0 {
        return false;
    }
    let Some(numerator) = exact_sum(twice, inner) else { return false };
    regression_slope(y) == numerator / 10.0
}

/// Excel 20527's five-point detector returns zero for strictly curved data.
/// Exact differences keep rounding from inventing or removing a turn.
fn strictly_curved_five(y: &[f64]) -> bool {
    if y.len() != 5 || y.iter().any(|&v| {
        v != 0.0 && !(2.0_f64.powi(-300)..=2.0_f64.powi(300)).contains(&v.abs())
    }) {
        return false;
    }
    let mut differences = [0.0; 4];
    for (i, pair) in y.windows(2).enumerate() {
        let Some(difference) = exact_sum(pair[1], -pair[0]) else {
            return false;
        };
        differences[i] = difference;
    }
    differences.windows(2).all(|pair| pair[0] < pair[1])
        || differences.windows(2).all(|pair| pair[0] > pair[1])
}

// R39's fixed parameter family, measured before the independent random fits.
// The later trend correction can report beta .001 for other alpha values;
// those corrected pairs must not become additional optimisation candidates.
const PARAMETERS: &[(f64, f64)] = &[
    (0.9, 0.001),
    (0.9, 0.899),
    (0.833333, 0.75),
    (0.75, 0.001),
    (0.75, 0.749),
    (0.666667, 0.166667),
    (0.666667, 0.5),
    (0.5, 0.001),
    (0.5, 0.25),
    (0.5, 0.499),
    (0.333333, 0.166667),
    (0.25, 0.001),
    (0.25, 0.249),
    (0.166667, 0.0833333),
    (0.126, 0.001),
    (0.126, 0.125),
    (0.1, 0.001),
    (0.1, 0.099),
    (0.002, 0.001),
    (0.998, 0.099),
    (0.998, 0.499),
    (0.998, 0.749),
    (0.998, 0.899),
];

fn regression_slope(y: &[f64]) -> f64 {
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

/// Verify that the moment-based slope is the correctly rounded centered
/// regression, without requiring every intermediate sum to be exact.
fn verified_regression_slope(y: &[f64]) -> bool {
    let denominator = match y.len() {
        5 => 10.0,
        7 => 28.0,
        _ => return false,
    };
    let slope = regression_slope(y);
    if slope == 0.0 || !slope.is_finite() {
        return false;
    }
    let mut high = 0.0;
    let mut low = 0.0;
    for (i, &value) in y.iter().enumerate() {
        let weight = i as f64 - (y.len() / 2) as f64;
        let product = value * weight;
        let product_error = value.mul_add(weight, -product);
        let Some((sum, carry)) = sum_and_error(high, product) else { return false };
        high = sum;
        let Some(accumulated) = exact_sum(low, carry).and_then(|x| exact_sum(x, product_error)) else {
            return false;
        };
        low = accumulated;
    }
    let product = slope * denominator;
    if !product.is_finite() {
        return false;
    }
    let product_error = slope.mul_add(denominator, -product);
    let Some(delta) = exact_sum(high, -product).and_then(|x| {
        exact_sum(low, -product_error).and_then(|y| exact_sum(x, y))
    }) else {
        return false;
    };
    // Compare the exact numerator residual with the two adjacent rounding
    // boundaries. Ties remain refused rather than assuming a tie rule here.
    let lower = (slope - slope.next_down()) * (denominator / 2.0);
    let upper = (slope.next_up() - slope) * (denominator / 2.0);
    -lower < delta && delta < upper
}

fn verified_five_point_extension(y: &[f64]) -> bool {
    if y.len() != 5 || !verified_regression_slope(y) {
        return false;
    }
    let slope = regression_slope(y);
    let mut extended = y.to_vec();
    for j in 5..7 {
        extended.push(y[j % 2] + (j - j % 2) as f64 * slope);
    }
    verified_regression_slope(&extended)
}

/// Keep squared curvature above the binary64 rounding floor for the history's
/// scale. Nearly linear histories need the unidentified confidence arithmetic.
fn resolved_five_point_curvature(y: &[f64]) -> bool {
    if y.len() != 5 {
        return false;
    }
    let scale = y.iter().fold(0.0_f64, |largest, value| largest.max(value.abs()));
    // sqrt(binary64 epsilon) is exactly 2^-26.
    let bound = scale * 2.0_f64.powi(-26);
    let mut steps = [0.0; 4];
    for (i, pair) in y.windows(2).enumerate() {
        let Some(step) = exact_sum(pair[1], -pair[0]) else { return false };
        steps[i] = step;
    }
    steps.windows(2).all(|pair| {
        exact_sum(pair[1], -pair[0]).is_some_and(|curvature| curvature.abs() > bound)
    })
}

/// R40's short-history impulse response identifies the first two phases plus
/// the original regression trend as the extension to seven observations.
/// The search runs on that extension; final state updates use the real history.
fn initial_slope(y: &[f64]) -> f64 {
    if y.len() >= 7 {
        return regression_slope(y);
    }
    let trend = regression_slope(y);
    let mut extended = y.to_vec();
    for j in y.len()..7 {
        extended.push(y[j % 2] + (j - j % 2) as f64 * trend);
    }
    regression_slope(&extended)
}

/// The parameter search validates on the last min(floor(0.4 N), max(10, 2p,
/// ceil(N/5))) of the N points searched, none up to ten points (eng23; period
/// one for the nonseasonal fit, whose probe_nons selections diverged from 52
/// points with STAT's ten-point tail).
fn search_window(searched: usize, period: usize) -> usize {
    if searched <= 10 {
        searched
    } else {
        ((searched as f64 * 0.4) as usize).min(10.max(2 * period).max(searched.div_ceil(5)))
    }
}

fn statistic_window(n: usize) -> usize {
    if n <= 10 {
        n
    } else {
        ((n as f64 * 0.4) as usize).min(10)
    }
}

struct Model {
    level: f64,
    trend: f64,
    seasons: Vec<f64>,
    period: usize,
    errors: Vec<f64>,
    predictions: Vec<f64>,
    trends: Vec<f64>,
}

struct Initial {
    level: f64,
    trend: f64,
    seasons: Vec<f64>,
    period: usize,
    gamma: f64,
}

impl Model {
    /// The application and parameter search use this same recurrence. During
    /// validation only the transition runs; observations no longer update it.
    fn run(y: &[f64], alpha: f64, beta: f64, validation: Option<usize>) -> Self {
        Self::run_initialized(
            y,
            alpha,
            beta,
            validation,
            Initial {
                level: y[0],
                trend: initial_slope(y),
                seasons: vec![0.0; 2],
                period: 2,
                gamma: f64::EPSILON,
            },
        )
    }

    fn run_initialized(
        y: &[f64],
        alpha: f64,
        beta: f64,
        validation: Option<usize>,
        initial: Initial,
    ) -> Self {
        let mut model = Self {
            level: initial.level - initial.trend,
            trend: initial.trend,
            seasons: initial.seasons,
            period: initial.period,
            errors: Vec::with_capacity(y.len()),
            predictions: Vec::with_capacity(y.len()),
            trends: Vec::with_capacity(y.len().saturating_sub(1)),
        };
        for (j, &v) in y.iter().enumerate() {
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
            if j >= 1 {
                model.trends.push(model.trend);
            }
        }
        model
    }

    fn squared_errors(&self) -> f64 {
        self.errors.iter().fold(0.0, |sum, &v| sum + v * v)
    }

    /// Excel's trend-stability test over the final half of the fitted trends;
    /// an unstable trend restarts from the history's end (nonseasonal and
    /// seasonal fits alike).
    fn trend_unstable(&self, y: &[f64]) -> bool {
        // This window is independent of STAT's tail. It retains the transitions
        // within the final half of the observations, including its first state.
        let window = if y.len() <= 10 {
            y.len()
        } else {
            y.len().div_ceil(2)
        };
        let count = window - 1;
        let trends = &self.trends[self.trends.len() - count..];
        use crate::ported::excel_numeric::{
            excel_x87_add as add, excel_x87_div as div, excel_x87_mul as mul, excel_x87_sub as sub,
        };
        // R46 trend-shift questions separate the selected tail from the
        // divisors. Odd histories retain ceil(n/2)-1 transitions but use
        // floor(n/2)-1 in both raw moments. Centered variance is a different
        // computation here, including its cancellation on near-flat trends.
        let divisor_window = if y.len() <= 10 { y.len() } else { y.len() / 2 };
        let divisor = (divisor_window - 1) as f64;
        let total = trends.iter().fold(0.0, |sum, &v| add(sum, v));
        let squares = trends.iter().fold(0.0, |sum, &v| add(sum, mul(v, v)));
        let mean = div(total, divisor);
        let variance = sub(div(squares, divisor), mul(mean, mean));
        mean.abs() < div(mul(variance.sqrt(), divisor_window as f64), divisor)
    }

    fn finish(&mut self, y: &[f64], beta: &mut f64) -> bool {
        let reset = self.trend_unstable(y);
        if reset {
            self.level = *y.last().unwrap() - self.seasons[(y.len() - 1) % 2];
            self.trend = initial_slope(y);
            *beta = 0.001;
        }
        let center = (self.seasons[0] + self.seasons[1]) / 2.0;
        self.level += center;
        for season in &mut self.seasons[..self.period] {
            *season -= center;
        }
        reset
    }

    fn statistics(&self, y: &[f64]) -> [f64; 4] {
        let count = statistic_window(y.len());
        let start = y.len() - count;
        let mae = self.errors[start..]
            .iter()
            .fold(0.0, |sum, &v| sum + v.abs())
            / count as f64;
        let rmse =
            (self.errors[start..].iter().fold(0.0, |sum, &v| sum + v * v) / count as f64).sqrt();
        let smape = (start..y.len()).fold(0.0, |sum, j| {
            let denominator = (y[j].abs() + self.predictions[j].abs()) / 2.0;
            sum + if denominator == 0.0 {
                0.0
            } else {
                self.errors[j].abs() / denominator
            }
        }) / count as f64;
        let prefix = if start == 0 { y.len() } else { start };
        let scale = y[..prefix]
            .windows(2)
            .fold(0.0, |sum, v| sum + (v[1] - v[0]).abs())
            / (prefix - 1) as f64;
        [
            if scale == 0.0 { 0.0 } else { mae / scale },
            smape,
            mae,
            rmse,
        ]
    }
}

/// Seasonal affine histories in the measured exact-arithmetic region. Period
/// two starts at five observations and permits an unfinished final cycle.
/// Every innovation is calculated by the application's shared loop.
/// The zero objective retains the measured starting parameters; a nonzero
/// innovation needs the still unidentified seasonal initialisation/search.
fn exact_seasonal(
    site: &Site<'_>,
    y: &[f64],
    period: usize,
    output: Output,
    statistic: f64,
    horizon: f64,
    automatic: bool,
) -> Step<CalcValue> {
    if !(5..=32).contains(&y.len())
        || !(2..=8).contains(&period)
        || (period != 2
            && (y.len() % period != 0 || y.len() < 2 * period
        // Three cycles give every phase a measured initial mean. R39 also
        // proved the shorter power-of-two histories starting at eight values.
        || (y.len() < 3 * period && (!period.is_power_of_two() || y.len() < 8))))
        || y.iter()
            .any(|&v| v != 0.0 && !(2.0_f64.powi(-400)..=2.0_f64.powi(400)).contains(&v.abs()))
    {
        return Err(unfinished(site));
    }
    // R48's short/partial-cycle proofs identify detection, fitted parameters
    // and zero variance. Fresh lattice-edge forecasts expose an unidentified
    // publication step when a large affine component cancels near zero.
    // Keep the entire newly extended forecast consumer refused; the previously
    // measured complete-cycle forecast region retains its existing checks.
    if output == Output::Forecast
        && period == 2
        && (y.len() < 6 || y.len() % 2 != 0 || (automatic && y.len() < 8))
    {
        return Err(unfinished(site));
    }
    let differences: Vec<_> = (0..period)
        .map(|j| exact_sum(y[j + period], -y[j]))
        .collect::<Option<_>>()
        .ok_or_else(|| unfinished(site))?;
    let trend = exact_total(&differences)
        .and_then(|v| exact_ratio(v, (period * period) as f64))
        .ok_or_else(|| unfinished(site))?;
    let mut adjusted = Vec::with_capacity(period);
    for (j, &v) in y.iter().take(period).enumerate() {
        let product = j as f64 * trend;
        if (j as f64).mul_add(trend, -product) != 0.0 {
            return Err(unfinished(site));
        }
        adjusted.push(exact_sum(v, -product).ok_or_else(|| unfinished(site))?);
    }
    let level = exact_total(&adjusted)
        .and_then(|v| exact_ratio(v, period as f64))
        .ok_or_else(|| unfinished(site))?;
    let mut seasons = vec![0.0; period];
    for (target, &v) in seasons.iter_mut().zip(&adjusted) {
        *target = exact_sum(v, -level).ok_or_else(|| unfinished(site))?;
    }
    if exact_total(&seasons[..period]) != Some(0.0) {
        return Err(unfinished(site));
    }
    let mut lattice = y.to_vec();
    lattice.extend_from_slice(&adjusted);
    lattice.extend_from_slice(&seasons[..period]);
    lattice.extend([level, trend]);
    let minimum = quantum_exponent(&lattice);
    if !(-900..=900).contains(&minimum) {
        return Err(unfinished(site));
    }
    let quantum = 2.0_f64.powi(minimum);
    let bound = (1u64 << 50) as f64 / (y.len() as f64).powi(3);
    if lattice.iter().any(|v| (v / quantum).abs() > bound) {
        return Err(unfinished(site));
    }
    if exact_sum(level, seasons[0]) != Some(y[0]) {
        return Err(unfinished(site));
    }
    let model = Model::run_initialized(
        y,
        0.751,
        0.001,
        None,
        Initial {
            level,
            trend,
            seasons,
            period,
            gamma: 0.001,
        },
    );
    if model.errors.iter().any(|&v| v != 0.0) {
        return Err(unfinished(site));
    }
    // Fresh automatic profiles distinguish a nonzero alternating component
    // from the affine case. Other detector shapes remain unidentified.
    if automatic && (period != 2 || model.seasons[0] == model.seasons[1]) {
        return Err(unfinished(site));
    }
    match output {
        Output::Forecast => {
            if horizon < 0.0 {
                return Err(unfinished(site));
            }
            let product = horizon * model.trend;
            if !product.is_finite() || horizon.mul_add(model.trend, -product) != 0.0 {
                return Err(unfinished(site));
            }
            let phase = ((y.len() - 1) as u64 + horizon as u64) % period as u64;
            let projected = exact_sum(model.level, product).ok_or_else(|| unfinished(site))?;
            site.number(
                exact_sum(projected, model.seasons[phase as usize])
                    .ok_or_else(|| unfinished(site))?,
            )
        }
        // The measured zero innovations imply zero forecast variance at every
        // admitted horizon. No general seasonal variance expression is assumed.
        Output::Confidence => site.number((model.squared_errors() / y.len() as f64).sqrt()),
        Output::Statistic => {
            let statistics = model.statistics(y);
            site.number(match statistic as u8 {
                1 => 0.751,
                2 | 3 => 0.001,
                4..=7 => statistics[statistic as usize - 4],
                _ => unreachable!(),
            })
        }
        Output::Seasonality => site.number(period as f64),
    }
}

fn nonseasonal(
    site: &Site<'_>,
    y: &[f64],
    output: Output,
    statistic: f64,
    horizon: f64,
    confidence: f64,
) -> Step<CalcValue> {
    nonseasonal_fit(site, y, output, statistic, horizon, confidence, false)
}

/// The nonseasonal fit. `general` admits the histories outside the measured
/// exact region (long, constant or extreme-scale histories): the same
/// recurrence and search, without the claim that its rounding is Excel's.
fn nonseasonal_fit(
    site: &Site<'_>,
    y: &[f64],
    output: Output,
    statistic: f64,
    horizon: f64,
    confidence: f64,
    general: bool,
) -> Step<CalcValue> {
    if !general
        && y.iter()
            .any(|&v| v != 0.0 && !(2.0_f64.powi(-400)..=2.0_f64.powi(400)).contains(&v.abs()))
    {
        return Err(unfinished(site));
    }
    // Gamma does not require the unidentified initial states. Fresh short and
    // rounded-constant selections retain the same two epsilon seasonal states.
    if output == Output::Statistic && statistic == 3.0 && (general || (3..=32).contains(&y.len())) {
        return site.number(f64::EPSILON);
    }
    if !general && !(3..=32).contains(&y.len()) {
        return Err(unfinished(site));
    }
    if output == Output::Seasonality {
        return site.number(0.0);
    }
    // Outside the exact short-history region, the private initial regression's
    // rounding remains unidentified. Every refusal remains a counted failure.
    // The binary64 regression and seven-point extension reproduce Excel on the
    // recorded short histories (199 of 204 cells; the other five are 1-3 ulp).
    let extended;
    let search_history = if y.len() < 7 {
        extended = {
            let trend = regression_slope(y);
            let mut e = y.to_vec();
            for j in y.len()..7 {
                e.push(y[j % 2] + (j - j % 2) as f64 * trend);
            }
            e
        };
        &extended[..]
    } else {
        y
    };
    // The exact-affine route has already failed here. Rounded constant
    // initialisation changes the search parameters in retained adjacent probes.
    if !general && y.iter().all(|&v| v == y[0]) {
        return Err(unfinished(site));
    }
    let start = search_history.len() - search_window(search_history.len(), 1);
    let validation = (start != 0).then_some(start);
    let mut best = f64::INFINITY;
    let mut chosen = PARAMETERS[0];
    for &(alpha, beta) in PARAMETERS {
        let score = Model::run(search_history, alpha, beta, validation).squared_errors();
        if score < best {
            best = score;
            chosen = (alpha, beta);
        }
    }
    if !best.is_finite() && !general {
        return Err(unfinished(site));
    }
    let (alpha, mut beta) = chosen;
    let mut model = Model::run(y, alpha, beta, None);
    let squared_errors = model.squared_errors();
    let statistics = model.statistics(y);
    model.finish(y, &mut beta);
    match output {
        Output::Forecast => {
            // A target inside the history extrapolates the final state
            // backwards (Touchstone, eng21): the same formula with h < 0.
            let phase = season_phase(y.len(), horizon, 2);
            // Frozen midpoint discriminators (dc-ymjf.2.148, Excel 16.0.20527.20196)
            // separate stored x87 PC64 multiply (20/20 vs 0/20 binary64 `*` and 0/20
            // unspilled 80-bit) and stored x87 PC64 level addition (16/16 vs 0/16 binary64 `+`)
            // before adding the seasonal term.
            use crate::ported::excel_numeric::{excel_x87_add, excel_x87_mul};
            let value = excel_x87_add(
                excel_x87_add(model.level, excel_x87_mul(horizon, model.trend)),
                model.seasons[phase],
            );
            if general {
                return finite(site, value);
            }
            site.number(value)
        }
        Output::Statistic => site.number(match statistic as u8 {
            1 => alpha,
            2 => beta,
            3 => f64::EPSILON,
            4..=7 => statistics[statistic as usize - 4],
            _ => unreachable!(),
        }),
        Output::Confidence => confidence_interval(
            site,
            squared_errors / y.len() as f64,
            alpha,
            beta,
            f64::EPSILON,
            2,
            horizon,
            confidence,
            general,
        ),
        Output::Seasonality => unreachable!(),
    }
}

/// The seasonal phase of an integer horizon from the last observation; negative
/// horizons (targets inside the history) count backwards.
fn season_phase(n: usize, horizon: f64, period: usize) -> usize {
    ((n as i64 - 1) + horizon as i64).rem_euclid(period as i64) as usize
}

/// Excel's closed-form class-1 additive-error variance factor at integer
/// horizon `h` (32-bit wrapped `2h-1`; negative wrap clamps to zero).
fn horizon_variance(h: f64, alpha: f64, beta: f64, gamma: f64, period: usize) -> f64 {
    let k = ((h - 1.0) / period as f64).floor();
    let p = period as f64;
    let twice_h_minus_one = (h as i32).wrapping_mul(2).wrapping_sub(1) as f64;
    let variance = 1.0
        + (h - 1.0)
            * ((h * alpha * beta + h * twice_h_minus_one * beta * beta / 6.0) + alpha * alpha);
    (variance + gamma * k * ((2.0 * alpha + gamma) + p * beta * (k + 1.0))).max(0.0)
}

/// Publish a FORECAST.ETS.CONFINT value at integer or fractional horizon `h`.
/// Fractional horizons linearly interpolate the forecast error variance
/// `mse * horizon_variance(...)` between floor(h) (zero at 0) and floor(h) + 1
/// before taking sqrt and the PC64-stored product with `z`.
#[allow(clippy::too_many_arguments)]
fn confidence_interval(
    site: &Site<'_>,
    mse: f64,
    alpha: f64,
    beta: f64,
    gamma: f64,
    period: usize,
    horizon: f64,
    confidence: f64,
    require_finite: bool,
) -> Step<CalcValue> {
    let forecast_variance = if horizon.fract() == 0.0 {
        mse * horizon_variance(horizon, alpha, beta, gamma, period)
    } else {
        let lower = horizon.floor();
        let fraction = horizon - lower;
        let high = mse * horizon_variance(lower + 1.0, alpha, beta, gamma, period);
        if lower == 0.0 {
            high * fraction
        } else {
            let low = mse * horizon_variance(lower, alpha, beta, gamma, period);
            low + (high - low) * fraction
        }
    };
    let probability = (1.0 + confidence) / 2.0;
    // Valid confidence immediately below one rounds this probability
    // to one. Sixteen fresh selections identify the internal fallback.
    let z = if probability == 1.0 {
        1.0
    } else {
        crate::ported::normal_log_family::norm_s_inv_kernel(probability).map_err(Exit::Error)?
    };
    // R40 independently certified product-midpoint questions identify
    // the same PC64-then-binary64 publication used by the forecast.
    let value = crate::ported::excel_numeric::excel_x87_mul(z, forecast_variance.sqrt());
    if require_finite {
        finite(site, value)
    } else {
        site.number(value)
    }
}

/// A calculated result; a non-finite one is Excel's overflow error.
fn finite(site: &Site<'_>, value: f64) -> Step<CalcValue> {
    if value.is_finite() {
        site.number(value)
    } else {
        fail(WorksheetErrorCode::Num)
    }
}

// ---------------------------------------------------------------------------
// General AAA path (dc-2u2g). Histories outside the measured exact regions
// above are calculated here rather than refused. Its shape is Excel 2016's
// additive ETS as measured through Touchstone (eng21, 2026-11-17):
// - parameters come from a finite candidate set, not a continuous optimiser:
//   every (alpha, beta, gamma) Excel reported for 400 random seasonal
//   histories is one of SEASONAL_PARAMETERS (nonseasonal: PARAMETERS);
// - the automatic period follows the strongest DFT bin of the least-squares
//   detrended history (linear trends of any size leave it unchanged);
// - the selection objective is the shared validation-window squared error;
// - the seasonal initial states are Excel's (excel_seasonal_initial, eng23).
// Not yet identified, so these results are not always Excel's bits: the
// detector's acceptance rule, the parameter search where it differs from the
// candidate scan (Excel's triple in about 85% of short seasonal probes), the
// arithmetic order of the initial states and the final state adjustment.
// ---------------------------------------------------------------------------

/// The (alpha, beta, gamma) candidates Excel 20527's seasonal search ends on:
/// every triple Excel reported for 16,020 probe histories (eng21 probe_triples,
/// eng21-23 initial-state probes) except the six that only its trend reset
/// produces (a candidate with beta 0.001 in place of its own beta; see
/// seasonal_general).
const SEASONAL_PARAMETERS: &[(f64, f64, f64)] = &[
    (0.002, 0.001, 0.125),
    (0.002, 0.001, 0.25),
    (0.002, 0.001, 0.5),
    (0.002, 0.001, 0.75),
    (0.1, 0.001, 0.899),
    (0.126, 0.001, 0.001),
    (0.126, 0.125, 0.001),
    (0.166667, 0.0833333, 0.0833333),
    (0.166667, 0.0833333, 0.75),
    (0.25, 0.001, 0.749),
    (0.25, 0.249, 0.749),
    (0.251, 0.001, 0.001),
    (0.251, 0.001, 0.25),
    (0.251, 0.25, 0.001),
    (0.251, 0.25, 0.25),
    (0.333333, 0.166667, 0.166667),
    (0.333333, 0.166667, 0.5),
    (0.5, 0.001, 0.499),
    (0.5, 0.25, 0.001),
    (0.5, 0.25, 0.25),
    (0.5, 0.499, 0.499),
    (0.501, 0.001, 0.001),
    (0.501, 0.5, 0.001),
    (0.666667, 0.166667, 0.166667),
    (0.666667, 0.5, 0.166667),
    (0.75, 0.001, 0.249),
    (0.75, 0.749, 0.249),
    (0.751, 0.001, 0.001),
    (0.751, 0.75, 0.001),
    (0.833333, 0.0833333, 0.0833333),
    (0.833333, 0.75, 0.0833333),
    (0.9, 0.001, 0.099),
    (0.9, 0.899, 0.099),
    (0.998, 0.099, 0.001),
    (0.998, 0.249, 0.001),
    (0.998, 0.499, 0.001),
    (0.998, 0.749, 0.001),
    (0.998, 0.899, 0.001),
];

/// Least-squares residuals of the history against its position 1..n.
fn detrended(y: &[f64]) -> Vec<f64> {
    let n = y.len() as f64;
    let mx = (n + 1.0) / 2.0;
    let my = y.iter().sum::<f64>() / n;
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for (i, &v) in y.iter().enumerate() {
        let dx = i as f64 + 1.0 - mx;
        sxy += dx * (v - my);
        sxx += dx * dx;
    }
    let slope = if sxx == 0.0 { 0.0 } else { sxy / sxx };
    y.iter()
        .enumerate()
        .map(|(i, &v)| v - (my + slope * (i as f64 + 1.0 - mx)))
        .collect()
}

/// Ordinary least-squares MSE and slope for evenly spaced samples `vals`.
fn ols_mse_and_slope(vals: &[f64]) -> (f64, f64) {
    let m = vals.len();
    if m < 2 {
        return (0.0, 0.0);
    }
    let mf = m as f64;
    let xm = 0.5 * (mf - 1.0);
    let ym = vals.iter().sum::<f64>() / mf;
    let sxx = mf * (mf * mf - 1.0) / 12.0;
    let mut sxy = 0.0;
    for (i, &v) in vals.iter().enumerate() {
        sxy += (i as f64 - xm) * (v - ym);
    }
    let slope = sxy / sxx;
    let mut sse = 0.0;
    for (i, &v) in vals.iter().enumerate() {
        let res = (v - ym) - slope * (i as f64 - xm);
        sse += res * res;
    }
    (sse / mf, slope)
}

/// Ordinary least-squares slope of `u[idx]` against integer positions `idx`.
fn ols_slope_indexed(u: &[f64], indices: &[usize]) -> f64 {
    if indices.len() < 2 {
        return 0.0;
    }
    let mf = indices.len() as f64;
    let mx = indices.iter().map(|&i| i as f64).sum::<f64>() / mf;
    let my = indices.iter().map(|&i| u[i]).sum::<f64>() / mf;
    let (mut sxx, mut sxy) = (0.0, 0.0);
    for &i in indices {
        let dx = i as f64 - mx;
        sxx += dx * dx;
        sxy += dx * (u[i] - my);
    }
    if sxx == 0.0 { 0.0 } else { sxy / sxx }
}

/// Fundamental harmonic power on the trailing whole cycles of the raw series `y`,
/// used by Excel 20527 to break ties among validated AUTOPERIOD candidates.
fn fundamental_power(y: &[f64], period: usize) -> f64 {
    let m = (y.len() / period) * period;
    let seg = &y[y.len() - m..];
    let (mut re, mut im) = (0.0, 0.0);
    for (t, &v) in seg.iter().enumerate() {
        let angle = 2.0 * std::f64::consts::PI * (t % period) as f64 / period as f64;
        re += v * angle.cos();
        im += v * angle.sin();
    }
    let mf = m as f64;
    re /= mf;
    im /= mf;
    re * re + im * im
}

fn argmax_slice(u: &[f64], lo: usize, hi: usize) -> usize {
    let mut best_i = lo;
    let mut best_v = u[lo];
    for (offset, &v) in u[lo + 1..=hi].iter().enumerate() {
        if v > best_v {
            best_v = v;
            best_i = lo + 1 + offset;
        }
    }
    best_i
}

fn push_unique(cands: &mut Vec<usize>, p: usize) {
    if !cands.contains(&p) {
        cands.push(p);
    }
}

/// The automatic period of FORECAST.ETS.SEASONALITY and seasonality 1.
/// Measured on Excel 20527 (eng24 boundary probes, ig1 AUTOPERIOD recovery):
/// - n = 5..10: dominant-bin ratio `>= 2 - 3/n` on the cosine-tapered residual
///   periodogram (no taper below 10; `floor(n/10)` end points from 10).
/// - n >= 11: AUTOPERIOD (Vlachos, Yu & Castelli 2005) over above-mean bins
///   `k >= 2` with rounded period `p_bin in 2..=floor(2n/5)`. Each bin maps to
///   ACF core `[lo-1, hi-1]` and hill window `[max(1, lo-1), min(n-1, hi+1)]`,
///   validated by a 2-segment piecewise-linear hill (minimizing sum of segment
///   MSEs) whose left/right slopes bracket the flanking slope (0 when the left
///   flank is empty), peak unbiased ACF `>= 0.45`, and short-period tail checks
///   (`p = 3`: `u[3] > u[2] && u[4] > u[5] && u[4] > u[6]` in short windows;
///   `p = 4`: `u[5] > u[6]`; `p = 5`: `slope(u[6..=9]) < 0`).
/// - Spectral 2.5-sigma outlier (`p_top >= 6`) enters the candidate set.
///   If its lag has autocorrelation below 0.45, the strongest eligible lower
///   frequency bin can supersede it by validating a longer period whose
///   second harmonic falls in the outlier bin (Excel 20625, dc-j9k.68).
///   Remaining candidates break ties by fundamental power on trailing whole
///   cycles of raw `y`. When no candidate survives, a dominant bin (`p <= 5`)
///   falls back to `p_top`.
fn detect_period(y: &[f64]) -> usize {
    const MIN_PEAK_CORRELATION: f64 = 0.45;
    let n = y.len();
    let last_lag = 2 * n / 5;
    if last_lag < 2 {
        return 0;
    }
    let e = detrended(y);
    let mean = e.iter().sum::<f64>() / n as f64;
    let d: Vec<f64> = e.iter().map(|v| v - mean).collect();
    let denominator: f64 = d.iter().map(|v| v * v).sum();
    let scale = y.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    if denominator == 0.0 || !denominator.is_finite() || denominator.sqrt() <= scale * 1e-12 {
        return 0;
    }
    let r = |k: usize| -> f64 { (0..n - k).map(|i| d[i] * d[i + k]).sum::<f64>() / denominator };
    if n == 5 {
        // Measured five-point boundary (eng21 five_quadric: 22 switch points
        // on 12 residual planes fit one quadric to 7e-7): period two exactly
        // when the residual periodogram at frequency two exceeds 1.4 times
        // the one at frequency one.
        let c1 = r(1) + r(4);
        let c2 = r(2) + r(3);
        let (w1, w2) = ((2.0 * std::f64::consts::PI / 5.0).cos(), (4.0 * std::f64::consts::PI / 5.0).cos());
        let p1 = 1.0 + 2.0 * c1 * w1 + 2.0 * c2 * w2;
        let p2 = 1.0 + 2.0 * c1 * w2 + 2.0 * c2 * w1;
        return if p2 > 1.4 * p1 { 2 } else { 0 };
    }

    let mut u = Vec::with_capacity(n);
    u.push(1.0);
    for k in 1..n {
        u.push(r(k) * n as f64 / (n - k) as f64);
    }

    let ends = n / 10;
    let mut te: Vec<f64> = if n >= 10 {
        (0..n)
            .map(|t| {
                let j = t.min(n - 1 - t);
                let w = if j >= ends {
                    1.0
                } else if ends == 1 {
                    0.5
                } else {
                    0.5 * (1.0 - (std::f64::consts::PI * (j as f64 + 0.5) / ends as f64).cos())
                };
                e[t] * w
            })
            .collect()
    } else {
        e
    };
    let te_mean = te.iter().sum::<f64>() / n as f64;
    for v in &mut te {
        *v -= te_mean;
    }

    let half = n / 2;
    let mut power = vec![0.0_f64; half + 1];
    for (k, slot) in power.iter_mut().enumerate().skip(1) {
        let (mut re, mut im) = (0.0, 0.0);
        for (t, &v) in te.iter().enumerate() {
            let angle = -2.0 * std::f64::consts::PI * ((k * t) % n) as f64 / n as f64;
            re += v * angle.cos();
            im += v * angle.sin();
        }
        *slot = re * re + im * im;
    }

    let mut top = 1;
    for k in 2..=half {
        if power[k] > power[top] {
            top = k;
        }
    }
    let second = (1..=half).filter(|&k| k != top).map(|k| power[k]).fold(0.0_f64, f64::max);
    let dominant = power[top] >= (2.0 - 3.0 / n as f64) * second;
    let p_top = (n as f64 / top as f64 - 0.5 - 1e-12).ceil() as usize;
    let dominant_period = if dominant && p_top >= 2 && p_top <= last_lag.min(5) { p_top } else { 0 };
    if n <= 10 {
        return dominant_period;
    }

    let outlier_period = if half >= 2 && p_top >= 6 && p_top <= last_lag && power[top] > 0.0 && power[top].is_finite() {
        let count = half as f64;
        let (total, squares) = power[1..].iter().fold((0.0, 0.0), |(sum, squares), &bin| {
            let normalized = bin / power[top];
            (sum + normalized, squares + normalized * normalized)
        });
        let mean_norm = total / count;
        let variance = (squares / (count - 1.0) - mean_norm * mean_norm).max(0.0);
        if 1.0 - mean_norm >= 2.5 * variance.sqrt() { p_top } else { 0 }
    } else {
        0
    };

    let mean_power = power[1..].iter().sum::<f64>() / half as f64;
    let bin_period = |k: usize| (n as f64 / k as f64 + 0.5).floor() as usize;
    let harmonic_bin = if outlier_period != 0 && u[outlier_period] < MIN_PEAK_CORRELATION {
        (2..top)
            .rev()
            .filter(|&k| bin_period(k) <= last_lag)
            .max_by(|&a, &b| power[a].total_cmp(&power[b]))
    } else {
        None
    };
    let tail_ok = |pk: usize| -> bool {
        if pk == 4 {
            6 < n && u[5] > u[6]
        } else if pk == 5 {
            9 < n && ols_mse_and_slope(&u[6..=9]).1 < 0.0
        } else {
            true
        }
    };

    let mut candidates = Vec::new();
    let mut flank = Vec::with_capacity(6);
    for k in 2..=half {
        if power[k] <= mean_power {
            continue;
        }
        let p_bin = bin_period(k);
        if p_bin < 2 || p_bin > last_lag {
            continue;
        }
        let inv_p = 1.0 / p_bin as f64;
        let inv_n = 1.0 / n as f64;
        let lo = (1.0 / (inv_p + inv_n) + 0.5).floor() as usize;
        let hi = (1.0 / (inv_p - inv_n) + 0.5).floor() as usize;
        let a = lo.saturating_sub(1);
        let b = hi.saturating_sub(1);
        let left = a.max(1);
        let right = (b + 2).min(n - 1);

        let pk = if right < left + 4 {
            if p_bin == 3
                && 3 <= last_lag
                && u[3] >= MIN_PEAK_CORRELATION
                && u[3] > u[2]
                && 6 < n
                && u[4] > u[5]
                && u[4] > u[6]
            {
                push_unique(&mut candidates, 3);
                continue;
            }
            argmax_slice(&u, left, right)
        } else {
            let mut best_err = f64::INFINITY;
            let mut best_sl = 0.0;
            let mut best_sr = 0.0;
            for t in (left + 2)..=(right - 2) {
                let (mse_l, sl) = ols_mse_and_slope(&u[left..=t]);
                let (mse_r, sr) = ols_mse_and_slope(&u[t..=right]);
                let err = mse_l + mse_r;
                if err < best_err {
                    best_err = err;
                    best_sl = sl;
                    best_sr = sr;
                }
            }

            let left_flank_start = a.saturating_sub(3).max(2);
            let bf = if left_flank_start >= a {
                0.0
            } else {
                flank.clear();
                flank.extend(left_flank_start..a);
                flank.extend((b + 1)..(b + 4).min(n));
                ols_slope_indexed(&u, &flank)
            };
            if !(best_sl > bf && best_sr < bf) {
                continue;
            }
            let (pk_lo, pk_hi) = if p_bin == 4 { (a.max(1), b.min(n - 1)) } else { (left, right) };
            argmax_slice(&u, pk_lo, pk_hi)
        };
        if pk >= 4 && pk <= last_lag && u[pk] >= MIN_PEAK_CORRELATION && tail_ok(pk) {
            if harmonic_bin == Some(k)
                && pk > outlier_period
                && (2.0 * n as f64 / pk as f64).round() as usize == top
            {
                return pk;
            }
            push_unique(&mut candidates, pk);
        }
    }

    if outlier_period != 0 {
        push_unique(&mut candidates, outlier_period);
    }
    if candidates.is_empty() {
        return dominant_period;
    }
    if candidates.len() == 1 {
        return candidates[0];
    }
    let mut best_p = candidates[0];
    let mut best_score = fundamental_power(y, best_p);
    for &p in &candidates[1..] {
        let s = fundamental_power(y, p);
        if s > best_score {
            best_score = s;
            best_p = p;
        }
    }
    best_p
}

/// The five candidates Excel 20527 selects among for period two on five
/// observations: all 655 recorded five-point selections (bank5, eng21) and
/// 120 live Touchstone five-point probes (ig1, dc-iuwct.54). Six- and
/// seven-point period-two histories search the full `SEASONAL_PARAMETERS` grid.
const SHORT_TWO_PARAMETERS: &[(f64, f64, f64)] = &[
    (0.751, 0.001, 0.001),
    (0.501, 0.001, 0.001),
    (0.251, 0.001, 0.001),
    (0.126, 0.001, 0.001),
    (0.002, 0.001, 0.125),
];

/// Excel's seasonal initial states for any history of at least two cycles
/// (eng23 rational tables; cb4/nx3 bit-exact arithmetic):
/// - a history shorter than N0 = ceil(3.5 p) is first extended to N0 points:
///   e_j = y_(j mod p) + (j - j mod p) tau, with tau the mean over the phases of
///   each phase's 1-indexed raw-moment least-squares slope divided by p^2;
/// - over the working length m = min(len, 5p), a subtract-first 2p-point
///   sliding moving average T_t (t = 0..m-2p) detrends e_(t + pivot) where
///   pivot = p/2 - 1; each phase's raw index is the mean of e_pos - T_t;
/// - the raw indices are centred by their mean c, shifting initial level
///   (y_0 - raw_0) + c and seasons_q = raw_q - c;
/// - the initial trend is the least-squares slope of the deseasonalised
///   (extended) history e_j - seasons_(j mod p).
/// Returns the initial states and the (extended) history.
fn excel_seasonal_initial(y: &[f64], period: usize) -> (Initial, Vec<f64>) {
    let p = period;
    let start = (7 * p).div_ceil(2);
    let mut e = y.to_vec();
    if y.len() < start {
        let n = y.len();
        let mut tau = 0.0;
        for q in 0..p {
            let phase: Vec<f64> = (q..n).step_by(p).map(|i| y[i]).collect();
            tau += regression_slope(&phase);
        }
        tau /= (p * p) as f64;
        for j in n..start {
            e.push(y[j % p] + (j - j % p) as f64 * tau);
        }
    }
    let m = e.len().min(5 * p);
    let w = 2 * p;
    let len = m - w;
    let pivot = p / 2 - 1;
    let mut sum = 0.0;
    for &v in &e[..w] {
        sum += v;
    }
    let mut trend = Vec::with_capacity(len);
    trend.push(sum / w as f64);
    for t in 1..len {
        sum = (sum - e[t - 1]) + e[t + w - 1];
        trend.push(sum / w as f64);
    }
    let mut phase_sum = vec![0.0; p];
    let mut phase_cnt = vec![0usize; p];
    for (t, &tr) in trend.iter().enumerate() {
        let pos = t + pivot;
        let q = pos % p;
        phase_sum[q] += e[pos] - tr;
        phase_cnt[q] += 1;
    }
    let mut raw = Vec::with_capacity(p);
    let mut center = 0.0;
    for q in 0..p {
        let mean = phase_sum[q] / phase_cnt[q] as f64;
        raw.push(mean);
        center += mean;
    }
    center /= p as f64;
    let seasons: Vec<f64> = raw.iter().map(|&v| v - center).collect();
    let adjusted: Vec<f64> = e.iter().enumerate().map(|(j, &v)| v - seasons[j % p]).collect();
    let level = (y[0] - raw[0]) + center;
    let initial = Initial { level, trend: regression_slope(&adjusted), seasons, period, gamma: 0.0 };
    (initial, e)
}

fn seasonal_general(
    site: &Site<'_>,
    y: &[f64],
    period: usize,
    output: Output,
    statistic: f64,
    horizon: f64,
    confidence: f64,
) -> Step<CalcValue> {
    if output == Output::Seasonality {
        return site.number(period as f64);
    }
    let short = period == 2 && y.len() == 5;
    // Below N0 the parameter search runs on the extension (eng23 probe_short:
    // Excel's triple in 447 of 520 short histories, 220 when searching the history).
    let (initial, search_history) = excel_seasonal_initial(y, period);
    let candidates = if short { SHORT_TWO_PARAMETERS } else { SEASONAL_PARAMETERS };
    // With the reset below and these candidates, search_window reproduces
    // Excel's triple for every one of 16,020 short and 687 longer probe
    // histories (eng23 wingrid3; eng21's list with STAT's ten-point tail and no
    // reset: 13,237 and, of eng21's 400, 218).
    let searched = search_history.len();
    let start = searched - search_window(searched, period);
    let validation = (start != 0).then_some(start);
    let run_on = |history: &[f64], (alpha, beta, gamma): (f64, f64, f64), validation: Option<usize>| {
        let initial = Initial { gamma, seasons: initial.seasons.clone(), ..initial };
        Model::run_initialized(history, alpha, beta, validation, initial)
    };
    let run = |candidate, validation| run_on(y, candidate, validation);
    let mut best = f64::INFINITY;
    let mut chosen = candidates[0];
    for &candidate in candidates {
        let score = run_on(&search_history, candidate, validation).squared_errors();
        if score < best {
            best = score;
            chosen = candidate;
        }
    }
    let (alpha, mut beta, gamma) = chosen;
    let mut model = run(chosen, None);
    // An unstable final trend restarts from the last observation with the
    // starting trend, and Excel then reports beta 0.001 (22 of 23 eng21
    // probe_fc forecasts that Excel's reported triple did not reproduce).
    if model.trend_unstable(y) {
        model.level = *y.last().unwrap() - model.seasons[(y.len() - 1) % period];
        model.trend = initial.trend;
        beta = 0.001;
    }
    let center = model.seasons.iter().fold(0.0, |sum, &v| sum + v) / period as f64;
    model.level += center;
    for season in &mut model.seasons {
        *season -= center;
    }
    match output {
        Output::Statistic => {
            let statistics = model.statistics(y);
            finite(site, match statistic as u8 {
                1 => alpha,
                2 => beta,
                3 => gamma,
                4..=7 => statistics[statistic as usize - 4],
                _ => unreachable!(),
            })
        }
        Output::Forecast => {
            let phase = season_phase(y.len(), horizon, period);
            // Frozen p=2 midpoint discriminators (dc-ymjf.2.148, 10/10 vs 0/10 binary64 `*`)
            // confirm the same stored x87 PC64 multiply + level addition as nonseasonal_fit.
            use crate::ported::excel_numeric::{excel_x87_add, excel_x87_mul};
            finite(
                site,
                excel_x87_add(
                    excel_x87_add(model.level, excel_x87_mul(horizon, model.trend)),
                    model.seasons[phase],
                ),
            )
        }
        Output::Confidence => confidence_interval(
            site,
            model.squared_errors() / y.len() as f64,
            alpha,
            beta,
            gamma,
            period,
            horizon,
            confidence,
            true,
        ),
        Output::Seasonality => unreachable!(),
    }
}

/// Route a history the measured paths refused: the automatic or explicit
/// period, then the seasonal or nonseasonal general fit.
fn general(
    site: &Site<'_>,
    y: &[f64],
    season: f64,
    output: Output,
    statistic: f64,
    horizon: f64,
    confidence: f64,
) -> Step<CalcValue> {
    let period = if season == 0.0 || y.len() <= 4 {
        0
    } else if season == 1.0 {
        detect_period(y)
    } else if season > (y.len() / 2) as f64 {
        0
    } else {
        season as usize
    };
    if period >= 2 {
        seasonal_general(site, y, period, output, statistic, horizon, confidence)
    } else {
        nonseasonal_fit(site, y, output, statistic, horizon, confidence, true)
    }
}

/// Keep a measured path's answer; where it is not held, calculate generally.
fn or_general(
    measured: Step<CalcValue>,
    general: impl FnOnce() -> Step<CalcValue>,
) -> Step<CalcValue> {
    match measured {
        Err(Exit::Fault(FunctionFault::NotHeld { .. })) => general(),
        other => other,
    }
}

pub(super) fn evaluate(
    site: &Site<'_>,
    args: &[CalcValue],
    sv: &mut EvalServices<'_>,
    output: Output,
) -> Step<CalcValue> {
    let offset = usize::from(matches!(output, Output::Forecast | Output::Confidence));
    // Excel 20527 coerces the target before inspecting history shape or option domains.
    let target = if offset == 1 { Some(num(site, args, 0)?) } else { None };
    // In the measured serial layout, STAT and SEASONALITY coerce scalar options
    // before rejecting a scalar history against a longer numeric timeline. Keep
    // this to the one-cell history and option kinds covered by controls; an
    // unreadable option keeps its typed refusal.
    if matches!(output, Output::Statistic | Output::Seasonality)
        && let Some(timeline) = args.get(1).and_then(CalcValue::as_array)
        && timeline.shape().cols == 1
        && (2..=32).contains(&timeline.shape().rows)
        && timeline.iter_row_major().all(|v| v.as_number().is_some_and(|n| n.is_finite() && !n.is_subnormal()))
        && !matches!(args[0].core_ref(), CoreRef::Array(_))
        && args[2..].iter().all(|v| match v.core_ref() {
            CoreRef::Number(n) => n.is_finite() && !n.is_subnormal(),
            CoreRef::Error(_) | CoreRef::Text(_) => true,
            _ => false,
        })
    {
        let history = grid(site, args, 0, sv)?;
        if history.rows == 1 && history.cols == 1
            && match history.cells[0].core_ref() {
                CoreRef::Number(n) => n.is_finite() && !n.is_subnormal(),
                CoreRef::Text(_) | CoreRef::Logical(_) | CoreRef::Empty | CoreRef::Error(_) => true,
                _ => false,
            }
        {
            for index in 2..args.len() {
                num(site, args, index)?;
            }
            return fail(WorksheetErrorCode::NA);
        }
    }
    // Excel 20527 checks this unequal-length shape before reading the scalar history,
    // even when that scalar holds an error. Optional scalar coercion can take priority,
    // so admit only the default-argument forms covered by independent controls.
    let default_options = match output {
        Output::Seasonality => args.len() == 2,
        Output::Statistic => args.len() == 3 && args[2].as_number().is_some_and(|n| {
            n.is_finite() && n.fract() == 0.0 && (1.0..=8.0).contains(&n)
        }),
        Output::Forecast | Output::Confidence => args.len() == 3
            && args[0].as_number().is_some_and(|n| n.is_finite() && !n.is_subnormal()),
    };
    let scalar_history = match args.get(offset).map(CalcValue::core_ref) {
        Some(CoreRef::Number(n)) => n.is_finite() && !n.is_subnormal(),
        Some(CoreRef::Text(_) | CoreRef::Logical(_) | CoreRef::Empty | CoreRef::Error(_)) => true,
        _ => false,
    };
    if default_options && scalar_history
        && let Some(timeline) = args.get(offset + 1).and_then(CalcValue::as_array)
    {
        let shape = timeline.shape();
        if shape.cols == 1 && (2..=16).contains(&shape.rows)
            && timeline.iter_row_major().all(|v| v.as_number().is_some_and(|n| n.is_finite() && !n.is_subnormal()))
        {
            return fail(WorksheetErrorCode::NA);
        }
    }
    let y = vector(site, args, offset, sv)?;
    let t = vector(site, args, offset + 1, sv)?;
    // Excel checks paired vector lengths before the scalar option domains.
    if y.len() != t.len() {
        return fail(WorksheetErrorCode::NA);
    }
    let statistic = if output == Output::Statistic {
        num(site, args, 2)?
    } else {
        8.0
    };
    let confidence = if output == Output::Confidence {
        opt(site, args, 3, 0.95)?
    } else {
        0.0
    };
    let option_start = match output {
        Output::Forecast | Output::Statistic => 3,
        Output::Confidence => 4,
        Output::Seasonality => 2,
    };
    let season = if output == Output::Seasonality {
        1.0
    } else {
        opt(site, args, option_start, 1.0)?
    };
    let completion_at = option_start + usize::from(output != Output::Seasonality);
    let completion = opt(site, args, completion_at, 1.0)?;
    let aggregation = opt(site, args, completion_at + 1, 1.0)?;
    if statistic.fract() != 0.0 || !(1.0..=8.0).contains(&statistic)
        || !(0.0..1.0).contains(&confidence)
        // Bisection locates the last admitted integer period at 8784; the
        // error applies to step statistics and confidence zero as well.
        || season < 0.0 || season > 8_784.0 || season.fract() != 0.0
        || (completion != 0.0 && completion != 1.0)
        || aggregation.fract() != 0.0 || !(1.0..=7.0).contains(&aggregation)
    {
        return fail(WorksheetErrorCode::Num);
    }
    let series = prepare(site, y, t, completion, aggregation)?;
    let target = target.unwrap_or(series.last);
    if target < series.first {
        // A reversed input timeline can admit extrapolation before its minimum
        // where an ascending one returns NUM. Its admission rule is still open.
        if !series.ordered {
            return Err(unfinished(site));
        }
        // Touchstone (eng21): within one step before the first timestamp the
        // forecast interpolates towards the backward extrapolation at
        // first - step; at or before that point it is NUM.
        if output != Output::Forecast {
            return fail(WorksheetErrorCode::Num);
        }
        // Excel measures prehistory from the last timestamp ((target - last)/step - first_horizon):
        // it snaps within 0.001 step of the first timestamp (along > -0.001) and refuses
        // more than 0.999 step before it (along < -0.999) (Touchstone, eng21 & ig1 dc-iuwct.54).
        let first_horizon = -((series.values.len() - 1) as f64);
        let along = (target - series.last) / series.step - first_horizon;
        if along < -0.999 {
            return fail(WorksheetErrorCode::Num);
        }
        let fitted = |h: f64| -> Step<f64> {
            match evaluate_fitted(site, &series, output, statistic, confidence, season, h)?.core_ref() {
                CoreRef::Number(v) => Ok(v),
                _ => Err(unfinished(site)),
            }
        };
        let first = fitted(first_horizon)?;
        if along > -0.001 {
            return site.number(first);
        }
        let second = fitted(first_horizon + 1.0)?;
        return finite(site, first + (second - first) * along);
    }
    if output == Output::Confidence && target <= series.last {
        // On directly observed, ascending timelines Excel returns zero inside
        // the last historical interval. Points near either endpoint snap to
        // the endpoint and return NUM, so keep to the independently measured
        // interior until the exact snapping boundary is identified.
        let fraction = (target - (series.last - series.step)) / series.step;
        if series.ordered && series.directly_observed
            && (4..=20).contains(&series.values.len()) && season <= 1.0
            && (1.0 / 128.0..=127.0 / 128.0).contains(&fraction)
        {
            return site.number(0.0);
        }
        return fail(WorksheetErrorCode::Num);
    }
    let horizon = if offset == 1 {
        // 2^31 times a finite positive binary64 step is exact when finite.
        // A rounded subtraction strictly above that representable bound is
        // also above it exactly, even when the subtraction cannot be stored
        // without loss. Excel rejects this horizon before fitting the model.
        let distance_bound = (i32::MAX as f64 + 1.0) * series.step;
        let rounded_distance = target - series.last;
        if distance_bound.is_finite()
            && rounded_distance.is_finite()
            && rounded_distance > distance_bound
        {
            return fail(WorksheetErrorCode::Num);
        }
        if output == Output::Confidence && rounded_distance.is_finite()
            && rounded_distance < (i32::MAX as f64) * series.step
        {
            // Excel forms the normal probability from a stored x87 complement.
            // When it rounds to one half, the confidence interval is zero.
            let tail = crate::ported::excel_numeric::excel_x87_sub(1.0, confidence);
            let probability = crate::ported::excel_numeric::excel_x87_sub(1.0, tail / 2.0);
            if probability == 0.5 {
                return site.number(0.0);
            }
        }
        // An inexact subtraction or division takes Excel's binary64 horizon
        // (the rounded quotient), which then snaps within 0.001 of a
        // timestamp: bank2 rows with targets one ULP from a timestamp.
        let exact_distance = exact_sum(target, -series.last);
        let distance = exact_distance.unwrap_or(target - series.last);
        let h = distance / series.step;
        let exact_division = exact_distance.is_some()
            && h * series.step == distance
            && (h.fract() == 0.0 || h.mul_add(series.step, -distance) == 0.0);
        if !exact_division && !matches!(output, Output::Forecast | Output::Confidence) {
            return Err(unfinished(site));
        }
        if h.fract() != 0.0 && (!matches!(output, Output::Forecast | Output::Confidence) || h >= i32::MAX as f64) {
            return Err(unfinished(site));
        }
        // Both target functions share this signed-32-bit future-index limit,
        // including the confidence-zero case (three translated/scaled timelines).
        if h > i32::MAX as f64 {
            return fail(WorksheetErrorCode::Num);
        }
        h
    } else {
        0.0
    };
    if output == Output::Statistic && statistic == 8.0 {
        return site.number(series.step);
    }
    if output == Output::Confidence && confidence == 0.0 {
        return site.number(0.0);
    }
    if matches!(output, Output::Forecast | Output::Confidence) && horizon.fract() != 0.0 {
        let lower_horizon = horizon.floor();
        let fraction = horizon - lower_horizon;
        // The interval is NUM at the last timestamp itself, and its forecast
        // variance grows linearly from zero across the first step.
        if output == Output::Confidence && lower_horizon == 0.0 {
            if fraction <= 0.001 {
                return fail(WorksheetErrorCode::Num);
            }
            if 1.0 - fraction <= 0.001 {
                return evaluate_fitted(site, &series, output, statistic, confidence, season, 1.0);
            }
            return evaluate_fitted(site, &series, output, statistic, confidence, season, horizon);
        }
        // Excel snaps targets within 0.001 of a timeline point; otherwise
        // FORECAST.ETS interpolates the two fitted forecasts with stored x87
        // operations and CONFINT interpolates the forecast error variance.
        if fraction <= 0.001 {
            return evaluate_fitted(site, &series, output, statistic, confidence, season, lower_horizon);
        }
        if 1.0 - fraction <= 0.001 {
            return evaluate_fitted(site, &series, output, statistic, confidence, season, lower_horizon + 1.0);
        }
        if output == Output::Confidence {
            return evaluate_fitted(site, &series, output, statistic, confidence, season, horizon);
        }
        if horizon < 0.0 {
            // Inside the history Excel anchors at the later timestamp and
            // uses the slope of the interval after it, in binary64, as it
            // does before the first timestamp (Touchstone, eng21).
            let anchor = lower_horizon + 1.0;
            let fitted = |h: f64| -> Step<f64> {
                match evaluate_fitted(site, &series, output, statistic, confidence, season, h)?.core_ref() {
                    CoreRef::Number(v) => Ok(v),
                    _ => Err(unfinished(site)),
                }
            };
            let first = fitted(anchor)?;
            let second = fitted(anchor + 1.0)?;
            return finite(site, first + (second - first) * (horizon - anchor));
        }
        let lower = evaluate_fitted(site, &series, output, statistic, confidence, season, lower_horizon)?;
        let upper = evaluate_fitted(site, &series, output, statistic, confidence, season, lower_horizon + 1.0)?;
        let (lower, upper) = match (lower.core_ref(), upper.core_ref()) {
            (CoreRef::Number(lower), CoreRef::Number(upper)) => (lower, upper),
            _ => return Err(unfinished(site)),
        };
        use crate::ported::excel_numeric::{excel_x87_add, excel_x87_mul, excel_x87_sub};
        let difference = excel_x87_sub(upper, lower);
        let increment = excel_x87_mul(difference, fraction);
        return site.number(excel_x87_add(lower, increment));
    }
    evaluate_fitted(site, &series, output, statistic, confidence, season, horizon)
}

fn evaluate_fitted(
    site: &Site<'_>,
    series: &Series,
    output: Output,
    statistic: f64,
    confidence: f64,
    season: f64,
    horizon: f64,
) -> Step<CalcValue> {
    or_general(
        evaluate_measured(site, series, output, statistic, confidence, season, horizon),
        || general(site, &series.values, season, output, statistic, horizon, confidence),
    )
}

fn evaluate_measured(
    site: &Site<'_>,
    series: &Series,
    output: Output,
    statistic: f64,
    confidence: f64,
    season: f64,
    horizon: f64,
) -> Step<CalcValue> {
    if !series.preparation_exact {
        return Err(unfinished(site));
    }
    // R46's adjacent/exponent controls identify the detector's minimum
    // history: lag two first becomes available at five prepared observations.
    // This does not identify the detector or fitted model for longer histories.
    if output == Output::Seasonality && series.values.len() <= 4 {
        return site.number(0.0);
    }
    // Fresh controls cover strict curvature with direct observations, shuffled
    // timelines, translations and scale changes. Other detector shapes and
    // the seasonal fitted model remain on their existing paths.
    if output == Output::Seasonality
        && series.directly_observed
        && strictly_curved_five(&series.values)
    {
        return site.number(0.0);
    }
    // R47 paired questions identify the nonseasonal fit before five prepared
    // observations, including explicit period two at length four. Admit its
    // proved zero-innovation region. Rounded short initialization and nonzero
    // confidence/statistic differences still keep the other short fits open.
    // Measured (dc-6332.5): every prepared history of four or fewer points is
    // fitted nonseasonally, whether the period is automatic, explicit two or zero.
    let short_history = series.values.len() <= 4;
    let nonseasonal_mode =
        season == 0.0 || short_history || season > (series.values.len() / 2) as f64;
    // The five-point curvature detector above identifies no seasonality for
    // these directly observed histories. The existing nonseasonal forecast
    // agrees with independent Excel controls for exact-scaled histories and
    // when an exact centered numerator verifies the initial slope. Other
    // histories and confidence intervals retain their existing refusals.
    if season == 1.0 && series.directly_observed && horizon >= 1.0
        && strictly_curved_five(&series.values)
        && (exact_scaled_history(&series.values).is_some()
            || exact_centered_five_slope(&series.values))
        && output == Output::Forecast
    {
        return nonseasonal(site, &series.values, output, statistic, horizon, confidence);
    }
    // Default-confidence controls cover these five-point fits when both the
    // original and extended initial slopes are verified. Keep horizons below
    // the signed-32-bit variance wrap boundary covered by this route.
    if season == 1.0 && series.directly_observed && horizon >= 1.0
        && horizon < 2.0_f64.powi(30) && horizon.fract() == 0.0
        && confidence == 0.95 && output == Output::Confidence
        && strictly_curved_five(&series.values)
        && resolved_five_point_curvature(&series.values)
        && verified_five_point_extension(&series.values)
    {
        return nonseasonal(site, &series.values, output, statistic, horizon, confidence);
    }
    if season == 1.0 && !nonseasonal_mode {
        match exact_seasonal(site, &series.values, 2, output, statistic, horizon, true) {
            Ok(value) => return Ok(value),
            Err(Exit::Fault(FunctionFault::NotHeld { .. })) => {}
            Err(why) => return Err(why),
        }
    }
    if season > 1.0 && !nonseasonal_mode {
        return exact_seasonal(
            site,
            &series.values,
            season as usize,
            output,
            statistic,
            horizon,
            false,
        );
    }
    let trend = match exact_trend(&series.values) {
        Some(trend) => trend,
        None => {
            if nonseasonal_mode {
                return nonseasonal(site, &series.values, output, statistic, horizon, confidence);
            }
            return Err(unfinished(site));
        }
    };
    if output == Output::Seasonality {
        return site.number(0.0);
    }
    // An explicit period longer than half the data uses the nonseasonal model.
    match output {
        Output::Forecast => {
            let product = horizon * trend;
            // The trend's quantum is at least 2^-900 and h is an integer, so
            // a nonzero multiplication residue cannot underflow here.
            if !product.is_finite() || horizon.mul_add(trend, -product) != 0.0 {
                return Err(unfinished(site));
            }
            site.number(
                exact_sum(*series.values.last().unwrap(), product)
                    .ok_or_else(|| unfinished(site))?,
            )
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
