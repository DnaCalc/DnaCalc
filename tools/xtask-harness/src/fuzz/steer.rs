//! Code-guided steering via `calc_func::trace` (`dc-ztvv.21`).
//!
//! Runs DNA Calc locally over oversampled candidate pools, tracks exercised kernel branches,
//! generic outcome classes, solver iteration buckets, and threshold margins, and biases each
//! round toward unexercised/rare routes and narrow predicate boundaries.

use super::domain::{DomainSpec, RegionSpec, mutate_case};
use super::rng::{Rng, canonical_case_key, parse_hex_bits};
use calc_func::trace::{RouteTrace, route_names, with_trace};
use calc_verdict::WitnessInvocation;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

/// One generated & locally predicted fuzz case.
#[derive(Clone, Debug)]
pub struct FuzzCase {
    pub id: String,
    pub region: String,
    pub round: usize,
    pub args: Vec<Value>,
    pub prediction: String,
    pub trace: RouteTrace,
    pub outcome_class: String,
}

/// Touchstone bulk invocation context matching GOVERT-OMEN (`1900`, `compat=2`, `cell_value2_bulk`).
pub fn touchstone_bulk_context() -> WitnessInvocation {
    WitnessInvocation {
        input_transport: Some("cell_value2_bulk".to_owned()),
        input_sheet_state: None,
        locale: None,
        decimal_separator: Some(".".to_owned()),
        thousands_separator: Some("\u{00a0}".to_owned()),
        date_system: Some("1900".to_owned()),
        compatibility_version: Some("2".to_owned()),
        calculation_clock: None,
        capture_date: None,
        addins_open: None,
        defined_names: None,
        cell_layout: Some("touchstone/1-bulk".to_owned()),
    }
}

/// Classify a witness-encoded result string into a coarse outcome class.
pub fn classify_outcome(encoded: &str) -> String {
    if let Some(rest) = encoded.strip_prefix("error:") {
        return format!("err:{rest}");
    }
    if let Some(rest) = encoded.strip_prefix("fault:") {
        return format!("fault:{rest}");
    }
    if encoded.starts_with("logical:") {
        return "logical".to_owned();
    }
    if encoded.starts_with("text:") {
        return "text".to_owned();
    }
    if let Some(bits) = parse_hex_bits(&Value::String(encoded.to_owned())) {
        let v = f64::from_bits(bits);
        if v == 0.0 {
            return "num:zero".to_owned();
        }
        let abs = v.abs();
        if abs < f64::MIN_POSITIVE {
            return "num:subnormal".to_owned();
        }
        if abs >= 549_755_813_888.0 {
            return "num:huge_r3".to_owned();
        }
        if abs >= 536_870_912.0 {
            return "num:huge_r2".to_owned();
        }
        if abs < 1e-6 {
            return if v < 0.0 { "num:tiny_neg" } else { "num:tiny_pos" }.to_owned();
        }
        return if v < 0.0 { "num:neg" } else { "num:pos" }.to_owned();
    }
    "other".to_owned()
}

/// Evaluate one case through the shared `witness` invocation route with branch tracing enabled.
pub fn evaluate_traced(
    function: &str,
    args: &[Value],
    context: &WitnessInvocation,
) -> (String, RouteTrace, String) {
    let (prediction, trace) = with_trace(|| {
        crate::witness::calculate_with_context(function, args, None, Some(context), true)
    });
    let outcome_class = classify_outcome(&prediction);
    (prediction, trace, outcome_class)
}

/// Tracks exercised kernel routes, generic outcome classes, solver iteration buckets, and
/// boundary-proximity exemplars across rounds.
#[derive(Clone, Debug)]
pub struct CoverageTracker {
    pub function: String,
    pub kernel_route_names: Vec<&'static str>,
    pub kernel_route_hits: Vec<usize>,
    pub outcome_hits: BTreeMap<String, usize>,
    pub iter_buckets: BTreeMap<&'static str, usize>,
    /// Best (smallest margin) exemplar case seen per boundary label.
    pub closest_by_boundary: BTreeMap<String, (f64, Vec<Value>, String)>,
    /// Exemplar parents per kernel route bit (for rare-route mutation).
    pub route_exemplars: Vec<Option<Vec<Value>>>,
}

impl CoverageTracker {
    pub fn new(function: &str) -> Self {
        let names = route_names(function).to_vec();
        let n = names.len();
        Self {
            function: function.to_owned(),
            kernel_route_names: names,
            kernel_route_hits: vec![0; n],
            outcome_hits: BTreeMap::new(),
            iter_buckets: BTreeMap::new(),
            closest_by_boundary: BTreeMap::new(),
            route_exemplars: vec![None; n],
        }
    }

    pub fn record(&mut self, args: &[Value], prediction: &str, trace: &RouteTrace, outcome_class: &str) {
        for (bit, slot) in self.kernel_route_hits.iter_mut().enumerate() {
            if trace.has_bit(bit as u32) {
                *slot += 1;
                if self.route_exemplars[bit].is_none() {
                    self.route_exemplars[bit] = Some(args.to_vec());
                }
            }
        }
        *self.outcome_hits.entry(outcome_class.to_owned()).or_insert(0) += 1;
        let bucket = match trace.iterations {
            0 => "0",
            1..=5 => "1..5",
            6..=20 => "6..20",
            21..=50 => "21..50",
            51..=100 => "51..100",
            101..=199 => "101..199",
            _ => "200+",
        };
        *self.iter_buckets.entry(bucket).or_insert(0) += 1;
        if let Some(label) = trace.closest_boundary
            && trace.min_boundary_margin.is_finite()
        {
            let entry = self
                .closest_by_boundary
                .entry(label.to_owned())
                .or_insert_with(|| (f64::INFINITY, Vec::new(), String::new()));
            if trace.min_boundary_margin < entry.0 {
                *entry = (trace.min_boundary_margin, args.to_vec(), prediction.to_owned());
            }
        }
    }

    /// Priority score for selecting a candidate: higher means rarer routes or closer to boundaries.
    pub fn interest_score(&self, trace: &RouteTrace, outcome_class: &str) -> f64 {
        let mut score = 1.0;
        for (bit, &hits) in self.kernel_route_hits.iter().enumerate() {
            if trace.has_bit(bit as u32) {
                if hits == 0 {
                    score += 100.0;
                } else if hits < 10 {
                    score += 25.0 / (hits as f64);
                } else if hits < 100 {
                    score += 4.0 / (hits as f64).sqrt();
                }
            }
        }
        let outcome_count = self.outcome_hits.get(outcome_class).copied().unwrap_or(0);
        if outcome_count == 0 {
            score += 40.0;
        } else if outcome_count < 25 {
            score += 8.0 / (outcome_count as f64).sqrt();
        }
        if trace.iterations >= 150 {
            score += 12.0;
        } else if trace.iterations >= 40 {
            score += 4.0;
        }
        if trace.min_boundary_margin.is_finite() && trace.min_boundary_margin < 1e-2 {
            score += (1.0 / (trace.min_boundary_margin + 1e-9)).ln().clamp(0.0, 20.0);
        }
        score
    }

    pub fn exercised_kernel_routes(&self) -> (usize, usize) {
        let hit = self.kernel_route_hits.iter().filter(|&&c| c > 0).count();
        (hit, self.kernel_route_names.len())
    }

    pub fn unexercised_route_names(&self) -> Vec<&'static str> {
        self.kernel_route_names
            .iter()
            .zip(&self.kernel_route_hits)
            .filter_map(|(&name, &hits)| (hits == 0).then_some(name))
            .collect()
    }

    /// Parents useful for steering mutations (rare-route exemplars + closest boundary points).
    pub fn steering_parents(&self) -> Vec<Vec<Value>> {
        let mut parents = Vec::new();
        for (bit, &hits) in self.kernel_route_hits.iter().enumerate() {
            if hits > 0 && hits < 100
                && let Some(args) = &self.route_exemplars[bit]
            {
                parents.push(args.clone());
            }
        }
        for (_, (_, args, _)) in &self.closest_by_boundary {
            if !args.is_empty() {
                parents.push(args.clone());
            }
        }
        parents
    }

    pub fn to_json(&self) -> Value {
        let routes: BTreeMap<&str, usize> = self
            .kernel_route_names
            .iter()
            .zip(&self.kernel_route_hits)
            .map(|(&n, &c)| (n, c))
            .collect();
        let boundaries: BTreeMap<&str, Value> = self
            .closest_by_boundary
            .iter()
            .map(|(k, (m, _, pred))| (k.as_str(), json!({"min_margin": m, "prediction": pred})))
            .collect();
        let (hit, total) = self.exercised_kernel_routes();
        json!({
            "kernel_routes_hit": hit,
            "kernel_routes_total": total,
            "kernel_routes": routes,
            "unexercised_kernel_routes": self.unexercised_route_names(),
            "outcome_classes": self.outcome_hits,
            "iteration_buckets": self.iter_buckets,
            "closest_boundaries": boundaries,
        })
    }
}

/// Generate a code-steered batch of `target` unique cases for one round.
///
/// Oversamples locally by `oversample` (e.g. 2x), evaluates every candidate in DNA Calc with
/// route tracing, adds targeted mutations around rare routes/boundaries and previous discrepancy
/// parents, and keeps the top-scored diverse selection while preserving regional quotas.
#[allow(clippy::too_many_arguments)]
pub fn generate_steered_round(
    spec: &DomainSpec,
    region_weights: &[f64],
    discrepancy_parents: &[(String, Vec<Value>)],
    tracker: &mut CoverageTracker,
    seen_keys: &mut HashSet<String>,
    context: &WitnessInvocation,
    round: usize,
    target: usize,
    next_id: &mut usize,
    rng: &mut Rng,
) -> Vec<FuzzCase> {
    if target == 0 {
        return Vec::new();
    }
    let total_weight: f64 = region_weights.iter().copied().sum::<f64>().max(1e-9);
    // Reserve up to 25% of adaptive rounds for mutations around previous discrepancies,
    // and up to 15% for code-steered mutations around rare routes / narrow boundaries.
    let disc_quota = if discrepancy_parents.is_empty() {
        0
    } else {
        (target / 4).min(discrepancy_parents.len().saturating_mul(16).max(64))
    };
    let steering_parents = tracker.steering_parents();
    let steer_quota = if steering_parents.is_empty() {
        0
    } else {
        (target * 15) / 100
    };
    let region_quota = target.saturating_sub(disc_quota + steer_quota);

    let mut chosen: Vec<FuzzCase> = Vec::with_capacity(target);
    let mut local_seen: HashSet<String> = HashSet::with_capacity(target * 2);

    #[allow(clippy::too_many_arguments)]
    fn accept_case(
        function: &str,
        round: usize,
        region: String,
        args: Vec<Value>,
        pred: String,
        trace: RouteTrace,
        outcome: String,
        tracker: &mut CoverageTracker,
        seen_keys: &mut HashSet<String>,
        next_id: &mut usize,
        chosen: &mut Vec<FuzzCase>,
    ) {
        let key = canonical_case_key(function, &args);
        if seen_keys.insert(key) {
            tracker.record(&args, &pred, &trace, &outcome);
            *next_id += 1;
            chosen.push(FuzzCase {
                id: format!("fuzz-{function}-r{round:02}-{:06}", *next_id),
                region,
                round,
                args,
                prediction: pred,
                trace,
                outcome_class: outcome,
            });
        }
    }

    // 1. Discrepancy-neighborhood densification (from earlier Excel mismatches)
    let mut attempts = 0usize;
    let max_disc_attempts = disc_quota.saturating_mul(6).max(64);
    while chosen.len() < disc_quota && attempts < max_disc_attempts {
        attempts += 1;
        let (parent_region, parent_args) = rng.pick(discrepancy_parents);
        let mutated = mutate_case(spec, parent_args, rng);
        let key = canonical_case_key(&spec.function, &mutated);
        if seen_keys.contains(&key) || !local_seen.insert(key) {
            continue;
        }
        let (pred, trace, outcome) = evaluate_traced(&spec.function, &mutated, context);
        accept_case(
            &spec.function,
            round,
            format!("densify:{parent_region}"),
            mutated,
            pred,
            trace,
            outcome,
            tracker,
            seen_keys,
            next_id,
            &mut chosen,
        );
    }

    // 2. Code-steered mutations around rare routes & boundary-proximity exemplars
    let steer_target_end = chosen.len() + steer_quota;
    if !steering_parents.is_empty() && steer_quota > 0 {
        let mut pool: Vec<(f64, Vec<Value>, String, RouteTrace, String)> =
            Vec::with_capacity(steer_quota * 2);
        let max_attempts = steer_quota.saturating_mul(4).max(128);
        for _ in 0..max_attempts {
            if pool.len() >= steer_quota * 2 {
                break;
            }
            let parent = rng.pick(&steering_parents);
            let mutated = mutate_case(spec, parent, rng);
            let key = canonical_case_key(&spec.function, &mutated);
            if seen_keys.contains(&key) || !local_seen.insert(key) {
                continue;
            }
            let (pred, trace, outcome) = evaluate_traced(&spec.function, &mutated, context);
            let score = tracker.interest_score(&trace, &outcome);
            pool.push((score, mutated, pred, trace, outcome));
        }
        pool.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, args, pred, trace, outcome) in pool {
            if chosen.len() >= steer_target_end {
                break;
            }
            accept_case(
                &spec.function,
                round,
                "code_steered".to_owned(),
                args,
                pred,
                trace,
                outcome,
                tracker,
                seen_keys,
                next_id,
                &mut chosen,
            );
        }
    }

    // 3. Per-region oversampled strategic generation with route-interest selection
    let remaining_total = target
        .saturating_sub(chosen.len())
        .max(region_quota.min(target.saturating_sub(chosen.len())));
    for (idx, region) in spec.regions.iter().enumerate() {
        let share = ((remaining_total as f64) * (region_weights[idx] / total_weight)).round() as usize;
        let quota = share.max(1).min(target.saturating_sub(chosen.len()));
        if quota == 0 {
            continue;
        }
        // Oversample 1.5x (up to +2000) so steering prefers rare routes without huge overhead.
        let pool_cap = quota + (quota / 2).min(2000);
        let mut pool: Vec<(f64, Vec<Value>, String, RouteTrace, String)> =
            Vec::with_capacity(pool_cap);
        let max_attempts = pool_cap.saturating_mul(3).max(32);
        for _ in 0..max_attempts {
            if pool.len() >= pool_cap {
                break;
            }
            let args = spec.generate(region.kind, rng);
            let key = canonical_case_key(&spec.function, &args);
            if seen_keys.contains(&key) || !local_seen.insert(key) {
                continue;
            }
            let (pred, trace, outcome) = evaluate_traced(&spec.function, &args, context);
            let score = tracker.interest_score(&trace, &outcome);
            // Immediately record brand-new routes so later pool items don't all chase the same bit.
            let has_unseen = (0..tracker.kernel_route_hits.len())
                .any(|b| trace.has_bit(b as u32) && tracker.kernel_route_hits[b] == 0);
            if has_unseen {
                accept_case(
                    &spec.function,
                    round,
                    region.name.to_owned(),
                    args,
                    pred,
                    trace,
                    outcome,
                    tracker,
                    seen_keys,
                    next_id,
                    &mut chosen,
                );
                if chosen.len() >= target {
                    break;
                }
            } else {
                pool.push((score, args, pred, trace, outcome));
            }
        }
        pool.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut taken = 0usize;
        for (_, args, pred, trace, outcome) in pool {
            if taken >= quota || chosen.len() >= target {
                break;
            }
            accept_case(
                &spec.function,
                round,
                region.name.to_owned(),
                args,
                pred,
                trace,
                outcome,
                tracker,
                seen_keys,
                next_id,
                &mut chosen,
            );
            taken += 1;
        }
    }

    // Top up to `target` if rounding left a few slots
    let mut fallback_attempts = 0usize;
    let max_fallback = target.saturating_sub(chosen.len()).saturating_mul(5).max(64);
    while chosen.len() < target && fallback_attempts < max_fallback {
        fallback_attempts += 1;
        let region: &RegionSpec = rng.pick(&spec.regions);
        let args = spec.generate(region.kind, rng);
        let key = canonical_case_key(&spec.function, &args);
        if seen_keys.contains(&key) {
            continue;
        }
        let (pred, trace, outcome) = evaluate_traced(&spec.function, &args, context);
        accept_case(
            &spec.function,
            round,
            region.name.to_owned(),
            args,
            pred,
            trace,
            outcome,
            tracker,
            seen_keys,
            next_id,
            &mut chosen,
        );
    }

    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_steered_rate_round_exercises_many_internal_routes() {
        let spec = DomainSpec::for_function("RATE").unwrap();
        let weights: Vec<f64> = spec.regions.iter().map(|r| r.base_weight).collect();
        let mut tracker = CoverageTracker::new("RATE");
        let mut seen = HashSet::new();
        let ctx = touchstone_bulk_context();
        let mut next_id = 0;
        let mut rng = Rng::from_seed_and_tag(12345, "RATE");
        let batch = generate_steered_round(
            &spec,
            &weights,
            &[],
            &mut tracker,
            &mut seen,
            &ctx,
            0,
            600,
            &mut next_id,
            &mut rng,
        );
        assert_eq!(batch.len(), 600);
        let (hit, total) = tracker.exercised_kernel_routes();
        assert!(
            hit >= 15,
            "expected >=15/30 RATE routes hit in 600 steered cases, got {hit}/{total}: {:?}",
            tracker.unexercised_route_names()
        );
    }
}
