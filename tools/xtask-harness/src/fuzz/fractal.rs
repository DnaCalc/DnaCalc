//! Fractal boundary refinement steered by DNA Calc (`dc-ztvv.26.5`).
//!
//! Uses DNA Calc's own cheap local execution (`calc_func::invoke` + `calc_func::trace`) to detect
//! candidate boundaries across four signal layers:
//! 1. `predicate_flip`: `number <-> error`, `error_A <-> error_B`, value-class switches;
//! 2. `route_switch`: internal kernel branch/route bitmask transitions;
//! 3. `iteration_regime`: solver iteration bucket transitions (e.g. fast -> slow -> cap);
//! 4. `outcome_regime`: sign, zero, subnormal, and magnitude regime switches.
//!
//! Around every candidate boundary, refinement proceeds self-similarly across three scales:
//! - **Decade** (`10^k` multiplicative and additive stencils + hint-file edge anchors),
//! - **Binade** (`±2^e`, top-of-binade, mid-binade, and intra-binade subdivisions inside every
//!   Decade bracket that changed),
//! - **ULP** (exact Value2-compact binary search down to adjacent binary64 words + consecutive runs).
//!
//! After pinpointing a depth-`d` boundary `(lo_bits, hi_bits)` on axis `A`, the same Decade ->
//! Binade -> ULP refinement recurses on secondary axes `B != A` while holding `A` pinned on both
//! adjacent doubles of the edge (`depth = d + 1`), discovering the "edges of the edges" (codimension-2
//! corners and interactions) deterministically before Excel is asked.

use super::boundary::{Frontier, get_axis_bits, predicate_tag, set_axis_f64};
use super::cluster::is_dna_fault;
use super::domain::{DomainSpec, ParamKind};
use super::rng::{
    compact_to_f64, f64_to_compact, is_value2_exact, parse_hex_bits, pow_u64, pow2_f64, step_ulps,
};
use super::steer::{FuzzCase, evaluate_traced};
use calc_func::trace::RouteTrace;
use calc_verdict::WitnessInvocation;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

/// Refinement scale in the self-similar fractal hierarchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(dead_code)]
pub enum FractalScale {
    Decade,
    Binade,
    Ulp,
    CoarseDiscrepancy,
}

impl FractalScale {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Decade => "decade",
            Self::Binade => "binade",
            Self::Ulp => "ulp",
            Self::CoarseDiscrepancy => "coarse_discrepancy",
        }
    }
}

/// Signal layer that changed across a boundary, ordered highest to lowest priority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TransitionKind {
    PredicateFlip,
    RouteSwitch,
    IterationRegime,
    OutcomeRegime,
}

impl TransitionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PredicateFlip => "predicate_flip",
            Self::RouteSwitch => "route_switch",
            Self::IterationRegime => "iteration_regime",
            Self::OutcomeRegime => "outcome_regime",
        }
    }
}

/// Coarse solver iteration bucket matching `steer::CoverageTracker`.
pub fn iteration_regime_bucket(iterations: u32) -> &'static str {
    match iterations {
        0 => "0",
        1..=5 => "1..5",
        6..=20 => "6..20",
        21..=50 => "21..50",
        51..=100 => "51..100",
        101..=199 => "101..199",
        _ => "200+",
    }
}

/// DNA Calc local state signature used to detect cheap candidate boundaries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundarySignature {
    pub predicate: String,
    pub route_mask: u64,
    pub iter_bucket: &'static str,
    pub outcome_class: String,
    pub is_fault: bool,
}

impl BoundarySignature {
    pub fn from_eval(prediction: &str, trace: &RouteTrace, outcome_class: &str) -> Self {
        Self {
            predicate: predicate_tag(prediction),
            route_mask: trace.route_mask,
            iter_bucket: iteration_regime_bucket(trace.iterations),
            outcome_class: outcome_class.to_owned(),
            is_fault: is_dna_fault(prediction),
        }
    }

    /// Highest-priority signal that changed between `self` and `other`, if any.
    /// Transitions involving a DNA-side harness fault (`fault:*`) are never boundaries.
    pub fn diff(&self, other: &Self) -> Option<TransitionKind> {
        if self.is_fault || other.is_fault {
            return None;
        }
        if self.predicate != other.predicate {
            Some(TransitionKind::PredicateFlip)
        } else if self.route_mask != other.route_mask {
            Some(TransitionKind::RouteSwitch)
        } else if self.iter_bucket != other.iter_bucket {
            Some(TransitionKind::IterationRegime)
        } else if self.outcome_class != other.outcome_class {
            Some(TransitionKind::OutcomeRegime)
        } else {
            None
        }
    }

    /// Equivalence key for bisecting a specific [`TransitionKind`].
    pub fn key_for(&self, kind: TransitionKind) -> String {
        match kind {
            TransitionKind::PredicateFlip => self.predicate.clone(),
            TransitionKind::RouteSwitch => format!("{}|{:x}", self.predicate, self.route_mask),
            TransitionKind::IterationRegime => {
                format!("{}|{:x}|{}", self.predicate, self.route_mask, self.iter_bucket)
            }
            TransitionKind::OutcomeRegime => format!(
                "{}|{:x}|{}|{}",
                self.predicate, self.route_mask, self.iter_bucket, self.outcome_class
            ),
        }
    }
}

/// Configuration for deterministic local fractal boundary discovery.
#[derive(Clone, Debug)]
pub struct FractalConfig {
    /// Maximum recursive depth (`0` = 1D Decade->Binade->ULP only; `1` = edges of edges; `2` = two levels).
    pub max_depth: usize,
    /// Total maximum frontiers to emit across all depths.
    pub max_frontiers: usize,
    /// Hard cap on local DNA Calc evaluations so fractal discovery stays fast and bounded.
    pub max_local_evals: usize,
    /// Maximum child boundaries discovered per parent edge at each recursive step.
    pub max_children_per_node: usize,
}

impl FractalConfig {
    pub fn new(max_frontiers: usize, max_depth: usize) -> Self {
        Self {
            max_depth,
            max_frontiers,
            max_local_evals: (max_frontiers.saturating_mul(250)).clamp(600, 24_000),
            max_children_per_node: 3,
        }
    }
}

/// Aggregate telemetry for a local fractal refinement pass.
#[derive(Clone, Debug, Default)]
pub struct FractalDiscoveryStats {
    pub local_evals: usize,
    pub decade_brackets_found: usize,
    pub binade_brackets_found: usize,
    pub ulp_frontiers_pinpointed: usize,
    pub by_depth: BTreeMap<usize, usize>,
    pub by_scale: BTreeMap<String, usize>,
    pub by_transition: BTreeMap<String, usize>,
}

impl FractalDiscoveryStats {
    pub fn record_frontier(&mut self, f: &Frontier) {
        *self.by_depth.entry(f.depth).or_insert(0) += 1;
        *self.by_scale.entry(f.scale.clone()).or_insert(0) += 1;
        *self.by_transition.entry(f.transition_kind.clone()).or_insert(0) += 1;
    }

    pub fn to_json(&self) -> Value {
        json!({
            "local_evals": self.local_evals,
            "decade_brackets_found": self.decade_brackets_found,
            "binade_brackets_found": self.binade_brackets_found,
            "ulp_frontiers_pinpointed": self.ulp_frontiers_pinpointed,
            "by_depth": self.by_depth,
            "by_scale": self.by_scale,
            "by_transition": self.by_transition,
        })
    }
}

/// Output of [`fractal_discover_local`].
#[derive(Clone, Debug)]
pub struct FractalDiscoveryResult {
    pub frontiers: Vec<Frontier>,
    pub stats: FractalDiscoveryStats,
}

/// Build a deterministic, sorted, Value2-exact **Decade-scale** stencil around `base_val`,
/// augmented with any hint-file edge anchors for the axis.
pub fn decade_stencil(base_val: f64, hint_anchors: &[f64]) -> Vec<f64> {
    let mut pts: Vec<f64> = Vec::with_capacity(64 + hint_anchors.len());
    let mut push_safe = |v: f64| {
        if is_value2_exact(v) {
            pts.push(if v == 0.0 { 0.0 } else { v });
        }
    };

    push_safe(base_val);
    for &a in &[0.0, -1.0, 1.0, -f64::MIN_POSITIVE, f64::MIN_POSITIVE] {
        push_safe(a);
    }
    for &h in hint_anchors {
        push_safe(h);
        if let Some(prev) = step_ulps(h, -1, true) {
            push_safe(prev);
        }
        if let Some(next) = step_ulps(h, 1, true) {
            push_safe(next);
        }
    }

    // Multiplicative decades around base_val (pure IEEE-754 integer powers of 10, no libm).
    if base_val != 0.0 && base_val.is_finite() {
        for &m in &[-100.0, -10.0, -2.0, -1.0, -0.5, -0.1, 0.1, 0.5, 0.9, 0.99, 1.01, 1.1, 2.0, 10.0, 100.0] {
            push_safe(base_val * m);
        }
        for exp in 1u64..=6 {
            let p10 = pow_u64(10.0, exp);
            push_safe(base_val * p10);
            push_safe(base_val / p10);
            push_safe(-base_val * p10);
            push_safe(-base_val / p10);
        }
    }

    // Additive decades around base_val.
    for exp in 0u64..=4 {
        let p10 = pow_u64(10.0, exp);
        push_safe(base_val + p10);
        push_safe(base_val - p10);
    }
    for exp in 1u64..=9 {
        let step = 1.0 / pow_u64(10.0, exp);
        push_safe(base_val + step);
        push_safe(base_val - step);
    }

    pts.sort_by_key(|&v| f64_to_compact(v));
    pts.dedup_by_key(|v| f64_to_compact(*v));
    pts
}

/// Extract unbiased IEEE-754 binary exponent for a nonzero normal finite double (`[-1022, 1023]`).
#[inline]
fn binary_exponent(v: f64) -> Option<i32> {
    if !v.is_finite() || v == 0.0 || v.is_subnormal() {
        return None;
    }
    Some(((v.to_bits() >> 52) & 0x7ff) as i32 - 1023)
}

/// Build a deterministic, sorted, Value2-exact **Binade-scale** stencil inside `[lo, hi]`
/// (including exact `±2^e` binade edges, `2^e - 1 ULP` tops, `1.5 * 2^e` mid-binades, and
/// compact-order intra-binade subdivisions).
pub fn binade_stencil(lo: f64, hi: f64) -> Vec<f64> {
    let ca = f64_to_compact(lo);
    let cb = f64_to_compact(hi);
    if cb <= ca {
        return Vec::new();
    }
    let mut pts: Vec<f64> = Vec::with_capacity(48);
    let mut push_in = |v: f64| {
        if is_value2_exact(v) {
            let c = f64_to_compact(v);
            if c >= ca && c <= cb {
                pts.push(if v == 0.0 { 0.0 } else { v });
            }
        }
    };

    push_in(lo);
    push_in(hi);
    for &z in &[-f64::MIN_POSITIVE, 0.0, f64::MIN_POSITIVE] {
        push_in(z);
    }

    // Add binade boundaries on the negative side and positive side of [lo, hi].
    let mut add_binades_for_sign = |sign: f64, min_abs: f64, max_abs: f64| {
        if max_abs < f64::MIN_POSITIVE {
            return;
        }
        let e_lo = binary_exponent(min_abs.max(f64::MIN_POSITIVE)).unwrap_or(-1022);
        let e_hi = binary_exponent(max_abs.min(f64::MAX)).unwrap_or(1023);
        if e_hi < e_lo {
            return;
        }
        let span = (e_hi - e_lo) as usize;
        let stride = (span / 12).max(1) as i32;
        let mut e = e_lo;
        while e <= e_hi {
            let p2 = pow2_f64(e);
            if is_value2_exact(p2) {
                let s = sign * p2;
                push_in(s);
                if let Some(prev) = step_ulps(s, -1, true) {
                    push_in(prev);
                }
                if let Some(next) = step_ulps(s, 1, true) {
                    push_in(next);
                }
                push_in(sign * (p2 * 1.5));
            }
            if e == e_hi {
                break;
            }
            e = (e + stride).min(e_hi);
        }
    };

    if lo < 0.0 {
        let neg_min_abs = if hi < 0.0 { -hi } else { f64::MIN_POSITIVE };
        let neg_max_abs = -lo;
        add_binades_for_sign(-1.0, neg_min_abs, neg_max_abs);
    }
    if hi > 0.0 {
        let pos_min_abs = if lo > 0.0 { lo } else { f64::MIN_POSITIVE };
        let pos_max_abs = hi;
        add_binades_for_sign(1.0, pos_min_abs, pos_max_abs);
    }

    // Also add 8 evenly spaced compact-order subdivisions so intra-binade edges are refined.
    let span = (cb - ca) as u128;
    let divs = span.saturating_sub(1).min(8);
    for j in 1..=divs {
        let c = ca + ((span * j) / (divs + 1)) as i128;
        if let Some(v) = compact_to_f64(c) {
            push_in(v);
        }
    }

    pts.sort_by_key(|&v| f64_to_compact(v));
    pts.dedup_by_key(|v| f64_to_compact(*v));
    pts
}

/// Build a deterministic **ULP-scale** stencil around an anchor value (`±{1,2,4,8,16,64,256,1024}`).
pub fn ulp_stencil(anchor: f64) -> Vec<f64> {
    const OFFSETS: &[i64] = &[
        -1024, -256, -64, -16, -8, -4, -2, -1, 0, 1, 2, 4, 8, 16, 64, 256, 1024,
    ];
    let mut pts = Vec::with_capacity(OFFSETS.len());
    for &d in OFFSETS {
        if let Some(v) = step_ulps(anchor, d, true)
            && is_value2_exact(v)
        {
            pts.push(if v == 0.0 { 0.0 } else { v });
        }
    }
    pts.sort_by_key(|&v| f64_to_compact(v));
    pts.dedup_by_key(|v| f64_to_compact(*v));
    pts
}

/// Exact Value2-compact binary search on DNA Calc along `(axis, cell)` between `lo_val` and
/// `hi_val` targeting `kind` (`PredicateFlip`, `RouteSwitch`, `IterationRegime`, or `OutcomeRegime`).
///
/// Converges in at most 64 steps to exact adjacent Value2-safe doubles (`compact(hi) == compact(lo) + 1`).
#[allow(clippy::too_many_arguments)]
pub fn bisect_signature_axis_cell(
    function: &str,
    base_args: &[Value],
    axis: usize,
    cell: Option<(usize, usize)>,
    lo_val: f64,
    hi_val: f64,
    kind: TransitionKind,
    context: &WitnessInvocation,
    eval_counter: &mut usize,
) -> Option<(u64, String, BoundarySignature, u64, String, BoundarySignature)> {
    if !lo_val.is_finite() || !hi_val.is_finite() || lo_val == hi_val || axis >= base_args.len() {
        return None;
    }
    let (mut a, mut b) = if f64_to_compact(lo_val) < f64_to_compact(hi_val) {
        (lo_val, hi_val)
    } else {
        (hi_val, lo_val)
    };
    let mut eval_at = |x: f64| -> (String, BoundarySignature) {
        let mut args = base_args.to_vec();
        set_axis_f64(&mut args, axis, cell, x);
        *eval_counter += 1;
        let (res, trace, oc) = evaluate_traced(function, &args, context);
        let sig = BoundarySignature::from_eval(&res, &trace, &oc);
        (res, sig)
    };
    let mut ea = eval_at(a);
    let mut eb = eval_at(b);
    if ea.1.is_fault || eb.1.is_fault {
        return None;
    }
    let ka = ea.1.key_for(kind);
    if ka == eb.1.key_for(kind) {
        return None;
    }

    for _ in 0..64 {
        let ca = f64_to_compact(a);
        let cb = f64_to_compact(b);
        if cb <= ca + 1 {
            break;
        }
        let mid_c = ca + (cb - ca) / 2;
        let Some(mid) = compact_to_f64(mid_c) else {
            break;
        };
        let em = eval_at(mid);
        if em.1.is_fault {
            return None;
        }
        if em.1.key_for(kind) == ka {
            a = mid;
            ea = em;
        } else {
            b = mid;
            eb = em;
        }
    }
    let a_bits = if a == 0.0 { 0u64 } else { a.to_bits() };
    let b_bits = if b == 0.0 { 0u64 } else { b.to_bits() };
    Some((a_bits, ea.0, ea.1, b_bits, eb.0, eb.1))
}

fn collect_probe_axes(spec: &DomainSpec) -> Vec<(usize, Option<(usize, usize)>)> {
    let mut axes = Vec::new();
    for (i, p) in spec.params.iter().enumerate() {
        if p.kind == ParamKind::ArrayNumeric {
            for r in 0..3 {
                axes.push((i, Some((r, 0))));
            }
        } else {
            axes.push((i, None));
        }
    }
    axes
}

fn collect_hint_anchors(spec: &DomainSpec) -> Vec<Vec<f64>> {
    let mut out = vec![Vec::new(); spec.params.len()];
    for (i, slot) in out.iter_mut().enumerate() {
        if let Some(arg) = spec.merged.args.get(i) {
            for edge in &arg.edges {
                if let Some(f) = edge
                    .value
                    .as_f64()
                    .or_else(|| parse_hex_bits(&edge.value).map(f64::from_bits))
                    && is_value2_exact(f)
                {
                    slot.push(if f == 0.0 { 0.0 } else { f });
                }
            }
            for sp in &arg.specials {
                if let Some(bits) = parse_hex_bits(sp) {
                    let f = f64::from_bits(bits);
                    if is_value2_exact(f) {
                        slot.push(if f == 0.0 { 0.0 } else { f });
                    }
                }
            }
            slot.sort_by_key(|&v| f64_to_compact(v));
            slot.dedup_by_key(|v| f64_to_compact(*v));
        }
    }
    out
}

fn axis_display_name(spec: &DomainSpec, axis: usize, cell: Option<(usize, usize)>) -> String {
    let base_name = spec
        .params
        .get(axis)
        .map_or_else(|| format!("arg{axis}"), |p| p.name.clone());
    match cell {
        None => base_name,
        Some((r, 0)) => format!("{base_name}[{r}]"),
        Some((r, c)) => format!("{base_name}[{r},{c}]"),
    }
}

fn format_frontier_kind(
    lo_sig: &BoundarySignature,
    hi_sig: &BoundarySignature,
    tk: TransitionKind,
) -> String {
    if lo_sig.predicate != hi_sig.predicate {
        format!("{}<->{}", lo_sig.predicate, hi_sig.predicate)
    } else {
        match tk {
            TransitionKind::PredicateFlip => format!("{}<->{}", lo_sig.predicate, hi_sig.predicate),
            TransitionKind::RouteSwitch => "route_flip".to_owned(),
            TransitionKind::IterationRegime => {
                format!("iter:{}<->{}", lo_sig.iter_bucket, hi_sig.iter_bucket)
            }
            TransitionKind::OutcomeRegime => {
                format!("regime:{}<->{}", lo_sig.outcome_class, hi_sig.outcome_class)
            }
        }
    }
}

/// Run one 1D self-similar **Decade -> Binade -> ULP** local refinement sweep along `(axis, cell)`
/// starting from `base_args`, returning up to `limit` pinpointed adjacent-double boundaries.
#[allow(clippy::too_many_arguments)]
fn refine_axis_multiscale(
    spec: &DomainSpec,
    base_args: &[Value],
    axis: usize,
    cell: Option<(usize, usize)>,
    hint_anchors: &[f64],
    context: &WitnessInvocation,
    allow_non_predicate: bool,
    depth: usize,
    parent_id: Option<&str>,
    limit: usize,
    max_evals: usize,
    seen_frontiers: &mut HashSet<String>,
    stats: &mut FractalDiscoveryStats,
    next_frontier_num: &mut usize,
) -> Vec<Frontier> {
    let mut found = Vec::new();
    if limit == 0 || stats.local_evals >= max_evals {
        return found;
    }
    let Some(base_bits) = get_axis_bits(base_args, axis, cell) else {
        return found;
    };
    let base_val = f64::from_bits(base_bits);
    if !base_val.is_finite() {
        return found;
    }

    // Scale 1: Decade stencil
    let decade_pts = decade_stencil(base_val, hint_anchors);
    let mut decade_evals: Vec<(f64, BoundarySignature)> = Vec::with_capacity(decade_pts.len());
    for &x in &decade_pts {
        if stats.local_evals >= max_evals {
            break;
        }
        let mut args = base_args.to_vec();
        set_axis_f64(&mut args, axis, cell, x);
        stats.local_evals += 1;
        let (res, trace, oc) = evaluate_traced(&spec.function, &args, context);
        decade_evals.push((x, BoundarySignature::from_eval(&res, &trace, &oc)));
    }

    // Collect Decade brackets that exhibit a signal transition, prioritized by TransitionKind.
    let mut decade_brackets: Vec<(TransitionKind, f64, f64)> = Vec::new();
    for pair in decade_evals.windows(2) {
        if let Some(tk) = pair[0].1.diff(&pair[1].1)
            && (allow_non_predicate || tk == TransitionKind::PredicateFlip)
        {
            decade_brackets.push((tk, pair[0].0, pair[1].0));
        }
    }
    decade_brackets.sort_by_key(|(tk, lo, _)| {
        let dist = (f64_to_compact(*lo) - f64_to_compact(base_val)).unsigned_abs();
        (*tk, dist)
    });
    stats.decade_brackets_found += decade_brackets.len();

    // Scale 2: Binade refinement inside each Decade bracket (capped at 4 brackets per axis sweep)
    for (dec_tk, dec_lo, dec_hi) in decade_brackets.into_iter().take(4) {
        if found.len() >= limit || stats.local_evals >= max_evals {
            break;
        }
        let binade_pts = binade_stencil(dec_lo, dec_hi);
        let mut binade_evals: Vec<(f64, BoundarySignature)> = Vec::with_capacity(binade_pts.len());
        for &x in &binade_pts {
            if stats.local_evals >= max_evals {
                break;
            }
            let mut args = base_args.to_vec();
            set_axis_f64(&mut args, axis, cell, x);
            stats.local_evals += 1;
            let (res, trace, oc) = evaluate_traced(&spec.function, &args, context);
            binade_evals.push((x, BoundarySignature::from_eval(&res, &trace, &oc)));
        }

        let mut binade_brackets: Vec<(TransitionKind, f64, f64)> = Vec::new();
        for pair in binade_evals.windows(2) {
            if let Some(tk) = pair[0].1.diff(&pair[1].1)
                && (allow_non_predicate || tk == TransitionKind::PredicateFlip)
            {
                binade_brackets.push((tk, pair[0].0, pair[1].0));
            }
        }
        if binade_brackets.is_empty() && !binade_evals.iter().any(|(_, sig)| sig.is_fault) {
            binade_brackets.push((dec_tk, dec_lo, dec_hi));
        }
        binade_brackets.sort_by_key(|(tk, lo, _)| {
            let dist = (f64_to_compact(*lo) - f64_to_compact(base_val)).unsigned_abs();
            (*tk, dist)
        });
        stats.binade_brackets_found += binade_brackets.len();

        // Scale 3: ULP exact bisection on each distinct Binade sub-bracket
        for (bin_tk, bin_lo, bin_hi) in binade_brackets.into_iter().take(2) {
            if found.len() >= limit || stats.local_evals >= max_evals {
                break;
            }
            let Some((lo_bits, lo_out, lo_sig, hi_bits, hi_out, hi_sig)) =
                bisect_signature_axis_cell(
                    &spec.function,
                    base_args,
                    axis,
                    cell,
                    bin_lo,
                    bin_hi,
                    bin_tk,
                    context,
                    &mut stats.local_evals,
                )
            else {
                continue;
            };
            if lo_sig.is_fault || hi_sig.is_fault {
                continue;
            }
            let Some(actual_tk) = lo_sig.diff(&hi_sig) else {
                continue;
            };
            if !allow_non_predicate && actual_tk != TransitionKind::PredicateFlip {
                continue;
            }
            let kind = format_frontier_kind(&lo_sig, &hi_sig, actual_tk);
            let coarse_key = format!(
                "{depth}|{axis}|{cell:?}|{:08x}|{kind}|{}",
                lo_bits >> 32,
                parent_id.unwrap_or("")
            );
            if !seen_frontiers.insert(coarse_key) {
                continue;
            }
            let axis_name = axis_display_name(spec, axis, cell);
            *next_frontier_num += 1;
            let frontier = Frontier {
                id: format!("frontier-{}-{:03}", spec.function, *next_frontier_num),
                function: spec.function.clone(),
                axis,
                axis_cell: cell,
                axis_name,
                kind,
                base_args: base_args.to_vec(),
                dna_lo_bits: lo_bits,
                dna_hi_bits: hi_bits,
                dna_lo_outcome: lo_out,
                dna_hi_outcome: hi_out,
                excel_lo_bits: None,
                excel_hi_bits: None,
                excel_lo_outcome: None,
                excel_hi_outcome: None,
                shifted_ulps: None,
                depth,
                scale: FractalScale::Ulp.as_str().to_owned(),
                transition_kind: actual_tk.as_str().to_owned(),
                parent_id: parent_id.map(str::to_owned),
            };
            stats.ulp_frontiers_pinpointed += 1;
            stats.record_frontier(&frontier);
            found.push(frontier);
        }
    }

    found
}

/// Deterministic DNA-Calc-steered fractal boundary discovery:
/// 1. Stratifies `seeds` across regions and high-interest traces (small margin / high iterations).
/// 2. At depth `0`, sweeps numeric axes via **Decade -> Binade -> ULP** refinement (PredicateFlip
///    first, then RouteSwitch / IterationRegime / OutcomeRegime).
/// 3. At depths `1..=cfg.max_depth`, pins each parent boundary right on its adjacent-double edge
///    (`dna_lo_bits` and `dna_hi_bits`) and recurses the same **Decade -> Binade -> ULP** refinement
///    along secondary axes `B != A` to find the "edges of the edges".
pub fn fractal_discover_local(
    spec: &DomainSpec,
    seeds: &[FuzzCase],
    context: &WitnessInvocation,
    cfg: &FractalConfig,
) -> FractalDiscoveryResult {
    let mut stats = FractalDiscoveryStats::default();
    let mut frontiers: Vec<Frontier> = Vec::new();
    if seeds.is_empty() || cfg.max_frontiers == 0 {
        return FractalDiscoveryResult { frontiers, stats };
    }
    let probe_axes = collect_probe_axes(spec);
    if probe_axes.is_empty() {
        return FractalDiscoveryResult { frontiers, stats };
    }
    let hint_anchors = collect_hint_anchors(spec);
    let mut seen_frontiers: HashSet<String> = HashSet::new();
    let mut next_frontier_num = 0usize;

    // Reserve slots for recursive depth >= 1 children when max_depth > 0 and > 1 numeric axis exists.
    let reserve_recursive = if cfg.max_depth > 0 && probe_axes.len() > 1 && cfg.max_frontiers >= 4 {
        (cfg.max_frontiers / 3).max(1)
    } else {
        0
    };
    let root_cap = cfg.max_frontiers.saturating_sub(reserve_recursive).max(1);

    // Stratify seeds across regions and sort by boundary proximity & solver iterations.
    let mut by_region: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (idx, s) in seeds.iter().enumerate() {
        by_region.entry(s.region.as_str()).or_default().push(idx);
    }
    for indices in by_region.values_mut() {
        indices.sort_by(|&i, &j| {
            let sa = &seeds[i];
            let sb = &seeds[j];
            let ka = (sa.trace.min_boundary_margin.is_finite(), sa.trace.iterations);
            let kb = (sb.trace.min_boundary_margin.is_finite(), sb.trace.iterations);
            kb.0.cmp(&ka.0)
                .then_with(|| kb.1.cmp(&ka.1))
                .then_with(|| sa.trace.min_boundary_margin.total_cmp(&sb.trace.min_boundary_margin))
                .then_with(|| i.cmp(&j))
        });
    }
    let mut order: Vec<usize> = Vec::with_capacity(seeds.len());
    let mut pos = 0usize;
    let probe_limit = seeds.len().min(cfg.max_frontiers * 12).max(48);
    while order.len() < probe_limit {
        let mut added = false;
        for indices in by_region.values() {
            if let Some(&idx) = indices.get(pos) {
                order.push(idx);
                added = true;
                if order.len() >= probe_limit {
                    break;
                }
            }
        }
        if !added {
            break;
        }
        pos += 1;
    }

    let max_per_axis = ((root_cap + probe_axes.len() - 1) / probe_axes.len()).max(2);
    let mut per_axis_count: BTreeMap<(usize, Option<(usize, usize)>), usize> = BTreeMap::new();
    let mut axis_cursor = 0usize;

    // Depth 0: two passes (predicate flips first, then route/iteration/regime switches).
    for allow_non_predicate in [false, true] {
        if frontiers.len() >= root_cap || stats.local_evals >= cfg.max_local_evals {
            break;
        }
        for &seed_idx in &order {
            if frontiers.len() >= root_cap || stats.local_evals >= cfg.max_local_evals {
                break;
            }
            let seed = &seeds[seed_idx];
            let mut chosen_axis = None;
            for offset in 0..probe_axes.len() {
                let ax = probe_axes[(axis_cursor + offset) % probe_axes.len()];
                if get_axis_bits(&seed.args, ax.0, ax.1).is_some()
                    && (*per_axis_count.get(&ax).unwrap_or(&0) < max_per_axis
                        || allow_non_predicate)
                {
                    chosen_axis = Some(ax);
                    axis_cursor = (axis_cursor + offset + 1) % probe_axes.len();
                    break;
                }
            }
            let Some((axis, cell)) = chosen_axis else {
                continue;
            };
            let anchors = hint_anchors.get(axis).map_or(&[][..], Vec::as_slice);
            let remaining = root_cap.saturating_sub(frontiers.len()).min(2);
            let discovered = refine_axis_multiscale(
                spec,
                &seed.args,
                axis,
                cell,
                anchors,
                context,
                allow_non_predicate,
                0,
                None,
                remaining,
                cfg.max_local_evals,
                &mut seen_frontiers,
                &mut stats,
                &mut next_frontier_num,
            );
            for f in discovered {
                *per_axis_count.entry((f.axis, f.axis_cell)).or_insert(0) += 1;
                frontiers.push(f);
                if frontiers.len() >= root_cap {
                    break;
                }
            }
        }
    }

    // Depths 1..=max_depth: recursive self-similar "edges of the edges" along secondary axes.
    let mut level_start = 0usize;
    let mut level_end = frontiers.len();
    for depth in 1..=cfg.max_depth {
        if frontiers.len() >= cfg.max_frontiers
            || stats.local_evals >= cfg.max_local_evals
            || level_start >= level_end
        {
            break;
        }
        let parents: Vec<Frontier> = frontiers[level_start..level_end].to_vec();
        level_start = frontiers.len();
        for parent in &parents {
            if frontiers.len() >= cfg.max_frontiers || stats.local_evals >= cfg.max_local_evals {
                break;
            }
            let mut children_for_parent = 0usize;
            // Pin the parent axis at BOTH sides of its exact adjacent-double edge.
            for &pinned_bits in &[parent.dna_lo_bits, parent.dna_hi_bits] {
                if children_for_parent >= cfg.max_children_per_node
                    || frontiers.len() >= cfg.max_frontiers
                    || stats.local_evals >= cfg.max_local_evals
                {
                    break;
                }
                let mut edge_args = parent.base_args.clone();
                if !set_axis_f64(
                    &mut edge_args,
                    parent.axis,
                    parent.axis_cell,
                    f64::from_bits(pinned_bits),
                ) {
                    continue;
                }
                // Sweep secondary numeric axes B != A around this pinned edge point.
                for &(b_axis, b_cell) in &probe_axes {
                    if (b_axis, b_cell) == (parent.axis, parent.axis_cell) {
                        continue;
                    }
                    if children_for_parent >= cfg.max_children_per_node
                        || frontiers.len() >= cfg.max_frontiers
                        || stats.local_evals >= cfg.max_local_evals
                    {
                        break;
                    }
                    if get_axis_bits(&edge_args, b_axis, b_cell).is_none() {
                        continue;
                    }
                    let anchors = hint_anchors.get(b_axis).map_or(&[][..], Vec::as_slice);
                    let remaining = (cfg.max_children_per_node - children_for_parent)
                        .min(cfg.max_frontiers - frontiers.len())
                        .min(1);
                    let children = refine_axis_multiscale(
                        spec,
                        &edge_args,
                        b_axis,
                        b_cell,
                        anchors,
                        context,
                        true,
                        depth,
                        Some(&parent.id),
                        remaining,
                        cfg.max_local_evals,
                        &mut seen_frontiers,
                        &mut stats,
                        &mut next_frontier_num,
                    );
                    for child in children {
                        children_for_parent += 1;
                        frontiers.push(child);
                        if frontiers.len() >= cfg.max_frontiers {
                            break;
                        }
                    }
                }
            }
        }
        level_end = frontiers.len();
    }

    // If recursive refinement left unused frontier slots, top up at depth 0 across remaining seeds.
    if frontiers.len() < cfg.max_frontiers && stats.local_evals < cfg.max_local_evals {
        for &seed_idx in &order {
            if frontiers.len() >= cfg.max_frontiers || stats.local_evals >= cfg.max_local_evals {
                break;
            }
            let seed = &seeds[seed_idx];
            for &(axis, cell) in &probe_axes {
                if frontiers.len() >= cfg.max_frontiers || stats.local_evals >= cfg.max_local_evals {
                    break;
                }
                if get_axis_bits(&seed.args, axis, cell).is_none() {
                    continue;
                }
                let anchors = hint_anchors.get(axis).map_or(&[][..], Vec::as_slice);
                let remaining = (cfg.max_frontiers - frontiers.len()).min(1);
                let extra = refine_axis_multiscale(
                    spec,
                    &seed.args,
                    axis,
                    cell,
                    anchors,
                    context,
                    true,
                    0,
                    None,
                    remaining,
                    cfg.max_local_evals,
                    &mut seen_frontiers,
                    &mut stats,
                    &mut next_frontier_num,
                );
                frontiers.extend(extra);
            }
        }
    }

    FractalDiscoveryResult { frontiers, stats }
}

/// Compact Decade + Binade probe values around a coarse Excel discrepancy coordinate `anchor`,
/// used when Excel disagreed in a smooth DNA Calc region to bracket Excel's hidden flip.
pub fn coarse_discrepancy_probe_points(anchor: f64) -> Vec<f64> {
    let mut pts: Vec<f64> = Vec::with_capacity(48);
    let mut push_safe = |v: f64| {
        if is_value2_exact(v) {
            pts.push(if v == 0.0 { 0.0 } else { v });
        }
    };
    push_safe(anchor);
    // ULP & Binade-scale local steps
    for &d in &[
        -1_099_511_627_776_i64,
        -16_777_216,
        -65_536,
        -1_024,
        -64,
        -4,
        -1,
        1,
        4,
        64,
        1_024,
        65_536,
        16_777_216,
        1_099_511_627_776,
    ] {
        if let Some(v) = step_ulps(anchor, d, true) {
            push_safe(v);
        }
    }
    // Binade boundaries adjacent to anchor
    if let Some(e) = binary_exponent(anchor) {
        let sign = if anchor < 0.0 { -1.0 } else { 1.0 };
        for de in [-2, -1, 0, 1, 2] {
            let p = pow2_f64((e + de).clamp(-1022, 1023));
            push_safe(sign * p);
            if let Some(prev) = step_ulps(sign * p, -1, true) {
                push_safe(prev);
            }
        }
    }
    // Decade-scale steps
    if anchor != 0.0 && anchor.is_finite() {
        for &m in &[-1.0, 0.1, 0.5, 0.9, 1.1, 2.0, 10.0] {
            push_safe(anchor * m);
        }
    }
    for &a in &[-1.0, -0.1, -0.01, -0.001, 0.001, 0.01, 0.1, 1.0] {
        push_safe(anchor + a);
    }
    push_safe(0.0);
    pts.sort_by_key(|&v| f64_to_compact(v));
    pts.dedup_by_key(|v| f64_to_compact(*v));
    pts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuzz::rng::{Rng, hex_f64};
    use crate::fuzz::steer::{CoverageTracker, generate_steered_round, touchstone_bulk_context};

    #[test]
    fn multiscale_stencils_are_sorted_unique_and_value2_exact() {
        let dec = decade_stencil(0.1, &[0.0, -1.0]);
        assert!(dec.len() >= 25);
        for w in dec.windows(2) {
            assert!(is_value2_exact(w[0]) && is_value2_exact(w[1]));
            assert!(f64_to_compact(w[0]) < f64_to_compact(w[1]));
        }
        assert!(dec.contains(&0.0));
        assert!(dec.contains(&-1.0));
        assert!(dec.contains(&0.1));

        let bin = binade_stencil(0.25, 4.0);
        assert!(bin.len() >= 10);
        assert!(bin.contains(&0.5));
        assert!(bin.contains(&1.0));
        assert!(bin.contains(&2.0));
        for w in bin.windows(2) {
            assert!(is_value2_exact(w[0]) && is_value2_exact(w[1]));
            assert!(f64_to_compact(w[0]) < f64_to_compact(w[1]));
        }

        let ulp = ulp_stencil(1.0);
        assert!(ulp.contains(&1.0));
        assert!(ulp.contains(&step_ulps(1.0, 1, true).unwrap()));
        assert!(ulp.contains(&step_ulps(1.0, -1, true).unwrap()));
    }

    #[test]
    fn fractal_discovery_finds_root_and_edge_of_edge_frontiers_deterministically() {
        let spec = DomainSpec::for_function("RATE").unwrap();
        let ctx = touchstone_bulk_context();
        let weights: Vec<f64> = spec.regions.iter().map(|r| r.base_weight).collect();
        let mut tracker = CoverageTracker::new("RATE");
        let mut seen = HashSet::new();
        let mut next_id = 0;
        let mut rng = Rng::from_seed_and_tag(20261124, "RATE");
        let seeds = generate_steered_round(
            &spec,
            &weights,
            &[],
            &mut tracker,
            &mut seen,
            &ctx,
            0,
            250,
            &mut next_id,
            &mut rng,
        );

        let fcfg = FractalConfig::new(18, 2);
        let r1 = fractal_discover_local(&spec, &seeds, &ctx, &fcfg);
        let r2 = fractal_discover_local(&spec, &seeds, &ctx, &fcfg);

        assert!(!r1.frontiers.is_empty(), "expected discovered frontiers");
        assert!(r1.stats.decade_brackets_found > 0, "expected decade brackets");
        assert!(r1.stats.binade_brackets_found > 0, "expected binade brackets");
        assert!(r1.stats.ulp_frontiers_pinpointed > 0, "expected ULP frontiers");

        // Verify exact adjacency on every discovered frontier and presence of depth >= 1 edge-of-edge children.
        let mut has_depth0 = false;
        let mut has_edge_of_edge = false;
        for f in &r1.frontiers {
            let ca = f64_to_compact(f64::from_bits(f.dna_lo_bits));
            let cb = f64_to_compact(f64::from_bits(f.dna_hi_bits));
            assert_eq!(cb, ca + 1, "frontier {} must be adjacent Value2 doubles", f.id);
            if f.depth == 0 {
                has_depth0 = true;
                assert!(f.parent_id.is_none());
            } else {
                has_edge_of_edge = true;
                assert!(f.parent_id.is_some(), "child frontier {} missing parent_id", f.id);
            }
        }
        assert!(has_depth0, "expected depth-0 frontiers");
        assert!(
            has_edge_of_edge,
            "expected recursive depth>=1 edge-of-edge frontiers, stats={:?}",
            r1.stats
        );

        // Strict determinism check.
        assert_eq!(r1.frontiers.len(), r2.frontiers.len());
        for (a, b) in r1.frontiers.iter().zip(&r2.frontiers) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.axis, b.axis);
            assert_eq!(a.depth, b.depth);
            assert_eq!(a.parent_id, b.parent_id);
            assert_eq!(a.dna_lo_bits, b.dna_lo_bits);
            assert_eq!(a.dna_hi_bits, b.dna_hi_bits);
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.transition_kind, b.transition_kind);
        }
    }

    #[test]
    fn fractal_discovery_uses_hint_edges_on_hinted_function_pv() {
        let spec = DomainSpec::for_function("PV").unwrap();
        assert!(spec.has_hint_file, "expected financial-tvm.json hint file for PV");
        let ctx = touchstone_bulk_context();
        let seed_args = vec![
            hex_f64(0.05),
            hex_f64(10.0),
            hex_f64(-100.0),
            hex_f64(0.0),
            hex_f64(0.0),
        ];
        let (pred, trace, oc) = evaluate_traced("PV", &seed_args, &ctx);
        let seeds = vec![FuzzCase {
            id: "seed-pv".to_owned(),
            region: "hint_anchors_and_edges".to_owned(),
            round: 0,
            args: seed_args,
            prediction: pred,
            trace,
            outcome_class: oc,
        }];
        let res = fractal_discover_local(&spec, &seeds, &ctx, &FractalConfig::new(8, 1));
        // PV(rate, 10, -100, 0, 0) has a sharp 1-point #DIV/0! singularity at rate = -1.0 (axis 0)
        // documented in financial-tvm.json: both sides (-1.0 - 1 ULP -> -1.0 and -1.0 -> -1.0 + 1 ULP)
        // must be discovered by the hint-anchored Decade -> Binade -> ULP fractal sweep!
        let found_rate_neg_one_right = res.frontiers.iter().any(|f| {
            f.axis == 0
                && f.dna_lo_bits == (-1.0_f64).to_bits()
                && f.dna_lo_outcome == "error:Div0"
                && f.dna_hi_outcome.starts_with("0x")
        });
        let found_rate_neg_one_left = res.frontiers.iter().any(|f| {
            f.axis == 0
                && f.dna_hi_bits == (-1.0_f64).to_bits()
                && f.dna_lo_outcome.starts_with("0x")
                && f.dna_hi_outcome == "error:Div0"
        });
        assert!(
            found_rate_neg_one_right && found_rate_neg_one_left,
            "expected PV rate=-1.0 #DIV/0!<->number singularity on both sides from hint-anchored fractal sweep; got {:?}",
            res.frontiers
                .iter()
                .map(|f| (&f.axis_name, format!("0x{:016x}", f.dna_lo_bits), &f.kind, f.depth))
                .collect::<Vec<_>>()
        );
        // And depth >= 1 edge-of-edge refinement pinned at rate = -1.0 discovers secondary nper edges!
        assert!(
            res.frontiers.iter().any(|f| f.depth >= 1 && f.parent_id.is_some()),
            "expected depth>=1 edge-of-edge child frontiers on PV"
        );
    }

    #[test]
    fn dna_fault_transitions_are_ignored_by_signature_bisection_and_fractal() {
        use super::super::cluster::is_dna_fault;
        use serde_json::json;

        let ok_sig = BoundarySignature::from_eval("0x3ff0000000000000", &RouteTrace::default(), "number");
        let err_sig = BoundarySignature::from_eval("error:Ref", &RouteTrace::default(), "error:Ref");
        let fault_a = BoundarySignature::from_eval(
            "fault:WitnessReferencePosition",
            &RouteTrace::default(),
            "fault",
        );
        let fault_b = BoundarySignature::from_eval(
            "fault:NotImplemented",
            &RouteTrace::default(),
            "fault",
        );
        assert_eq!(ok_sig.diff(&fault_a), None, "number<->fault must not be a transition");
        assert_eq!(err_sig.diff(&fault_a), None, "error<->fault must not be a transition");
        assert_eq!(fault_a.diff(&fault_b), None, "fault<->fault must not be a transition");

        // OFFSET(range, row_offset, col_offset): row_offset = 0 -> fault:WitnessReferencePosition,
        // row_offset = -1000 -> error:Ref. Neither bisection nor fractal discovery may emit a fault frontier.
        let ctx = touchstone_bulk_context();
        let base_args = vec![
            json!({"range": [["0x3ff0000000000000"]]}),
            hex_f64(0.0),
            hex_f64(0.0),
        ];
        let mut evals = 0usize;
        let bisected = bisect_signature_axis_cell(
            "OFFSET",
            &base_args,
            1,
            None,
            -1000.0,
            0.0,
            TransitionKind::PredicateFlip,
            &ctx,
            &mut evals,
        );
        assert!(
            bisected.is_none(),
            "bisect_signature_axis_cell must reject brackets with fault:* endpoints, got {bisected:?}"
        );

        let spec = DomainSpec::for_function("OFFSET").unwrap();
        let err_args = vec![
            json!({"range": [["0x3ff0000000000000"]]}),
            hex_f64(-1000.0),
            hex_f64(0.0),
        ];
        let mut seeds = Vec::new();
        for (idx, args) in [base_args, err_args].into_iter().enumerate() {
            let (pred, trace, oc) = evaluate_traced("OFFSET", &args, &ctx);
            seeds.push(FuzzCase {
                id: format!("seed-offset-{idx}"),
                region: "boundary_test".to_owned(),
                round: 0,
                args,
                prediction: pred,
                trace,
                outcome_class: oc,
            });
        }
        let res = fractal_discover_local(&spec, &seeds, &ctx, &FractalConfig::new(8, 1));
        for f in &res.frontiers {
            assert!(
                !is_dna_fault(&f.dna_lo_outcome)
                    && !is_dna_fault(&f.dna_hi_outcome)
                    && !f.kind.contains("fault:"),
                "fractal_discover_local emitted a DNA-fault frontier: {f:?}"
            );
        }
    }
}
