//! Portable, zero-cost-when-inactive route and branch tracer for code-guided fuzzing (`dc-ztvv.21`).
//!
//! When tracing is inactive (the default for all production, benchmark and ordinary test calls),
//! instrumented kernels pay only a single thread-local boolean check and make zero allocations.
//! When active inside [`with_trace`], instrumented functions record which internal branches and
//! predicate regimes fired, how many solver iterations ran, and the closest relative distance to
//! key threshold predicates so `cargo xtask fuzz` can steer subsequent rounds toward unexercised
//! paths and narrow boundaries.

use std::cell::{Cell, RefCell};

/// Named route bits for `RATE` (`financial_time_value_family::rate` / `rate_secant`).
pub const RATE_ROUTES: &[&str] = &[
    "guess_le_neg_one_value",
    "balance_rate_zero",
    "balance_base_nonpos",
    "balance_growth_err",
    "balance_end_term_ftz",
    "balance_timing_beginning",
    "flat_change_hit",
    "flat_change_subnormal_neg",
    "flat_change_nudge_up",
    "flat_change_nudge_down",
    "flat_change_tie",
    "flat_change_double_num",
    "divisor_daz_div0",
    "numerator_ftz",
    "quotient_overflow_neg_period_accept",
    "quotient_overflow_neg_period_reject",
    "next_nonfinite_num",
    "converged_strict_fast",
    "converged_strict_mid",
    "converged_strict_slow",
    "converged_zero_step",
    "post_budget_accept",
    "post_budget_reject_step",
    "post_budget_reject_value",
    "post_budget_reject_base",
    "final_base_below_tol_num",
    "root_regime_ii",
    "root_regime_iii_plus",
    "root_negative",
    "root_zero",
];

/// Named route bits for `IRR` (`cashflow_rate_family::irr_kernel` / `irr_u_secant`).
pub const IRR_ROUTES: &[&str] = &[
    "guess_invalid_value",
    "cashflow_validation_err",
    "retry_0_user_guess_ok",
    "retry_1_default_0_1_ok",
    "retry_2_neg_0_1_ok",
    "retry_3_two_thirds_ok",
    "all_retries_failed_num",
    "npv_v_zero_or_base_nonfinite",
    "npv_divisor_zero_or_nonfinite",
    "npv_term_nonfinite",
    "flat_change_nudge_up",
    "flat_change_nudge_down",
    "flat_change_double_continue",
    "divisor_daz_abort",
    "next_nonfinite_abort",
    "converged_strict_fast",
    "converged_strict_slow",
    "post_budget_accept",
    "post_budget_reject",
    "crossed_neg_v_or_publish_reject",
];

/// Named route bits for `YIELD` (`bond_core_family::yield_kernel`).
pub const YIELD_ROUTES: &[&str] = &[
    "context_or_arg_err",
    "single_period_n1",
    "single_period_dsc_zero",
    "multi_period_seed_nonfinite",
    "multi_period_basis_30_360",
    "multi_period_basis_actual",
    "newton_base_zero_or_neg_frac_num",
    "newton_neg_base_int_off",
    "newton_neg_base_zero_coupon_subnormal_slope",
    "newton_step_nonfinite_num",
    "newton_converged_fast",
    "newton_converged_mid",
    "newton_converged_slow",
    "newton_budget_exhausted_100",
    "newton_final_nonfinite_num",
];

/// Look up the named route table for an invariant function name (empty for functions without
/// custom kernel hooks; the fuzzer still tracks generic outcome routes for every function).
pub fn route_names(function: &str) -> &'static [&'static str] {
    match function {
        "RATE" => RATE_ROUTES,
        "IRR" => IRR_ROUTES,
        "YIELD" => YIELD_ROUTES,
        _ => &[],
    }
}

/// Captured branch/route telemetry for a single function invocation.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteTrace {
    /// Bitmask of internal routes/branches hit during the call (bit `i` = `route_names(f)[i]`).
    pub route_mask: u64,
    /// Solver iterations executed (0 for non-iterative paths).
    pub iterations: u32,
    /// Smallest relative distance observed to any tracked threshold predicate (`f64::INFINITY` if none).
    pub min_boundary_margin: f64,
    /// Name of the closest boundary predicate observed, if any.
    pub closest_boundary: Option<&'static str>,
}

impl Default for RouteTrace {
    fn default() -> Self {
        Self {
            route_mask: 0,
            iterations: 0,
            min_boundary_margin: f64::INFINITY,
            closest_boundary: None,
        }
    }
}

impl RouteTrace {
    /// Returns whether route bit `bit` fired during this call.
    pub fn has_bit(&self, bit: u32) -> bool {
        bit < 64 && (self.route_mask & (1u64 << bit)) != 0
    }
}

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static CURRENT: RefCell<RouteTrace> = RefCell::new(RouteTrace::default());
}

/// Fast check whether route tracing is active on the current thread.
#[inline]
pub fn is_active() -> bool {
    ACTIVE.with(Cell::get)
}

/// Record route bit `bit` (`0..64`) if tracing is active.
#[inline]
pub(crate) fn hit(bit: u32) {
    if bit < 64 && is_active() {
        CURRENT.with(|cell| {
            cell.borrow_mut().route_mask |= 1u64 << bit;
        });
    }
}

/// Record the maximum solver iteration count observed during the call.
#[inline]
pub(crate) fn record_iterations(iters: u32) {
    if is_active() {
        CURRENT.with(|cell| {
            let mut trace = cell.borrow_mut();
            if iters > trace.iterations {
                trace.iterations = iters;
            }
        });
    }
}

/// Observe how close `value` came to a threshold predicate `threshold`.
#[inline]
pub(crate) fn observe_boundary(label: &'static str, value: f64, threshold: f64) {
    if !is_active() || !value.is_finite() {
        return;
    }
    let margin = if threshold == 0.0 {
        value.abs()
    } else {
        (value - threshold).abs() / threshold.abs()
    };
    if margin.is_finite() {
        CURRENT.with(|cell| {
            let mut trace = cell.borrow_mut();
            if margin < trace.min_boundary_margin {
                trace.min_boundary_margin = margin;
                trace.closest_boundary = Some(label);
            }
        });
    }
}

struct TraceGuard(bool);

impl Drop for TraceGuard {
    fn drop(&mut self) {
        ACTIVE.with(|active| active.set(self.0));
    }
}

/// Execute `f` with route tracing enabled on the current thread, returning its output and the
/// captured [`RouteTrace`]. Safe across panics and nested calls.
pub fn with_trace<R>(f: impl FnOnce() -> R) -> (R, RouteTrace) {
    let previous_active = ACTIVE.with(|active| {
        let prev = active.get();
        active.set(true);
        prev
    });
    let previous_trace = CURRENT.with(|cell| cell.replace(RouteTrace::default()));
    let _guard = TraceGuard(previous_active);
    let result = f();
    let captured = CURRENT.with(|cell| cell.replace(previous_trace));
    (result, captured)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{CalcArray, CalcValue, NumericMode};
    use crate::{EvalServices, FunctionCallScratch, Notation, PreparedArguments, catalogue, invoke};

    struct SliceArgs<'a>(&'a [CalcValue]);
    impl PreparedArguments for SliceArgs<'_> {
        fn count(&self) -> usize {
            self.0.len()
        }
        fn argument(&mut self, index: usize) -> CalcValue {
            self.0[index].clone()
        }
    }

    fn eval(name: &str, args: &[CalcValue]) -> CalcValue {
        let target = catalogue().resolve_surface(Notation::Call, name).unwrap();
        let env = crate::locale::CalcEnvironment::dates(crate::locale::DateSystem::System1900);
        let mut services = EvalServices::new(&NumericMode::Fast).with_environment(&env);
        let mut scratch = FunctionCallScratch::default();
        let mut prepared = SliceArgs(args);
        invoke(target, &mut prepared, &mut services, &mut scratch).unwrap()
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rate_trace_is_zero_effect_and_records_routes() {
        let args = [
            CalcValue::number(10.0),
            CalcValue::number(-120.0),
            CalcValue::number(1000.0),
        ];
        let untraced = eval("RATE", &args);
        let (traced, trace) = with_trace(|| eval("RATE", &args));
        assert_eq!(untraced, traced);
        assert!(trace.iterations > 0);
        assert!(trace.has_bit(17) || trace.has_bit(18), "mask={:#x}", trace.route_mask);
        assert!(trace.min_boundary_margin.is_finite());

        // Guess <= -1 triggers bit 0.
        let bad_guess = [
            CalcValue::number(10.0),
            CalcValue::number(-120.0),
            CalcValue::number(1000.0),
            CalcValue::number(0.0),
            CalcValue::number(0.0),
            CalcValue::number(-1.0),
        ];
        let (_, bad_trace) = with_trace(|| eval("RATE", &bad_guess));
        assert!(bad_trace.has_bit(0), "expected guess_le_neg_one_value");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn irr_and_yield_traces_record_routes_without_altering_results() {
        let flows = CalcValue::array(
            CalcArray::from_rows(vec![vec![
                CalcValue::number(-100.0),
                CalcValue::number(60.0),
                CalcValue::number(60.0),
            ]])
            .unwrap(),
        );
        let irr_plain = eval("IRR", &[flows.clone()]);
        let (irr_traced, irr_trace) = with_trace(|| eval("IRR", &[flows]));
        assert_eq!(irr_plain, irr_traced);
        assert!(irr_trace.has_bit(2), "expected retry_0_user_guess_ok");
        assert!(irr_trace.iterations > 0);

        let yield_args = [
            CalcValue::number(44927.0),
            CalcValue::number(46753.0),
            CalcValue::number(0.05),
            CalcValue::number(95.0),
            CalcValue::number(100.0),
            CalcValue::number(2.0),
            CalcValue::number(0.0),
        ];
        let y_plain = eval("YIELD", &yield_args);
        let (y_traced, y_trace) = with_trace(|| eval("YIELD", &yield_args));
        assert_eq!(y_plain, y_traced);
        assert!(y_trace.has_bit(4), "expected multi_period_basis_30_360");
        assert!(y_trace.iterations > 0);
    }
}
