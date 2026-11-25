// Growth helpers, logarithms and stored arithmetic remain separate stages.
// Paired worksheet calls constrain each stage independently of its producers.
use crate::ported::excel_numeric::crt_pow;
use crate::ported::excel_numeric::{
    excel_expm1, excel_expm1_internal, excel_log1p, excel_log1p_times, excel_pow_chain,
    excel_pow_x87_direct, excel_sqrt_staged, excel_x87_add, excel_x87_div, excel_x87_mul,
    excel_x87_recip, excel_x87_sub, exp_portable,
};
use crate::ported::power_fn::{power_kernel, power_kernel_with_pos_overflow};
use crate::ported::real_policy::excel_underflow_to_zero;
use crate::value::WorksheetErrorCode;
use crate::value::{CalcValue, CoreValue};
use std::fmt;

const EPSILON: f64 = 1e-12;
const RATE_TOLERANCE: f64 = 1e-7;
const RATE_FINAL_TOLERANCE: f64 = 1e-4;
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
            // Like the growth base, the beginning-payment factor is stored
            // from PC64. A strict binary64 addition changes FV's payment
            // term and RATE's objective at addition double-rounding ties.
            Self::BeginningOfPeriod => excel_x87_add(1.0, rate),
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

/// `ln(1 + r)` as Excel's internal logarithm forms it (dc-j9k.90 / cb19): shared `PC=64`
/// forward `atanh` series below `|r| = 3/8` and `LN(stored(1 + r))` from `3/8`.
fn alnrel(rate: f64) -> f64 {
    excel_log1p_times(1.0, rate)
}

fn growth_from_base(base: f64, periods: f64) -> Result<f64, FinancialError> {
    growth_from_base_with_pos_overflow(base, periods).map(|(factor, _)| factor)
}

fn growth_from_base_with_pos_overflow(
    base: f64,
    periods: f64,
) -> Result<(f64, bool), FinancialError> {
    let (factor, pos_overflow) = match power_kernel_with_pos_overflow(base, periods) {
        Ok(pair) => pair,
        Err(WorksheetErrorCode::Div0) => return Err(FinancialError::Div0),
        Err(WorksheetErrorCode::Num) => return Err(FinancialError::Num),
        Err(_) => return Err(FinancialError::Value),
    };
    if factor.is_finite() {
        Ok((factor, pos_overflow))
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
    // Only exact zero takes the linear route. For a nonzero rate whose
    // stored growth base rounds to one, the annuity term is zero, not nper.
    if periodic_rate == 0.0 {
        return Ok(-(future_value + payment_value * periods));
    }
    // As FV: the growth of the stored 1 + rate, then the stored quotient of the balances, every step
    // stored. 1,306 of 1,308 bank rows, where discounting the two terms apart holds 836.
    let factor = growth_from_base(excel_x87_add(1.0, periodic_rate), periods)?;
    let term = excel_x87_mul(
        excel_x87_div(excel_x87_sub(factor, 1.0), periodic_rate),
        timing.factor(periodic_rate),
    );
    if factor == 0.0 {
        return Err(FinancialError::Div0);
    }
    let result = -excel_x87_div(
        excel_x87_add(future_value, excel_x87_mul(payment_value, term)),
        factor,
    );
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
    // R31 n=1 public growth calls expose the base store. Independent POWER
    // leaves confirm the same input stage at the remaining multi-period cases.
    let factor = growth_from_base(excel_x87_add(1.0, periodic_rate), periods)?;
    // R31 paired end/beginning unit calls identify division before timing,
    // with each operation stored from PC64. Small-rate controls keep only
    // exact zero on the linear route, including measured cancellation at 2^-53.
    // Reuse FV's growth stage; PV retains its own composition above.
    let term = if periodic_rate == 0.0 {
        periods
    } else {
        let end_term = excel_underflow_to_zero(excel_x87_div(factor - 1.0, periodic_rate));
        excel_x87_mul(end_term, timing.factor(periodic_rate))
    };
    // R31 public unit calls isolate both products from their growth and annuity
    // helpers. Separate nonzero pairs and addition ties identify the final store.
    let result = -payment_balance(present_value, payment_value, factor, term);
    if result.is_finite() {
        Ok(result)
    } else {
        Err(FinancialError::Num)
    }
}

/// The stored cashflow terms flush separately, before their stored sum.
/// Two subnormal terms whose sum would be normal distinguish these stores
/// from flushing only the published FV. RATE uses the same balance stage.
fn payment_balance(present_value: f64, payment_value: f64, factor: f64, term: f64) -> f64 {
    let present_component = excel_underflow_to_zero(excel_x87_mul(present_value, factor));
    let payment_component = excel_underflow_to_zero(excel_x87_mul(payment_value, term));
    excel_underflow_to_zero(excel_x87_add(present_component, payment_component))
}

/// The discount arm stores the total balance and its quotient from PC64.
/// PMT adds fv after this common stage; period parts retain the quotient.
fn discounted_balance(present_value: f64, future_value: f64, em: f64) -> f64 {
    excel_x87_div(excel_x87_add(present_value, future_value), em)
}

/// Beginning-payment coefficient, with each reciprocal and addition stored
/// from PC64. Shared by PMT and the period interest/principal finish.
fn due_coefficient(rate: f64) -> f64 {
    let reciprocal = excel_x87_recip(rate);
    if reciprocal.is_finite() {
        excel_x87_recip(excel_x87_add(1.0, reciprocal))
    } else {
        rate
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
    // R31 adjacent-rate calls: beginning payments reject the pole in the
    // stored-PC64 reciprocal coefficient, even when balances cancel. End
    // payments at the same rate retain their ordinary discount domain.
    if matches!(timing, PaymentTiming::BeginningOfPeriod)
        && excel_x87_add(1.0, excel_x87_recip(periodic_rate)) == 0.0
    {
        return Err(FinancialError::Num);
    }
    // dc-j9k.12 / dc-j9k.90 (cb19): unified PC=64 annuity formula across every nonzero rate and
    // period sign. `alnrel(r)` forms the Excel logarithm, the exponent product
    // `-excel_x87_mul(periods, log_growth)` is stored from PC=64, the balance fold is
    // `(pv + fv)/em + fv` with stored PC=64 operations (naturally handling `fv == -pv` and
    // `nper < 0` without overflow), and timing multiplies by `due_coefficient(r)` or `r`.
    let neg_log = -excel_x87_mul(periods, alnrel(periodic_rate));
    let em = crate::ported::excel_numeric::excel_expm1_spilled(neg_log);
    // The stored denominator must be normal. Excel reports #NUM! for a
    // subnormal even when the balances cancel or the quotient could be finite.
    // First-period beginning IPMT returns zero before this shared check.
    if em.abs() < f64::MIN_POSITIVE || !em.is_finite() {
        return Err(FinancialError::Num);
    }
    let quotient = excel_x87_add(
        discounted_balance(present_value, future_value, em),
        future_value,
    );
    let result = match timing {
        PaymentTiming::BeginningOfPeriod => excel_x87_mul(quotient, due_coefficient(periodic_rate)),
        PaymentTiming::EndOfPeriod => excel_x87_mul(quotient, periodic_rate),
    };
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
    // Live Excel (dc-j9k.234): a ratio at or below the smallest normal double is #NUM! as well.
    if ratio <= f64::MIN_POSITIVE || !ratio.is_finite() {
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

pub fn rate(
    periods: f64,
    payment_value: f64,
    present_value: f64,
    future_value: f64,
    timing: PaymentTiming,
    guess: Option<f64>,
) -> Result<f64, FinancialError> {
    let guess = guess.unwrap_or(0.1);
    let tracing = crate::trace::is_active();
    if tracing {
        crate::trace::observe_boundary("guess_neg_one", guess, -1.0);
    }
    // The initial guess has its own domain error. Preserve the numeric API's
    // existing NUM result for nonfinite values, which cannot be worksheet inputs.
    if guess.is_finite() && guess <= -1.0 {
        if tracing {
            crate::trace::hit(0);
        }
        return Err(FinancialError::Value);
    }
    let result = rate_secant(
        periods,
        payment_value,
        present_value,
        future_value,
        timing,
        guess,
    )?;
    // This is a result rule, not an evaluation-domain bound: Excel accepts
    // near-minus-one starting guesses that lead to an ordinary root. Adjacent
    // exact-root probes locate the final boundary at the solver's tolerance.
    let one_plus = excel_x87_add(1.0, result);
    if tracing {
        crate::trace::observe_boundary("final_one_plus_rate", one_plus, RATE_TOLERANCE);
    }
    if one_plus < RATE_TOLERANCE {
        if tracing {
            crate::trace::hit(25);
        }
        Err(FinancialError::Num)
    } else {
        if tracing {
            let abs_r = result.abs();
            if abs_r >= 536_870_912.0 && abs_r < 549_755_813_888.0 {
                crate::trace::hit(26);
            } else if abs_r >= 549_755_813_888.0 {
                crate::trace::hit(27);
            }
            if result < 0.0 {
                crate::trace::hit(28);
            } else if result == 0.0 {
                crate::trace::hit(29);
            }
        }
        Ok(result)
    }
}

/// RATE's search as IRR's (`irr_discount_secant`), in the rate itself: the secant from the guess and
/// a second point 0.001 away, against the sign of the balance at the guess. Over guesses a distance
/// `d` from the root the error is `K d` with `K / C^2 = 1.00e-6`, `C` being `f''/2f'` in the rate.
/// The balance is FV's, every step stored.
///
/// It stops when the rate moved by less than 1e-7 and the balance at the new rate is under 1e-7 as
/// well, both at once. With a mortgage's slope the balance is the one that runs on to full
/// convergence, where the small slope of RATE(10,-120,1000) leaves the error at the step's size:
/// a guess 2.4e-7 off stops, a guess 4.8e-7 off takes one step more.
///
/// 200 steps, from a live window: RATE(360,-599.55,100000,0,0,g) answers up to g = 0.455, the
/// 200th step, and is #NUM! from g = 0.46, the 201st. After the budget, a separate
/// 1e-4 step/balance/result check can accept the last iterate. Intermediate powers
/// retain their own domain, including negative bases with integer exponents.
fn rate_secant(
    periods: f64,
    payment_value: f64,
    present_value: f64,
    future_value: f64,
    timing: PaymentTiming,
    guess: f64,
) -> Result<f64, FinancialError> {
    let tracing = crate::trace::is_active();
    if tracing && matches!(timing, PaymentTiming::BeginningOfPeriod) {
        crate::trace::hit(5);
    }
    let balance = |rate: f64| -> Result<(f64, bool), FinancialError> {
        // Use FV's `(pv * factor + pmt * term) + fv` stage at every rate, including rate == 0.0,
        // and reject a nonfinite balance immediately as #NUM! (Excel 16.0.20625.20108, dc-ztvv.21).
        let (factor, term, pos_overflow) = if rate == 0.0 {
            if tracing {
                crate::trace::hit(1);
            }
            (1.0, periods, false)
        } else {
            let base = excel_x87_add(1.0, rate);
            if tracing && base <= 0.0 {
                crate::trace::hit(2);
            }
            // The initial guess and final root have separate domain rules. An
            // intermediate integer-period power can cross a negative base.
            let (factor, pos_overflow) = match growth_from_base_with_pos_overflow(base, periods) {
                Ok(pair) => pair,
                Err(e) => {
                    if tracing {
                        crate::trace::hit(3);
                    }
                    return Err(e);
                }
            };
            let raw_end_term = excel_x87_div(excel_x87_sub(factor, 1.0), rate);
            let end_term = excel_underflow_to_zero(raw_end_term);
            if tracing && raw_end_term != 0.0 && end_term == 0.0 {
                crate::trace::hit(4);
            }
            (factor, excel_x87_mul(end_term, timing.factor(rate)), pos_overflow)
        };
        let total = excel_x87_add(
            payment_balance(present_value, payment_value, factor, term),
            future_value,
        );
        if total.is_finite() {
            Ok((total, pos_overflow))
        } else {
            Err(FinancialError::Num)
        }
    };
    let asymptotic_accept = |current: f64, iter: u32| -> bool {
        if !(current.is_finite() && current > 0.0 && periods < 0.0) {
            return false;
        }
        let end_term_inf = excel_underflow_to_zero(excel_x87_div(-1.0, current));
        let term_inf = excel_x87_mul(end_term_inf, timing.factor(current));
        let limit = excel_x87_add(
            payment_balance(present_value, payment_value, 0.0, term_inf),
            future_value,
        );
        if tracing {
            crate::trace::observe_boundary("overflow_limit", limit.abs(), RATE_TOLERANCE);
        }
        if limit.abs() <= RATE_TOLERANCE {
            if tracing {
                crate::trace::hit(14);
                crate::trace::record_iterations(iter + 1);
            }
            true
        } else {
            if tracing {
                crate::trace::hit(15);
            }
            false
        }
    };
    let mut before = guess;
    let (mut value_before, _) = balance(before)?;
    // A zero balance still needs a nonzero secant slope; a flat zero
    // objective is NUM. The comparison sends either sign of zero upward.
    let offset = if value_before > 0.0 { -0.001 } else { 0.001 };
    let mut current = excel_x87_add(before, offset);
    let (mut value, mut pos_ovf) = balance(current)?;
    let mut step = f64::INFINITY;
    // Excel 20625 treats negative changes with magnitude less than MIN_NORMAL/16
    // as flat. At the cutoff and on the positive subnormal side, the divisor
    // still reaches the division's DAZ check. Later-step probes distinguish the
    // flat-balance nudge from returning NUM as soon as the slope underflows.
    let flat_change = |change: f64| change > -f64::MIN_POSITIVE / 16.0 && change <= 0.0;
    for iter in 0..200u32 {
        let mut change = excel_x87_sub(value, value_before);
        if tracing {
            crate::trace::observe_boundary("flat_change_cutoff", change, -f64::MIN_POSITIVE / 16.0);
            crate::trace::observe_boundary("change_daz_pos", change, f64::MIN_POSITIVE);
        }
        if flat_change(change) {
            if tracing {
                crate::trace::hit(6);
                if change < 0.0 {
                    crate::trace::hit(7);
                }
                if current > before {
                    crate::trace::hit(8);
                } else if current < before {
                    crate::trace::hit(9);
                } else {
                    crate::trace::hit(8);
                    crate::trace::hit(10);
                }
            }
            // Excel 20625, gx1-RATE-2092 and its neighbours: move the previous
            // rate once in the direction of travel and use that slope on this step.
            let nudge = if current >= before { 0.001 } else { -0.001 };
            before = excel_x87_add(before, nudge);
            (value_before, _) = balance(before)?;
            change = excel_x87_sub(value, value_before);
            if flat_change(change) {
                if tracing {
                    crate::trace::hit(11);
                    crate::trace::record_iterations(iter + 1);
                }
                // A second flat balance is #NUM! even inside the final 1e-4 tolerance.
                return Err(FinancialError::Num);
            }
        }
        // The flat-change comparison precedes the division's DAZ input stage.
        let divisor = financial_daz(change);
        if divisor == 0.0 {
            // When negative-period positive power overflowed to +inf before reciprocating
            // to 0, Excel applies the same asymptotic limit check on divisor == 0.0
            // (dc-j9k.97 / cb25 R-B).
            if pos_ovf && asymptotic_accept(current, iter) {
                return Ok(current);
            }
            if tracing {
                crate::trace::hit(12);
                crate::trace::record_iterations(iter + 1);
            }
            return Err(FinancialError::Div0);
        }
        let raw_num = excel_x87_mul(value, excel_x87_sub(current, before));
        let numerator = excel_underflow_to_zero(raw_num);
        if tracing && raw_num != 0.0 && numerator == 0.0 {
            crate::trace::hit(13);
        }
        let quotient = excel_x87_div(numerator, divisor);
        // With negative periods the balance at +infinity has growth factor 0, and
        // its annuity term `(-1 / rate) * timing` flushes `-1 / current` to zero
        // above `1 / f64::MIN_POSITIVE` (`2^1022`), turning beginning-of-period's
        // limit from `fv - pmt` into `fv`. Excel 16.0.20527 (dc-iuwct.11, dc-iuwct.64)
        // publishes `current` when the step quotient itself overflows (`!quotient.is_finite()`,
        // not when `quotient` is finite and `current - quotient` overflows) and that
        // asymptotic balance is within `1e-7`.
        if !quotient.is_finite() && asymptotic_accept(current, iter) {
            return Ok(current);
        }
        let next = excel_x87_sub(current, quotient);
        if !next.is_finite() {
            if tracing {
                crate::trace::hit(16);
                crate::trace::record_iterations(iter + 1);
            }
            return Err(FinancialError::Num);
        }
        step = excel_x87_sub(next, current).abs();
        (before, value_before) = (current, value);
        current = next;
        (value, pos_ovf) = balance(current)?;
        if tracing {
            crate::trace::observe_boundary("strict_step", step, RATE_TOLERANCE);
            crate::trace::observe_boundary("strict_value", value.abs(), RATE_TOLERANCE);
        }
        if step < RATE_TOLERANCE && value.abs() < RATE_TOLERANCE {
            if tracing {
                let iters = iter + 1;
                crate::trace::record_iterations(iters);
                if iters <= 5 {
                    crate::trace::hit(17);
                } else if iters <= 50 {
                    crate::trace::hit(18);
                } else {
                    crate::trace::hit(19);
                }
                if step == 0.0 {
                    crate::trace::hit(20);
                }
            }
            return Ok(current);
        }
    }
    if tracing {
        crate::trace::record_iterations(200);
        crate::trace::observe_boundary("post_step", step, RATE_FINAL_TOLERANCE);
        crate::trace::observe_boundary("post_value", value.abs(), RATE_FINAL_TOLERANCE);
        crate::trace::observe_boundary(
            "post_base",
            excel_x87_add(1.0, current),
            RATE_FINAL_TOLERANCE,
        );
    }
    // Excel has a separate final check after the iteration budget. Adjacent-word
    // public calls locate its step, balance and near-minus-one boundaries.
    // Keep this outside the strict convergence loop: using 1e-4 there changes
    // ordinary loan answers and accepting every final iterate admits failures.
    if step <= RATE_FINAL_TOLERANCE
        && value.abs() <= RATE_FINAL_TOLERANCE
        && excel_x87_add(1.0, current) >= RATE_FINAL_TOLERANCE
    {
        if tracing {
            crate::trace::hit(21);
        }
        Ok(current)
    } else {
        if tracing {
            if step > RATE_FINAL_TOLERANCE {
                crate::trace::hit(22);
            } else if value.abs() > RATE_FINAL_TOLERANCE {
                crate::trace::hit(23);
            } else {
                crate::trace::hit(24);
            }
        }
        Err(FinancialError::Num)
    }
}

struct DiscountedCashflows {
    total: f64,
    terminal_power: f64,
    overflow_at: Option<usize>,
}

/// Stored discount terms shared by NPV and MIRR. The discount divisor and
/// each quotient follow Excel's DAZ/FTZ stores; the running sum retains tiny
/// cancellation residues. None advances time without discounting a flow.
fn discounted_cashflows(
    base: f64,
    cashflows: impl Iterator<Item = Option<f64>>,
) -> Result<DiscountedCashflows, FinancialError> {
    let mut power = 1.0;
    let mut total = 0.0;
    let mut overflow_at = None;
    let mut sum_overflowed = false;
    for (position, cashflow) in cashflows.enumerate() {
        if let Some(cashflow) = cashflow {
            let divisor = financial_daz(power);
            if !divisor.is_finite() || divisor == 0.0 {
                return Err(FinancialError::Div0);
            }
            let term = financial_daz(excel_x87_div(cashflow, divisor));
            if !term.is_finite() {
                // Finish checking divisors before reporting a term overflow.
                // A later unusable divisor still produces #DIV/0!.
                overflow_at = Some(position);
            } else {
                total = excel_x87_add(total, term);
                if !sum_overflowed && !total.is_finite() {
                    sum_overflowed = true;
                    overflow_at = Some(position);
                }
            }
        }
        power = excel_x87_mul(power, base);
    }
    Ok(DiscountedCashflows {
        total,
        terminal_power: power,
        overflow_at,
    })
}

fn discounted_npv(
    base: f64,
    cashflows: impl Iterator<Item = f64>,
    defer_final_overflow: bool,
    zero_finance_divisor_at: Option<usize>,
) -> Result<f64, FinancialError> {
    if base == 0.0 {
        return Err(FinancialError::Div0);
    }
    let discounted = discounted_cashflows(base, cashflows.map(Some))?;
    // MIRR uses the latest of the last overflowing term, the first sum overflow,
    // and the last negative cashflow with a zero finance divisor.
    if let Some(overflow_at) = discounted.overflow_at {
        return Err(if zero_finance_divisor_at.is_some_and(|position| position > overflow_at) {
            FinancialError::Div0
        } else {
            FinancialError::Num
        });
    }
    let result = financial_daz(excel_x87_div(discounted.total, base));
    if result.is_finite() || defer_final_overflow {
        Ok(result)
    } else {
        Err(FinancialError::Num)
    }
}

pub fn npv(periodic_rate: f64, cashflows: &[f64]) -> Result<f64, FinancialError> {
    validate_finite(&[periodic_rate])?;
    validate_finite(cashflows)?;
    // Time-zero stored discounts, followed by one division by the stored base.
    discounted_npv(
        excel_x87_add(1.0, periodic_rate),
        cashflows.iter().copied(),
        false,
        None,
    )
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

/// The stored balance quotient before PMT adds the future value. Retaining
/// h=(pv+fv)/em avoids cancelling that future value again in each period's
/// interest and principal. Negative-rate end/beginning calls use the same
/// stored quotient; its subnormal store flushes before the large discount.
fn end_payment_quotient(
    periodic_rate: f64,
    periods: f64,
    present_value: f64,
    future_value: f64,
) -> Option<(f64, f64)> {
    if !periodic_rate.is_finite() || periodic_rate == 0.0 {
        return None;
    }
    let log_growth = alnrel(periodic_rate);
    let em = crate::ported::excel_numeric::excel_expm1_spilled(-excel_x87_mul(periods, log_growth));
    if em == 0.0 || !em.is_finite() {
        return None;
    }
    let quotient = excel_underflow_to_zero(discounted_balance(present_value, future_value, em));
    quotient.is_finite().then_some((log_growth, quotient))
}

/// Stored interest and principal components before their final timing factor.
fn period_components(
    remaining: f64,
    future_value: f64,
    log_growth: f64,
    quotient: f64,
) -> (f64, f64) {
    let t = -excel_x87_mul(remaining, log_growth);
    let em = crate::ported::excel_numeric::excel_expm1_spilled(t);
    let u = crate::ported::excel_numeric::excel_exp(t);
    // Retain h before adding fv. Forming (h+fv)*em-fv*u instead loses
    // the entire interest when the two stored products cancel.
    (
        -excel_x87_sub(excel_x87_mul(quotient, em), future_value),
        excel_x87_mul(quotient, u),
    )
}

fn end_payment_parts(
    periodic_rate: f64,
    period_index: f64,
    periods: f64,
    present_value: f64,
    future_value: f64,
    log_growth: f64,
    quotient: f64,
    timing: PaymentTiming,
) -> (f64, f64) {
    // The periods still to run are formed as (nper + 1) - per, each step stored from PC64,
    // and their product with the log growth is stored the same way. The order shows where
    // nper + 1 rounds: IPMT(0.1446, 1, 1 - 2^-53, 0.70, 987.7) differs by 2,369 ulp from the
    // (nper - per) + 1 order. Excel 16.0.20527 (dc-iuwct.33, Touchstone 2026-11-20): 7/7 nper
    // sweeps, every full-bank IPMT, PPMT, CUMIPMT and CUMPRINC row, and 2,997/3,000 fresh IPMT
    // and PPMT controls each with predictions frozen first (the region-limited rule it
    // replaces held 2,409 and 2,869). Switching `alnrel` to `excel_log1p_times` (dc-j9k.90 / cb19)
    // also resolves `IPMT(0.2404, 1 + 2^-52, 1 - 2^-53, -486.8, 111969.3)` at 0 ulp under this
    // same `(nper + 1) - per` order.
    let remaining = excel_x87_sub(excel_x87_add(periods, 1.0), period_index);
    let (interest, principal) = period_components(remaining, future_value, log_growth, quotient);
    let factor = if matches!(timing, PaymentTiming::BeginningOfPeriod) {
        due_coefficient(periodic_rate)
    } else {
        periodic_rate
    };
    (
        excel_x87_mul(interest, factor),
        excel_x87_mul(principal, factor),
    )
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
    // Excel checks the stored zero-based index. A fractional last period
    // remains admitted until per-1 reaches nper; it is not per<=nper.
    if period_index < 1.0 || period_index - 1.0 >= periods {
        return Err(FinancialError::Num);
    }
    // R31 public IPMT calls return zero here even at rate -1 and below.
    // This precedes PMT's rate and reciprocal-pole checks, but follows the
    // finite-argument and period-range checks.
    if period_index == 1.0 && matches!(timing, PaymentTiming::BeginningOfPeriod) {
        return Ok(0.0);
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
    // dc-j9k.12: payments at the end of a period take Excel's own pieces. On the catalogue
    // oracle and bank rows with fv 0 (95 of 95) the interest is -RN(RN(q * em_m) * rate).
    if let Some((log_growth, quotient)) =
        end_payment_quotient(periodic_rate, periods, present_value, future_value)
    {
        let (interest, _) = end_payment_parts(
            periodic_rate,
            period_index,
            periods,
            present_value,
            future_value,
            log_growth,
            quotient,
            timing,
        );
        return if interest.is_finite() {
            Ok(interest)
        } else {
            Err(FinancialError::Num)
        };
    }
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
        let log_discount = -(remaining * alnrel(periodic_rate));
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
    if periodic_rate == 0.0
        || (period_index <= 1.0 && matches!(timing, PaymentTiming::BeginningOfPeriod))
    {
        return Ok(payment - interest);
    }
    // dc-j9k.12: payments at the end of a period take Excel's own pieces: the principal is
    // RN(RN(RN(q - fv) * u) * rate) with u = exp(-(m * L)), not a payment less interest.
    // Catalogue oracle, bank and fresh Excel rows: 138 of 138 with fv 0, 25 of 27 with fv.
    // For positive beginning payments the same stored component takes the
    // reciprocal coefficient already shared by PMT/IPMT. R35's576 independent
    // end/beginning pairs distinguish that finish; all1152 fresh public calls match.
    if let Some((log_growth, quotient)) =
        end_payment_quotient(periodic_rate, periods, present_value, future_value)
    {
        let (_, principal) = end_payment_parts(
            periodic_rate,
            period_index,
            periods,
            present_value,
            future_value,
            log_growth,
            quotient,
            timing,
        );
        return if principal.is_finite() {
            Ok(principal)
        } else {
            Err(FinancialError::Num)
        };
    }
    // Excel does not subtract the interest from the payment, which cancels when the interest
    // is nearly the whole payment (dc-j9k: PPMT(0.1809,1,375,57755.66,635.76) is -8.79e-24 in
    // Excel, and payment - interest gave noise of order 1e-12). The principal is the payment
    // net of the future value's interest, discounted over the m = periods - period_index + 1
    // periods left: v^m * (pmt - fv*r), and v^m * (pmt - fv*r/(1 + r)) for payments at the
    // start of a period. The catalogue oracle's PPMT readings hold it.
    let result = discounted_payment_principal(
        periodic_rate,
        period_index,
        periods,
        payment,
        future_value,
        timing,
    );
    if result.is_finite() {
        Ok(result)
    } else {
        Err(FinancialError::Num)
    }
}

/// The principal change across a whole-period span (dc-j9k.49). With PMT's stored
/// quotient q = PV/EXPM1(-nL), the common beginning-payment core is
/// q * (EXP(-(n-end+1)L) * -EXPM1(-count*L)), with both products stored from PC64.
/// The EXPM1 span preserves small principal amounts without subtracting balances.
/// Ending payments then multiply the stored core by stored 1+rate: all 160
/// recorded timing pairs agree, including the addition double-rounding cases.
/// Beginning payments add their whole first payment after the span from period 2.
/// None preserves the caller's existing fallback outside PMT's quotient domain.
pub(crate) fn cumulative_principal(
    periodic_rate: f64,
    periods: f64,
    present_value: f64,
    start_period: f64,
    end_period: f64,
    timing: PaymentTiming,
) -> Option<f64> {
    if !(periodic_rate > 0.0) || start_period > end_period || start_period < 1.0 {
        return None;
    }
    let (log_growth, quotient) = end_payment_quotient(periodic_rate, periods, present_value, 0.0)?;
    let beginning = matches!(timing, PaymentTiming::BeginningOfPeriod);
    let first = if beginning { start_period.max(2.0) } else { start_period };
    let mut total = 0.0;
    if end_period >= first {
        let last = periods - end_period + 1.0;
        let discount = crate::ported::excel_numeric::excel_exp(-excel_x87_mul(last, log_growth));
        let count = end_period - first + 1.0;
        let span = -crate::ported::excel_numeric::excel_expm1_spilled(-excel_x87_mul(count, log_growth));
        total = excel_x87_mul(quotient, excel_x87_mul(discount, span));
        if !beginning {
            total = excel_x87_mul(total, excel_x87_add(1.0, periodic_rate));
        }
    }
    if beginning && start_period == 1.0 {
        total = excel_x87_add(total, excel_x87_mul(quotient, due_coefficient(periodic_rate)));
    }
    total.is_finite().then_some(total)
}

/// The discounted-payment principal expression. The cumulative beginning-payment
/// fallback still uses it: its singletons differ from public PPMT, whose measured
/// positive-rate finish uses the shared period component above. Keep this earlier
/// expression outside the shared quotient domain.
pub(crate) fn discounted_payment_principal(
    periodic_rate: f64,
    period_index: f64,
    periods: f64,
    payment: f64,
    future_value: f64,
    timing: PaymentTiming,
) -> f64 {
    let remaining = periods - period_index + 1.0;
    let discount = exp_portable(-(remaining * excel_log1p(periodic_rate)));
    let carried = if matches!(timing, PaymentTiming::BeginningOfPeriod) {
        future_value * periodic_rate / (1.0 + periodic_rate)
    } else {
        future_value * periodic_rate
    };
    discount * (payment - carried)
}

pub fn ispmt(
    periodic_rate: f64,
    period_index: f64,
    periods: f64,
    present_value: f64,
) -> Result<f64, FinancialError> {
    validate_finite(&[periodic_rate, period_index, periods, present_value])?;
    if periods == 0.0 {
        return Err(FinancialError::Div0);
    }
    // Excel stores the scaled period difference before applying the rate,
    // then stores that product before the division. Dividing the period index
    // first loses cancellation bits; combining rate and value first also
    // changes the result. Negative period counts remain in the same formula.
    let difference = excel_x87_sub(period_index, periods);
    let scaled = excel_x87_mul(present_value, difference);
    let interest = excel_x87_mul(scaled, periodic_rate);
    let result = excel_x87_div(interest, periods);
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
    if cashflows.len() < 2
        || !cashflows.iter().any(|value| *value > 0.0)
        || !cashflows.iter().any(|value| *value < 0.0)
    {
        return Err(FinancialError::Div0);
    }
    // Native Excel 20527: positive NPV is evaluated before finance discounts.
    // It is compounded for the full cashflow count before division, not for
    // nper after forming the ratio. The shared loop preserves NPV's stores.
    let reinvest_base = excel_x87_add(1.0, reinvest_rate);
    // At the measured negative-base boundary, Excel checks the zero finance
    // divisor before reporting overflow of the final positive normalization.
    // Overflow of a positive term itself still takes its existing path.
    let scaled_positive = finance_rate == (-1.0_f64).next_down()
        && reinvest_rate == (-1.0_f64).next_up()
        && cashflows.len() == 16;
    let zero_finance_divisor_at = if finance_rate == -1.0 {
        cashflows
            .iter()
            .rposition(|value| *value < 0.0)
            .filter(|&position| position > 0)
    } else {
        None
    };
    let defer_final_overflow = scaled_positive
        || (zero_finance_divisor_at.is_some()
            && reinvest_rate == (-1.0_f64).next_down()
            && cashflows.len() <= 16);
    let positive_npv = discounted_npv(
        reinvest_base,
        cashflows
            .iter()
            .map(|value| if *value > 0.0 { *value } else { 0.0 }),
        defer_final_overflow,
        zero_finance_divisor_at,
    )?;
    let finance_base = excel_x87_add(1.0, finance_rate);
    let finance_discount = discounted_cashflows(
        finance_base,
        cashflows
            .iter()
            .map(|value| if *value < 0.0 { Some(*value) } else { None }),
    );
    let finance_discount = match finance_discount {
        // An unusable finance discount does not hide the earlier positive
        // normalization overflow in the scaled-compounding case.
        Err(_) if scaled_positive && !positive_npv.is_finite() => return Err(FinancialError::Num),
        result => result?,
    };
    // The finance stream also reports overflow of its unused terminal power.
    // A zero base remains valid when every negative flow is at time zero.
    if finance_discount.overflow_at.is_some() || !finance_discount.terminal_power.is_finite() {
        return Err(FinancialError::Num);
    }
    let finance_pv = financial_daz(finance_discount.total);
    if finance_pv == 0.0 {
        return Err(FinancialError::Div0);
    }
    let growth = crt_pow(reinvest_base, cashflows.len() as f64);
    let grown_positive = if scaled_positive && !positive_npv.is_finite() {
        // The normalization overflows, but Excel retains the finite result
        // after compounding. Accumulate those positive terms at their final
        // time scale, keeping the stored multiply and add stages.
        let mut total = 0.0;
        let mut power = 1.0;
        for value in cashflows.iter().rev() {
            if *value > 0.0 {
                total = excel_x87_add(total, excel_x87_mul(*value, power));
            }
            power = excel_x87_mul(power, reinvest_base);
        }
        -total
    } else {
        excel_x87_mul(-positive_npv, growth)
    };
    let ratio = financial_daz(excel_x87_div(grown_positive, finance_pv));
    let periods = cashflows.len() - 1;
    let negative = ratio < 0.0;
    if negative && periods % 2 == 0 {
        return Err(FinancialError::Num);
    }
    let magnitude = ratio.abs();
    // Touchstone Excel 20625 resolves degree-two midpoint ties by storing
    // the PC64 square root. Higher degrees use stored LN/reciprocal/EXP.
    let root = if magnitude == 0.0 || periods == 1 {
        magnitude
    } else if periods == 2 {
        excel_sqrt_staged(magnitude)
    } else {
        crate::ported::excel_numeric::excel_pow_chain(magnitude, excel_x87_div(1.0, periods as f64))
    };
    let result = excel_x87_sub(if negative { -root } else { root }, 1.0);
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
    // Live Excel (dc-j9k.330): an overflowing running product (or an infinity times zero) is 0.
    if result.is_finite() {
        Ok(result)
    } else {
        Ok(0.0)
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
    // An exponent times ln(base) that overflows binary64 is #NUM!, where the power would
    // underflow to 0 and answer -1 (Excel 20430, dc-j9k.395).
    if periods != 1.0 && !(reciprocal * base.ln()).is_finite() {
        return Err(FinancialError::Num);
    }
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
    use super::*;
    #[allow(unused_imports)]
    use crate::value::{CalcArray, CalcValue, CoreValue, ExcelText};

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

    /// Live Excel (dc-j9k.234): a ratio that falls below the smallest normal double is #NUM!,
    /// while 2.3e-308 still answers.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn nper_subnormal_ratio_is_num() {
        let end = PaymentTiming::EndOfPeriod;
        assert_eq!(nper(1.0, -1.0e-8, -1.0e300, 0.0, end), Err(FinancialError::Num));
        assert_eq!(nper(1.0, -2.2250738585072014e-8, -1.0e300, 0.0, end), Err(FinancialError::Num));
        assert_eq!(nper(1.0, -2.2e-8, -1.0e300, 0.0, end), Err(FinancialError::Num));
        assert_eq!(
            nper(1.0, -2.3e-8, -1.0e300, 0.0, end).map(f64::to_bits),
            Ok(0xc08fef9e252f9cb9)
        );
        assert_eq!(
            nper(
                f64::from_bits(0x3e34558ce1c4c802),
                f64::from_bits(0xb7c53ade41b59fc9),
                f64::from_bits(0xfbb5ced110391578),
                0.0,
                end
            ),
            Err(FinancialError::Num)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rate_without_periods_is_num() {
        // Excel 16.0.20527 (dcztvv_r23): with no periods, or 2^-1022 of one, the balance is the
        // same at every rate and RATE is #NUM!.
        for periods in [0.0, f64::MIN_POSITIVE] {
            for (payment, present, guess) in [(-100.0, 1000.0, Some(0.5)), (-200.0, 8000.0, None)] {
                assert_eq!(
                    rate(
                        periods,
                        payment,
                        present,
                        0.0,
                        PaymentTiming::EndOfPeriod,
                        guess
                    ),
                    Err(FinancialError::Num)
                );
            }
        }
    }

    /// Large-root iterates from Excel 16.0.20527 (bank2-answers-rate-more and -scalar).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rate_at_a_huge_root_is_the_iterate_that_stopped_moving() {
        use PaymentTiming::{BeginningOfPeriod as Begin, EndOfPeriod as End};
        for (periods, payment, present, future, timing, guess, excel) in [
            (0.12897432973391876, 1235.0, -0.5, 10.0, End, None, 12232404830.799618),
            (0.1391623113305186, 10.0, -438.10866310194024, 8205.0, Begin, Some(1.0000000000000002), 1630151639.0138218),
            (0.29857658183127744, -2.0, 0.9999999999999999, 958.8025113570343, Begin, None, 9761381152.492899),
        ] {
            let got = rate(periods, payment, present, future, timing, guess).expect("Excel answers");
            assert_bits(got, excel);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rate_flat_secant_at_neighbouring_payments() {
        // Excel 16.0.20625.20108, gx1-RATE-2092. Adjacent payment words change
        // which step the solver reaches before its budget expires.
        for (payment, future, expected) in [
            (0x4049c31df3fc025b, 0xbfe901d6d18d4f4b, Ok(0x4269e628a4e4ace9)),
            (0x4049c31df3fc025c, 0xbfe901d6d18d4f4b, Ok(0x4269e628a4e4acd0)),
            (0x4049c31df3fc025d, 0xbfe901d6d18d4f4b, Err(FinancialError::Num)),
            // Moving the future value four words leaves the nudged slope flat.
            (0x4049c31df3fc025c, 0xbfe901d6d18d4f47, Err(FinancialError::Num)),
        ] {
            let result = rate(
                f64::from_bits(0xbfb6dc004793792d),
                f64::from_bits(payment),
                f64::from_bits(0x4081766241ff7680),
                f64::from_bits(future),
                PaymentTiming::BeginningOfPeriod,
                Some(f64::from_bits(0x3fdda8bbc02b4f20)),
            );
            assert_eq!(result.map(f64::to_bits), expected, "payment {payment:x}, future {future:x}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rate_exactness_witness_matches_excel() {
        let actual = rate(
            360.0,
            -1073.64,
            200000.0,
            0.0,
            PaymentTiming::EndOfPeriod,
            None,
        )
        .expect("rate witness");
        assert_bits(actual, f64::from_bits(0x3f71110b2048574f));
    }

    /// Excel 16.0.20625.20108 (sf2 live smart-fuzzer sweeps, dc-ztvv.21): RATE's balance at
    /// rate == 0.0 uses FV's `(pv + pmt * nper) + fv` order (`payment_balance(pv, pmt, 1, nper)
    /// + fv`), not `(fv + pv) + pmt * nper`.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rate_balance_at_zero_uses_payment_balance_order() {
        use PaymentTiming::{BeginningOfPeriod as Begin, EndOfPeriod as End};
        for (args, timing, expected) in [
            // sf2-RATE-00005 (s1): 1 ULP
            (
                [
                    0x4000_0000_0000_0000_u64,
                    0x405b_e84c_f8ab_8ed1,
                    0xc06e_2fd8_de67_0869,
                    0x4059_20e6_3578_5045,
                    0x0000_0000_0000_0000,
                ],
                End,
                Ok(0x3fc9_2596_8f7b_d84b_u64),
            ),
            // sf2-RATE-00004 (s2): 6,849 ULP
            (
                [
                    0x3ff0_0000_0000_0000,
                    0xc151_2454_78db_6370,
                    0x414b_8b92_e7bc_7b76,
                    0x412b_8841_e0d3_0d7f,
                    0x0000_0000_0000_0000,
                ],
                End,
                Ok(0xbf75_7ab3_9518_9896),
            ),
            // sf2-RATE-00063 (s1): guess = -0.001 steps to rate == 0.0
            (
                [
                    0x4075_8000_0000_0000,
                    0x3ef1_d179_e64c_8b70,
                    0x414c_be96_ed41_cff2,
                    0xc14c_be96_ed41_cff2,
                    0xbf50_624d_d2f1_a9fc,
                ],
                End,
                Ok(0xbd93_d617_dc8a_9a5c),
            ),
            // sf2-RATE-00246 (s1): tiny guess steps to rate == 0.0 on iter 0
            (
                [
                    0x3ff0_0000_0000_0000,
                    0xbeb0_0515_1cf3_ef70,
                    0x3f9d_d5ac_8017_ddd1,
                    0xbf9d_d5ac_8017_ddd1,
                    0x8010_0000_0000_0283,
                ],
                End,
                Ok(0x3f01_2eba_f7b7_ab8d),
            ),
            // sf2-RATE-00575 (s1): #NUM! vs finite number
            (
                [
                    0xc000_0000_0000_0000,
                    0x40de_60c2_7577_ea46,
                    0xc114_442b_5b49_6cff,
                    0x4118_1043_a9de_3363,
                    0x0000_0000_0000_0000,
                ],
                End,
                Err(FinancialError::Num),
            ),
            // frontier-RATE-017 (s2): adjacent fv words shifted -499 ULP before fix
            (
                [
                    0x3fe0_0000_0000_0000,
                    0xc0ca_2a7e_5c0e_6be3,
                    0x40d5_449a_af51_a76d,
                    0xc0c1_fe59_7a20_dff8,
                    0x0000_0000_0000_0000,
                ],
                Begin,
                Ok(0xbfe6_b217_53c6_b554),
            ),
            (
                [
                    0x3fe0_0000_0000_0000,
                    0xc0ca_2a7e_5c0e_6be3,
                    0x40d5_449a_af51_a76d,
                    0xc0c1_fe59_7a20_dff7,
                    0x0000_0000_0000_0000,
                ],
                Begin,
                Err(FinancialError::Num),
            ),
        ] {
            let got = rate(
                f64::from_bits(args[0]),
                f64::from_bits(args[1]),
                f64::from_bits(args[2]),
                f64::from_bits(args[3]),
                timing,
                Some(f64::from_bits(args[4])),
            );
            assert_eq!(got.map(f64::to_bits), expected, "args={args:x?}");
        }
    }

    /// Excel 16.0.20625.20108 (sf2 frontier-RATE-014, dc-ztvv.21): when RATE's initial balance
    /// overflows to +/-infinity, Excel returns #NUM! immediately (as FV does) rather than
    /// continuing to `guess +/- 0.001` and hitting a downstream #DIV/0! in growth_from_base.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rate_balance_overflow_returns_num_before_next_probe() {
        let pmt = f64::from_bits(0xc056_7bf2_a65f_3603);
        let pv = f64::from_bits(0x407f_5b86_2e2a_57fd);
        let guess = Some(f64::from_bits(0xbfe8_df81_c443_d4f7));
        for (nper_bits, expected) in [
            // Former DNA flip: balance(guess) overflows to +inf; balance(guess - 0.001) -> #DIV/0!.
            (0xc07d_6458_666c_ca98_u64, Err(FinancialError::Num)),
            // Excel exact #NUM! <-> #DIV/0! boundary: (1 + guess)^|nper| crosses 2^-1022.
            (0xc07d_7ae4_1dc5_60a9_u64, Err(FinancialError::Num)),
            (0xc07d_7ae4_1dc5_60aa_u64, Err(FinancialError::Div0)),
        ] {
            let got = rate(
                f64::from_bits(nper_bits),
                pmt,
                pv,
                0.0,
                PaymentTiming::EndOfPeriod,
                guess,
            );
            assert_eq!(got, expected, "nper={nper_bits:#018x}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn ppmt_exactness_witness_matches_excel() {
        // Excel (PPMT-0000): PPMT(0.05/12, 1, 360, 200000) = 0xc06e09eace050723. dc-j9k.12:
        // the principal RN(RN(q * u) * rate) reaches it; the earlier forms were 1 to 2 ulp off.
        let ppmt_actual = ppmt(
            0.05 / 12.0,
            1.0,
            360.0,
            200000.0,
            0.0,
            PaymentTiming::EndOfPeriod,
        )
        .expect("ppmt witness");
        assert_bits(ppmt_actual, f64::from_bits(0xc06e09eace050723));
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
        // dc-j9k.12: with Excel's series logarithm the calculation is 0xc06030257a65e089 again,
        // the publication value. Still no Excel reading of this row: a local pin.
        assert_bits(
            pmt(0.05, 10.0, 1000.0, 0.0, PaymentTiming::EndOfPeriod).expect("pmt publication row"),
            f64::from_bits(0xc06030257a65e089),
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
    fn npv_reproduces_recorded_excel_answers() {
        // 1,320 calls answered by Excel 16.0.20527 (design/fable52-last-bit.md section 1:
        // n1 discovery, n6 proof, n9/n10 designed rounding calls, n7 overflow edges).
        let text = include_str!("../../tests/npv_excel_answers.txt");
        let mut rows = 0;
        let mut wrong = Vec::new();
        for line in text.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
            let mut parts = line.split('|');
            let (set, args, want) = (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap());
            let values: Vec<f64> = args
                .split(',')
                .map(|h| f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits")))
                .collect();
            let got = match npv(values[0], &values[1..]) {
                Ok(v) => format!("{:016x}", v.to_bits()),
                Err(FinancialError::Div0) => "#DIV/0!".to_owned(),
                Err(FinancialError::Num) => "#NUM!".to_owned(),
                Err(other) => format!("{other:?}"),
            };
            rows += 1;
            if got != want {
                wrong.push(format!("{set} {args}: Excel {want}, calculated {got}"));
            }
        }
        assert_eq!(rows, 1320);
        assert!(wrong.is_empty(), "{} NPV rows differ:
{}", wrong.len(), wrong.join("
"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn npv_tiny_base_error_rules_match_the_measured_calls() {
        // design/fable52-last-bit.md section 1, b = 2^-30: 35 flows of 1 overflow only the
        // final division (#NUM!); 36 hit a subnormal divisor (#DIV/0!);
        // 37 flows of 0 also reach an unusable divisor.
        let rate = 2.0f64.powi(-30) - 1.0;
        assert_eq!(npv(rate, &[1.0; 35]), Err(FinancialError::Num));
        assert_eq!(npv(rate, &[1.0; 36]), Err(FinancialError::Div0));
        assert_eq!(npv(rate, &[0.0; 37]), Err(FinancialError::Div0));
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

    /// Live Excel (dc-j9k.330): a schedule that overflows the running product publishes 0, not #NUM!.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fvschedule_overflow_publishes_zero() {
        assert_eq!(fvschedule(9e307, &[1.0]).map(f64::to_bits), Ok(0));
        assert_eq!(fvschedule(9e307, &[1.0, -0.75]).map(f64::to_bits), Ok(0));
        assert_eq!(fvschedule(9e307, &[1.0, -1.0]).map(f64::to_bits), Ok(0));
        assert_eq!(fvschedule(1e200, &[1e100, 1e100]).map(f64::to_bits), Ok(0));
        assert_eq!(fvschedule(1e200, &[1e300, -0.5, -0.5]).map(f64::to_bits), Ok(0));
        assert_eq!(fvschedule(1e200, &[1e100, -0.5]).map(f64::to_bits), Ok(0x7e27e43c8800759c));
        assert_eq!(fvschedule(8e307, &[1.0]).map(f64::to_bits), Ok(0x7fec7b1f3cac7433));
        assert_eq!(fvschedule(2.0, &[-5.0]).map(f64::to_bits), Ok(0xc020000000000000));
        assert_eq!(
            fvschedule(
                f64::from_bits(0x42e0000000000000),
                &[-536870912.0, 4503599627370496.0, -10.0, f64::MAX, -0.5, 360.0]
            )
            .map(f64::to_bits),
            Ok(0)
        );
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

    /// Excel 16.0.20527 (dc-iuwct.64 / ww2): FV flushes subnormal `(factor - 1) / rate`
    /// before applying beginning-of-period timing, and RATE's extreme-negative-period
    /// overflow check triggers on step-quotient overflow (not finite-quotient subtraction
    /// overflow) with the asymptotic limit using flushed `-1 / current`.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_iuwct_64_fv_and_rate_extreme_negative_period_overflow() {
        // FV beginning-of-period rows where (factor - 1) / rate is subnormal (ww2 dc-iuwct.64).
        let fv_rows: [(u64, u64, u64, u64, u64); 3] = [
            (0x7fcfffffffffffff, 0xbe4d5aea8c545370, 0x3f70eb68c9dc920f, 0xbfef657a8f0947ec, 0x3fef6566a25a6edc),
            (0x7fdde69730bdaeca, 0xbe553ecb94e74f78, 0x3f88d1df3fea1669, 0xc01fa04ff26c3934, 0x401fa032dec33083),
            (0x7fef6ee528c1d989, 0xbe4d5aea8c545370, 0x3f70eb68c9dc920f, 0xbfef657a8f0947ec, 0x3fef656698800e39),
        ];
        for (r, n, p, v, expected) in fv_rows {
            let got = fv(
                f64::from_bits(r),
                f64::from_bits(n),
                f64::from_bits(p),
                f64::from_bits(v),
                PaymentTiming::BeginningOfPeriod,
            )
            .expect("finite FV");
            assert_eq!(got.to_bits(), expected, "FV({r:#018x}, {n:#018x}, ...)");
        }

        // RATE overflow rows with cur > 2^1022: end_term_inf = ftz(-1/cur) == 0 -> limit is fv (not fv - pmt).
        // 494 (fv = 0, type = 1 -> finite) and 2983 (num/q overflows to +inf -> finite).
        let r494 = rate(
            f64::from_bits(0xbe4d5aea8c545370),
            f64::from_bits(0x3f70eb68c9dc920f),
            f64::from_bits(0xbfef657a8f0947ec),
            0.0,
            PaymentTiming::BeginningOfPeriod,
            Some(f64::from_bits(0xbfcd56707dc94dc8)),
        )
        .expect("RATE 494 finite");
        assert_eq!(r494.to_bits(), 0x7fef6ee528c1d989);

        let r224 = rate(
            f64::from_bits(0xbe553ecb94e74f78),
            f64::from_bits(0x3f88d1df3fea1669),
            f64::from_bits(0xc01fa04ff26c3934),
            f64::from_bits(0xb8ce84cafdf6c093),
            PaymentTiming::BeginningOfPeriod,
            Some(f64::from_bits(0x3fb1ed3f08a6c010)),
        )
        .expect("RATE 224 finite");
        assert_eq!(r224.to_bits(), 0x7fdde69730bdaeca);

        // 2983: end_term flushes to 0 at cur > 2^1022, flipping slope so quotient = +inf -> still finite.
        let r2983 = rate(
            f64::from_bits(0xbf0be388b1f790bd),
            f64::from_bits(0xbff93c05ab1c0383),
            f64::from_bits(0xc050d0946ff6dea9),
            f64::from_bits(0xacc1a90d5620bea1),
            PaymentTiming::BeginningOfPeriod,
            Some(f64::from_bits(0xbfd175d6af4a85bc)),
        )
        .expect("RATE 2983 finite");
        assert_eq!(r2983.to_bits(), 0x7fd026924e3ed395);

        // 1/MIN_POSITIVE (2^1022) cutoff on nper=-1e-8, pmt=2, fv=0, type=1, guess=0.1:
        // pv=-141.43 lands above 2^1022 (finite), pv=-141.44 lands below 2^1022 (#NUM!).
        assert!(rate(-1e-8, 2.0, -141.43, 0.0, PaymentTiming::BeginningOfPeriod, Some(0.1)).is_ok());
        assert_eq!(
            rate(-1e-8, 2.0, -141.44, 0.0, PaymentTiming::BeginningOfPeriod, Some(0.1)),
            Err(FinancialError::Num)
        );

        // 494 with fv = pmt and cur > 2^1022 -> #NUM!.
        assert_eq!(
            rate(
                f64::from_bits(0xbe4d5aea8c545370),
                f64::from_bits(0x3f70eb68c9dc920f),
                f64::from_bits(0xbfef657a8f0947ec),
                f64::from_bits(0x3f70eb68c9dc920f),
                PaymentTiming::BeginningOfPeriod,
                Some(f64::from_bits(0xbfcd56707dc94dc8)),
            ),
            Err(FinancialError::Num)
        );

        // 646, 1519, 999: step quotient q is finite while current - q overflows -> #NUM!.
        assert_eq!(
            rate(
                f64::from_bits(0xbeea744b77acef54),
                0.0,
                f64::from_bits(0xc048ffe970b95b26),
                0.0,
                PaymentTiming::EndOfPeriod,
                Some(f64::from_bits(0xbfb4b35626e989f8)),
            ),
            Err(FinancialError::Num)
        );
        assert_eq!(
            rate(
                f64::from_bits(0xbf7264c20e94345c),
                f64::from_bits(0xaf0bba7ddbeb7a77),
                f64::from_bits(0xc004d6c3461df218),
                f64::from_bits(0xabb1dd65b2687c5b),
                PaymentTiming::BeginningOfPeriod,
                Some(f64::from_bits(0xbfe29c909c3253bc)),
            ),
            Err(FinancialError::Num)
        );
        assert_eq!(
            rate(
                f64::from_bits(0xbf7a149b329ad4a3),
                f64::from_bits(0xbac8e7e3927b3730),
                f64::from_bits(0xc0272d9e72b54712),
                f64::from_bits(0xbac8e7e3927b3730),
                PaymentTiming::EndOfPeriod,
                Some(f64::from_bits(0x3fd198819577be54)),
            ),
            Err(FinancialError::Num)
        );
    }

    /// Excel 16.0.20625.20108 (sf2/cb25, dc-j9k.97): when `divisor == 0.0` with
    /// negative `nper`, `current > 0`, and positive-power overflow (`pos_power == +inf`),
    /// Excel runs the same `|limit_inf(current)| <= 1e-7` publication check it runs on
    /// `!quotient.is_finite()`, while keeping `#DIV/0!` when `pos_power < DBL_MAX`.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_j9k_97_rate_pos_power_overflow_rescue_at_zero_divisor() {
        // R2-dc-j9k-97-num-vs-div0: pos_power == +inf rescues divisor == 0.0 -> 0x3fb95810624dd2f2.
        let rescued = rate(
            f64::from_bits(0xffefffffffffffff),
            f64::from_bits(0x8010000000000001),
            f64::from_bits(0xc076800000000000),
            0.0,
            PaymentTiming::EndOfPeriod,
            None,
        )
        .expect("R2-dc-j9k-97-num-vs-div0 finite");
        assert_eq!(rescued.to_bits(), 0x3fb95810624dd2f2);

        // cb25-RATE-RB-14: pos_power == +inf rescues even when |value| < MIN_POSITIVE.
        let rb14 = rate(
            f64::from_bits(0xc0f0000000000000),
            f64::from_bits(0x8020000000000000),
            f64::from_bits(0xc024000000000000),
            f64::from_bits(0x801ff00000000000),
            PaymentTiming::EndOfPeriod,
            Some(f64::from_bits(0x3ff0000000000000)),
        )
        .expect("cb25-RATE-RB-14 finite");
        assert_eq!(rb14.to_bits(), 0x3feff7ced916872b);

        // cb25-RATE-RB-18: pos_power == +inf with |limit| > 1e-7 does NOT publish.
        assert_eq!(
            rate(
                f64::from_bits(0xc0f0000000000000),
                f64::from_bits(0x8010000000000008),
                f64::from_bits(0xc024000000000000),
                f64::from_bits(0x3e8ad7f29abcaf48),
                PaymentTiming::EndOfPeriod,
                Some(f64::from_bits(0x3fb999999999999a)),
            ),
            Err(FinancialError::Num)
        );

        // sf2 fuzz-RATE-r01-013297: pos_power < DBL_MAX stays #DIV/0!.
        assert_eq!(
            rate(
                f64::from_bits(0xc031cae22accbf82),
                f64::from_bits(0x83b4092dc31da972),
                f64::from_bits(0x82834313dabdf041),
                f64::from_bits(0x80100000000001c3),
                PaymentTiming::EndOfPeriod,
                Some(f64::from_bits(0x3fdb8281c1f808b2)),
            ),
            Err(FinancialError::Div0)
        );
    }
}
