// The annuity compositions were tuned and live-Excel-validated (W108 Bead C)
// against the portable correctly-rounded numeric core. They intentionally do
// NOT use the x87 `excel_exp`/`excel_log` worksheet backends yet; migrating the
// financial substrate to the x87 primitives (`fyl2xp1`/`f2xm1`) is tracked as
// W108 Phase C and requires its own live-Excel re-validation.
use crate::ported::excel_numeric::{
    excel_expm1, excel_expm1_internal, excel_log1p, excel_pow_chain, excel_pow_x87_direct,
    excel_x87_add, excel_x87_div, excel_x87_mul, excel_x87_recip, excel_x87_sub, exp_portable,
};
use crate::ported::power_fn::power_kernel;
use crate::value::WorksheetErrorCode;
use crate::value::{CalcValue, CoreValue};
use std::fmt;
use crate::ported::excel_numeric::crt_pow;

const EPSILON: f64 = 1e-12;
const RATE_TOLERANCE: f64 = 1e-7;
const RATE_MAX_ITERATIONS: usize = 20;
const RATE_BRACKET_MAX_ITERATIONS: usize = 64;
const MIN_VALID_RATE: f64 = -0.999_999_999;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentTiming {
    EndOfPeriod,
    BeginningOfPeriod,
}

impl PaymentTiming {
    fn factor(self, rate: f64) -> f64 {
        match self {
            Self::EndOfPeriod => 1.0,
            Self::BeginningOfPeriod => 1.0 + rate,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinancialError {
    Num,
    Div0,
    Value,
    NoConvergence,
}

impl fmt::Display for FinancialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for FinancialError {}

fn validate_finite(values: &[f64]) -> Result<(), FinancialError> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(FinancialError::Value)
    }
}

#[inline]
fn financial_daz(value: f64) -> f64 {
    if value.abs() < f64::MIN_POSITIVE {
        0.0_f64.copysign(value)
    } else {
        value
    }
}

fn growth(periodic_rate: f64, periods: f64) -> Result<f64, FinancialError> {
    validate_finite(&[periodic_rate, periods])?;
    let base = 1.0 + periodic_rate;
    if base <= 0.0 {
        return Err(FinancialError::Num);
    }
    let factor = match power_kernel(base, periods) {
        Ok(factor) => factor,
        Err(WorksheetErrorCode::Div0) => return Err(FinancialError::Div0),
        Err(WorksheetErrorCode::Num) => return Err(FinancialError::Num),
        Err(_) => return Err(FinancialError::Value),
    };
    if factor.is_finite() {
        Ok(factor)
    } else {
        Err(FinancialError::Num)
    }
}

fn annuity_term(
    periodic_rate: f64,
    periods: f64,
    timing: PaymentTiming,
) -> Result<f64, FinancialError> {
    if periodic_rate.abs() < EPSILON {
        return Ok(periods);
    }
    let factor = growth(periodic_rate, periods)?;
    let term = timing.factor(periodic_rate) * (factor - 1.0) / periodic_rate;
    if term.is_finite() {
        Ok(term)
    } else {
        Err(FinancialError::Num)
    }
}

fn growth_allow_nonpositive_base(periodic_rate: f64, periods: f64) -> Result<f64, FinancialError> {
    validate_finite(&[periodic_rate, periods])?;
    let base = 1.0 + periodic_rate;
    let factor = match power_kernel(base, periods) {
        Ok(factor) => factor,
        Err(WorksheetErrorCode::Div0) => return Err(FinancialError::Div0),
        Err(WorksheetErrorCode::Num) => return Err(FinancialError::Num),
        Err(_) => return Err(FinancialError::Value),
    };
    if factor.is_finite() {
        Ok(factor)
    } else {
        Err(FinancialError::Num)
    }
}

fn annuity_term_allow_nonpositive_base(
    periodic_rate: f64,
    periods: f64,
    timing: PaymentTiming,
) -> Result<f64, FinancialError> {
    if periodic_rate.abs() < EPSILON {
        return Ok(periods);
    }
    let factor = growth_allow_nonpositive_base(periodic_rate, periods)?;
    let term = timing.factor(periodic_rate) * (factor - 1.0) / periodic_rate;
    if term.is_finite() {
        Ok(term)
    } else {
        Err(FinancialError::Num)
    }
}

fn balance_equation(
    periodic_rate: f64,
    periods: f64,
    payment_value: f64,
    present_value: f64,
    future_value: f64,
    timing: PaymentTiming,
) -> Result<f64, FinancialError> {
    validate_finite(&[
        periodic_rate,
        periods,
        payment_value,
        present_value,
        future_value,
    ])?;
    if periodic_rate.abs() < EPSILON {
        return Ok(future_value + present_value + payment_value * periods);
    }
    let factor = growth(periodic_rate, periods)?;
    let term = annuity_term(periodic_rate, periods, timing)?;
    let value = future_value + present_value * factor + payment_value * term;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(FinancialError::Num)
    }
}

pub fn pv(
    periodic_rate: f64,
    periods: f64,
    payment_value: f64,
    future_value: f64,
    timing: PaymentTiming,
) -> Result<f64, FinancialError> {
    validate_finite(&[periodic_rate, periods, payment_value, future_value])?;
    if periodic_rate.abs() < EPSILON {
        return Ok(-(future_value + payment_value * periods));
    }
    let factor = growth_allow_nonpositive_base(periodic_rate, periods)?;
    let term = annuity_term_allow_nonpositive_base(periodic_rate, periods, timing)?;
    if factor == 0.0 {
        return Err(FinancialError::Div0);
    }
    let result = -(future_value + payment_value * term) / factor;
    if result.is_finite() {
        Ok(result)
    } else {
        Err(FinancialError::Num)
    }
}

pub fn fv(
    periodic_rate: f64,
    periods: f64,
    payment_value: f64,
    present_value: f64,
    timing: PaymentTiming,
) -> Result<f64, FinancialError> {
    validate_finite(&[periodic_rate, periods, payment_value, present_value])?;
    let factor = growth_allow_nonpositive_base(periodic_rate, periods)?;
    let term = annuity_term_allow_nonpositive_base(periodic_rate, periods, timing)?;
    let result = -(present_value * factor + payment_value * term);
    if result.is_finite() {
        Ok(result)
    } else {
        Err(FinancialError::Num)
    }
}

/// PMT on the correctly-rounded numeric core (W108 Bead C).
///
/// PV-side stable discount form (the live-Excel search winner). Instead of the
/// FV `powi` factor + `(F-1)/r` term (catastrophically wrong on the PMT lane —
/// up to ~5.5e8 ULP), this reproduces Excel's own `exp`/`log1p`/`expm1` chain:
///
/// ```text
/// negL  = -(n * log1p(r))        // -n·ln(1+r)
/// invF  = exp(negL)              // (1+r)^-n   (no cancellation)
/// denom = -expm1(negL)           // 1 - (1+r)^-n  (stable; avoids 1 - tiny)
/// tf    = 1 + r·type
/// pmt   = -(pv + fv·invF) · r / (tf · denom)
/// ```
///
/// The transcendentals are the faithful (~0.5-ULP) deterministic core, matching
/// Excel's bespoke routine on ~99% of inputs. The residual is the tracked W108
/// floor (≤2-3 ULP open discrepancy), NOT an acceptance.
pub fn pmt(
    periodic_rate: f64,
    periods: f64,
    present_value: f64,
    future_value: f64,
    timing: PaymentTiming,
) -> Result<f64, FinancialError> {
    validate_finite(&[periodic_rate, periods, present_value, future_value])?;
    if periods == 0.0 {
        return Err(FinancialError::Num);
    }
    // W108 Bead C: the zero-rate special case is EXACT `r == 0.0`, matching Excel
    // and the live-search model — a merely-tiny rate (e.g. 1e-13) is NOT collapsed
    // to the linear branch (that was a ~3813-ULP miss); it flows through the
    // stable discount form (1-ULP on PMT-0025).
    if periodic_rate == 0.0 {
        return Ok(-(future_value + present_value) / periods);
    }
    // Growth-domain guard: (1+r) <= 0 has no real (1+r)^-n — #NUM!, matching the
    // prior `growth` guard and live Excel.
    if 1.0 + periodic_rate <= 0.0 {
        return Err(FinancialError::Num);
    }
    // W109 G6-01 (2026-07-23, multi-agent workflow): the transcendental substrate is
    // the x87 chain (Excel's PMT `exp` is fFEXP, proven 234/234; `expm1` is the internal
    // x87 double-rounded Kahan), and the combine is the live-Excel-pinned **quotient-first,
    // ·rate LAST** SSE2 op-graph `pmt = RN(RN(RN(num/em)/tf)·r)` — NOT the former
    // reciprocal-multiply product order. `log1p` stays correctly-rounded (proven CR via the
    // confound-free |tau|≥1 oracle: FYL2XP1 == CR bit-for-bit on the PMT domain). These
    // three corrections lift heldout 46%→56%, combsweep 62%→100%, po2 61%→87%, genrate
    // 47%→73%. Residual = the expm1 |tau|<1 non-CR Kahan double-rounding (≤165/234 ceiling,
    // a tracked Excel imprecision — see docs/function-lane/W109_G6...20260720.md).
    let neg_log = -(periods * excel_log1p(periodic_rate));
    let em = excel_expm1_internal(neg_log); // (1+r)^-n − 1  (x87 Kahan; negative for r>0)
    if em == 0.0 {
        return Err(FinancialError::Num);
    }
    let inv_factor = 1.0 + em; // v = (1+r)^-n
    let tf = timing.factor(periodic_rate); // 1 + r·type
    let numerator = present_value + future_value * inv_factor;
    let result = ((numerator / em) / tf) * periodic_rate;
    if result.is_finite() {
        Ok(result)
    } else {
        Err(FinancialError::Num)
    }
}

/// Worksheet `NPER` — bit-exact to 64-bit Excel on `x86_64` (W109 Phase 2,
/// identified by calculation-graph search; 1,729/1,729 live main-path
/// witnesses bit-exact, max ULP 0, build 20131). The body is the legacy x87
/// spill loop — every arithmetic assignment is double-rounded
/// (`RN53(RN64(op))`) and both logarithms are the x87 worksheet `ln`:
///
/// ```text
/// rate == 0 (either sign, EXACT — no epsilon band):
///     nper = RN53(RN64( -(RN53(RN64(fv+pv))) / pmt ))     (#DIV/0! on pmt=0)
/// else:
///     tf    = RN53(RN64(1 + rate·type))
///     tfp   = RN53(RN64(tf·pmt))
///     num   = RN53(RN64(tfp - RN53(RN64(fv·rate))))
///     den   = RN53(RN64(tfp + RN53(RN64(pv·rate))))
///     ratio = RN53(RN64(num/den))                          (ratio<=0 -> #NUM!)
///     nper  = RN53(RN64( ln_x87(ratio) / ln_x87(RN53(RN64(1+rate))) ))
///             (1+rate rounding to 1 -> ln=0 -> #DIV/0!, live-probed;
///              1+rate <= 0 -> #NUM!)
/// ```
///
/// Live-probed lanes: tiny nonzero rates take the MAIN path (degrading
/// numerically exactly like Excel, then #DIV/0! once `1+rate == 1`);
/// `NPER(0, 0, ..)` is `#DIV/0!`; `rate <= -1` is `#NUM!`. This replaces the
/// former portable-CR-log path and its `EPSILON` small-rate band (both ruled
/// out by the oracle bits — see DISCREPANCY_RULED_OUT_LEDGER.csv G6-08).
pub fn nper(
    periodic_rate: f64,
    payment_value: f64,
    present_value: f64,
    future_value: f64,
    timing: PaymentTiming,
) -> Result<f64, FinancialError> {
    use crate::ported::excel_numeric::{
        excel_log, excel_x87_add, excel_x87_div, excel_x87_mul, excel_x87_sub,
    };

    validate_finite(&[periodic_rate, payment_value, present_value, future_value])?;
    if periodic_rate == 0.0 {
        if payment_value == 0.0 {
            return Err(FinancialError::Div0); // live-probed: #DIV/0!
        }
        let sum = excel_x87_add(future_value, present_value);
        let result = excel_x87_div(-sum, payment_value);
        return if result.is_finite() {
            Ok(result)
        } else {
            Err(FinancialError::Num)
        };
    }
    let tf = match timing {
        PaymentTiming::EndOfPeriod => 1.0,
        PaymentTiming::BeginningOfPeriod => excel_x87_add(1.0, periodic_rate),
    };
    let tf_pmt = excel_x87_mul(tf, payment_value);
    let numerator = excel_x87_sub(tf_pmt, excel_x87_mul(future_value, periodic_rate));
    let denominator = excel_x87_add(tf_pmt, excel_x87_mul(present_value, periodic_rate));
    let ratio = excel_x87_div(numerator, denominator);
    if ratio <= 0.0 || !ratio.is_finite() {
        return Err(FinancialError::Num);
    }
    let base = excel_x87_add(1.0, periodic_rate);
    if base <= 0.0 {
        return Err(FinancialError::Num);
    }
    let ln_den = excel_log(base);
    if ln_den == 0.0 {
        return Err(FinancialError::Div0); // 1+rate rounded to 1 (live-probed)
    }
    let result = excel_x87_div(excel_log(ratio), ln_den);
    if result.is_finite() {
        Ok(result)
    } else {
        Err(FinancialError::Num)
    }
}

fn values_bracket_root(lower_value: f64, upper_value: f64) -> bool {
    (lower_value < 0.0 && upper_value > 0.0) || (lower_value > 0.0 && upper_value < 0.0)
}

fn bisect_rate_bracket(
    mut lower: f64,
    mut lower_value: f64,
    mut upper: f64,
    upper_value: f64,
    periods: f64,
    payment_value: f64,
    present_value: f64,
    future_value: f64,
    timing: PaymentTiming,
) -> Result<f64, FinancialError> {
    if lower_value.abs() <= RATE_TOLERANCE {
        return Ok(lower);
    }
    if upper_value.abs() <= RATE_TOLERANCE {
        return Ok(upper);
    }
    if !values_bracket_root(lower_value, upper_value) {
        return Err(FinancialError::NoConvergence);
    }

    for _ in 0..RATE_BRACKET_MAX_ITERATIONS {
        let midpoint = (lower + upper) / 2.0;
        let midpoint_value = balance_equation(
            midpoint,
            periods,
            payment_value,
            present_value,
            future_value,
            timing,
        )?;
        if midpoint_value.abs() <= RATE_TOLERANCE {
            return Ok(midpoint);
        }

        if values_bracket_root(lower_value, midpoint_value) {
            upper = midpoint;
        } else {
            lower = midpoint;
            lower_value = midpoint_value;
        }
    }

    let midpoint = (lower + upper) / 2.0;
    let midpoint_value = balance_equation(
        midpoint,
        periods,
        payment_value,
        present_value,
        future_value,
        timing,
    )?;
    if midpoint_value.abs() <= RATE_TOLERANCE || (upper - lower).abs() <= EPSILON {
        Ok(midpoint)
    } else {
        Err(FinancialError::NoConvergence)
    }
}

fn search_rate_bracket_from_zero(
    zero_value: f64,
    initial_rate: f64,
    periods: f64,
    payment_value: f64,
    present_value: f64,
    future_value: f64,
    timing: PaymentTiming,
) -> Result<Option<f64>, FinancialError> {
    let mut positive_rate = initial_rate.abs().max(0.1);
    for _ in 0..RATE_BRACKET_MAX_ITERATIONS {
        let positive_value = balance_equation(
            positive_rate,
            periods,
            payment_value,
            present_value,
            future_value,
            timing,
        )?;
        if positive_value.abs() <= RATE_TOLERANCE {
            return Ok(Some(positive_rate));
        }
        if values_bracket_root(zero_value, positive_value) {
            return bisect_rate_bracket(
                0.0,
                zero_value,
                positive_rate,
                positive_value,
                periods,
                payment_value,
                present_value,
                future_value,
                timing,
            )
            .map(Some);
        }

        positive_rate *= 2.0;
        if !positive_rate.is_finite() {
            break;
        }
    }

    let mut negative_rate = if initial_rate < 0.0 {
        initial_rate
    } else {
        -0.1
    };
    if negative_rate <= MIN_VALID_RATE {
        negative_rate = -0.1;
    }
    for _ in 0..RATE_BRACKET_MAX_ITERATIONS {
        let negative_value = balance_equation(
            negative_rate,
            periods,
            payment_value,
            present_value,
            future_value,
            timing,
        )?;
        if negative_value.abs() <= RATE_TOLERANCE {
            return Ok(Some(negative_rate));
        }
        if values_bracket_root(negative_value, zero_value) {
            return bisect_rate_bracket(
                negative_rate,
                negative_value,
                0.0,
                zero_value,
                periods,
                payment_value,
                present_value,
                future_value,
                timing,
            )
            .map(Some);
        }

        let margin = 1.0 + negative_rate;
        if margin <= EPSILON {
            break;
        }
        negative_rate = -1.0 + margin / 2.0;
        if negative_rate <= MIN_VALID_RATE {
            negative_rate = MIN_VALID_RATE + EPSILON;
        }
    }

    Ok(None)
}

pub fn rate(
    periods: f64,
    payment_value: f64,
    present_value: f64,
    future_value: f64,
    timing: PaymentTiming,
    guess: Option<f64>,
) -> Result<f64, FinancialError> {
    let root = rate_root(periods, payment_value, present_value, future_value, timing, guess)?;
    Ok(polish_root(root, |r| {
        balance_equation(r, periods, payment_value, present_value, future_value, timing)
    }))
}

/// A root `seed` of `f`, polished by Newton steps (a central-difference slope) to full binary64
/// convergence (dc-ifc): Excel's RATE converges fully where the 1e-7 tolerance of the search
/// stops early (RATE(48,-200,8000): Excel 7e-16 from the exact root, the search's root 2e-13).
/// The steps stop when they no longer contract, keeping whichever adjacent value has the smaller
/// residual, or when `f` fails, keeping the last value.
pub(crate) fn polish_root(seed: f64, f: impl Fn(f64) -> Result<f64, FinancialError>) -> f64 {
    let mut rate = seed;
    let mut last_step = f64::INFINITY;
    for _ in 0..60 {
        let Ok(value) = f(rate) else { break };
        if value == 0.0 {
            break;
        }
        let h = rate.abs().max(1e-4) * 1e-6;
        let (Ok(up), Ok(down)) = (f(rate + h), f(rate - h)) else { break };
        let slope = (up - down) / (2.0 * h);
        if !slope.is_finite() || slope == 0.0 {
            break;
        }
        let next = rate - value / slope;
        if !next.is_finite() || next.to_bits() == rate.to_bits() || next <= MIN_VALID_RATE {
            break;
        }
        let step = (next - rate).abs();
        if step >= last_step {
            if f(next).is_ok_and(|v| v.abs() <= value.abs()) {
                rate = next;
            }
            break;
        }
        last_step = step;
        rate = next;
    }
    rate
}

/// RATE's root search: secant steps from the guess, bisection once a bracket is found, to a
/// residual of at most [`RATE_TOLERANCE`].
fn rate_root(
    periods: f64,
    payment_value: f64,
    present_value: f64,
    future_value: f64,
    timing: PaymentTiming,
    guess: Option<f64>,
) -> Result<f64, FinancialError> {
    validate_finite(&[periods, payment_value, present_value, future_value])?;
    let mut prev_rate = 0.0;
    let mut prev_value = balance_equation(
        prev_rate,
        periods,
        payment_value,
        present_value,
        future_value,
        timing,
    )?;
    let mut current_rate = guess.unwrap_or(0.1);
    if current_rate <= MIN_VALID_RATE {
        current_rate = MIN_VALID_RATE;
    }
    let mut current_value = balance_equation(
        current_rate,
        periods,
        payment_value,
        present_value,
        future_value,
        timing,
    )?;

    if current_value.abs() <= RATE_TOLERANCE {
        return Ok(current_rate);
    }
    if values_bracket_root(prev_value, current_value) {
        return bisect_rate_bracket(
            prev_rate,
            prev_value,
            current_rate,
            current_value,
            periods,
            payment_value,
            present_value,
            future_value,
            timing,
        );
    }

    for _ in 0..RATE_MAX_ITERATIONS {
        if current_value.abs() <= RATE_TOLERANCE {
            return Ok(current_rate);
        }

        let denominator = current_value - prev_value;
        let next_rate = if denominator.abs() > EPSILON {
            current_rate - current_value * (current_rate - prev_rate) / denominator
        } else {
            let h = current_rate.abs().max(1.0) * 1e-6;
            let lower = (current_rate - h).max(-0.999_999_999);
            let upper = current_rate + h;
            let lower_value = balance_equation(
                lower,
                periods,
                payment_value,
                present_value,
                future_value,
                timing,
            )?;
            let upper_value = balance_equation(
                upper,
                periods,
                payment_value,
                present_value,
                future_value,
                timing,
            )?;
            let derivative = (upper_value - lower_value) / (upper - lower);
            if derivative.abs() <= EPSILON {
                return Err(FinancialError::NoConvergence);
            }
            current_rate - current_value / derivative
        };

        if !next_rate.is_finite() || next_rate <= MIN_VALID_RATE {
            break;
        }

        prev_rate = current_rate;
        prev_value = current_value;
        current_rate = next_rate;
        current_value = balance_equation(
            current_rate,
            periods,
            payment_value,
            present_value,
            future_value,
            timing,
        )?;

        if values_bracket_root(prev_value, current_value) {
            return bisect_rate_bracket(
                prev_rate,
                prev_value,
                current_rate,
                current_value,
                periods,
                payment_value,
                present_value,
                future_value,
                timing,
            );
        }
    }

    if current_value.abs() <= RATE_TOLERANCE {
        Ok(current_rate)
    } else if let Some(root) = search_rate_bracket_from_zero(
        balance_equation(
            0.0,
            periods,
            payment_value,
            present_value,
            future_value,
            timing,
        )?,
        guess.unwrap_or(0.1),
        periods,
        payment_value,
        present_value,
        future_value,
        timing,
    )? {
        Ok(root)
    } else {
        Err(FinancialError::NoConvergence)
    }
}

pub fn npv(periodic_rate: f64, cashflows: &[f64]) -> Result<f64, FinancialError> {
    validate_finite(&[periodic_rate])?;
    validate_finite(cashflows)?;
    if cashflows.is_empty() {
        return Err(FinancialError::Value);
    }
    let base = 1.0 + periodic_rate;
    if base == 0.0 {
        return Err(FinancialError::Div0);
    }
    let mut total = 0.0;
    let mut discount = 1.0;
    for cashflow in cashflows {
        discount *= base;
        total += *cashflow / discount;
    }
    if total.is_finite() {
        Ok(total)
    } else {
        Err(FinancialError::Num)
    }
}

/// Remaining balance after `periods` payments, on the exp/log1p/expm1 core (W108
/// Bead C). This is the FV of an annuity expressed with the stable transcendental
/// chain, dedicated to the IPMT reconstruction so it shares PMT's op-order:
///
/// ```text
/// fv_exp(r, m, pmt, pv, tf): if m==0 -> -pv ; if r==0 -> -(pv + pmt·m)
///   L  = m · log1p(r) ; F = exp(L) ; em = expm1(L)
///   -> -(pv·F + pmt·tf·(em/r))
/// ```
///
/// `tf` is `1 + r·type` for the IPMT timing (passed in so beginning/end share one
/// body). NOTE: near-payoff periods still suffer cancellation (~hundreds of ULP);
/// that is better than the prior `powi` FV path's thousands but a fully-stable
/// last-period path is NOT solved — documented W108 residual.
fn fv_exp(rate: f64, m: f64, payment: f64, present_value: f64, tf: f64) -> f64 {
    if m == 0.0 {
        return -present_value;
    }
    if rate == 0.0 {
        return -(present_value + payment * m);
    }
    let log_growth = m * excel_log1p(rate);
    let factor = exp_portable(log_growth);
    let expm1_growth = excel_expm1(log_growth);
    -(present_value * factor + payment * tf * (expm1_growth / rate))
}

/// IPMT on the correctly-rounded numeric core (W108 Bead C). Interest portion of
/// payment `period_index`: the remaining balance entering the period times `r`,
/// reconstructed from the core PMT and the `fv_exp` balance recurrence.
pub fn ipmt(
    periodic_rate: f64,
    period_index: f64,
    periods: f64,
    present_value: f64,
    future_value: f64,
    timing: PaymentTiming,
) -> Result<f64, FinancialError> {
    validate_finite(&[
        periodic_rate,
        period_index,
        periods,
        present_value,
        future_value,
    ])?;
    if period_index < 1.0 || period_index > periods {
        return Err(FinancialError::Num);
    }
    // W109 G6-01 (2026-07-23): EXACT zero-rate special case (mirrors PMT's W108 Bead C),
    // NOT an `abs() < EPSILON` band. Live-Excel: IPMT(0,per,..)=+0.0, but a merely-tiny rate
    // (e.g. ±1e-13) flows through the MAIN path (IPMT(1e-13,1,360,2e5)=−2e-8, IPMT(−1e-13,..)=
    // +2e-8) — the old band wrongly collapsed tiny/negative rates to 0.0 and dropped the sign.
    if periodic_rate == 0.0 {
        return Ok(0.0);
    }
    if 1.0 + periodic_rate <= 0.0 {
        return Err(FinancialError::Num);
    }
    let periodic_payment = pmt(periodic_rate, periods, present_value, future_value, timing)?;
    let tf = timing.factor(periodic_rate); // 1 + r·type
    let type_is_beginning = matches!(timing, PaymentTiming::BeginningOfPeriod);
    // Remaining balance entering `period_index` (the `rip` term); interest = rip·r. Excel
    // computes it backward from the end of the annuity (dc-j9k): the balance B after
    // `period_index - 1` periods is the one that `m = periods - period_index + 1` more periods
    // take to -fv, B = -fv·v^m + pmt·(1 + r·type)·(v^m - 1)/r with v = 1/(1 + r), so late
    // periods of a long, high-rate annuity do not lose the balance to cancellation. Against
    // the catalogue oracle's IPMT and PPMT readings, the forward recurrence missed by up to
    // 0.6 in absolute terms (PPMT(0.1713,191,264,-8172.57) was -0.588 where Excel gives
    // 0.0116); the backward form is within a few ulp of Excel on every reading.
    let rip = if period_index <= 1.0 {
        if type_is_beginning {
            0.0
        } else {
            -present_value
        }
    } else {
        let remaining = periods - period_index + 1.0;
        let log_discount = -(remaining * excel_log1p(periodic_rate));
        let discount = exp_portable(log_discount); // v^m
        let discount_m1 = excel_expm1(log_discount); // v^m - 1
        let balance =
            -future_value * discount + periodic_payment * tf * (discount_m1 / periodic_rate);
        if type_is_beginning {
            -balance / (1.0 + periodic_rate)
        } else {
            -balance
        }
    };
    let result = rip * periodic_rate;
    if result.is_finite() {
        Ok(result)
    } else {
        Err(FinancialError::Num)
    }
}

pub fn ppmt(
    periodic_rate: f64,
    period_index: f64,
    periods: f64,
    present_value: f64,
    future_value: f64,
    timing: PaymentTiming,
) -> Result<f64, FinancialError> {
    let payment = pmt(periodic_rate, periods, present_value, future_value, timing)?;
    // IPMT's checks (period range, rate) and its zero-rate case apply to PPMT too.
    let interest = ipmt(
        periodic_rate,
        period_index,
        periods,
        present_value,
        future_value,
        timing,
    )?;
    if periodic_rate == 0.0 || (period_index <= 1.0 && matches!(timing, PaymentTiming::BeginningOfPeriod)) {
        return Ok(payment - interest);
    }
    // Excel does not subtract the interest from the payment, which cancels when the interest
    // is nearly the whole payment (dc-j9k: PPMT(0.1809,1,375,57755.66,635.76) is -8.79e-24 in
    // Excel, and payment - interest gave noise of order 1e-12). The principal is the payment
    // net of the future value's interest, discounted over the m = periods - period_index + 1
    // periods left: v^m * (pmt - fv*r), and v^m * (pmt - fv*r/(1 + r)) for payments at the
    // start of a period. The catalogue oracle's PPMT readings hold it.
    let remaining = periods - period_index + 1.0;
    let discount = exp_portable(-(remaining * excel_log1p(periodic_rate)));
    let carried = if matches!(timing, PaymentTiming::BeginningOfPeriod) {
        future_value * periodic_rate / (1.0 + periodic_rate)
    } else {
        future_value * periodic_rate
    };
    let result = discount * (payment - carried);
    if result.is_finite() {
        Ok(result)
    } else {
        Err(FinancialError::Num)
    }
}

pub fn ispmt(
    periodic_rate: f64,
    period_index: f64,
    periods: f64,
    present_value: f64,
) -> Result<f64, FinancialError> {
    validate_finite(&[periodic_rate, period_index, periods, present_value])?;
    if periods <= 0.0 {
        return Err(FinancialError::Num);
    }
    let result = present_value * periodic_rate * (period_index / periods - 1.0);
    if result.is_finite() {
        Ok(result)
    } else {
        Err(FinancialError::Num)
    }
}

pub fn mirr(
    cashflows: &[f64],
    finance_rate: f64,
    reinvest_rate: f64,
) -> Result<f64, FinancialError> {
    validate_finite(&[finance_rate, reinvest_rate])?;
    validate_finite(cashflows)?;
    if cashflows.len() < 2 {
        return Err(FinancialError::Div0);
    }
    let has_positive = cashflows.iter().any(|value| *value > 0.0);
    let has_negative = cashflows.iter().any(|value| *value < 0.0);
    if !has_positive || !has_negative {
        return Err(FinancialError::Div0);
    }
    if (1.0 + finance_rate).abs() < EPSILON || (1.0 + reinvest_rate).abs() < EPSILON {
        return Err(FinancialError::Div0);
    }
    let periods = (cashflows.len() - 1) as f64;
    let mut finance_pv = 0.0;
    let mut reinvest_fv = 0.0;
    for (idx, cashflow) in cashflows.iter().enumerate() {
        let idxf = idx as f64;
        if *cashflow < 0.0 {
            finance_pv += *cashflow / crt_pow(1.0 + finance_rate, idxf);
        } else if *cashflow > 0.0 {
            reinvest_fv += *cashflow * crt_pow(1.0 + reinvest_rate, periods - idxf);
        }
    }
    if finance_pv >= 0.0 || reinvest_fv <= 0.0 {
        return Err(FinancialError::Div0);
    }
    let result = crt_pow(-reinvest_fv / finance_pv, 1.0 / periods) - 1.0;
    if result.is_finite() {
        Ok(result)
    } else {
        Err(FinancialError::Num)
    }
}

pub fn fvschedule(principal: f64, schedule: &[f64]) -> Result<f64, FinancialError> {
    validate_finite(&[principal])?;
    validate_finite(schedule)?;
    let mut result = principal;
    for rate in schedule {
        result *= 1.0 + rate;
    }
    if result.is_finite() {
        Ok(result)
    } else {
        Err(FinancialError::Num)
    }
}

pub fn pduration(
    periodic_rate: f64,
    present_value: f64,
    future_value: f64,
) -> Result<f64, FinancialError> {
    validate_finite(&[periodic_rate, present_value, future_value])?;
    if periodic_rate <= 0.0 || present_value <= 0.0 || future_value <= 0.0 {
        return Err(FinancialError::Num);
    }
    // Live Excel 16.0 b20326 Value2: PDURATION is the split worksheet-LN
    // graph (LN(fv)-LN(pv))/LN(1+rate), not LN(fv/pv)/LN(1+rate).
    // 80/80 exact on a fresh mixed grid; the fused-ratio LN form is 20/80.
    use crate::ported::excel_numeric::excel_log;
    let result =
        (excel_log(future_value) - excel_log(present_value)) / excel_log(1.0 + periodic_rate);
    if result.is_finite() {
        Ok(result)
    } else {
        Err(FinancialError::Num)
    }
}

pub fn rri(periods: f64, present_value: f64, future_value: f64) -> Result<f64, FinancialError> {
    validate_finite(&[periods, present_value, future_value])?;
    // W109's live edge-domain batteries pin the guard order as tightly as the
    // positive-domain power graph. Positive subnormal periods are rejected
    // before the equality shortcut, while MIN_POSITIVE is admitted.
    if periods < f64::MIN_POSITIVE {
        return Err(FinancialError::Num);
    }

    // This legacy financial body observes DAZ-normalized value inputs.
    // Equality precedes the sign guards, so equal zeros/subnormals (and even
    // equal negative normal values) publish +0.
    let present_value = financial_daz(present_value);
    let future_value = financial_daz(future_value);
    if present_value == future_value {
        return Ok(0.0);
    }
    // A present value of 0 and values of opposite signs are #NUM!; two negative values have a
    // positive ratio and are computed like positive ones (dc-ifc, Excel 16.0.20430 on 2026-10-27:
    // RRI(1,-1,-3) is 2, RRI(2,-4,-9) 0.5, RRI(2,-5,0) and RRI(2,5,0) -1, RRI(2,0,5),
    // RRI(2,-5,5) and RRI(2,5,-5) #NUM!).
    let opposite = (present_value < 0.0) != (future_value < 0.0) && future_value != 0.0;
    if present_value == 0.0 || opposite {
        return Err(FinancialError::Num);
    }

    // W109 G6-13 / BUG-FUNC-044: RRI is a legacy x87 spill body. Its wrapper
    // division, reciprocal, and final subtraction are each RN53(RN64(op)); the
    // power itself is the raw x87 chain
    // `exp(RN53(RN64((1/n) * ln(base))))`. Native `powf` misses the near-one
    // cancellation ladder by 2^27 output ULP, and `ln(base)/n` misses 97 of the
    // fresh 4,900 rows. This is NOT the worksheet POWER wrapper: RRI(2,1,2)
    // takes the raw chain and bypasses POWER's exponent-0.5 sqrt shortcut.
    // The published quotient is DAZ-normalized too. Its zero lane is exactly
    // -1 and must bypass the raw LN/EXP helper, whose contract requires a
    // finite positive base.
    let base = financial_daz(excel_x87_div(future_value, present_value));
    if base == 0.0 {
        return Ok(-1.0);
    }
    if !base.is_finite() {
        return Err(FinancialError::Num);
    }
    let reciprocal = excel_x87_recip(periods);
    if !reciprocal.is_finite() {
        return Err(FinancialError::Num);
    }

    // periods==1 is an exact identity route, not merely an exponent that
    // happens to equal one. Adjacent binary64 period values take the raw chain.
    let powered = if periods == 1.0 {
        base
    } else {
        excel_pow_chain(base, reciprocal)
    };
    let result = excel_x87_sub(powered, 1.0);
    if result.is_finite() {
        Ok(result)
    } else {
        Err(FinancialError::Num)
    }
}

pub fn nominal(effect_rate: f64, periods_per_year: f64) -> Result<f64, FinancialError> {
    validate_finite(&[effect_rate, periods_per_year])?;
    let periods = periods_per_year.trunc();
    if effect_rate <= 0.0 || periods < 1.0 {
        return Err(FinancialError::Num);
    }
    // W109 G6-14 / BUG-FUNC-045: NOMINAL has two legacy x87 power routes after
    // truncating npery. For n<=2 (equivalently reciprocal exponent >=0.5), the
    // base and exponent stay register-continuous through FYL2X/F2XM1/FSCALE;
    // for n>=3, Excel uses the stored-ln/product `excel_pow_chain` substrate.
    // Both routes store the completed power to binary64 before subtracting one,
    // then multiply by the truncated period count. The `1+effect` base itself
    // is an x87 double-rounded add (targeted live discriminators 4/4).
    let base = excel_x87_add(1.0, effect_rate);
    let reciprocal = 1.0 / periods;
    let powered = if periods <= 2.0 {
        excel_pow_x87_direct(base, reciprocal)
    } else {
        excel_pow_chain(base, reciprocal)
    };
    let result = periods * (powered - 1.0);
    if result.is_finite() {
        Ok(result)
    } else {
        Err(FinancialError::Num)
    }
}

pub fn effect(nominal_rate: f64, periods_per_year: f64) -> Result<f64, FinancialError> {
    validate_finite(&[nominal_rate, periods_per_year])?;
    let periods = periods_per_year.trunc();
    if nominal_rate <= 0.0 || periods < 1.0 {
        return Err(FinancialError::Num);
    }
    // W109 G6-12 / BUG-FUNC-043: EFFECT's integer power is LSB-first binary
    // exponentiation, but it belongs to the legacy x87 spill-loop body class:
    // EVERY accumulator multiply and squaring is RN53(RN64(a*b)). POWER's
    // ordinary-f64 integer helper is therefore not interchangeable. The base
    // wrapper is also x87-spilled (`1 + nominal/n`; four targeted add-rounding
    // probes distinguish it 4/4). The loop is selected only while the truncated
    // count is below `u32::MAX`; at and above that exact boundary Excel switches
    // to the stored-base raw x87 LN/product/EXP chain. Banked grid 315/315;
    // fresh blind grid 870/870; fresh boundary/extreme-domain battery 160/160.
    let base = excel_x87_add(1.0, excel_x87_div(nominal_rate, periods));
    let compounded = if periods < u32::MAX as f64 {
        let mut exponent = periods as u32;
        let mut accumulator = 1.0;
        let mut factor = base;
        while exponent > 0 {
            if exponent & 1 == 1 {
                accumulator = excel_x87_mul(accumulator, factor);
            }
            exponent >>= 1;
            if exponent > 0 {
                factor = excel_x87_mul(factor, factor);
            }
        }
        accumulator
    } else {
        excel_pow_chain(base, periods)
    };
    let result = excel_x87_sub(compounded, 1.0);
    if result.is_finite() {
        Ok(result)
    } else {
        Err(FinancialError::Num)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FinancialSurfaceEvalError {
    ArityMismatch {
        expected_min: usize,
        expected_max: usize,
        actual: usize,
    },
    Kernel(FinancialError),
}

fn numeric_result(
    value: Result<f64, FinancialError>,
) -> Result<CalcValue, FinancialSurfaceEvalError> {
    value
        .map(CalcValue::number)
        .map_err(FinancialSurfaceEvalError::Kernel)
}

pub fn map_financial_time_value_error_to_ws(
    error: &FinancialSurfaceEvalError,
) -> WorksheetErrorCode {
    match error {
        FinancialSurfaceEvalError::ArityMismatch { .. } => WorksheetErrorCode::Value,
        FinancialSurfaceEvalError::Kernel(FinancialError::Num)
        | FinancialSurfaceEvalError::Kernel(FinancialError::NoConvergence) => {
            WorksheetErrorCode::Num
        }
        FinancialSurfaceEvalError::Kernel(FinancialError::Div0) => WorksheetErrorCode::Div0,
        FinancialSurfaceEvalError::Kernel(FinancialError::Value) => WorksheetErrorCode::Value,
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use crate::value::{CalcArray, CalcValue, CoreValue, ExcelText};
    use super::*;

    fn assert_close(actual: f64, expected: f64, tolerance: f64) {
        let delta = (actual - expected).abs();
        assert!(
            delta <= tolerance,
            "expected {expected}, got {actual}, delta {delta}, tolerance {tolerance}"
        );
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
    fn pmt_and_npv_exactness_witness_rows_are_pinned() {
        // W108 Bead C — CLOSED. On the correctly-rounded numeric core (PV-side
        // stable discount form), PMT(0.05/12, 360, 200000) now matches live Excel
        // bit-for-bit at 0xc090c692af15f63a (= -1073.6432460242781), the
        // w108-b2-financial PMT-0000 target. The prior wrong-kernel local was
        // -1073.6432460242763 (0xc090c692af15f632, ~8 ULP off Excel).
        let pmt_actual = pmt(
            0.05 / 12.0,
            360.0,
            200000.0,
            0.0,
            PaymentTiming::EndOfPeriod,
        )
        .expect("pmt witness");
        assert_bits(pmt_actual, f64::from_bits(0xc090c692af15f63a));
        assert_bits(
            npv(0.1, &[-10000.0, 3000.0, 4200.0, 6800.0]).expect("npv witness"),
            1188.4434123352207,
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn w108_bead_c_bit_exact_witnesses_match_live_excel() {
        // New W108 Bead C bit-exact witnesses drawn from the w108-b2-financial
        // live-Excel corpus (242 cases). These pin OxFunc==Excel bit-for-bit on the
        // CR-core PMT family, guarding the kernel rewrite against regression.
        // PMT-0001 (type=1 annuity-due).
        assert_bits(
            pmt(
                0.05 / 12.0,
                360.0,
                200000.0,
                0.0,
                PaymentTiming::BeginningOfPeriod,
            )
            .expect("PMT-0001"),
            f64::from_bits(0xc090b4c0d059daa6),
        );
        // PMT-0000 (canonical 30y mortgage payment) — Excel-exact.
        assert_bits(
            pmt(
                0.05 / 12.0,
                360.0,
                200000.0,
                0.0,
                PaymentTiming::EndOfPeriod,
            )
            .expect("PMT-0000"),
            f64::from_bits(0xc090c692af15f63a),
        );
        // IPMT-0000 (first-period interest) — Excel-exact.
        assert_bits(
            ipmt(
                0.05 / 12.0,
                1.0,
                360.0,
                200000.0,
                0.0,
                PaymentTiming::EndOfPeriod,
            )
            .expect("IPMT-0000"),
            f64::from_bits(0xc08a0aaaaaaaaaab),
        );
    }

    /// W109 NPER identification pins — live Excel 16.0 build 20131 bits.
    /// One pin per identified staging axis (x87 spill-loop arithmetic, x87
    /// worksheet ln for both logs, double-rounded `1+rate` base, double-rounded
    /// final divide, double-rounded linear zero-rate branch) plus the
    /// live-probed error lanes. The 1,729-row live sweep is replayed by the
    /// racer's oracle-cache verifier.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn nper_matches_live_excel_pinned_witnesses() {
        let rows: &[(u64, u64, u64, u64, PaymentTiming, u64)] = &[
            // NPER-0000 — the former 1-ULP catalog witness, now Excel-exact.
            (
                0x3f4b4e81b4e81b4f, // 0.0008333333333333334
                0xc059000000000000, // -100
                0x40c3880000000000, // 10000
                0x0000000000000000,
                PaymentTiming::EndOfPeriod,
                0x405a1d41fa9d1c4a,
            ),
            // W108 corpus rows (still exact under the identified path).
            (
                0x3f71111111111111,
                0xc097700000000000,
                0x41086a0000000000,
                0x0000000000000000,
                PaymentTiming::EndOfPeriod,
                0x406860e8f2a3c05a,
            ),
            (
                0x3f762fc962fc9630, // NPER-0005, type=1
                0xc092c00000000000,
                0x41024f8000000000,
                0x0000000000000000,
                PaymentTiming::BeginningOfPeriod,
                0x4069e55a897343be,
            ),
            (
                0x3f71111111111111, // NPER-0006, nonzero fv
                0xc097700000000000,
                0x41086a0000000000,
                0x40c3880000000000,
                PaymentTiming::EndOfPeriod,
                0x406933c5abe16989,
            ),
            // 1+rate double-rounding window (kills a strict base add).
            (
                0x3e953366e7dfffae,
                0xc07f400cf09f8f07,
                0x40c3880000000000,
                0x0000000000000000,
                PaymentTiming::EndOfPeriod,
                0x4033fffc10d95175,
            ),
            // final-division double-rounding window.
            (
                0x3fcdf7a38b15bcac,
                0xc10bc9b28757a1a4,
                0x412aa42e8d5d0907,
                0x0000000000000000,
                PaymentTiming::EndOfPeriod,
                0x4025b05836d37ea6,
            ),
            // zero-rate linear branch: fv+pv add window and divide window.
            (
                0x0000000000000000,
                0xc05e000000000000,
                0x41298c2d40ecfef9,
                0x3feb43c1097800ad,
                PaymentTiming::EndOfPeriod,
                0x40bb4032169232bc,
            ),
            (
                0x0000000000000000,
                0xc096864a8c753ce2,
                0x40f05290cc46d61d,
                0x0000000000000000,
                PaymentTiming::EndOfPeriod,
                0x4047305d73617c0a,
            ),
            // tiny nonzero rate stays on the MAIN path (no epsilon branch).
            (
                0x3e112e0be826d695, // 1e-9
                0xc05e000000000000,
                0x40c3880000000000,
                0x0000000000000000,
                PaymentTiming::EndOfPeriod,
                0x4054d55546f1cd61,
            ),
        ];
        for (i, &(rb, pmtb, pvb, fvb, timing, excel)) in rows.iter().enumerate() {
            let got = nper(
                f64::from_bits(rb),
                f64::from_bits(pmtb),
                f64::from_bits(pvb),
                f64::from_bits(fvb),
                timing,
            )
            .unwrap_or_else(|e| panic!("pin {i}: unexpected error {e:?}"));
            assert_eq!(
                got.to_bits(),
                excel,
                "pin {i}: got 0x{:016x} want 0x{excel:016x}",
                got.to_bits()
            );
        }
        // Live-probed error lanes.
        assert_eq!(
            nper(0.0, 0.0, 100.0, 0.0, PaymentTiming::EndOfPeriod),
            Err(FinancialError::Div0),
            "NPER(0,0,..) is #DIV/0!"
        );
        assert_eq!(
            nper(1.0e-18, -120.0, 10000.0, 0.0, PaymentTiming::EndOfPeriod),
            Err(FinancialError::Div0),
            "1+rate == 1 -> #DIV/0!"
        );
        assert_eq!(
            nper(-1.0, -120.0, 10000.0, 0.0, PaymentTiming::EndOfPeriod),
            Err(FinancialError::Num)
        );
        assert_eq!(
            nper(0.05, 0.0, 100.0, 0.0, PaymentTiming::EndOfPeriod),
            Err(FinancialError::Num),
            "pmt=0 on the main path is #NUM!"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rate_exactness_witness_pins_current_local_value_and_excel_gap() {
        let actual = rate(
            360.0,
            -1073.64,
            200000.0,
            0.0,
            PaymentTiming::EndOfPeriod,
            None,
        )
        .expect("rate witness");
        // dc-ifc: polished to full convergence, 55 ulp below Excel (0x3f71110b2048574f); the
        // search's 1e-7 residual stopped 586 ulp above it (0.0041666445363460975).
        let current_local = f64::from_bits(0x3f71110b20485718);
        let excel_target = f64::from_bits(0x3f71110b2048574f);

        assert_bits(actual, current_local);
        assert_ne!(actual.to_bits(), excel_target.to_bits());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn ppmt_exactness_witness_pins_current_local_bits_and_excel_gap() {
        // W108 Bead C — open discrepancy, MAGNITUDE COLLAPSED. On the CR core,
        // PPMT(0.05/12, 1, 360, 200000) = 0xc06e09eace050724; Excel (PPMT-0000) is
        // 0xc06e09eace050723 — a 1-ULP gap (Excel's bespoke transcendental last
        // bit). The prior wrong-kernel local was 0xc06e09eace0506e4 (~63 ULP off
        // Excel). Pin the NEW local bits with the Excel target recorded; the 1-ULP
        // residual is a tracked W108 open bug, not an acceptance.
        // dc-j9k (2026-10-24): PPMT now discounts the principal directly, v^m*(pmt - fv*r),
        // which matches Excel on 58 of 150 catalogue-oracle PPMT readings and brings the rest
        // within 84 ulp (the subtraction pmt - IPMT missed by up to whole units); on this
        // witness it gives 0xc06e09eace050721, 2 ulp below Excel where the subtraction was 1
        // above.
        let ppmt_actual = ppmt(
            0.05 / 12.0,
            1.0,
            360.0,
            200000.0,
            0.0,
            PaymentTiming::EndOfPeriod,
        )
        .expect("ppmt witness");
        let ppmt_current_local = f64::from_bits(0xc06e09eace050721);
        let ppmt_excel_target = f64::from_bits(0xc06e09eace050723);

        assert_bits(ppmt_actual, ppmt_current_local);
        assert_ne!(ppmt_actual.to_bits(), ppmt_excel_target.to_bits());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn effect_exactness_witness_matches_excel_target() {
        let actual = effect(0.05, 12.0).expect("effect witness");
        let prior_local = f64::from_bits(0x3faa31e46c681b80);
        let excel_target = f64::from_bits(0x3faa31e46c681bc0);

        assert_bits(actual, excel_target);
        assert_ne!(actual.to_bits(), prior_local.to_bits());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn effect_widened_publication_family_rows_pin_current_local_bits() {
        for (periods, expected) in [
            (2.0_f64, f64::from_bits(0x3fa9eb851eb851e0)),
            (4.0_f64, f64::from_bits(0x3faa1581d7dbf480)),
            (12.0_f64, f64::from_bits(0x3faa31e46c681bc0)),
            (365.0_f64, f64::from_bits(0x3faa3fbbb959cb00)),
        ] {
            let actual = effect(0.05, periods).expect("effect widened family row");
            assert_bits(actual, expected);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn effect_uses_x87_spill_binexp_on_banked_and_blind_discriminators() {
        // BUG-FUNC-043 / G6-12. The ordinary POWER integer path diverges on the
        // first four rows; the last four independently pin the x87-DR base add.
        // Excel's x87-spill body publishes every pinned value (banked 315/315,
        // fresh blind 870/870, targeted base-add discriminators 4/4).
        for (nominal_bits, periods_bits, excel_bits) in [
            (
                0x3fa0000000000000_u64,
                0x4020000000000000_u64,
                0x3fa038708c703800_u64,
            ),
            (
                0x3fa2000000000000_u64,
                0x4022000000000000_u64,
                0x3fa248a8fcfca840_u64,
            ),
            (
                0x3ef4000000000000_u64,
                0x4014000000000000_u64,
                0x3ef4000a00020000_u64,
            ),
            (
                0x3fe9000000000000_u64,
                0x4069000000000000_u64,
                0x3ff2e4e18daed698_u64,
            ),
            (
                0x3f4b9617a9a5a876_u64,
                0x4085500000000000_u64,
                0x3f4b990fc35d0000_u64,
            ),
            (
                0x3ea73473ef62382d_u64,
                0x4073600000000000_u64,
                0x3ea734745e800000_u64,
            ),
            (
                0x3f642dcd48c15028_u64,
                0x4079000000000000_u64,
                0x3f6434274b6dee00_u64,
            ),
            (
                0x3f307f0b9d772723_u64,
                0x4074f00000000000_u64,
                0x3f307f9348af0000_u64,
            ),
        ] {
            let actual = effect(f64::from_bits(nominal_bits), f64::from_bits(periods_bits))
                .expect("EFFECT exactness discriminator");
            assert_eq!(actual.to_bits(), excel_bits);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn effect_switches_to_raw_pow_chain_at_u32_max_truncated_periods() {
        // BUG-FUNC-043 / G6-12. Live Excel 16.0 build 20228 x64, CV2,
        // NoCache. The dispatch predicate is applied after truncation:
        // 4_294_967_294 stays on the x87-DR LSB loop, while 4_294_967_295
        // and every larger finite count use the stored-base raw power chain.
        for (periods_bits, excel_bits) in [
            (0x41efffffffc00000_u64, 0x3faa403b3dfedfa0_u64),
            (0x41efffffffd80000_u64, 0x3faa403b3dfedfa0_u64),
            (0x41efffffffe00000_u64, 0x3faa403b3ea009a0_u64),
            (0x41effffffff80000_u64, 0x3faa403b3ea009a0_u64),
            (0x41f0000000000000_u64, 0x3faa403b3ebaf340_u64),
        ] {
            let actual = effect(0.05, f64::from_bits(periods_bits))
                .expect("EFFECT u32 dispatch discriminator");
            assert_eq!(actual.to_bits(), excel_bits);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rri_uses_raw_x87_pow_chain_and_x87_spill_wrapper() {
        // BUG-FUNC-044 / G6-13. The first three rows pin the cancellation ladder;
        // the fourth separates reciprocal-multiply from `ln(base)/n`; the fifth
        // proves RRI bypasses POWER's exponent-0.5 sqrt dispatch. The remaining
        // rows independently distinguish x87-DR quotient, reciprocal, and final
        // subtraction sites from ordinary binary64 wrapper operations.
        for (periods_bits, pv_bits, fv_bits, excel_bits) in [
            (
                0x4050000000000000_u64,
                0x3ff0000000000000_u64,
                0x3ff0000100000000_u64,
                0x3e4fffff00000000_u64,
            ),
            (
                0x4030000000000000_u64,
                0x3ff0000000000000_u64,
                0x3ff0000040000000_u64,
                0x3e4fffffc0000000_u64,
            ),
            (
                0x4010000000000000_u64,
                0x3ff0000000000000_u64,
                0x3ff0000010000000_u64,
                0x3e4ffffff0000000_u64,
            ),
            (
                0x4022000000000000_u64,
                0x3ff0000000000000_u64,
                0x4063c8c0e120f369_u64,
                0x3fe82c52bd05167a_u64,
            ),
            (
                0x4000000000000000_u64,
                0x3ff0000000000000_u64,
                0x4000000000000000_u64,
                0x3fda827999fcef30_u64,
            ),
            (
                0x3fe6e006120d042e_u64,
                0x402978cc6ac1e168_u64,
                0x40233c61102a784a_u64,
                0xbfd4ca3920945ee6_u64,
            ),
            (
                0x4005212d6e8139b0_u64,
                0x3fcab99943a5b3ab_u64,
                0x4004e632e21067d4_u64,
                0x3ff9a5b86541f846_u64,
            ),
            (
                0x3fe8e568cfd0b2e3_u64,
                0x3fca61d325e4cc68_u64,
                0x4003de1b6b8eca5b_u64,
                0x40378351f3b47272_u64,
            ),
            (
                0x3fcbfcb653b0b35f_u64,
                0x4008495ceb04db83_u64,
                0x4030d4b519cf7924_u64,
                0x40a3b424c968f2bf_u64,
            ),
            (
                0x3fd139ecfdbe8f22_u64,
                0x4014ddff55ccc4ac_u64,
                0x3fd1fca822b3c00d_u64,
                0xbfefffd76a65a154_u64,
            ),
            (
                0x3fc90ab8bd6cd61e_u64,
                0x402cfffb4faa7327_u64,
                0x3ff148ab7524aae2_u64,
                0xbfeffffc6585a794_u64,
            ),
        ] {
            let actual = rri(
                f64::from_bits(periods_bits),
                f64::from_bits(pv_bits),
                f64::from_bits(fv_bits),
            )
            .expect("RRI exactness discriminator");
            assert_eq!(actual.to_bits(), excel_bits);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rri_matches_excel_daz_guard_order_and_exact_period_identity() {
        // BUG-FUNC-044 / G6-13. Live Excel 16.0 build 20228 x64, CV2,
        // NoCache. These rows represent the 60-row edge-domain, 35-row blind
        // disagreement, and six-row exact-period batteries. They pin the
        // subnormal-period cutoff, input/quotient DAZ, equality-before-sign
        // ordering, the zero-base -1 route, and the narrow periods==1 identity.
        const PZ: u64 = 0x0000_0000_0000_0000;
        const NZ: u64 = 0x8000_0000_0000_0000;
        const MIN_SUB: u64 = 0x0000_0000_0000_0001;
        const MAX_SUB: u64 = 0x000f_ffff_ffff_ffff;
        const MIN_NORMAL: u64 = 0x0010_0000_0000_0000;
        const ONE: u64 = 0x3ff0_0000_0000_0000;
        const TWO: u64 = 0x4000_0000_0000_0000;
        const MAX_FINITE: u64 = 0x7fef_ffff_ffff_ffff;

        for (periods_bits, pv_bits, fv_bits, expected_bits) in [
            // Equality after DAZ, before the value sign guards.
            (ONE, PZ, NZ, PZ),
            (ONE, MIN_SUB, MAX_SUB, PZ),
            (ONE, 0xbff0_0000_0000_0000, 0xbff0_0000_0000_0000, PZ),
            // Zero/subnormal future and quotient lanes.
            (ONE, TWO, PZ, 0xbff0_0000_0000_0000),
            (ONE, TWO, MIN_SUB, 0xbff0_0000_0000_0000),
            (
                MAX_FINITE,
                0x4340_0000_0000_0000,
                MIN_NORMAL,
                0xbff0_0000_0000_0000,
            ),
            (MAX_FINITE, TWO, 0x0020_0000_0000_0000, PZ),
            // periods==1 is a direct identity; adjacent period doubles are not.
            (ONE, ONE, MAX_FINITE, MAX_FINITE),
            (ONE, ONE, 0x7fef_ffff_ffff_fffe, 0x7fef_ffff_ffff_fffe),
            (ONE, ONE, 0x3ff0_0000_0000_0001, 0x3cb0_0000_0000_0000),
            (
                0x3fef_ffff_ffff_ffff,
                ONE,
                MAX_FINITE,
                0x7fef_ffff_ffff_ff2a,
            ),
            (
                0x3ff0_0000_0000_0001,
                ONE,
                MAX_FINITE,
                0x7fef_ffff_ffff_fb2a,
            ),
        ] {
            let actual = rri(
                f64::from_bits(periods_bits),
                f64::from_bits(pv_bits),
                f64::from_bits(fv_bits),
            )
            .expect("RRI edge-domain numeric witness");
            assert_eq!(actual.to_bits(), expected_bits);
        }

        for (periods_bits, pv_bits, fv_bits) in [
            (MAX_SUB, ONE, ONE),
            (MIN_SUB, TWO, PZ),
            (ONE, TWO, 0x8010_0000_0000_0000),
            (ONE, MIN_SUB, MAX_FINITE),
        ] {
            assert_eq!(
                rri(
                    f64::from_bits(periods_bits),
                    f64::from_bits(pv_bits),
                    f64::from_bits(fv_bits),
                ),
                Err(FinancialError::Num)
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn nominal_uses_direct_x87_then_raw_pow_routes_with_stored_power() {
        // BUG-FUNC-045 / G6-14. These pins cover: native-powf cancellation;
        // the same-effect n=2/n=3 route boundary; x87-DR stored base creation;
        // the required store before direct FYL2X; and inner subtraction before
        // the final period multiply. The composite scores 842/842 across the
        // fresh adjacent-family and boundary-follow-up corpora.
        for (effect_bits, periods_bits, excel_bits) in [
            (
                0x3e00000080000000_u64,
                0x4018000000000000_u64,
                0x3e00000200000000_u64,
            ),
            (
                0x400bf137020fc250_u64,
                0x4000000000000000_u64,
                0x4001e9f49f3f60d8_u64,
            ),
            (
                0x400bf137020fc250_u64,
                0x4008000000000000_u64,
                0x3fff342fbb6b38db_u64,
            ),
            (
                0x3eb4b5bf2f080059_u64,
                0x4000000000000000_u64,
                0x3eb4b5bec3c00000_u64,
            ),
            (
                0x3e5bc03275fff5c1_u64,
                0x4000000000000000_u64,
                0x3e5bc03278000000_u64,
            ),
            (
                0x3d8683445358528c_u64,
                0x4000000000000000_u64,
                0x3d86840000000000_u64,
            ),
            (
                0x3f7aa803f3d0ccd0_u64,
                0x4000000000000000_u64,
                0x3f7a9cf2ed8bec00_u64,
            ),
            (
                0x3dc87a14003e7108_u64,
                0x4010000000000000_u64,
                0x3dc87a0000000000_u64,
            ),
            (
                0x3d81af400727aaf4_u64,
                0x402a000000000000_u64,
                0x3d81ac0000000000_u64,
            ),
            (
                0x3eb4b5bf2f080059_u64,
                0x408df00000000000_u64,
                0x3eb4b5be44200000_u64,
            ),
            (
                0x3e2329aa1001d534_u64,
                0x4074700000000000_u64,
                0x3e2329a380000000_u64,
            ),
        ] {
            let actual = nominal(f64::from_bits(effect_bits), f64::from_bits(periods_bits))
                .expect("NOMINAL exactness discriminator");
            assert_eq!(actual.to_bits(), excel_bits);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn nominal_of_effect_stays_just_below_nominal_input_for_widened_rows() {
        for periods in [2.0_f64, 4.0, 12.0, 365.0] {
            let effective = effect(0.05, periods).expect("effect widened row");
            let nominal_roundtrip = nominal(effective, periods).expect("nominal widened row");
            assert!(
                nominal_roundtrip < 0.05,
                "periods={periods} nominal_roundtrip={nominal_roundtrip}"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn core_annuity_functions_match_known_examples() {
        let monthly_rate = 0.08 / 12.0;
        let periods = 10.0;
        let principal = 10_000.0;
        let payment = pmt(
            monthly_rate,
            periods,
            principal,
            0.0,
            PaymentTiming::EndOfPeriod,
        )
        .expect("pmt");
        assert_close(payment, -1037.0320893591607, 1e-9);

        let future = fv(
            monthly_rate,
            periods,
            payment,
            principal,
            PaymentTiming::EndOfPeriod,
        )
        .expect("fv");
        assert_close(future, 0.0, 1e-7);

        let present = pv(
            monthly_rate,
            periods,
            payment,
            0.0,
            PaymentTiming::EndOfPeriod,
        )
        .expect("pv");
        assert_close(present, principal, 1e-7);

        let recovered_nper = nper(
            monthly_rate,
            payment,
            principal,
            0.0,
            PaymentTiming::EndOfPeriod,
        )
        .expect("nper");
        assert_close(recovered_nper, periods, 1e-9);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn core_annuity_functions_match_pinned_excel_publication_rows() {
        // PV/FV are UNCHANGED by W108 Bead C (two-kernel `powi`/`powf` split kept)
        // and still match their pinned publication values bit-for-bit.
        assert_eq!(
            pv(0.05, 10.0, -100.0, 0.0, PaymentTiming::EndOfPeriod),
            Ok(772.1734929184813)
        );
        assert_eq!(
            fv(0.05, 10.0, -100.0, 0.0, PaymentTiming::EndOfPeriod),
            Ok(1257.789253554883)
        );
        // W108 Bead C: PMT moved onto the CR core (exp/log1p/expm1). The new local
        // is 0xc06030257a65e08a (-129.5045749654567), 1 ULP from the prior
        // publication value -129.50457496545667 (0xc06030257a65e089). This row is
        // NOT in the live-Excel corpus (no bit-oracle), so pin the NEW core bits;
        // the ±1-ULP delta from the doc value is a tracked W108 note.
        assert_bits(
            pmt(0.05, 10.0, 1000.0, 0.0, PaymentTiming::EndOfPeriod).expect("pmt publication row"),
            f64::from_bits(0xc06030257a65e08a),
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rate_recovers_known_periodic_rate_from_annuity_identity() {
        let expected_rate = 0.01;
        let payment = pmt(expected_rate, 48.0, 8000.0, 0.0, PaymentTiming::EndOfPeriod)
            .expect("pmt for rate inversion");
        let recovered = rate(
            48.0,
            payment,
            8000.0,
            0.0,
            PaymentTiming::EndOfPeriod,
            Some(0.1),
        )
        .expect("rate");
        assert_close(recovered, expected_rate, 1e-7);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rate_default_guess_converges_on_mortgage_style_lane() {
        let expected_rate = 0.004166644536345589;
        let omitted_guess = rate(
            360.0,
            -1073.64,
            200000.0,
            0.0,
            PaymentTiming::EndOfPeriod,
            None,
        )
        .expect("rate with omitted guess");
        assert_close(omitted_guess, expected_rate, 1e-12);

        let default_guess = rate(
            360.0,
            -1073.64,
            200000.0,
            0.0,
            PaymentTiming::EndOfPeriod,
            Some(0.1),
        )
        .expect("rate with default 0.1 guess");
        assert_close(default_guess, expected_rate, 1e-12);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn npv_matches_microsoft_sample() {
        let got = npv(0.1, &[-10000.0, 3000.0, 4200.0, 6800.0]).expect("npv");
        assert_close(got, 1188.4434123352216, 1e-9);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn ipmt_and_ppmt_partition_payment() {
        let monthly_rate = 0.10 / 12.0;
        let periods = 3.0 * 12.0;
        let present_value = 8000.0;
        let payment = pmt(
            monthly_rate,
            periods,
            present_value,
            0.0,
            PaymentTiming::EndOfPeriod,
        )
        .expect("payment");
        let interest = ipmt(
            monthly_rate,
            1.0,
            periods,
            present_value,
            0.0,
            PaymentTiming::EndOfPeriod,
        )
        .expect("ipmt");
        let principal = ppmt(
            monthly_rate,
            1.0,
            periods,
            present_value,
            0.0,
            PaymentTiming::EndOfPeriod,
        )
        .expect("ppmt");
        assert_close(interest + principal, payment, 1e-9);
        assert_close(interest, -66.66666666666667, 1e-9);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn ispmt_uses_even_principal_schedule() {
        let first = ispmt(0.1, 1.0, 4.0, 1000.0).expect("ispmt first");
        assert_close(first, -75.0, 1e-9);

        let later = ispmt(0.1, 4.0, 4.0, 1000.0).expect("ispmt later");
        assert_close(later, 0.0, 1e-9);

        let pre_schedule = ispmt(0.1, 0.0, 4.0, 1000.0).expect("ispmt zero");
        assert_close(pre_schedule, -100.0, 1e-9);

        let beyond_schedule = ispmt(0.1, 5.0, 4.0, 1000.0).expect("ispmt beyond");
        assert_close(beyond_schedule, 25.0, 1e-9);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn mirr_matches_manual_compounding_identity() {
        let cashflows = [-120000.0, 39000.0, 30000.0, 21000.0, 37000.0, 46000.0];
        let got = mirr(&cashflows, 0.10, 0.12).expect("mirr");
        assert_close(got, 0.1260941303659051, 1e-9);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fvschedule_matches_microsoft_sample() {
        let got = fvschedule(1.0, &[0.09, 0.11, 0.10]).expect("fvschedule");
        assert_close(got, 1.33089, 1e-12);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn pduration_and_rri_match_documented_examples() {
        let pd = pduration(0.025, 2000.0, 2200.0).expect("pduration");
        assert_close(pd, 3.859866162622662, 1e-9);
        assert_eq!(pd.to_bits(), 0x400ee10182bb35e8);
        assert_eq!(
            pduration(0.04, 999.0, 1234.0).unwrap().to_bits(),
            0x40158bc055fd3ec8
        );
        assert_eq!(
            pduration(0.03, 12000.0, 15000.0).unwrap().to_bits(),
            0x401e3251e38ae145
        );

        let rr = rri(96.0, 10000.0, 11000.0).expect("rri");
        assert_close(rr, 0.0009933073762913303, 1e-12);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn nominal_and_effect_are_consistent_with_microsoft_example() {
        let nominal_rate = nominal(0.053543, 4.0).expect("nominal");
        assert_close(nominal_rate, 0.052500319868356016, 1e-12);

        let effect_rate = effect(nominal_rate, 4.0).expect("effect");
        assert_close(effect_rate, 0.053543, 1e-9);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn numeric_domain_errors_are_reported() {
        assert_eq!(pduration(0.0, 100.0, 120.0), Err(FinancialError::Num));
        assert_eq!(nominal(0.05, 0.0), Err(FinancialError::Num));
        assert_eq!(npv(-1.0, &[1.0]), Err(FinancialError::Div0));
        assert_eq!(mirr(&[1.0, 2.0], 0.1, 0.1), Err(FinancialError::Div0));
        assert_eq!(
            ipmt(0.1, 0.0, 10.0, 1000.0, 0.0, PaymentTiming::EndOfPeriod),
            Err(FinancialError::Num)
        );
    }

    // BUG-FUNC-034: annuity-due (type=1) interest for per>=2 must subtract the
    // beginning-of-period payment before multiplying by rate. Closed-form pins
    // below are derived from the corrected formula and are fixed-pending Excel
    // bit-pinning by the probe lane.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn ipmt_ppmt_type_one_witness_candidates_pending_excel_bit_pinning() {
        let rate = 0.1;
        let nper = 3.0;
        let pv = 1000.0;
        let payment =
            pmt(rate, nper, pv, 0.0, PaymentTiming::BeginningOfPeriod).expect("type-one payment");

        let interest: Vec<f64> = (1..=3)
            .map(|per| {
                ipmt(
                    rate,
                    per as f64,
                    nper,
                    pv,
                    0.0,
                    PaymentTiming::BeginningOfPeriod,
                )
                .expect("type-one ipmt")
            })
            .collect();

        assert_close(interest[0], 0.0, 1e-12);
        assert_close(interest[1], -63.44410876132934, 1e-9);
        assert_close(interest[2], -33.232628398791576, 1e-9);

        // Standard annuity-due identity: ipmt_type1(per) == ipmt_type0(per)/(1+rate).
        for per in 2..=3 {
            let due = ipmt(
                rate,
                per as f64,
                nper,
                pv,
                0.0,
                PaymentTiming::BeginningOfPeriod,
            )
            .expect("ipmt due");
            let ordinary = ipmt(rate, per as f64, nper, pv, 0.0, PaymentTiming::EndOfPeriod)
                .expect("ipmt ordinary");
            assert_close(due, ordinary / (1.0 + rate), 1e-9);
        }

        let principal: Vec<f64> = (1..=3)
            .map(|per| {
                ppmt(
                    rate,
                    per as f64,
                    nper,
                    pv,
                    0.0,
                    PaymentTiming::BeginningOfPeriod,
                )
                .expect("type-one ppmt")
            })
            .collect();
        for (idx, principal_value) in principal.iter().enumerate() {
            assert_close(principal_value + interest[idx], payment, 1e-9);
        }
        assert_close(principal[1], -302.1148036253773, 1e-9);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn ipmt_type_one_per_equal_one_is_zero_interest() {
        assert_eq!(
            ipmt(0.1, 1.0, 3.0, 1000.0, 0.0, PaymentTiming::BeginningOfPeriod),
            Ok(0.0)
        );
    }

    // W109 G6-01: the EPSILON-band bug fix. Live-Excel (captured 2026-07-23): a merely-tiny
    // rate flows through the MAIN path (NOT collapsed to 0.0), and only EXACT r==0 -> +0.0.
    // Values match Excel to ≤1 ULP (the tracked PMT/balance residual); the point of the fix is
    // that the band no longer zeroes tiny/negative rates.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn ipmt_tiny_rate_flows_through_main_path() {
        let t = PaymentTiming::EndOfPeriod;
        // Was 0.0 (buggy band); now ~ -pv*r = -2e-8 (Excel -1.999999...e-8), NOT collapsed.
        let a = ipmt(1e-13, 1.0, 360.0, 200_000.0, 0.0, t).unwrap();
        assert!(
            a < 0.0 && (a - (-2e-8)).abs() < 1e-22,
            "tiny +rate flows through: {a:e}"
        );
        let b = ipmt(1e-13, 2.0, 360.0, 200_000.0, 0.0, t).unwrap();
        assert!(
            b < 0.0
                && (b - f64::from_bits(0xbe556a498245c036)).abs() <= 2.0 * f64::EPSILON * b.abs()
        );
        // Negative tiny rate flows through too (old band's abs() wrongly caught it) -> +interest.
        let c = ipmt(-1e-13, 1.0, 360.0, 200_000.0, 0.0, t).unwrap();
        assert!(
            c > 0.0 && (c - 2e-8).abs() < 1e-22,
            "tiny -rate flows through: {c:e}"
        );
        // EXACT zero rate -> +0.0 (sign preserved), bit-exact.
        assert_eq!(
            ipmt(0.0, 1.0, 360.0, 200_000.0, 0.0, t).unwrap().to_bits(),
            0x0000000000000000
        );
    }

    // BUG-FUNC-038: NPV/FV/PV rate<=-1 lanes (live-Excel probe matrix).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn npv_negative_base_lanes_match_excel_probe_matrix() {
        // rate < -1: finite alternating-sign discounting (Excel =NPV(-2,100) = -100).
        assert_eq!(npv(-2.0, &[100.0]), Ok(-100.0));
        assert_eq!(npv(-2.0, &[100.0, 100.0]), Ok(0.0));
        // rate = -1: division by (1+rate) is exactly zero -> #DIV/0!.
        assert_eq!(npv(-1.0, &[100.0]), Err(FinancialError::Div0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fv_negative_base_lanes_match_excel_probe_matrix() {
        // Integer nper at rate <= -1 (including rate = -1) publishes a number.
        assert_eq!(
            fv(-1.0, 3.0, -100.0, 0.0, PaymentTiming::EndOfPeriod),
            Ok(100.0)
        );
        assert_eq!(
            fv(-2.0, 3.0, -100.0, 0.0, PaymentTiming::EndOfPeriod),
            Ok(100.0)
        );
        assert_eq!(
            fv(-3.0, 2.0, -100.0, 0.0, PaymentTiming::EndOfPeriod),
            Ok(-100.0)
        );
        // Fractional nper at rate < -1 -> #NUM! (matches Excel).
        assert_eq!(
            fv(-2.0, 2.5, 0.0, -100.0, PaymentTiming::EndOfPeriod),
            Err(FinancialError::Num)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn pv_negative_base_lanes_match_excel_probe_matrix() {
        // Integer nper at rate < -1 publishes a number.
        assert_eq!(
            pv(-2.0, 3.0, -100.0, 0.0, PaymentTiming::EndOfPeriod),
            Ok(-100.0)
        );
        // rate = -1: division by the zero growth factor -> #DIV/0!.
        assert_eq!(
            pv(-1.0, 3.0, -100.0, 0.0, PaymentTiming::EndOfPeriod),
            Err(FinancialError::Div0)
        );
        // Fractional nper at rate < -1 -> #NUM! (matches Excel).
        assert_eq!(
            pv(-2.0, 2.5, 0.0, -100.0, PaymentTiming::EndOfPeriod),
            Err(FinancialError::Num)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn pmt_ipmt_nper_keep_num_at_rate_at_or_below_minus_one() {
        assert_eq!(
            pmt(-2.0, 3.0, -100.0, 0.0, PaymentTiming::EndOfPeriod),
            Err(FinancialError::Num)
        );
        assert_eq!(
            pmt(-3.0, 2.0, -100.0, 0.0, PaymentTiming::EndOfPeriod),
            Err(FinancialError::Num)
        );
        assert_eq!(
            ipmt(-2.0, 1.0, 3.0, -100.0, 0.0, PaymentTiming::EndOfPeriod),
            Err(FinancialError::Num)
        );
        assert_eq!(
            nper(-2.0, 10.0, -100.0, 0.0, PaymentTiming::EndOfPeriod),
            Err(FinancialError::Num)
        );
    }
}
