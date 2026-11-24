//! Boundary mapping via IEEE-754 ordered-bit bisection and consecutive-double windows (`dc-ztvv.21`).
//!
//! 1. Locates flips in DNA Calc's own predicate (`number <-> error`, `error_A <-> error_B`, or
//!    distinct kernel route signatures) along a single numeric axis in ~52 local calls.
//! 2. Emits dense consecutive-double windows (`[lo - W ..= hi + W]`) around every frontier so
//!    Excel can confirm whether the frontier sits at the exact same binary64 word.
//! 3. When Excel disagrees on a boundary window, supports parallel batched 64-point bisection
//!    (matching `tools/excel-service/client.py:bisect_edge`) to pinpoint Excel's exact flip.

use super::domain::DomainSpec;
use super::rng::{
    Rng, canonical_case_key, compact_to_f64, f64_to_compact, f64_to_ordered, hex_f64,
    parse_hex_bits, step_ulps,
};
use super::steer::{FuzzCase, evaluate_traced};
use calc_verdict::WitnessInvocation;
use serde_json::{Value, json};
use std::collections::HashSet;

/// Coarse predicate tag for number-vs-error and error-vs-error frontier discovery.
pub fn predicate_tag(encoded: &str) -> String {
    if encoded.starts_with("0x") {
        "number".to_owned()
    } else if encoded.starts_with("error:") {
        encoded.to_owned()
    } else if encoded.starts_with("logical:") {
        "logical".to_owned()
    } else if encoded.starts_with("text:") {
        "text".to_owned()
    } else {
        encoded.to_owned()
    }
}

/// Read the raw binary64 bits at `(axis, cell)` (`cell == None` for scalar, `Some((r, c))` for
/// a 2D numeric array argument like `IRR`'s `values`).
pub fn get_axis_bits(args: &[Value], axis: usize, cell: Option<(usize, usize)>) -> Option<u64> {
    let slot = args.get(axis)?;
    match cell {
        None => parse_hex_bits(slot),
        Some((r, c)) => {
            let rows = slot.as_array()?;
            let row = rows.get(r)?.as_array()?;
            parse_hex_bits(row.get(c)?)
        }
    }
}

/// Replace the value at `(axis, cell)` with Value2-safe hex double `val`.
pub fn set_axis_f64(args: &mut [Value], axis: usize, cell: Option<(usize, usize)>, val: f64) -> bool {
    let Some(slot) = args.get_mut(axis) else {
        return false;
    };
    match cell {
        None => {
            *slot = hex_f64(val);
            true
        }
        Some((r, c)) => {
            let Some(rows) = slot.as_array_mut() else {
                return false;
            };
            let Some(row) = rows.get_mut(r).and_then(Value::as_array_mut) else {
                return false;
            };
            if c >= row.len() {
                return false;
            }
            row[c] = hex_f64(val);
            true
        }
    }
}

/// One discovered frontier along a single numeric argument (or array cell) axis.
#[derive(Clone, Debug)]
pub struct Frontier {
    pub id: String,
    pub function: String,
    pub axis: usize,
    pub axis_cell: Option<(usize, usize)>,
    pub axis_name: String,
    pub kind: String,
    pub base_args: Vec<Value>,
    pub dna_lo_bits: u64,
    pub dna_hi_bits: u64,
    pub dna_lo_outcome: String,
    pub dna_hi_outcome: String,
    /// Optional Excel-confirmed frontier after probing the consecutive window / Excel bisection.
    pub excel_lo_bits: Option<u64>,
    pub excel_hi_bits: Option<u64>,
    pub excel_lo_outcome: Option<String>,
    pub excel_hi_outcome: Option<String>,
    pub shifted_ulps: Option<i64>,
}

impl Frontier {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "function": self.function,
            "axis": self.axis,
            "axis_cell": self.axis_cell.map(|(r, c)| [r, c]),
            "axis_name": self.axis_name,
            "kind": self.kind,
            "base_args": self.base_args,
            "dna_lo_bits": format!("0x{:016x}", self.dna_lo_bits),
            "dna_hi_bits": format!("0x{:016x}", self.dna_hi_bits),
            "dna_lo_outcome": self.dna_lo_outcome,
            "dna_hi_outcome": self.dna_hi_outcome,
            "excel_lo_bits": self.excel_lo_bits.map(|b| format!("0x{b:016x}")),
            "excel_hi_bits": self.excel_hi_bits.map(|b| format!("0x{b:016x}")),
            "excel_lo_outcome": self.excel_lo_outcome,
            "excel_hi_outcome": self.excel_hi_outcome,
            "shifted_ulps": self.shifted_ulps,
        })
    }
}

/// Bisect locally on DNA Calc along scalar or array-cell numeric argument `(axis, cell)` between
/// `lo_val` and `hi_val` where `predicate(lo) != predicate(hi)`, returning exact adjacent
/// Value2-safe doubles `(lo_bits, lo_result, hi_bits, hi_result)`.
pub fn bisect_local_axis_cell(
    function: &str,
    base_args: &[Value],
    axis: usize,
    cell: Option<(usize, usize)>,
    lo_val: f64,
    hi_val: f64,
    context: &WitnessInvocation,
    route_mode: bool,
) -> Option<(u64, String, u64, String)> {
    if !lo_val.is_finite() || !hi_val.is_finite() || lo_val == hi_val || axis >= base_args.len() {
        return None;
    }
    let (mut a, mut b) = if f64_to_ordered(lo_val) < f64_to_ordered(hi_val) {
        (lo_val, hi_val)
    } else {
        (hi_val, lo_val)
    };
    let eval_at = |x: f64| -> (String, u64) {
        let mut args = base_args.to_vec();
        set_axis_f64(&mut args, axis, cell, x);
        let (res, trace, _) = evaluate_traced(function, &args, context);
        (res, trace.route_mask)
    };
    let key_of = |(res, mask): &(String, u64)| -> String {
        if route_mode {
            format!("{}|{:x}", predicate_tag(res), mask)
        } else {
            predicate_tag(res)
        }
    };
    let mut ea = eval_at(a);
    let mut eb = eval_at(b);
    let ka = key_of(&ea);
    if ka == key_of(&eb) {
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
        if key_of(&em) == ka {
            a = mid;
            ea = em;
        } else {
            b = mid;
            eb = em;
        }
    }
    let a_bits = if a == 0.0 { 0u64 } else { a.to_bits() };
    let b_bits = if b == 0.0 { 0u64 } else { b.to_bits() };
    Some((a_bits, ea.0, b_bits, eb.0))
}

#[allow(dead_code)]
pub fn bisect_local_axis(
    function: &str,
    base_args: &[Value],
    axis: usize,
    lo_val: f64,
    hi_val: f64,
    context: &WitnessInvocation,
    route_mode: bool,
) -> Option<(u64, String, u64, String)> {
    bisect_local_axis_cell(function, base_args, axis, None, lo_val, hi_val, context, route_mode)
}

/// Discover DNA Calc number-vs-error and route-change frontiers by probing axis sweeps from
/// interesting seed cases, then bisecting every detected flip to adjacent doubles.
pub fn discover_local_frontiers(
    spec: &DomainSpec,
    seeds: &[FuzzCase],
    context: &WitnessInvocation,
    max_frontiers: usize,
    _rng: &mut Rng,
) -> Vec<Frontier> {
    if seeds.is_empty() || max_frontiers == 0 {
        return Vec::new();
    }
    let mut frontiers = Vec::new();
    let mut seen_frontiers: HashSet<String> = HashSet::new();

    // Candidate numeric axes: all scalar numeric params PLUS first 3 rows of numeric arrays (e.g. IRR values[0..3]).
    let mut probe_axes: Vec<(usize, Option<(usize, usize)>)> = Vec::new();
    for (i, p) in spec.params.iter().enumerate() {
        if p.kind == super::domain::ParamKind::ArrayNumeric {
            for r in 0..3 {
                probe_axes.push((i, Some((r, 0))));
            }
        } else {
            probe_axes.push((i, None));
        }
    }
    if probe_axes.is_empty() {
        return frontiers;
    }

    // Stratify candidate seeds across regions AND high-interest (small-margin / high-iter) cases
    // so frontier mapping covers every axis and region rather than clustering on one 200-iter basin.
    let mut by_region: std::collections::BTreeMap<&str, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (idx, s) in seeds.iter().enumerate() {
        by_region.entry(s.region.as_str()).or_default().push(idx);
    }
    for indices in by_region.values_mut() {
        indices.sort_by(|&i, &j| {
            let sa = &seeds[i];
            let sb = &seeds[j];
            let key_a = (
                sa.trace.min_boundary_margin.is_finite(),
                sa.trace.iterations,
            );
            let key_b = (
                sb.trace.min_boundary_margin.is_finite(),
                sb.trace.iterations,
            );
            key_b
                .0
                .cmp(&key_a.0)
                .then_with(|| key_b.1.cmp(&key_a.1))
                .then_with(|| {
                    sa.trace
                        .min_boundary_margin
                        .total_cmp(&sb.trace.min_boundary_margin)
                })
        });
    }
    // Round-robin interleave across regions so every strategic region gets fair frontier probes.
    let mut order: Vec<usize> = Vec::with_capacity(seeds.len());
    let mut pos = 0usize;
    let probe_limit = seeds.len().min(max_frontiers * 16).max(64);
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

    let factors: &[f64] = &[
        -10.0, -2.0, -1.0, -0.5, -0.1, 0.0, 0.5, 0.9, 0.99, 1.01, 1.1, 2.0, 10.0, 100.0,
    ];
    let additive: &[f64] = &[-1.0, -0.1, -0.01, -1e-4, -1e-7, 1e-7, 1e-4, 0.01, 0.1, 1.0];

    let max_per_axis = ((max_frontiers + probe_axes.len() - 1) / probe_axes.len()).max(2);
    let mut per_axis_count: std::collections::BTreeMap<(usize, Option<(usize, usize)>), usize> =
        std::collections::BTreeMap::new();
    let mut axis_cursor = 0usize;

    // Pass 0: prioritize predicate flips (number <-> error, error <-> error) across all axes.
    // Pass 1: fill remaining slots with route_flip frontiers.
    for allow_route_only in [false, true] {
        if frontiers.len() >= max_frontiers {
            break;
        }
        for &seed_idx in &order {
            if frontiers.len() >= max_frontiers {
                break;
            }
            let seed = &seeds[seed_idx];
            // Pick the next numeric axis (scalar or array cell) in round-robin order.
            let mut chosen_axis = None;
            for offset in 0..probe_axes.len() {
                let ax = probe_axes[(axis_cursor + offset) % probe_axes.len()];
                if get_axis_bits(&seed.args, ax.0, ax.1).is_some()
                    && (*per_axis_count.get(&ax).unwrap_or(&0) < max_per_axis || allow_route_only)
                {
                    chosen_axis = Some(ax);
                    axis_cursor = (axis_cursor + offset + 1) % probe_axes.len();
                    break;
                }
            }
            let Some((axis, cell)) = chosen_axis else {
                continue;
            };
            let Some(base_bits) = get_axis_bits(&seed.args, axis, cell) else {
                continue;
            };
            let base_val = f64::from_bits(base_bits);
            if !base_val.is_finite() {
                continue;
            }
            let base_pred = predicate_tag(&seed.prediction);
            let base_mask = seed.trace.route_mask;

            // Probe offsets around base_val to find an opposite-side bracket
            let mut bracket: Option<(f64, bool)> = None;
            for &f in factors {
                let cand = base_val * f;
                if !cand.is_finite() || cand.is_subnormal() || cand == base_val {
                    continue;
                }
                let mut args = seed.args.clone();
                set_axis_f64(&mut args, axis, cell, cand);
                let (res, trace, _) = evaluate_traced(&spec.function, &args, context);
                if predicate_tag(&res) != base_pred {
                    bracket = Some((cand, false));
                    break;
                }
                if allow_route_only && bracket.is_none() && trace.route_mask != base_mask {
                    bracket = Some((cand, true));
                }
            }
            if bracket.is_none() {
                for &d in additive {
                    let cand = base_val + d;
                    if !cand.is_finite() || cand.is_subnormal() || cand == base_val {
                        continue;
                    }
                    let mut args = seed.args.clone();
                    set_axis_f64(&mut args, axis, cell, cand);
                    let (res, trace, _) = evaluate_traced(&spec.function, &args, context);
                    if predicate_tag(&res) != base_pred {
                        bracket = Some((cand, false));
                        break;
                    }
                    if allow_route_only && bracket.is_none() && trace.route_mask != base_mask {
                        bracket = Some((cand, true));
                    }
                }
            }
            let Some((other_val, route_only)) = bracket else {
                continue;
            };
            if route_only && !allow_route_only {
                continue;
            }
            let Some((lo_bits, lo_out, hi_bits, hi_out)) = bisect_local_axis_cell(
                &spec.function,
                &seed.args,
                axis,
                cell,
                base_val,
                other_val,
                context,
                route_only,
            ) else {
                continue;
            };
            let kind = if predicate_tag(&lo_out) != predicate_tag(&hi_out) {
                format!("{}<->{}", predicate_tag(&lo_out), predicate_tag(&hi_out))
            } else {
                "route_flip".to_owned()
            };
            if !allow_route_only && kind == "route_flip" {
                continue;
            }
            // Deduplicate by (axis, cell, top 32 bits of lo_bits, kind) so nearby perturbations
            // don't crowd out distinct regions of the frontier map.
            let coarse_key = format!("{axis}|{cell:?}|{:08x}|{kind}", lo_bits >> 32);
            if !seen_frontiers.insert(coarse_key) {
                continue;
            }
            let base_name = spec
                .params
                .get(axis)
                .map_or_else(|| format!("arg{axis}"), |p| p.name.clone());
            let axis_name = match cell {
                None => base_name,
                Some((r, 0)) => format!("{base_name}[{r}]"),
                Some((r, c)) => format!("{base_name}[{r},{c}]"),
            };
            *per_axis_count.entry((axis, cell)).or_insert(0) += 1;
            frontiers.push(Frontier {
                id: format!("frontier-{}-{:03}", spec.function, frontiers.len() + 1),
                function: spec.function.clone(),
                axis,
                axis_cell: cell,
                axis_name,
                kind,
                base_args: seed.args.clone(),
                dna_lo_bits: lo_bits,
                dna_hi_bits: hi_bits,
                dna_lo_outcome: lo_out,
                dna_hi_outcome: hi_out,
                excel_lo_bits: None,
                excel_hi_bits: None,
                excel_lo_outcome: None,
                excel_hi_outcome: None,
                shifted_ulps: None,
            });
        }
    }

    frontiers
}

/// Result of expanding consecutive-double windows around a slice of frontiers.
pub struct ExpandedFrontierWindows {
    /// Newly unseen window cases to freeze and evaluate in Excel.
    pub cases: Vec<(usize, FuzzCase)>,
    /// Every valid window point `(rel_frontier_idx, axis_bits, canonical_key)` — including
    /// points already in `seen_keys` so callers can feed cached Excel answers into
    /// [`record_excel_window_flips`].
    pub window_refs: Vec<(usize, u64, String)>,
}

/// Expand every frontier into a dense window of consecutive Value2-safe doubles
/// `[lo - half_window ..= lo]` and `[hi ..= hi + half_window]`, returning both the newly
/// unseen cases to ask Excel and the full list of window references (including `seen_keys` hits).
pub fn expand_frontier_windows_with_refs(
    frontiers: &[Frontier],
    half_window: i64,
    context: &WitnessInvocation,
    seen_keys: &mut HashSet<String>,
    round: usize,
    next_id: &mut usize,
) -> ExpandedFrontierWindows {
    let mut cases = Vec::new();
    let mut window_refs = Vec::new();
    for (fidx, frontier) in frontiers.iter().enumerate() {
        let lo = f64::from_bits(frontier.dna_lo_bits);
        let hi = f64::from_bits(frontier.dna_hi_bits);
        let mut pts = Vec::new();
        for d in -half_window..=0 {
            if let Some(v) = step_ulps(lo, d, true) {
                pts.push(v);
            }
        }
        for d in 0..=half_window {
            if let Some(v) = step_ulps(hi, d, true) {
                pts.push(v);
            }
        }
        let mut local_bits = HashSet::new();
        for v in pts {
            let bits = if v == 0.0 { 0u64 } else { v.to_bits() };
            if !local_bits.insert(bits) {
                continue;
            }
            let mut args = frontier.base_args.clone();
            if !set_axis_f64(&mut args, frontier.axis, frontier.axis_cell, v) {
                continue;
            }
            let key = canonical_case_key(&frontier.function, &args);
            window_refs.push((fidx, bits, key.clone()));
            if !seen_keys.insert(key) {
                continue;
            }
            let (prediction, trace, outcome_class) = evaluate_traced(&frontier.function, &args, context);
            *next_id += 1;
            cases.push((
                fidx,
                FuzzCase {
                    id: format!("fuzz-{}-r{round:02}-{:06}", frontier.function, *next_id),
                    region: format!("boundary:{}", frontier.axis_name),
                    round,
                    args,
                    prediction,
                    trace,
                    outcome_class,
                },
            ));
        }
    }
    ExpandedFrontierWindows { cases, window_refs }
}

/// Expand every frontier into a dense window of consecutive Value2-safe doubles
/// `[lo - half_window ..= lo]` and `[hi ..= hi + half_window]` for local dry-run generation.
pub fn expand_frontier_windows(
    frontiers: &[Frontier],
    half_window: i64,
    context: &WitnessInvocation,
    seen_keys: &mut HashSet<String>,
    round: usize,
    next_id: &mut usize,
) -> Vec<(usize, FuzzCase)> {
    expand_frontier_windows_with_refs(frontiers, half_window, context, seen_keys, round, next_id)
        .cases
}

/// Update each frontier with Excel's observed flip inside its probed consecutive window.
/// Only adjacent Value2-safe pairs (`f64_to_compact(hi) == f64_to_compact(lo) + 1`) qualify so
/// holes from `Unread` rows or missing probes are never reported as exact Excel flips.
/// When a chaotic window has multiple adjacent flips (e.g. oscillatory 200-step boundaries),
/// picks the flip closest to DNA Calc's `dna_lo_bits` (preferring matching transition direction
/// on ties) so matching windows accurately report `shifted_ulps = 0`.
pub fn record_excel_window_flips(
    frontiers: &mut [Frontier],
    probed: &[(usize, u64, String)], // (frontier_idx, axis_bits, excel_outcome)
) {
    let mut by_frontier: std::collections::BTreeMap<usize, Vec<(u64, String)>> =
        std::collections::BTreeMap::new();
    for (idx, bits, outcome) in probed {
        by_frontier
            .entry(*idx)
            .or_default()
            .push((*bits, outcome.clone()));
    }
    for (idx, mut pts) in by_frontier {
        let Some(frontier) = frontiers.get_mut(idx) else {
            continue;
        };
        pts.sort_by_key(|(bits, _)| f64_to_compact(f64::from_bits(*bits)));
        pts.dedup_by_key(|(bits, _)| f64_to_compact(f64::from_bits(*bits)));
        let dna_c = f64_to_compact(f64::from_bits(frontier.dna_lo_bits));
        let dna_lo_tag = predicate_tag(&frontier.dna_lo_outcome);
        let dna_hi_tag = predicate_tag(&frontier.dna_hi_outcome);
        let mut best: Option<(u128, u8, i128, &(u64, String), &(u64, String))> = None;
        for pair in pts.windows(2) {
            let c0 = f64_to_compact(f64::from_bits(pair[0].0));
            let c1 = f64_to_compact(f64::from_bits(pair[1].0));
            if c1 != c0 + 1 {
                continue;
            }
            let p0 = predicate_tag(&pair[0].1);
            let p1 = predicate_tag(&pair[1].1);
            if p0 != p1 {
                let shift = c0 - dna_c;
                let dist = shift.unsigned_abs();
                let dir_pen = u8::from(!(p0 == dna_lo_tag && p1 == dna_hi_tag));
                if best.is_none_or(|(bd, bp, _, _, _)| (dist, dir_pen) < (bd, bp)) {
                    best = Some((dist, dir_pen, shift, &pair[0], &pair[1]));
                }
            }
        }
        if let Some((_, _, shift, lo, hi)) = best {
            frontier.excel_lo_bits = Some(lo.0);
            frontier.excel_hi_bits = Some(hi.0);
            frontier.excel_lo_outcome = Some(lo.1.clone());
            frontier.excel_hi_outcome = Some(hi.1.clone());
            frontier.shifted_ulps = i64::try_from(shift).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuzz::steer::touchstone_bulk_context;

    #[test]
    fn local_bisection_pinpoints_rate_guess_neg_one_boundary() {
        let ctx = touchstone_bulk_context();
        let base = vec![
            hex_f64(10.0),
            hex_f64(-120.0),
            hex_f64(1000.0),
            hex_f64(0.0),
            hex_f64(0.0),
            hex_f64(0.1),
        ];
        // At guess = -1.0 RATE returns #VALUE!; one ULP above -1.0 it enters the secant solver
        // (returning #NUM! for nper=10 because -1+eps diverges), so bisection hits the exact
        // #VALUE! <-> #NUM! double boundary at -1.0.
        let (lo, lo_out, hi, hi_out) =
            bisect_local_axis("RATE", &base, 5, -2.0, 0.1, &ctx, false).expect("flip found");
        assert_eq!(lo, (-1.0_f64).to_bits(), "expected -1.0 as last #VALUE! word");
        assert_eq!(lo_out, "error:Value");
        assert_eq!(
            hi,
            step_ulps(-1.0, 1, true).unwrap().to_bits(),
            "expected nextafter(-1, 0) as first non-#VALUE! word"
        );
        assert_ne!(hi_out, "error:Value", "expected non-#VALUE!, got {hi_out}");

        // And bisecting from nextafter(-1, 0) to +0.1 pinpoints the #NUM! <-> number convergence frontier.
        let (lo2, lo2_out, hi2, hi2_out) = bisect_local_axis(
            "RATE",
            &base,
            5,
            f64::from_bits(hi),
            0.1,
            &ctx,
            false,
        )
        .expect("number<->#NUM! flip found");
        assert_eq!(lo2_out, "error:Num");
        assert!(hi2_out.starts_with("0x"), "expected number, got {hi2_out}");
        assert_eq!(
            f64_to_ordered(f64::from_bits(lo2)) + 1,
            f64_to_ordered(f64::from_bits(hi2))
        );
    }

    #[test]
    fn array_cell_bisection_pinpoints_irr_cashflow_sign_boundary() {
        let ctx = touchstone_bulk_context();
        let base = vec![
            Value::Array(vec![
                Value::Array(vec![hex_f64(-100.0)]),
                Value::Array(vec![hex_f64(120.0)]),
            ]),
            hex_f64(0.1),
        ];
        // Sweeping values[1] from -10.0 (#NUM!, all-negative flows) to +120.0 (number) must hit
        // the exact adjacent Value2-safe double boundary at 0.0 -> +MIN_POSITIVE (or first valid root).
        let (lo, lo_out, hi, hi_out) = bisect_local_axis_cell(
            "IRR",
            &base,
            0,
            Some((1, 0)),
            -10.0,
            120.0,
            &ctx,
            false,
        )
        .expect("IRR cashflow boundary found");
        assert_eq!(lo_out, "error:Num");
        assert!(hi_out.starts_with("0x"), "expected number, got {hi_out}");
        assert!(f64_to_ordered(f64::from_bits(lo)) < f64_to_ordered(f64::from_bits(hi)));
    }

    #[test]
    fn compact_bisection_across_zero_lands_on_min_positive_and_zero() {
        let ctx = touchstone_bulk_context();
        let base = vec![
            hex_f64(44927.0),
            hex_f64(46753.0),
            hex_f64(0.05),
            hex_f64(95.0),
            hex_f64(100.0),
            hex_f64(2.0),
            hex_f64(0.0),
        ];
        // YIELD basis < 0 is #NUM!, basis == 0 is valid; compact bisection must hit
        // exact Value2-adjacent -MIN_POSITIVE and 0.0.
        let (lo, lo_out, hi, hi_out) =
            bisect_local_axis("YIELD", &base, 6, -1.0, 0.0, &ctx, false).expect("basis flip");
        assert_eq!(lo, (-f64::MIN_POSITIVE).to_bits());
        assert_eq!(lo_out, "error:Num");
        assert_eq!(hi, 0u64);
        assert!(hi_out.starts_with("0x"), "got {hi_out}");
    }

    #[test]
    fn record_excel_window_flips_prefers_closest_flip_on_oscillatory_window() {
        let lo = 0xc1e3_b4f2_ebf9_c628_u64;
        let hi = 0xc1e3_b4f2_ebf9_c627_u64;
        let mut frontiers = vec![Frontier {
            id: "f1".into(),
            function: "RATE".into(),
            axis: 1,
            axis_cell: None,
            axis_name: "pmt".into(),
            kind: "error:Num<->number".into(),
            base_args: vec![],
            dna_lo_bits: lo,
            dna_hi_bits: hi,
            dna_lo_outcome: "error:Num".into(),
            dna_hi_outcome: "0x3f7113e771f30824".into(),
            excel_lo_bits: None,
            excel_hi_bits: None,
            excel_lo_outcome: None,
            excel_hi_outcome: None,
            shifted_ulps: None,
        }];
        let probed = vec![
            (0, 0xc1e3_b4f2_ebf9_c62a, "error:Num".to_owned()),
            (0, 0xc1e3_b4f2_ebf9_c629, "0x3f7113e771f30826".to_owned()),
            (0, lo, "error:Num".to_owned()),
            (0, hi, "0x3f7113e771f30824".to_owned()),
            (0, 0xc1e3_b4f2_ebf9_c626, "0x3f7113e771f30823".to_owned()),
            (0, 0xc1e3_b4f2_ebf9_c625, "error:Num".to_owned()),
        ];
        record_excel_window_flips(&mut frontiers, &probed);
        assert_eq!(frontiers[0].excel_lo_bits, Some(lo));
        assert_eq!(frontiers[0].excel_hi_bits, Some(hi));
        assert_eq!(frontiers[0].shifted_ulps, Some(0));
    }

    #[test]
    fn record_excel_window_flips_rejects_non_adjacent_holes() {
        let lo = (-1.0_f64).to_bits();
        let hi = step_ulps(-1.0, 1, true).unwrap().to_bits();
        let hi_plus_1 = step_ulps(-1.0, 2, true).unwrap().to_bits();
        let lo_minus_1 = step_ulps(-1.0, -1, true).unwrap().to_bits();

        // Case 1: `hi` is missing (hole due to seen_keys skip or Unread row).
        // `lo` -> `hi_plus_1` spans 2 ULPs, so it must NOT be reported as an exact Excel flip.
        let mut frontiers = vec![Frontier {
            id: "f-hole".into(),
            function: "RATE".into(),
            axis: 5,
            axis_cell: None,
            axis_name: "guess".into(),
            kind: "error:Value<->number".into(),
            base_args: vec![],
            dna_lo_bits: lo,
            dna_hi_bits: hi,
            dna_lo_outcome: "error:Value".into(),
            dna_hi_outcome: "0x3fa999999999999a".into(),
            excel_lo_bits: None,
            excel_hi_bits: None,
            excel_lo_outcome: None,
            excel_hi_outcome: None,
            shifted_ulps: None,
        }];
        let probed_with_hole = vec![
            (0, lo_minus_1, "error:Value".to_owned()),
            (0, lo, "error:Value".to_owned()),
            // `hi` omitted!
            (0, hi_plus_1, "0x3fa999999999999a".to_owned()),
        ];
        record_excel_window_flips(&mut frontiers, &probed_with_hole);
        assert_eq!(
            frontiers[0].excel_lo_bits, None,
            "non-adjacent hole across lo..lo+2 must not be recorded as an exact flip"
        );
        assert_eq!(frontiers[0].excel_hi_bits, None);
        assert_eq!(frontiers[0].shifted_ulps, None);

        // Case 2: Hole across zero (-MIN_POSITIVE to +MIN_POSITIVE with 0.0 missing) must be rejected.
        let neg_min = (-f64::MIN_POSITIVE).to_bits();
        let pos_min = f64::MIN_POSITIVE.to_bits();
        let mut zero_frontier = vec![Frontier {
            id: "f-zero".into(),
            function: "YIELD".into(),
            axis: 6,
            axis_cell: None,
            axis_name: "basis".into(),
            kind: "error:Num<->number".into(),
            base_args: vec![],
            dna_lo_bits: neg_min,
            dna_hi_bits: 0u64,
            dna_lo_outcome: "error:Num".into(),
            dna_hi_outcome: "0x3fa999999999999a".into(),
            excel_lo_bits: None,
            excel_hi_bits: None,
            excel_lo_outcome: None,
            excel_hi_outcome: None,
            shifted_ulps: None,
        }];
        let probed_zero_hole = vec![
            (0, neg_min, "error:Num".to_owned()),
            // 0.0 omitted!
            (0, pos_min, "0x3fa999999999999a".to_owned()),
        ];
        record_excel_window_flips(&mut zero_frontier, &probed_zero_hole);
        assert_eq!(
            zero_frontier[0].excel_lo_bits, None,
            "missing 0.0 between -MIN_POSITIVE and +MIN_POSITIVE must not be reported as adjacent"
        );

        // Case 3: When 0.0 IS present, -MIN_POSITIVE -> 0.0 IS Value2-adjacent and accepted.
        let probed_zero_complete = vec![
            (0, neg_min, "error:Num".to_owned()),
            (0, 0u64, "0x3fa999999999999a".to_owned()),
            (0, pos_min, "0x3fa999999999999a".to_owned()),
        ];
        record_excel_window_flips(&mut zero_frontier, &probed_zero_complete);
        assert_eq!(zero_frontier[0].excel_lo_bits, Some(neg_min));
        assert_eq!(zero_frontier[0].excel_hi_bits, Some(0u64));
        assert_eq!(zero_frontier[0].shifted_ulps, Some(0));

        // Case 4: Window with a non-adjacent hole at lo..lo+2 AND a genuine adjacent flip at lo+2..lo+3
        // skips the hole and picks the adjacent flip (+2 ULPs).
        let hi_plus_2 = step_ulps(-1.0, 3, true).unwrap().to_bits();
        let probed_hole_and_real = vec![
            (0, lo, "error:Value".to_owned()),
            // `hi` omitted!
            (0, hi_plus_1, "0x3fa999999999999a".to_owned()),
            (0, hi_plus_2, "error:Value".to_owned()),
        ];
        record_excel_window_flips(&mut frontiers, &probed_hole_and_real);
        assert_eq!(frontiers[0].excel_lo_bits, Some(hi_plus_1));
        assert_eq!(frontiers[0].excel_hi_bits, Some(hi_plus_2));
        assert_eq!(frontiers[0].shifted_ulps, Some(2));
    }
}
