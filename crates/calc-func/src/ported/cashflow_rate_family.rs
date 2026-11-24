use crate::ported::excel_numeric::crt_pow;
use crate::value::WorksheetErrorCode;
use crate::value::{CalcValue, CoreRef, CoreValue};

const ROOT_TOLERANCE: f64 = 1e-8;
const ROOT_DERIVATIVE_EPS: f64 = 1e-12;
const ROOT_MAX_ITERATIONS: usize = 100;
const MIN_VALID_RATE: f64 = -0.999_999_999;
const IRR_PUBLICATION_ULP_SCAN_RADIUS: usize = 16;
const ROOT_POLISH_ITERATIONS: usize = 64;
const XIRR_TWO_CASHFLOW_RELATIVE_BRACKET_TOLERANCE: f64 = 2e-8;
const XIRR_TWO_CASHFLOW_MAX_BRACKET_EXPANSIONS: usize = 128;
const XIRR_GENERAL_POSITIVE_ROOT_BRENT_EPS: f64 = 1e-8;
const XIRR_GENERAL_POSITIVE_ROOT_MAX_BRACKET_EXPANSIONS: usize = 128;

#[derive(Debug, Clone, PartialEq)]
pub enum CashflowRateEvalError {
    ArityMismatch {
        expected_min: usize,
        expected_max: usize,
        actual: usize,
    },
    Domain(WorksheetErrorCode),
}

fn numeric_from_cell(cell: &CalcValue) -> Result<f64, CashflowRateEvalError> {
    match cell.core_ref() {
        CoreRef::Number(n) => Ok(n),
        CoreRef::Error(code) => Err(CashflowRateEvalError::Domain(code)),
        CoreRef::Text(_)
        | CoreRef::Logical(_)
        | CoreRef::Empty
        | CoreRef::Missing
        | CoreRef::Array(_)
        | CoreRef::Reference(_) => Err(CashflowRateEvalError::Domain(WorksheetErrorCode::Value)),
    }
}

pub(crate) fn scalar_number_from_eval(value: &CalcValue) -> Result<f64, CashflowRateEvalError> {
    match value.core_ref() {
        CoreRef::Number(n) => Ok(n),
        CoreRef::Logical(flag) => Ok(if flag { 1.0 } else { 0.0 }),
        CoreRef::Error(code) => Err(CashflowRateEvalError::Domain(code)),
        CoreRef::Array(array) => {
            let shape = array.shape();
            if shape.rows == 1 && shape.cols == 1 {
                numeric_from_cell(array.get(0, 0).expect("single cell"))
            } else {
                Err(CashflowRateEvalError::Domain(WorksheetErrorCode::Value))
            }
        }
        CoreRef::Text(_) | CoreRef::Reference(_) => {
            Err(CashflowRateEvalError::Domain(WorksheetErrorCode::Value))
        }
        _ => Err(CashflowRateEvalError::Domain(WorksheetErrorCode::Value)),
    }
}

pub(crate) fn collect_numeric_vector_from_eval(
    value: &CalcValue,
) -> Result<Vec<f64>, CashflowRateEvalError> {
    match value.core_ref() {
        CoreRef::Number(n) => Ok(vec![n]),
        CoreRef::Error(code) => Err(CashflowRateEvalError::Domain(code)),
        CoreRef::Array(array) => {
            let shape = array.shape();
            if shape.rows > 1 && shape.cols > 1 {
                return Err(CashflowRateEvalError::Domain(WorksheetErrorCode::Ref));
            }
            let mut out = Vec::with_capacity(shape.rows * shape.cols);
            for cell in array.iter_row_major() {
                out.push(numeric_from_cell(cell)?);
            }
            Ok(out)
        }
        CoreRef::Text(_) | CoreRef::Logical(_) | CoreRef::Reference(_) => {
            Err(CashflowRateEvalError::Domain(WorksheetErrorCode::Value))
        }
        _ => Err(CashflowRateEvalError::Domain(WorksheetErrorCode::Value)),
    }
}

/// Excel `IRR` collects the values array/range with these rules (verified vs live
/// Excel 16.0 b20026, identical for array constants and ranges):
/// numbers are included; text, logical, and blank/empty cells are **ignored**;
/// and **any** error cell maps to `#VALUE!` (the specific code is not propagated).
/// Excel 20527 also reads rectangular arrays and ranges in row-major order.
/// A whole-argument scalar error still propagates as usual. This differs from the
/// strict positional collector used by XNPV/XIRR, whose values must stay aligned
/// with the parallel dates array.
pub(crate) fn collect_irr_values_from_eval(
    value: &CalcValue,
) -> Result<Vec<f64>, CashflowRateEvalError> {
    match value.core_ref() {
        CoreRef::Number(n) => Ok(vec![n]),
        CoreRef::Error(code) => Err(CashflowRateEvalError::Domain(code)),
        CoreRef::Array(array) => {
            let shape = array.shape();
            let mut out = Vec::with_capacity(shape.rows * shape.cols);
            for cell in array.iter_row_major() {
                match cell.core_ref() {
                    CoreRef::Number(n) => out.push(n),
                    CoreRef::Error(_) => {
                        return Err(CashflowRateEvalError::Domain(WorksheetErrorCode::Value));
                    }
                    CoreRef::Text(_) | CoreRef::Logical(_) | CoreRef::Empty | CoreRef::Missing => {} // ignored, like Excel
                    CoreRef::Array(_) | CoreRef::Reference(_) => {
                        return Err(CashflowRateEvalError::Domain(WorksheetErrorCode::Value));
                    }
                }
            }
            Ok(out)
        }
        CoreRef::Text(_) | CoreRef::Logical(_) | CoreRef::Reference(_) => {
            Err(CashflowRateEvalError::Domain(WorksheetErrorCode::Value))
        }
        _ => Err(CashflowRateEvalError::Domain(WorksheetErrorCode::Value)),
    }
}

pub(crate) fn serial_from_number(value: f64) -> Result<i64, CashflowRateEvalError> {
    if !value.is_finite() {
        return Err(CashflowRateEvalError::Domain(WorksheetErrorCode::Num));
    }
    let serial = value.trunc() as i64;
    if serial < 0 {
        return Err(CashflowRateEvalError::Domain(WorksheetErrorCode::Num));
    }
    Ok(serial)
}

pub(crate) fn collect_serial_vector_from_eval(
    value: &CalcValue,
) -> Result<Vec<i64>, CashflowRateEvalError> {
    match value.core_ref() {
        CoreRef::Number(n) => Ok(vec![serial_from_number(n)?]),
        CoreRef::Error(code) => Err(CashflowRateEvalError::Domain(code)),
        CoreRef::Array(array) => {
            let shape = array.shape();
            if shape.rows > 1 && shape.cols > 1 {
                return Err(CashflowRateEvalError::Domain(WorksheetErrorCode::Ref));
            }
            let mut out = Vec::with_capacity(shape.rows * shape.cols);
            for cell in array.iter_row_major() {
                out.push(serial_from_number(numeric_from_cell(cell)?)?);
            }
            Ok(out)
        }
        CoreRef::Text(_) | CoreRef::Logical(_) | CoreRef::Reference(_) => {
            Err(CashflowRateEvalError::Domain(WorksheetErrorCode::Value))
        }
        _ => Err(CashflowRateEvalError::Domain(WorksheetErrorCode::Value)),
    }
}

fn validate_cashflows(cashflows: &[f64]) -> Result<(), WorksheetErrorCode> {
    if cashflows.len() < 2 {
        return Err(WorksheetErrorCode::Num);
    }
    if !cashflows.iter().all(|value| value.is_finite()) {
        return Err(WorksheetErrorCode::Value);
    }
    let has_positive = cashflows.iter().any(|value| *value > 0.0);
    let has_negative = cashflows.iter().any(|value| *value < 0.0);
    if !has_positive || !has_negative {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(())
}

fn validate_xcashflow_inputs(values: &[f64], dates: &[i64]) -> Result<(), WorksheetErrorCode> {
    validate_cashflows(values)?;
    if values.len() != dates.len() || dates.is_empty() {
        return Err(WorksheetErrorCode::Num);
    }
    let anchor = dates[0];
    if dates.iter().any(|date| *date < anchor) {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(())
}

// XNPV is pure discounting — no sign-change or minimum-count requirement.
// Keep: equal-length arrays, non-empty, all values finite, no date before anchor.
fn validate_xnpv_inputs(values: &[f64], dates: &[i64]) -> Result<(), WorksheetErrorCode> {
    if values.len() != dates.len() || dates.is_empty() {
        return Err(WorksheetErrorCode::Num);
    }
    if !values.iter().all(|v| v.is_finite()) {
        return Err(WorksheetErrorCode::Value);
    }
    let anchor = dates[0];
    if dates.iter().any(|date| *date < anchor) {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(())
}

fn xnpv_kernel_raw(rate: f64, values: &[f64], dates: &[i64]) -> Result<f64, WorksheetErrorCode> {
    validate_xnpv_inputs(values, dates)?;
    if !rate.is_finite() || rate <= MIN_VALID_RATE {
        return Err(WorksheetErrorCode::Num);
    }
    let base = 1.0 + rate;
    let anchor = dates[0];
    let mut total = 0.0;
    for (value, date) in values.iter().zip(dates.iter()) {
        let years = (*date - anchor) as f64 / 365.0;
        total += *value / crt_pow(base, years);
    }
    if total.is_finite() {
        Ok(total)
    } else {
        Err(WorksheetErrorCode::Num)
    }
}

fn xnpv_derivative(rate: f64, values: &[f64], dates: &[i64]) -> Result<f64, WorksheetErrorCode> {
    validate_xnpv_inputs(values, dates)?;
    if !rate.is_finite() || rate <= MIN_VALID_RATE {
        return Err(WorksheetErrorCode::Num);
    }
    let base = 1.0 + rate;
    let anchor = dates[0];
    let mut total = 0.0;
    for (value, date) in values.iter().zip(dates.iter()) {
        let years = (*date - anchor) as f64 / 365.0;
        if years == 0.0 {
            continue;
        }
        total += -years * *value / crt_pow(base, years + 1.0);
    }
    if total.is_finite() {
        Ok(total)
    } else {
        Err(WorksheetErrorCode::Num)
    }
}

fn ordered_rate(rate: f64) -> u64 {
    let bits = rate.to_bits();
    if rate.is_sign_negative() {
        !bits
    } else {
        bits | (1 << 63)
    }
}

fn rate_from_ordered(key: u64) -> f64 {
    f64::from_bits(if key & (1 << 63) == 0 {
        !key
    } else {
        key & !(1 << 63)
    })
}

/// Find the end of the residual plateau by doubling the distance, then bisecting its last
/// interval. Near zero, `1 + rate` can be constant over millions of rates: walking them one
/// by one is unbounded in practice. There are only 64 bits in the ordered representation,
/// so each end needs at most 64 distance probes and 64 bisections. A plateau with no finite
/// end returns #NUM!.
fn residual_plateau_end<F>(
    root: u64,
    residual: u64,
    lower: bool,
    f: &mut F,
) -> Result<u64, WorksheetErrorCode>
where
    F: FnMut(f64) -> Result<f64, WorksheetErrorCode>,
{
    let mut inside = root;
    let mut on_plateau = |rate| match f(rate) {
        Ok(value) => Ok(value.abs().to_bits() == residual),
        // A probe outside the NPV domain bounds the plateau just as a different residual
        // does. Bisect back into the valid interval instead of rejecting the root.
        Err(WorksheetErrorCode::Num) => Ok(false),
        Err(code) => Err(code),
    };
    for bit in 0..64 {
        let distance = 1u64 << bit;
        let outside = if lower {
            root.saturating_sub(distance)
        } else {
            root.saturating_add(distance)
        };
        let candidate = rate_from_ordered(outside);
        if candidate.is_finite() && on_plateau(candidate)? {
            inside = outside;
            continue;
        }
        let mut outside = outside;
        for _ in 0..64 {
            if inside.abs_diff(outside) <= 1 {
                // No finite end exists when even the last finite rate is on the plateau.
                if !rate_from_ordered(outside).is_finite() {
                    return Err(WorksheetErrorCode::Num);
                }
                return Ok(inside);
            }
            let midpoint = inside.min(outside) + inside.abs_diff(outside) / 2;
            let candidate = rate_from_ordered(midpoint);
            if candidate.is_finite() && on_plateau(candidate)? {
                inside = midpoint;
            } else {
                outside = midpoint;
            }
        }
        return Err(WorksheetErrorCode::Num);
    }
    Err(WorksheetErrorCode::Num)
}

fn midpoint_of_local_min_abs_residual_plateau<F>(
    root: f64,
    mut f: F,
) -> Result<f64, WorksheetErrorCode>
where
    F: FnMut(f64) -> Result<f64, WorksheetErrorCode>,
{
    let mut best_rate = root;
    let mut best_abs_bits = f(root)?.abs().to_bits();

    let mut lower_probe = root;
    let mut upper_probe = root;
    for _ in 0..IRR_PUBLICATION_ULP_SCAN_RADIUS {
        lower_probe = lower_probe.next_down();
        if lower_probe.is_finite() {
            let abs_bits = f(lower_probe)?.abs().to_bits();
            if abs_bits < best_abs_bits {
                best_abs_bits = abs_bits;
                best_rate = lower_probe;
            }
        }

        upper_probe = upper_probe.next_up();
        if upper_probe.is_finite() {
            let abs_bits = f(upper_probe)?.abs().to_bits();
            if abs_bits < best_abs_bits {
                best_abs_bits = abs_bits;
                best_rate = upper_probe;
            }
        }
    }

    let root = ordered_rate(best_rate);
    let lower = residual_plateau_end(root, best_abs_bits, true, &mut f)?;
    let upper = residual_plateau_end(root, best_abs_bits, false, &mut f)?;
    // Both zero representations are in the same plateau. Publish the exact zero root rather
    // than letting the different spacings on either side choose a subnormal rate.
    if rate_from_ordered(lower) <= 0.0 && rate_from_ordered(upper) >= 0.0 {
        return Ok(0.0);
    }
    Ok(rate_from_ordered(lower + (upper - lower) / 2))
}

fn bounded_rate_solve<F, D>(
    guess: f64,
    mut f: F,
    mut derivative: D,
) -> Result<f64, WorksheetErrorCode>
where
    F: FnMut(f64) -> Result<f64, WorksheetErrorCode>,
    D: FnMut(f64) -> Result<f64, WorksheetErrorCode>,
{
    let mut prev_rate = 0.0;
    let mut prev_value = f(prev_rate)?;
    let mut current_rate = if guess <= MIN_VALID_RATE { 0.1 } else { guess };
    let mut current_value = f(current_rate)?;

    for _ in 0..ROOT_MAX_ITERATIONS {
        if current_value.abs() <= ROOT_TOLERANCE {
            return polish_rate(current_rate, &mut f, &mut derivative);
        }

        let secant_next = if (current_value - prev_value).abs() > ROOT_DERIVATIVE_EPS {
            let next = current_rate
                - current_value * (current_rate - prev_rate) / (current_value - prev_value);
            (next.is_finite() && next > MIN_VALID_RATE).then_some(next)
        } else {
            None
        };

        let newton_next = if let Ok(deriv) = derivative(current_rate) {
            if deriv.abs() > ROOT_DERIVATIVE_EPS {
                let next = current_rate - current_value / deriv;
                (next.is_finite() && next > MIN_VALID_RATE).then_some(next)
            } else {
                None
            }
        } else {
            None
        };

        let next_rate = secant_next.or(newton_next).unwrap_or_else(|| {
            if current_rate > 0.0 {
                current_rate / 2.0
            } else {
                0.1
            }
        });

        prev_rate = current_rate;
        prev_value = current_value;
        current_rate = if next_rate <= MIN_VALID_RATE {
            0.1
        } else {
            next_rate
        };
        current_value = f(current_rate)?;
    }

    if current_value.abs() <= ROOT_TOLERANCE {
        polish_rate(current_rate, &mut f, &mut derivative)
    } else {
        Err(WorksheetErrorCode::Num)
    }
}

/// Once the bracketed solve is within `ROOT_TOLERANCE` of a root, the residual
/// `|NPV|` can still leave the *rate* far (10^4–10^5 ULP) from the true root when
/// the NPV slope is gentle — beyond the `IRR_PUBLICATION_ULP_SCAN_RADIUS` plateau
/// scan that selects Excel's published double. Polish the seed with analytic
/// Newton to a rate fixpoint so the plateau sits adjacent to the |NPV|-minimal
/// double (for a representable exact root this lands on it bit-for-bit).
fn polish_rate<F, D>(seed: f64, f: &mut F, derivative: &mut D) -> Result<f64, WorksheetErrorCode>
where
    F: FnMut(f64) -> Result<f64, WorksheetErrorCode>,
    D: FnMut(f64) -> Result<f64, WorksheetErrorCode>,
{
    let mut rate = seed;
    let mut last_step = f64::INFINITY;
    for _ in 0..ROOT_POLISH_ITERATIONS {
        let value = f(rate)?;
        let deriv = match derivative(rate) {
            // Currency units do not decide whether a nonzero slope can polish a rate.
            Ok(d) if d != 0.0 => d,
            _ => break,
        };
        let next = rate - value / deriv;
        if !next.is_finite() || next <= MIN_VALID_RATE {
            break;
        }
        if next.to_bits() == rate.to_bits() {
            break; // bit-level fixpoint
        }
        let step = (next - rate).abs();
        if step >= last_step {
            // No longer contracting (rounding noise near the root): keep whichever
            // of the adjacent candidates has the smaller residual.
            if f(next)?.abs() <= value.abs() {
                rate = next;
            }
            break;
        }
        last_step = step;
        rate = next;
    }
    Ok(rate)
}

#[inline]
fn irr_daz(value: f64) -> f64 {
    if value.abs() < f64::MIN_POSITIVE {
        0.0_f64.copysign(value)
    } else {
        value
    }
}

#[derive(Clone, Copy)]
enum IrrStep {
    Ok(f64),
    RetryNum,
    HardDiv0,
}

/// IRR: the secant method in `u = rate / (1 + rate) = 1 - 1 / (1 + rate)`.
///
/// The coordinate `u` is an affine shift of the discount factor `v = 1 - u = 1 / (1 + rate)`,
/// preserving polynomial curvature and two-flow linearity while retaining full binary64 precision
/// near `rate = 0` (`u ≈ rate`) and quantizing large rates to `u ∈ [0.5, 1)` (`ulp = 2^-53`,
/// matching live Excel 16.0.20527 on `IRR({-1, M}, M - 1)` for `M = 2..=20` and `g = -15 * 2^-60`).
///
/// NPV is accumulated with `base = 1 / (1 - u)` and sequential stored products `power *= base`
/// (`discounted_cashflows` staging, not binary exponentiation). The search takes `u1 = u0 ± 0.001`
/// against the sign of `y0` with no early zero exit (`IRR({-1, 2^-46}, -1 + 2^-46)` is `#NUM!`
/// because `u0 + 0.001 == u0`). On `y1 == y0`, `u0` steps by `±0.001` in the sign of `u1 > u0`
/// and proceeds in the same iteration when `y1 != y0`. Up to 200 iterations, stopping when
/// `|u2 - u1| < 1e-7` and `|y2| < 1e-7`, with an after-budget `<= 1e-4` check. Intermediate
/// iterates may cross `1 - u < 0`, while publication requires `1 - u > 0` and `1 / (1 - u) - 1 > -1`.
fn irr_u_secant(cashflows: &[f64], u_start: f64) -> IrrStep {
    use crate::ported::excel_numeric::{
        excel_x87_add as add, excel_x87_div as div, excel_x87_mul as mul, excel_x87_sub as sub,
    };
    let tracing = crate::trace::is_active();
    if !u_start.is_finite() {
        return IrrStep::RetryNum;
    }
    let npv = |u: f64| -> Result<f64, IrrStep> {
        let v = sub(1.0, u);
        if v == 0.0 {
            if tracing {
                crate::trace::hit(7);
            }
            return Err(IrrStep::HardDiv0);
        }
        if tracing && v < 0.0 {
            crate::trace::hit(19);
        }
        let base = div(1.0, v);
        if !base.is_finite() || base == 0.0 {
            if tracing {
                crate::trace::hit(7);
            }
            return Err(IrrStep::HardDiv0);
        }
        let mut total = 0.0;
        let mut power = 1.0;
        for (k, &flow) in cashflows.iter().enumerate() {
            if k > 0 {
                power = mul(power, base);
            }
            let divisor = irr_daz(power);
            if !divisor.is_finite() || divisor == 0.0 {
                if tracing {
                    crate::trace::hit(8);
                }
                return Err(IrrStep::HardDiv0);
            }
            let term = irr_daz(div(flow, divisor));
            if !term.is_finite() {
                if tracing {
                    crate::trace::hit(9);
                }
                return Err(IrrStep::RetryNum);
            }
            total = add(total, term);
        }
        if total.is_finite() {
            Ok(total)
        } else {
            Err(IrrStep::RetryNum)
        }
    };
    let publish = |u: f64, min_v: f64| -> IrrStep {
        let v = sub(1.0, u);
        if tracing {
            crate::trace::observe_boundary("irr_publish_v", v, min_v);
        }
        if v < min_v {
            if tracing {
                crate::trace::hit(19);
            }
            return IrrStep::RetryNum;
        }
        let rate = crate::ported::real_policy::excel_underflow_to_zero(sub(div(1.0, v), 1.0));
        let ok = rate.is_finite() && rate > -1.0;
        if tracing && !ok {
            crate::trace::hit(19);
        }
        if ok {
            IrrStep::Ok(if rate == 0.0 { 0.0 } else { rate })
        } else {
            IrrStep::RetryNum
        }
    };
    // Unified 16-bit high-word `rgw[3] == 0` flat check (`0.0 <= change < 2^-1026`),
    // mirroring RATE's negative-side check on `y0 - y1` (dc-j9k.104 / cb25).
    let is_flat = |chg: f64| (0.0..f64::MIN_POSITIVE / 16.0).contains(&chg);

    let mut before = u_start;
    let mut value_before = match npv(before) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let offset = if value_before > 0.0 { -0.001 } else { 0.001 };
    let mut current = add(before, offset);
    let mut value = match npv(current) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut step = f64::INFINITY;
    for iter in 0..200u32 {
        let mut change = sub(value, value_before);
        if is_flat(change) {
            let nudge = if current > before { 0.001 } else { -0.001 };
            if tracing {
                crate::trace::hit(if current > before { 10 } else { 11 });
            }
            before = add(before, nudge);
            value_before = match npv(before) {
                Ok(v) => v,
                Err(e) => return e,
            };
            change = sub(value, value_before);
            if is_flat(change) {
                if tracing {
                    crate::trace::hit(12);
                    crate::trace::record_iterations(iter + 1);
                }
                return IrrStep::RetryNum;
            }
        }
        let divisor = irr_daz(change);
        if divisor == 0.0 {
            if tracing {
                crate::trace::hit(13);
                crate::trace::record_iterations(iter + 1);
            }
            return IrrStep::HardDiv0;
        }
        let numerator = irr_daz(mul(value, sub(current, before)));
        let next = sub(current, div(numerator, divisor));
        if !next.is_finite() {
            if tracing {
                crate::trace::hit(14);
                crate::trace::record_iterations(iter + 1);
            }
            return IrrStep::RetryNum;
        }
        step = sub(next, current).abs();
        (before, value_before) = (current, value);
        current = next;
        value = match npv(current) {
            Ok(v) => v,
            Err(e) => return e,
        };
        if tracing {
            crate::trace::observe_boundary("irr_strict_step", step, 1e-7);
            crate::trace::observe_boundary("irr_strict_value", value.abs(), 1e-7);
        }
        if step < 1e-7 && value.abs() < 1e-7 {
            if tracing {
                let iters = iter + 1;
                crate::trace::record_iterations(iters);
                if iters <= 5 {
                    crate::trace::hit(15);
                } else if iters > 50 {
                    crate::trace::hit(16);
                }
            }
            return publish(current, 1e-7);
        }
    }
    if tracing {
        crate::trace::record_iterations(200);
        crate::trace::observe_boundary("irr_post_step", step, 1e-4);
        crate::trace::observe_boundary("irr_post_value", value.abs(), 1e-4);
    }
    // After the budget, RATE's separate 1e-4 step and balance check (RATE_FINAL_TOLERANCE):
    // cash flows of 7e10 cannot bring the balance under 1e-7, and Excel 16.0.20527 answers
    // the iterate at budget for IRR({-70000,12000,15000,18000,21000,26000}*2^20) (dcztvv_r24
    // D3378); from 2^25 on #NUM!.
    let published = if step <= 1e-4 && value.abs() <= 1e-4 {
        publish(current, 1e-4)
    } else {
        IrrStep::RetryNum
    };
    if tracing {
        crate::trace::hit(if matches!(published, IrrStep::Ok(_)) { 17 } else { 18 });
    }
    published
}

pub(crate) fn irr_kernel(cashflows: &[f64], guess: Option<f64>) -> Result<f64, WorksheetErrorCode> {
    use crate::ported::excel_numeric::{excel_x87_add as add, excel_x87_div as div};
    let tracing = crate::trace::is_active();
    let guess = guess.unwrap_or(0.1);
    if tracing {
        crate::trace::observe_boundary("irr_guess_neg_one", guess, -1.0);
        crate::trace::observe_boundary("irr_guess_2p53", guess.abs(), 9_007_199_254_740_992.0);
    }
    // The initial growth factor must be positive and retain its unit
    // increment. At 2^53 adjacent guesses alternate between VALUE and NUM;
    // a magnitude cutoff would erase that stored-addition boundary.
    if guess.is_finite() && (guess <= -1.0 || add(1.0, guess) == guess) {
        if tracing {
            crate::trace::hit(0);
        }
        return Err(WorksheetErrorCode::Value);
    }
    // No sign-change pre-check in IRR (dc-j9k.104 / cb25): single-sign arrays enter
    // `irr_u_secant` so power overflow/underflow returns `#DIV/0!` and tiny positive
    // arrays with `|npv| < 1e-7` match Excel.
    if !cashflows.iter().all(|v| v.is_finite()) {
        if tracing {
            crate::trace::hit(1);
        }
        return Err(WorksheetErrorCode::Value);
    }
    // Excel strips leading zero cashflows before discounting (factoring out base^-start)
    // while retaining interior and trailing zeros (dc-j9k.102 / cb25).
    let Some(start) = cashflows.iter().position(|&v| v != 0.0) else {
        if tracing {
            crate::trace::hit(1);
        }
        return Err(WorksheetErrorCode::Num);
    };
    let flows = &cashflows[start..];
    // When the search from the given guess fails with a retryable #NUM!, Excel 16.0.20527
    // retries from the default guess 0.1 (`u = 0.1 / 1.1`), then `u = -0.1`, then `u = 2.0 / 3.0`
    // (`guess = 2.0`), before returning `#NUM!`. `#DIV/0!` propagates immediately (dc-j9k.103).
    let u_guess = div(guess, add(1.0, guess));
    let attempts = [
        (Some(u_guess), 2u32),
        ((guess != 0.1).then(|| div(0.1, add(1.0, 0.1))), 3u32),
        (Some(-0.1), 4u32),
        (Some(div(2.0, 3.0)), 5u32),
    ];
    for (u_start, route_bit) in attempts {
        let Some(u_start) = u_start else {
            continue;
        };
        match irr_u_secant(flows, u_start) {
            IrrStep::Ok(r) => {
                if tracing {
                    crate::trace::hit(route_bit);
                }
                return Ok(r);
            }
            IrrStep::HardDiv0 => return Err(WorksheetErrorCode::Div0),
            IrrStep::RetryNum => continue,
        }
    }
    if tracing {
        crate::trace::hit(6);
    }
    Err(WorksheetErrorCode::Num)
}

/// Worksheet `XNPV` — bit-exact to 64-bit Excel on `x86_64` (W109 pilot,
/// identified by calculation-graph search and validated on 1,650 live-Excel
/// witnesses, build 20131, max ULP 0):
///
/// * guard: `rate <= 0` (including `-0.0`) publishes `#NUM!` — live-Excel
///   probed; `+1e-300` evaluates;
/// * `base = RN53(RN64(1 + rate))` — x87 double-rounded add;
/// * `years = (date - anchor) / 365` — strict binary64 division (proven
///   observationally equivalent to the double-rounded form over the entire
///   integer-serial date domain: no delta in `1..2.9M` hits a
///   double-rounding window);
/// * `pow = POWER(base, years)` — the FULL worksheet POWER kernel
///   (`power_kernel`), including the exact-integer binary-exponentiation
///   publication (integer year counts take that path, not `exp(y·ln x)`);
/// * `term = RN53(RN64(value / pow))` — x87 double-rounded divide;
/// * `total = RN53(RN64(total + term))` per step, forward order — the legacy
///   x87 memory-spill accumulation loop.
///
/// The internal solver substrate `xnpv_kernel_raw` (used by XIRR, whose
/// iterates legitimately go negative) is intentionally NOT this path; its
/// staging is a separate identification (catalog G6 solver lane).
pub fn xnpv_kernel(rate: f64, values: &[f64], dates: &[i64]) -> Result<f64, WorksheetErrorCode> {
    use crate::ported::excel_numeric::{excel_x87_add, excel_x87_div};
    use crate::ported::power_fn::power_kernel;

    validate_xnpv_inputs(values, dates)?;
    if !rate.is_finite() || rate <= 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    let base = excel_x87_add(1.0, rate);
    let anchor = dates[0];
    let mut total = 0.0;
    for (value, date) in values.iter().zip(dates.iter()) {
        let years = (*date - anchor) as f64 / 365.0;
        let pow = power_kernel(base, years)?;
        let term = excel_x87_div(*value, pow);
        total = excel_x87_add(total, term);
    }
    if total.is_finite() {
        Ok(total)
    } else {
        Err(WorksheetErrorCode::Num)
    }
}

fn xirr_two_cashflow_root(values: &[f64], dates: &[i64]) -> Result<f64, WorksheetErrorCode> {
    if values.len() != 2 || dates.len() != 2 {
        return Err(WorksheetErrorCode::Num);
    }
    let first = values[0];
    let second = values[1];
    let years = (dates[1] - dates[0]) as f64 / 365.0;
    if years <= 0.0 || first == 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    let ratio = -second / first;
    if !ratio.is_finite() || ratio <= 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    let root = crt_pow(ratio, 1.0 / years) - 1.0;
    if root.is_finite() && root > MIN_VALID_RATE {
        Ok(root)
    } else {
        Err(WorksheetErrorCode::Num)
    }
}

fn xirr_two_cashflow_positive_root_excel_like(
    values: &[f64],
    dates: &[i64],
    guess: f64,
) -> Result<f64, WorksheetErrorCode> {
    if !guess.is_finite() || guess <= 0.0 {
        return Err(WorksheetErrorCode::Num);
    }

    let mut lower = 0.0;
    let mut lower_value = xnpv_kernel_raw(lower, values, dates)?;
    let mut upper = guess;
    let mut upper_value = xnpv_kernel_raw(upper, values, dates)?;

    if lower_value == 0.0 {
        return Ok(lower);
    }
    if upper_value == 0.0 {
        return Ok(upper);
    }

    let mut expansions = 0usize;
    while lower_value.signum() == upper_value.signum() {
        lower = upper;
        lower_value = upper_value;
        upper *= 2.0;
        if !upper.is_finite() || upper <= MIN_VALID_RATE {
            return Err(WorksheetErrorCode::Num);
        }
        upper_value = xnpv_kernel_raw(upper, values, dates)?;
        expansions += 1;
        if expansions > XIRR_TWO_CASHFLOW_MAX_BRACKET_EXPANSIONS {
            return Err(WorksheetErrorCode::Num);
        }
    }

    for _ in 0..ROOT_MAX_ITERATIONS {
        let midpoint = (lower + upper) / 2.0;
        let relative_width = (upper - lower) / midpoint.abs();
        if relative_width <= XIRR_TWO_CASHFLOW_RELATIVE_BRACKET_TOLERANCE {
            return Ok(midpoint);
        }

        let midpoint_value = xnpv_kernel_raw(midpoint, values, dates)?;
        if midpoint_value == 0.0 {
            return Ok(midpoint);
        }
        if lower_value.signum() == midpoint_value.signum() {
            lower = midpoint;
            lower_value = midpoint_value;
        } else {
            upper = midpoint;
        }
    }

    Err(WorksheetErrorCode::Num)
}

fn xirr_general_positive_root_excel_like(
    values: &[f64],
    dates: &[i64],
    guess: f64,
) -> Result<f64, WorksheetErrorCode> {
    if !guess.is_finite() || guess <= 0.0 {
        return Err(WorksheetErrorCode::Num);
    }

    let zero = 0.0;
    let zero_value = xnpv_kernel_raw(zero, values, dates)?;
    if zero_value == 0.0 {
        return Ok(zero);
    }

    let mut lower = guess;
    let mut lower_value = xnpv_kernel_raw(lower, values, dates)?;
    if lower_value == 0.0 {
        return Ok(lower);
    }

    let mut upper;

    if zero_value.signum() != lower_value.signum() {
        lower = zero;
        lower_value = zero_value;
        upper = guess;
        let upper_value = xnpv_kernel_raw(upper, values, dates)?;
        if upper_value == 0.0 {
            return Ok(upper);
        }
    } else {
        upper = guess * 2.0;
        if !upper.is_finite() || upper <= MIN_VALID_RATE {
            return Err(WorksheetErrorCode::Num);
        }
        let mut upper_value = xnpv_kernel_raw(upper, values, dates)?;

        let mut expansions = 0usize;
        while lower_value.signum() == upper_value.signum() {
            lower = upper;
            lower_value = upper_value;
            upper *= 2.0;
            if !upper.is_finite() || upper <= MIN_VALID_RATE {
                return Err(WorksheetErrorCode::Num);
            }
            upper_value = xnpv_kernel_raw(upper, values, dates)?;
            expansions += 1;
            if expansions > XIRR_GENERAL_POSITIVE_ROOT_MAX_BRACKET_EXPANSIONS {
                return Err(WorksheetErrorCode::Num);
            }
        }
    }

    for _ in 0..ROOT_MAX_ITERATIONS {
        let midpoint = (lower + upper) / 2.0;
        let bracket_width = upper - lower;
        let brent_tol = XIRR_GENERAL_POSITIVE_ROOT_BRENT_EPS * (1.0 + 2.0 * midpoint.abs());
        if bracket_width <= brent_tol {
            return Ok(midpoint);
        }

        let midpoint_value = xnpv_kernel_raw(midpoint, values, dates)?;
        if midpoint_value == 0.0 {
            return Ok(midpoint);
        }
        if lower_value.signum() == midpoint_value.signum() {
            lower = midpoint;
            lower_value = midpoint_value;
        } else {
            upper = midpoint;
        }
    }

    Err(WorksheetErrorCode::Num)
}

pub(crate) fn xirr_kernel(
    values: &[f64],
    dates: &[i64],
    guess: Option<f64>,
) -> Result<f64, WorksheetErrorCode> {
    validate_xcashflow_inputs(values, dates)?;
    let guess = guess.unwrap_or(0.1);
    if values.len() == 2 && dates.len() == 2 {
        let root = xirr_two_cashflow_root(values, dates)?;
        if root >= 0.0 && guess < 0.0 {
            return Err(WorksheetErrorCode::Num);
        }
        if root >= 0.0 {
            return xirr_two_cashflow_positive_root_excel_like(values, dates, guess);
        }
        return Ok(root);
    }

    if guess > 0.0 {
        if let Ok(root) = xirr_general_positive_root_excel_like(values, dates, guess) {
            return Ok(root);
        }
    }

    bounded_rate_solve(
        guess,
        |rate| xnpv_kernel_raw(rate, values, dates),
        |rate| xnpv_derivative(rate, values, dates),
    )
}

pub fn map_cashflow_rate_error_to_ws(error: &CashflowRateEvalError) -> WorksheetErrorCode {
    match error {
        CashflowRateEvalError::ArityMismatch { .. } => WorksheetErrorCode::Value,
        CashflowRateEvalError::Domain(code) => *code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::value::{CalcArray, CalcValue, CoreValue, ExcelText};
    use std::collections::BTreeMap;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn irr_values_collection_matches_excel_skip_and_error_rules() {
        use crate::value::ExcelText;
        let col = |cells: Vec<CalcValue>| {
            CalcValue::array(
                CalcArray::from_rows(cells.into_iter().map(|c| vec![c]).collect()).unwrap(),
            )
        };
        // Any error cell -> #VALUE! (specific code not propagated), matching live
        // Excel 16.0 b20026 for both array constants and ranges (BUG-FUNC-028 IRR).
        assert_eq!(
            collect_irr_values_from_eval(&col(vec![
                CalcValue::number(1.0),
                CalcValue::number(-2.0),
                CalcValue::error(WorksheetErrorCode::NA),
            ])),
            Err(CashflowRateEvalError::Domain(WorksheetErrorCode::Value))
        );
        // Text and logical cells are ignored; numbers kept in order.
        assert_eq!(
            collect_irr_values_from_eval(&col(vec![
                CalcValue::number(1.0),
                CalcValue::logical(true),
                CalcValue::number(-2.0),
                CalcValue::text(ExcelText::from_interop_assignment("x")),
            ])),
            Ok(vec![1.0, -2.0])
        );
    }

    struct MockResolver {
        map: BTreeMap<String, CalcValue>,
    }

    fn col(values: &[f64]) -> CalcValue {
        CalcValue::array(
            CalcArray::from_rows(
                values
                    .iter()
                    .copied()
                    .map(|n| vec![CalcValue::number(n)])
                    .collect(),
            )
            .unwrap(),
        )
    }

    fn num(n: f64) -> CalcValue {
        CalcValue::number(n)
    }

    fn assert_close(left: f64, right: f64) {
        assert!((left - right).abs() < 1e-8, "left={left}, right={right}");
    }

    fn assert_bits(actual: f64, expected: f64) {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "{actual} vs {expected}"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn irr_exactness_witness_matches_excel_target() {
        let actual = irr_kernel(&[-10000.0, 3000.0, 4200.0, 6800.0], None).expect("irr witness");
        let prior_local = 0.1634056006889894_f64;
        let excel_target = 0.16340560068898924_f64;

        assert_bits(actual, excel_target);
        assert_ne!(actual.to_bits(), prior_local.to_bits());
    }

    /// Native and browser: flows too large for the 1e-7 balance test end on the after-budget
    /// 1e-4 check. Excel 16.0.20527 through Touchstone: the flows scaled by 2^0, 2^10 and 2^20
    /// answer these bits (2^20 is the corpus's dcztvv_r24 D3378); 2^25 and beyond are #NUM!.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn irr_of_large_flows_ends_on_the_after_budget_check() {
        let flows = [-70000.0, 12000.0, 15000.0, 18000.0, 21000.0, 26000.0];
        let scaled = |k: i32| flows.map(|f| f * 2f64.powi(k));
        for (k, excel) in [(0, 0x3fb62d7220a36a20_u64), (10, 0x3fb62d7220a36cd0), (20, 0x3fb62d7220a36cc0)] {
            let got = irr_kernel(&scaled(k), None).unwrap_or_else(|e| panic!("2^{k}: {e:?}"));
            assert_eq!(got.to_bits(), excel, "2^{k}: {got}");
        }
        for k in [25, 30, 40, 60] {
            assert_eq!(irr_kernel(&scaled(k), None), Err(WorksheetErrorCode::Num), "2^{k}");
        }
    }

    /// Native and browser: a guess whose search fails starts again from 0.1. Excel 16.0.20527
    /// through Touchstone: from guesses 2, 3, 5, 10 and 100 these flows answer the default
    /// guess's bits.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn irr_from_a_failing_guess_is_the_default_guess_s_answer() {
        for (flows, excel) in [
            (&[-1000.0, 0.001, 0.01, 0.1, 1.0, 10.0, 100.0, 1000.0][..], 0x3f8f91595cbb3a00_u64),
            (&[-1000.0, 1.0, 1.0, 1.0, 1.0, 1200.0][..], 0x3fa3670ea5237340),
        ] {
            for guess in [None, Some(2.0), Some(3.0), Some(5.0), Some(10.0), Some(100.0)] {
                let got = irr_kernel(flows, guess).expect("Excel answers");
                assert_eq!(got.to_bits(), excel, "{flows:?} from {guess:?}: {got}");
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn irr_u_coordinate_sequential_discount_and_retry_match_excel() {
        // Live Excel 16.0.20527 through Touchstone:
        // 1) u = r / (1 + r) quantization at large exact roots IRR([-1, M], M - 1):
        for (m, excel_bits) in [
            (3.0, 0x3fff_ffff_ffff_fffe_u64),
            (5.0, 0x4010_0000_0000_0001),
            (7.0, 0x4017_ffff_ffff_fffd),
            (13.0, 0x4028_0000_0000_0005),
            (19.0, 0x4031_ffff_ffff_fffb),
        ] {
            let got = irr_kernel(&[-1.0, m], Some(m - 1.0)).expect("exact-root M");
            assert_eq!(got.to_bits(), excel_bits, "M={m}: {got}");
        }
        // 2) Precision near rate = 0: u0 = g / (1 + g) distinguishes g = -15 * 2^-60 from -14 * 2^-60
        //    and single-ULP guesses around 0.21 that share the same binary64 (1 + g).
        assert_eq!(
            irr_kernel(&[1000.0, -1210.0], Some(-15.0 * 2f64.powi(-60)))
                .unwrap()
                .to_bits(),
            0x3fca_e147_ae14_7ad8
        );
        assert_eq!(
            irr_kernel(&[1000.0, -1210.0], Some(-14.0 * 2f64.powi(-60)))
                .unwrap()
                .to_bits(),
            0x3fca_e147_ae14_7ae0
        );
        assert_eq!(
            irr_kernel(&[-1000.0, 1210.0], Some(f64::from_bits(0x3fca_e147_ae14_7ade)))
                .unwrap()
                .to_bits(),
            0x3fca_e147_ae14_7ae8
        );
        // 3) No early zero exit: at k = 46, y0 == 0 yet u0 + 0.001 == u0 stalls to #NUM!.
        assert_eq!(
            irr_kernel(&[-1.0, 2f64.powi(-45)], Some(-1.0 + 2f64.powi(-45)))
                .unwrap()
                .to_bits(),
            0xbfefffff_ffffff00_u64
        );
        assert_eq!(
            irr_kernel(&[-1.0, 2f64.powi(-46)], Some(-1.0 + 2f64.powi(-46))),
            Err(WorksheetErrorCode::Num)
        );
        // 4) Flat-secant (y1 == y0) nudge in sign(u1 > u0) and retry ladder (u = -0.1, u = 2/3):
        assert_eq!(
            irr_kernel(
                &[-1000.0 * 1_048_576.0, 1210.0 * 1_048_576.0],
                Some(f64::from_bits(0x3fca_e147_ae14_7ade))
            )
            .unwrap()
            .to_bits(),
            0x3fca_e147_ae14_7ae0
        );
        assert_eq!(
            irr_kernel(&[-0.125 * 1e10, 0.75 * 1e10, -1.5 * 1e10, 1e10], Some(0.1))
                .unwrap()
                .to_bits(),
            0x3fef_ffea_e148_f0ba
        );
        assert_eq!(
            irr_kernel(&[0.25 * 1e12, -1e12, 1e12], Some(0.1))
                .unwrap()
                .to_bits(),
            0x3fef_ffff_f381_d24e
        );
        assert_eq!(
            irr_kernel(&[0.25 * 1e14, -1e14, 1e14], Some(0.1))
                .unwrap()
                .to_bits(),
            0x3fef_ffff_fd46_4a10
        );
        assert_eq!(
            irr_kernel(&[0.25 * 3e14, -3e14, 3e14], Some(0.1))
                .unwrap()
                .to_bits(),
            0x3fef_ffff_f64c_a29a
        );
        // 5) Transient 1 - u < 0 allowed, final rate <= -1 rejected (retries default 0.1):
        assert_eq!(
            irr_kernel(&[-85.0, -192.0, 170.0, 128.0], Some(2.0))
                .unwrap()
                .to_bits(),
            0x3fa6_0407_53c3_6000
        );
        assert_eq!(
            irr_kernel(&[-281.0, -185.0, 981.0], Some(10.0))
                .unwrap()
                .to_bits(),
            0x3fe2_2d6a_1a67_87a4
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn irr_matches_simple_two_cashflow_identity() {
        let got = irr_kernel(&[-100.0, 121.0], None).unwrap();
        assert_close(got, 0.21);
    }

    /// Excel 16.0.20625.20108 (sf2/cb25, dc-j9k.102): IRR strips leading zero cashflows
    /// before discounting while retaining interior and trailing zeros.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn irr_leading_zero_cashflows_are_stripped_while_interior_zeros_stay_active() {
        // I1-dc-j9k-102-leading-zero-gross:
        let gross = irr_kernel(
            &[
                0.0,
                f64::from_bits(0xc0a79bc50cf035b4),
                f64::from_bits(0x400b4f5a771262fa),
                0.0,
            ],
            Some(f64::from_bits(0xbfd54b4b6ca9f5bc)),
        )
        .expect("leading-zero gross rep");
        assert_eq!(gross.to_bits(), 0xbfeff6bee6887bc9);

        // I1-dc-j9k-102-leading-zero-num-err:
        let num_err = irr_kernel(
            &[
                0.0,
                f64::from_bits(0xc05e19197dba11b4),
                f64::from_bits(0x4008a772835ab2dc),
                0.0,
            ],
            Some(f64::from_bits(0xbfd02ad0a38fffb1)),
        )
        .expect("leading-zero num-err rep");
        assert_eq!(num_err.to_bits(), 0xbfef2e4dc255d8a0);

        // cb25-IRR-IC-00: leading zero is a no-op; interior zero keeps period spacing.
        assert_eq!(
            irr_kernel(&[0.0, -100.0, 125.0], None).unwrap().to_bits(),
            0x3fd0000000000000
        );
        assert_close(irr_kernel(&[0.0, -100.0, 0.0, 121.0], None).unwrap(), 0.1);
        assert_eq!(irr_kernel(&[0.0, 0.0, 0.0], None), Err(WorksheetErrorCode::Num));
    }

    /// Excel 16.0.20625.20108 (sf2/cb25, dc-j9k.103): `#DIV/0!` inside `irr_u_secant`
    /// propagates immediately instead of falling through the guess-retry ladder.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn irr_div0_propagates_without_retry() {
        // I2-dc-j9k-103-div0-vs-num:
        assert_eq!(
            irr_kernel(
                &[
                    f64::from_bits(0xab2bff2ee48e0530),
                    f64::from_bits(0x4000000000000000),
                ],
                None
            ),
            Err(WorksheetErrorCode::Div0)
        );
        // Trailing zeros still advance power*=base and trigger #DIV/0! on overflow (cb25 IE-05):
        let mut long_flows = vec![22.0; 46];
        long_flows[0] = -100.0;
        assert_eq!(
            irr_kernel(&long_flows, Some(f64::from_bits(0x41a7d78400000000))),
            Err(WorksheetErrorCode::Div0)
        );
    }

    /// Excel 16.0.20625.20108 (sf2/cb25, dc-j9k.104): IRR's symmetric pole check
    /// (`v = 1 - u >= 1e-7` in-loop, `>= 1e-4` post-budget), high-word flat-slope
    /// check (`0 <= change < 2^-1026`) with strict `current > before` nudge and
    /// `RetryNum` on second flat, and no sign-change pre-check.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn irr_pole_flat_slope_and_single_sign_frontiers_match_excel() {
        // I2-dc-j9k-103-div0-vs-finite (needs v >= 1e-7 to avoid huge root before hitting v == 0):
        assert_eq!(
            irr_kernel(
                &[
                    f64::from_bits(0xbff0000000000000),
                    f64::from_bits(0x4340000000000002),
                ],
                None
            ),
            Err(WorksheetErrorCode::Div0)
        );
        // I3-dc-j9k-104-huge-root-vs-num:
        assert_eq!(
            irr_kernel(
                &[
                    f64::from_bits(0xbe7ad7f29abcaf48),
                    f64::from_bits(0x408f400000000000),
                ],
                None
            ),
            Err(WorksheetErrorCode::Num)
        );
        // I3-dc-j9k-104-near-neg1-vs-num:
        assert_eq!(
            irr_kernel(
                &[
                    f64::from_bits(0x41ffadf9355c8248),
                    f64::from_bits(0xbedc89cd5f474471),
                ],
                Some(f64::from_bits(0x433fffffffffffff))
            ),
            Err(WorksheetErrorCode::Num)
        );
        // I3-dc-j9k-104-huge-vs-moderate-root:
        assert_eq!(
            irr_kernel(
                &[
                    f64::from_bits(0xbe810a4cb3aa9e03),
                    f64::from_bits(0x411c2034d4f46c5e),
                    f64::from_bits(0x3ed4c7455ccc98f4),
                    f64::from_bits(0x40985d09dc90d26b),
                    f64::from_bits(0xc102873257e856a4),
                ],
                None
            )
            .unwrap()
            .to_bits(),
            0xbfd3e7d32e6d11fa
        );
        // I3-dc-j9k-104-finite-vs-num (all-positive cashflows reaching |npv| < 1e-7):
        assert_eq!(
            irr_kernel(
                &[
                    f64::from_bits(0x2b2bff2ee48e0530),
                    f64::from_bits(0x3d719799812dea11),
                    f64::from_bits(0x4000000000000000),
                    f64::from_bits(0x2b2bff2ee48e0530),
                ],
                Some(f64::from_bits(0xbfefffffffffffff))
            )
            .unwrap()
            .to_bits(),
            0x4157e9ff1e0aa91b
        );
        // cb25 I-A (+2^-1026 flat-slope boundary: IA-03 #NUM! vs IA-05 #DIV/0!):
        assert_eq!(
            irr_kernel(
                &[
                    f64::from_bits(0x80123456789abcde),
                    f64::from_bits(0x006f3ffffffffa62),
                ],
                Some(0.1)
            ),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            irr_kernel(
                &[
                    f64::from_bits(0x80123456789abcde),
                    f64::from_bits(0x006f3ffffffffa64),
                ],
                Some(0.1)
            ),
            Err(WorksheetErrorCode::Div0)
        );
        // cb25 I-B post-budget v >= 1e-4 accept (IB-12) vs reject (IB-15):
        assert_eq!(
            irr_kernel(
                &[
                    f64::from_bits(0x3cd203af9ee75616),
                    f64::from_bits(0xbdd07de9de0c2eba),
                    f64::from_bits(0x3eb929ce84e00090),
                    f64::from_bits(0xbf947a17f6a63e20),
                    f64::from_bits(0x4062bf0a42288f2e),
                    f64::from_bits(0xc1224e5407adfb50),
                    f64::from_bits(0x41cdcb1b22bfae16),
                ],
                Some(f64::from_bits(0x40b3874000000000))
            )
            .unwrap()
            .to_bits(),
            0x40b38747385223d0
        );
        assert_eq!(
            irr_kernel(
                &[
                    f64::from_bits(0x41e78f90e385635d),
                    f64::from_bits(0xc2ea6d2c0fe1a5a2),
                    f64::from_bits(0x43d96801226cb19b),
                    f64::from_bits(0xc4bb23b0163c891e),
                    f64::from_bits(0x459164fb98008491),
                    f64::from_bits(0xc65ac1b8f5ad4bec),
                    f64::from_bits(0x4716dd977fc8483c),
                    f64::from_bits(0xc7c0bfb4fd5e0b57),
                ],
                Some(f64::from_bits(0x40b4820000000000))
            ),
            Err(WorksheetErrorCode::Num)
        );
        // cb25 I-F strict current > before nudge (IF-00):
        assert_eq!(
            irr_kernel(
                &[
                    f64::from_bits(0x41ff7a8c48000000),
                    f64::from_bits(0xbf40060780fdc161),
                ],
                Some(f64::from_bits(0x433fffffffffffff))
            )
            .unwrap()
            .to_bits(),
            0xbfeffffffffffdf7
        );
        // Single-sign array enters solver: power overflow -> #DIV/0!, linear -> #NUM!.
        assert_eq!(
            irr_kernel(&[100.0; 40], Some(1e10)),
            Err(WorksheetErrorCode::Div0)
        );
        assert_eq!(
            irr_kernel(&[100.0, 200.0], None),
            Err(WorksheetErrorCode::Num)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn irr_representable_root_is_bit_exact_after_polish() {
        // G6 solver substrate: IRR({1,-2}) has the exact representable root 1.0.
        // The gentle NPV slope left the pre-polish seed ~114720 ULP short (|NPV|<1e-8
        // satisfied far from the root); the Newton rate-polish now lets the
        // publication plateau land on 1.0 bit-for-bit (Excel returns exactly 1.0,
        // and here OxFunc beats the F# reference, which is 1 ULP off).
        assert_bits(irr_kernel(&[1.0, -2.0], None).unwrap(), 1.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn irr_plateau_search_has_a_finite_probe_budget() {
        let mut probes = 0;
        let result = midpoint_of_local_min_abs_residual_plateau(0.1, |_| {
            probes += 1;
            Ok(1e-9)
        });
        assert_eq!(result, Err(WorksheetErrorCode::Num));
        assert!(probes <= 33 + 2 * 128, "{probes} probes");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn irr_plateau_bisects_a_probe_outside_the_rate_domain() {
        // A pure search invariant, not an Excel reading: the plateau has two finite ends,
        // even though the first exponential probe beyond its lower end crosses the domain.
        let result = midpoint_of_local_min_abs_residual_plateau(0.0, |rate: f64| {
            if rate <= -1.0 {
                Err(WorksheetErrorCode::Num)
            } else {
                Ok(if rate.abs() <= 1e-16 { 0.0 } else { 1.0 })
            }
        });
        assert_eq!(result, Ok(0.0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn irr_plateau_bisects_a_nonfinite_distance_probe() {
        // Doubling overshoots the finite range before reaching these finite plateau ends.
        for (root, lower, end) in [
            (0.1, false, 1e308_f64.next_down()),
            (-0.1, true, (-1e308_f64).next_up()),
        ] {
            let result = residual_plateau_end(
                ordered_rate(root),
                1.0_f64.to_bits(),
                lower,
                &mut |rate: f64| Ok(if rate.abs() < 1e308 { 1.0 } else { 2.0 }),
            );
            assert_eq!(result, Ok(ordered_rate(end)));
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xnpv_matches_one_year_discount_identity() {
        let got = xnpv_kernel(0.1, &[-100.0, 121.0], &[45000, 45365]).unwrap();
        assert_close(got, 10.0);
    }

    /// W109 XNPV identification pins — live Excel 16.0 build 20131 bits.
    /// Each row exercises one identified staging axis; together they pin the
    /// full graph: `RN53(RN64(1+rate))` base, full POWER kernel per term
    /// (integer binexp dispatch included), `RN53(RN64(value/pow))` term, and
    /// the forward per-step-stored x87 accumulation loop. The broader
    /// 1,530-row live sweep is replayed by
    /// `calc_graph_racer verify_xnpv_promotion` over the oracle cache.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xnpv_matches_live_excel_pinned_witnesses() {
        struct Pin {
            rate_bits: u64,
            values_bits: &'static [u64],
            date_serials: &'static [i64],
            excel_bits: u64,
        }
        let pins = [
            // catalog witness (former 16-ULP row G6-11)
            Pin {
                rate_bits: 0x3fa999999999999a,
                values_bits: &[0xc08f400000000000, 0x407f400000000000, 0x4082c00000000000],
                date_serials: &[43831, 44013, 44562],
                excel_bits: 0x404010550d1e8460,
            },
            // integer-year dispatch witness (killed the fractional-only POWER)
            Pin {
                rate_bits: 0x3f3f280c4b1268d3,
                values_bits: &[
                    0xc113a0669520ead7,
                    0xc122a72668355649,
                    0x41259dd158517e83,
                    0x4116a564d581cf23,
                    0x40fd227f6ec7fccc,
                    0xc12709fd89ae57e6,
                ],
                date_serials: &[45964, 46118, 46688, 46875, 47369, 47789],
                excel_bits: 0xc11dcfd2f902283c,
            },
            // 1+rate double-rounding window (killed the strict base add)
            Pin {
                rate_bits: 0x3d7b8c800561cf83,
                values_bits: &[0x0000000000000000, 0x412e848000000000],
                date_serials: &[40000, 40500],
                excel_bits: 0x412e847fffffb807,
            },
            // value/pow double-rounding window (killed the strict term divide)
            Pin {
                rate_bits: 0x3fb1eb851eb851ec,
                values_bits: &[0x0000000000000000, 0x41354601a948c971],
                date_serials: &[40000, 40313],
                excel_bits: 0x41341305e157affe,
            },
            // pure-summation probe, all dates equal (killed the strict sum:
            // the per-step-stored x87 accumulate differs on this window)
            Pin {
                rate_bits: 0x3fa999999999999a,
                values_bits: &[0x413dd9cc1b49078c, 0x3ff68b7b5cb7ff42, 0x3eaa90fad706a60f],
                date_serials: &[40000, 40000, 40000],
                excel_bits: 0x413dd9cd8400caa0,
            },
            // long multi-flow accumulation witness
            Pin {
                rate_bits: 0x3ff6a37ea9daf7d2,
                values_bits: &[
                    0xc0f0842896944e0a,
                    0x412d422be0336426,
                    0xc118bf10872fc5f4,
                    0x412e059bbb0021b8,
                    0xc12a402e6b32bad4,
                    0x40fc28b53eca41ee,
                    0x41178fab6fd23b36,
                    0x412c00559d46d1da,
                ],
                date_serials: &[31100, 31238, 31292, 31940, 32638, 33302, 33458, 34117],
                excel_bits: 0x411d00ee1d249213,
            },
            // tiny positive rate is accepted (guard boundary)
            Pin {
                rate_bits: 0x01a56e1fc2f8f359, // 1e-300
                values_bits: &[0xc08f400000000000, 0x4091300000000000],
                date_serials: &[40000, 40200],
                excel_bits: 0x4059000000000000, // exactly 100.0
            },
        ];
        for (i, p) in pins.iter().enumerate() {
            let values: Vec<f64> = p.values_bits.iter().map(|b| f64::from_bits(*b)).collect();
            let got = xnpv_kernel(f64::from_bits(p.rate_bits), &values, p.date_serials)
                .unwrap_or_else(|e| panic!("pin {i}: unexpected error {e:?}"));
            assert_eq!(
                got.to_bits(),
                p.excel_bits,
                "pin {i}: got 0x{:016x} want 0x{:016x}",
                got.to_bits(),
                p.excel_bits
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xirr_matches_simple_one_year_identity() {
        let got = xirr_kernel(&[-100.0, 121.0], &[45000, 45365], None).unwrap();
        assert_close(got, 0.21);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xirr_simple_positive_root_matches_exact_excel_publication_witnesses() {
        assert_close(
            xirr_kernel(&[-100.0, 121.0], &[45000, 45365], None).unwrap(),
            0.209_999_996_423_721_44,
        );
        assert_close(
            xirr_kernel(&[-100.0, 121.0], &[45000, 45365], Some(0.5)).unwrap(),
            0.209_999_997_168_779,
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xnpv_rejects_dates_before_anchor() {
        assert_eq!(
            xnpv_kernel(0.1, &[-100.0, 121.0], &[45000, 44999]),
            Err(WorksheetErrorCode::Num)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xirr_requires_sign_change() {
        assert_eq!(
            xirr_kernel(&[100.0, 121.0], &[45000, 45365], None),
            Err(WorksheetErrorCode::Num)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xirr_finds_negative_root_when_it_is_the_only_root() {
        let got = xirr_kernel(
            &[206_101_714.849_377, -156_650_972.542_65],
            &[36585, 36616],
            Some(-0.1),
        )
        .unwrap();
        assert_close(got, -0.960_452_189_296_483_9);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xirr_negative_guess_does_not_jump_to_positive_root_only_domain() {
        assert_eq!(
            xirr_kernel(
                &[15_108_163.384_092_3, -75_382_259.662_842_4],
                &[36585, 36616],
                Some(-0.1),
            ),
            Err(WorksheetErrorCode::Num)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xirr_two_cashflow_positive_root_matches_excel_guess_matrix() {
        let values = [15_108_163.384_092_3, -75_382_259.662_842_4];
        let dates = [36585, 36616];
        let cases = [
            (0.0001, 165_601_347.174_400_03),
            (0.01, 165_601_345.280_000_06),
            (0.1, 165_601_345.600_000_05),
            (1.0, 165_601_347.0),
            (10.0, 165_601_346.25),
            (100.0, 165_601_345.312_5),
            (1000.0, 165_601_346.679_687_5),
        ];

        for (guess, expected) in cases {
            let got = xirr_kernel(&values, &dates, Some(guess)).unwrap();
            assert_close(got, expected);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xirr_general_positive_root_matches_excel_guess_matrix() {
        let values = [-10_000.0, 2_750.0, 4_250.0, 3_250.0, 2_750.0];
        let dates = [44927, 45108, 45292, 45473, 45658];
        let cases = [
            (0.01, 0.244_491_829_872_131),
            (0.1, 0.244_491_833_448_41),
            (0.5, 0.244_491_834_193_468),
            (1.0, 0.244_491_834_193_468),
        ];

        for (guess, expected) in cases {
            assert_close(xirr_kernel(&values, &dates, Some(guess)).unwrap(), expected);
        }
    }

    fn assert_exact(got: f64, expected: f64, label: &str) {
        let diff = (got - expected).abs();
        assert!(
            diff < 1e-14,
            "{label}: got={got:.17e}, expected={expected:.17e}, diff={diff:.3e}"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xirr_general_positive_root_matches_widened_excel_value2_matrix() {
        let cases: &[(&[f64], &[i64], f64, f64, &str)] = &[
            (
                &[-10_000.0, 2_750.0, 4_250.0, 3_250.0, 2_750.0],
                &[44927, 45108, 45292, 45473, 45658],
                0.01,
                0.244_491_829_872_131,
                "w087_seed g=0.01",
            ),
            (
                &[-10_000.0, 2_750.0, 4_250.0, 3_250.0, 2_750.0],
                &[44927, 45108, 45292, 45473, 45658],
                0.1,
                0.244_491_833_448_41,
                "w087_seed g=0.1",
            ),
            (
                &[-10_000.0, 2_750.0, 4_250.0, 3_250.0, 2_750.0],
                &[44927, 45108, 45292, 45473, 45658],
                0.5,
                0.244_491_834_193_468,
                "w087_seed g=0.5",
            ),
            (
                &[-10_000.0, 2_750.0, 4_250.0, 3_250.0, 2_750.0],
                &[44927, 45108, 45292, 45473, 45658],
                1.0,
                0.244_491_834_193_468,
                "w087_seed g=1.0",
            ),
            (
                &[-1_000.0, 300.0, 400.0, 500.0],
                &[45000, 45100, 45200, 45365],
                0.01,
                0.320_753_083_229_065,
                "adjacent_a g=0.01",
            ),
            (
                &[-1_000.0, 300.0, 400.0, 500.0],
                &[45000, 45100, 45200, 45365],
                0.1,
                0.320_753_079_652_786,
                "adjacent_a g=0.1",
            ),
            (
                &[-1_000.0, 300.0, 400.0, 500.0],
                &[45000, 45100, 45200, 45365],
                0.5,
                0.320_753_075_182_438,
                "adjacent_a g=0.5",
            ),
            (
                &[-1_000.0, 300.0, 400.0, 500.0],
                &[45000, 45100, 45200, 45365],
                1.0,
                0.320_753_075_182_438,
                "adjacent_a g=1.0",
            ),
            (
                &[-5_000.0, 1_500.0, 1_500.0, 1_500.0, 1_500.0],
                &[45000, 45090, 45180, 45270, 45360],
                0.01,
                0.351_695_961_952_21,
                "adjacent_b g=0.01",
            ),
            (
                &[-5_000.0, 1_500.0, 1_500.0, 1_500.0, 1_500.0],
                &[45000, 45090, 45180, 45270, 45360],
                0.1,
                0.351_695_960_760_117,
                "adjacent_b g=0.1",
            ),
            (
                &[-5_000.0, 1_500.0, 1_500.0, 1_500.0, 1_500.0],
                &[45000, 45090, 45180, 45270, 45360],
                0.5,
                0.351_695_962_250_233,
                "adjacent_b g=0.5",
            ),
            (
                &[-5_000.0, 1_500.0, 1_500.0, 1_500.0, 1_500.0],
                &[45000, 45090, 45180, 45270, 45360],
                1.0,
                0.351_695_962_250_233,
                "adjacent_b g=1.0",
            ),
            (
                &[-12_000.0, 1_000.0, 2_000.0, 3_000.0, 7_000.0],
                &[45000, 45150, 45300, 45450, 45600],
                0.01,
                0.062_376_437_187_194_8,
                "adjacent_c g=0.01",
            ),
            (
                &[-12_000.0, 1_000.0, 2_000.0, 3_000.0, 7_000.0],
                &[45000, 45150, 45300, 45450, 45600],
                0.1,
                0.062_376_442_551_612_9,
                "adjacent_c g=0.1",
            ),
            (
                &[-12_000.0, 1_000.0, 2_000.0, 3_000.0, 7_000.0],
                &[45000, 45150, 45300, 45450, 45600],
                0.5,
                0.062_376_443_296_670_9,
                "adjacent_c g=0.5",
            ),
            (
                &[-12_000.0, 1_000.0, 2_000.0, 3_000.0, 7_000.0],
                &[45000, 45150, 45300, 45450, 45600],
                1.0,
                0.062_376_443_296_670_9,
                "adjacent_c g=1.0",
            ),
            (
                &[-2_000.0, 2_500.0, 100.0],
                &[45000, 45180, 45365],
                0.01,
                0.672_046_403_884_887,
                "adjacent_d g=0.01",
            ),
            (
                &[-2_000.0, 2_500.0, 100.0],
                &[45000, 45180, 45365],
                0.1,
                0.672_046_405_076_981,
                "adjacent_d g=0.1",
            ),
            (
                &[-2_000.0, 2_500.0, 100.0],
                &[45000, 45180, 45365],
                0.5,
                0.672_046_400_606_632,
                "adjacent_d g=0.5",
            ),
            (
                &[-2_000.0, 2_500.0, 100.0],
                &[45000, 45180, 45365],
                1.0,
                0.672_046_400_606_632,
                "adjacent_d g=1.0",
            ),
            (
                &[-4_000.0, 500.0, 800.0, 1_200.0, 2_200.0],
                &[45000, 45045, 45120, 45210, 45400],
                0.01,
                0.253_582_987_785_339,
                "adjacent_e g=0.01",
            ),
            (
                &[-4_000.0, 500.0, 800.0, 1_200.0, 2_200.0],
                &[45000, 45045, 45120, 45210, 45400],
                0.1,
                0.253_582_996_129_99,
                "adjacent_e g=0.1",
            ),
            (
                &[-4_000.0, 500.0, 800.0, 1_200.0, 2_200.0],
                &[45000, 45045, 45120, 45210, 45400],
                0.5,
                0.253_582_991_659_641,
                "adjacent_e g=0.5",
            ),
            (
                &[-4_000.0, 500.0, 800.0, 1_200.0, 2_200.0],
                &[45000, 45045, 45120, 45210, 45400],
                1.0,
                0.253_582_991_659_641,
                "adjacent_e g=1.0",
            ),
        ];

        for &(values, dates, guess, expected, label) in cases {
            let got = xirr_kernel(values, dates, Some(guess)).unwrap();
            assert_exact(got, expected, label);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xnpv_rejects_length_mismatch() {
        assert_eq!(
            xnpv_kernel(0.1, &[-100.0, 121.0], &[45000]),
            Err(WorksheetErrorCode::Num)
        );
    }

    // Live Excel 16.0 probe: =XNPV(0.1,{100,200},{45000,45100}) → 294.8451203808644
    // Verifies that XNPV no longer requires a sign change (all-positive series).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xnpv_live_excel_probe_all_positive_two_cashflows() {
        let got = xnpv_kernel(0.1, &[100.0, 200.0], &[45000, 45100]).unwrap();
        assert_bits(got, 294.8451203808644_f64);
    }

    // XNPV of a single cashflow at the anchor date equals that cashflow (rate has no effect).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xnpv_single_element_equals_anchor_cashflow() {
        let got = xnpv_kernel(0.1, &[500.0], &[45000]).unwrap();
        assert_bits(got, 500.0_f64);
    }

    // All-negative series: pure discounting still works (no sign-change requirement).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xnpv_all_negative_series_accepted() {
        // −100 at anchor + −110 one year later at 10% discount: −100 + −110/1.1 = −200
        let got = xnpv_kernel(0.1, &[-100.0, -110.0], &[45000, 45365]).unwrap();
        assert_close(got, -200.0);
    }
}
