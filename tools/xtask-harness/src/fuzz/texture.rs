//! Catalogue of interesting texture cases (`dc-ztvv.26.8`).
//!
//! Owner direction (2026-11-24): keep and catalogue interesting cases even when they match Excel:
//! - `boundary_representative`: adjacent-double boundaries (`error_A <-> error_B`, shifted edges),
//! - `error_vs_number_neighbour`: adjacent binary64 pairs straddling `number <-> error:*`,
//! - `route_switch`: minimal exemplars of kernel route bits and route-flip boundaries,
//! - `near_tie`: closest approaches to kernel threshold predicates and near-integer / near-halfway values,
//! - `regime_change`: solver iteration-regime and sign/zero/magnitude outcome-regime switches,
//! - `extreme_magnitude`: extreme finite normal inputs (`±DBL_MAX`, `±MIN_POSITIVE`, `|x|>=1e200`,
//!   `0<|x|<=1e-200`, `±2^53`) and extreme outputs.
//!
//! Every entry records `function`, `args` as hex bits, `why` it is interesting, and the `recipe`
//! it came from, enabling automated regression tests, cross-build / cross-CPU re-validation, and
//! handbook examples.

use super::boundary::{Frontier, predicate_tag, set_axis_f64};
use super::cluster::arg_complexity;
use super::domain::DomainSpec;
use super::fractal::iteration_regime_bucket;
use super::rng::{GENERATOR_VERSION, canonical_case_key, parse_hex_bits};
use super::steer::{FuzzCase, evaluate_traced, touchstone_bulk_context};
use super::{FuzzConfig, FuzzSummary, utc_timestamp_now};
use calc_verdict::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::Path;

pub const TEXTURE_CATALOGUE_FORMAT: &str = "dnacalc-texture-catalogue/1";

/// Six canonical texture categories (`dc-ztvv.26.8`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextureKind {
    ErrorVsNumberNeighbour,
    BoundaryRepresentative,
    RouteSwitch,
    NearTie,
    RegimeChange,
    ExtremeMagnitude,
}

impl TextureKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ErrorVsNumberNeighbour => "error_vs_number_neighbour",
            Self::BoundaryRepresentative => "boundary_representative",
            Self::RouteSwitch => "route_switch",
            Self::NearTie => "near_tie",
            Self::RegimeChange => "regime_change",
            Self::ExtremeMagnitude => "extreme_magnitude",
        }
    }

    pub const fn short_tag(self) -> &'static str {
        match self {
            Self::ErrorVsNumberNeighbour => "evn",
            Self::BoundaryRepresentative => "bnd",
            Self::RouteSwitch => "rte",
            Self::NearTie => "tie",
            Self::RegimeChange => "reg",
            Self::ExtremeMagnitude => "mag",
        }
    }
}

/// Compact provenance identifying the fuzzer recipe that produced a texture case.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureRecipeRef {
    pub generator_version: String,
    pub seed: u64,
    pub budget: usize,
    pub fractal_depth: usize,
    pub calc_func_commit: String,
    pub region: String,
    pub hint_family: Option<String>,
}

/// One catalogued interesting texture case (kept even when DNA Calc matches Excel).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextureCase {
    pub id: String,
    pub function: String,
    pub args: Vec<Value>,
    pub kind: TextureKind,
    pub why: String,
    pub recipe: TextureRecipeRef,
    pub dna_calc_result: String,
    pub route_mask: String,
    pub iterations: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boundary_margin: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_excel: Option<bool>,
}

/// Versioned catalogue of interesting texture cases (`dnacalc-texture-catalogue/1`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextureCatalogue {
    pub format: String,
    pub updated_utc: String,
    pub entries: Vec<TextureCase>,
}

/// Result of replaying a [`TextureCatalogue`] through real `calc_func::invoke`.
#[derive(Clone, Debug)]
pub struct TextureValidationReport {
    pub total: usize,
    pub matched_recorded: usize,
    pub drifted: Vec<String>,
    pub functions: BTreeSet<String>,
    pub by_kind: BTreeMap<String, usize>,
}

fn deterministic_texture_id(function: &str, kind: TextureKind, args: &[Value]) -> String {
    let key = format!(
        "{function}|{}|{}",
        kind.as_str(),
        serde_json::to_string(&Value::Array(args.to_vec())).unwrap_or_default()
    );
    let hash = sha256_hex(key.as_bytes());
    format!("tex-{}-{}-{}", function.to_ascii_lowercase(), kind.short_tag(), &hash[..10])
}

fn args_complexity(args: &[Value]) -> u64 {
    (args.len() as u64) * 25 + args.iter().map(arg_complexity).sum::<u64>()
}

fn has_extreme_magnitude_arg(args: &[Value]) -> Option<String> {
    for (idx, arg) in args.iter().enumerate() {
        if let Some(bits) = parse_hex_bits(arg) {
            let v = f64::from_bits(bits);
            let abs = v.abs();
            if abs == f64::MAX {
                return Some(format!("arg[{idx}] is +-DBL_MAX (0x{bits:016x})"));
            }
            if abs == f64::MIN_POSITIVE {
                return Some(format!("arg[{idx}] is +-MIN_POSITIVE (0x{bits:016x})"));
            }
            if abs >= 1e200 {
                return Some(format!("arg[{idx}] has huge magnitude {v:.3e} (0x{bits:016x})"));
            }
            if abs > 0.0 && abs <= 1e-200 {
                return Some(format!("arg[{idx}] has tiny magnitude {v:.3e} (0x{bits:016x})"));
            }
            if abs == 9_007_199_254_740_992.0 || abs == 9_007_199_254_740_991.0 {
                return Some(format!("arg[{idx}] sits at 2^53 integer-precision edge (0x{bits:016x})"));
            }
        } else if let Some(rows) = arg.as_array() {
            for row in rows {
                if let Some(cells) = row.as_array() {
                    for cell in cells {
                        if let Some(bits) = parse_hex_bits(cell) {
                            let v = f64::from_bits(bits);
                            let abs = v.abs();
                            if abs >= 1e200 || (abs > 0.0 && abs <= 1e-200) {
                                return Some(format!(
                                    "array arg[{idx}] contains extreme magnitude {v:.3e} (0x{bits:016x})"
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn has_near_tie_arg(args: &[Value]) -> Option<String> {
    for (idx, arg) in args.iter().enumerate() {
        if let Some(bits) = parse_hex_bits(arg) {
            let v = f64::from_bits(bits);
            if !v.is_finite() || v.abs() > 1e9 {
                continue;
            }
            let frac = v.fract().abs();
            let dist_half = (frac - 0.5).abs();
            if dist_half > 0.0 && dist_half <= 1e-12 {
                return Some(format!(
                    "arg[{idx}] is within {dist_half:.2e} of a half-integer tie (0x{bits:016x})"
                ));
            }
            let dist_int = frac.min(1.0 - frac);
            if dist_int > 0.0 && dist_int <= 1e-12 {
                return Some(format!(
                    "arg[{idx}] is within {dist_int:.2e} of an integer boundary (0x{bits:016x})"
                ));
            }
        }
    }
    None
}

fn is_error_vs_number_frontier(f: &Frontier) -> bool {
    let a = predicate_tag(&f.dna_lo_outcome);
    let b = predicate_tag(&f.dna_hi_outcome);
    (a == "number" && b.starts_with("error:")) || (b == "number" && a.starts_with("error:"))
}

/// Deterministically extract up to `max_entries` interesting texture cases from a fuzz sweep
/// (covering all 6 categories, keeping matching cases as well as discrepant ones).
#[allow(clippy::too_many_arguments)]
pub fn extract_texture_catalogue(
    cfg: &FuzzConfig,
    spec: &DomainSpec,
    summary: &FuzzSummary,
    cases_sample: &[FuzzCase],
    excel_answers: &HashMap<String, String>,
    calc_func_commit: &str,
    max_entries: usize,
) -> TextureCatalogue {
    let ctx = touchstone_bulk_context();
    let func = &summary.function;
    let hint_family = spec.merged.hint_family.clone();

    // Keyed by (TextureKind, bucket_subkey) -> (complexity, TextureCase) so each distinct texture
    // phenomenon keeps its cleanest minimal-complexity representative.
    let mut best_by_bucket: BTreeMap<(TextureKind, String), (u64, TextureCase)> = BTreeMap::new();

    let make_recipe = |region: &str| TextureRecipeRef {
        generator_version: GENERATOR_VERSION.to_owned(),
        seed: summary.seed,
        budget: summary.budget,
        fractal_depth: cfg.fractal_depth,
        calc_func_commit: calc_func_commit.to_owned(),
        region: region.to_owned(),
        hint_family: hint_family.clone(),
    };

    let matched_status = |args: &[Value], dna_pred: &str| -> Option<bool> {
        let key = canonical_case_key(func, args);
        excel_answers.get(&key).map(|ex| {
            super::cluster::judge_pair(ex, dna_pred).0 == calc_verdict::Severity::Exact
        })
    };

    let mut consider = |kind: TextureKind, bucket: String, case: TextureCase| {
        let score = args_complexity(&case.args);
        let key = (kind, bucket);
        match best_by_bucket.get(&key) {
            Some((prev_score, prev_case))
                if *prev_score < score || (*prev_score == score && prev_case.id <= case.id) => {}
            _ => {
                best_by_bucket.insert(key, (score, case));
            }
        }
    };

    // 1. Frontiers -> ErrorVsNumberNeighbour, BoundaryRepresentative, RouteSwitch, RegimeChange.
    for f in &summary.frontiers {
        let (kind, bucket_prefix) = if is_error_vs_number_frontier(f) {
            (TextureKind::ErrorVsNumberNeighbour, format!("evn:{}:{}:{}", f.axis_name, f.kind, f.depth))
        } else if f.transition_kind == "route_switch" {
            (TextureKind::RouteSwitch, format!("rf:{}:{}", f.axis_name, f.depth))
        } else if f.transition_kind == "iteration_regime" || f.transition_kind == "outcome_regime" {
            (TextureKind::RegimeChange, format!("rc:{}:{}", f.axis_name, f.kind))
        } else {
            (TextureKind::BoundaryRepresentative, format!("bnd:{}:{}:{}", f.axis_name, f.kind, f.depth))
        };

        for (side, bits, outcome) in [
            ("lo", f.dna_lo_bits, &f.dna_lo_outcome),
            ("hi", f.dna_hi_bits, &f.dna_hi_outcome),
        ] {
            let mut args = f.base_args.clone();
            if !set_axis_f64(&mut args, f.axis, f.axis_cell, f64::from_bits(bits)) {
                continue;
            }
            let (pred, trace, _) = evaluate_traced(func, &args, &ctx);
            let why = format!(
                "{side} side of {} {} frontier on {} (depth={}, scale={}, lo=0x{:016x} {}, hi=0x{:016x} {})",
                f.transition_kind,
                f.kind,
                f.axis_name,
                f.depth,
                f.scale,
                f.dna_lo_bits,
                f.dna_lo_outcome,
                f.dna_hi_bits,
                f.dna_hi_outcome,
            );
            let entry = TextureCase {
                id: deterministic_texture_id(func, kind, &args),
                function: func.clone(),
                args: args.clone(),
                kind,
                why,
                recipe: make_recipe(&format!("frontier:{}", f.axis_name)),
                dna_calc_result: if pred.is_empty() { outcome.clone() } else { pred.clone() },
                route_mask: format!("0x{:x}", trace.route_mask),
                iterations: trace.iterations,
                boundary_margin: trace.min_boundary_margin.is_finite().then_some(trace.min_boundary_margin),
                matched_excel: matched_status(&args, &pred),
            };
            consider(kind, format!("{bucket_prefix}:{side}"), entry);
        }
    }

    // 2. CoverageTracker kernel route exemplars -> RouteSwitch.
    for (bit, opt_args) in summary.tracker.route_exemplars.iter().enumerate() {
        let Some(args) = opt_args else {
            continue;
        };
        let route_name = summary
            .tracker
            .kernel_route_names
            .get(bit)
            .copied()
            .unwrap_or("route");
        let (pred, trace, _) = evaluate_traced(func, args, &ctx);
        let entry = TextureCase {
            id: deterministic_texture_id(func, TextureKind::RouteSwitch, args),
            function: func.clone(),
            args: args.clone(),
            kind: TextureKind::RouteSwitch,
            why: format!("exercises kernel route bit {bit} ({route_name})"),
            recipe: make_recipe("tracker:route_exemplar"),
            dna_calc_result: pred.clone(),
            route_mask: format!("0x{:x}", trace.route_mask),
            iterations: trace.iterations,
            boundary_margin: trace.min_boundary_margin.is_finite().then_some(trace.min_boundary_margin),
            matched_excel: matched_status(args, &pred),
        };
        consider(TextureKind::RouteSwitch, format!("bit:{bit:02}"), entry);
    }

    // 3. CoverageTracker closest boundary margins -> NearTie.
    for (label, (margin, args, _)) in &summary.tracker.closest_by_boundary {
        if args.is_empty() || !margin.is_finite() {
            continue;
        }
        let (pred, trace, _) = evaluate_traced(func, args, &ctx);
        let entry = TextureCase {
            id: deterministic_texture_id(func, TextureKind::NearTie, args),
            function: func.clone(),
            args: args.clone(),
            kind: TextureKind::NearTie,
            why: format!("closest approach to kernel boundary predicate '{label}' (margin {margin:.3e})"),
            recipe: make_recipe("tracker:closest_boundary"),
            dna_calc_result: pred.clone(),
            route_mask: format!("0x{:x}", trace.route_mask),
            iterations: trace.iterations,
            boundary_margin: Some(*margin),
            matched_excel: matched_status(args, &pred),
        };
        consider(TextureKind::NearTie, format!("pred:{label}"), entry);
    }

    // 4. Evaluated sweep cases -> NearTie, RegimeChange, ExtremeMagnitude, and fallback RouteSwitch.
    for c in cases_sample {
        let iter_b = iteration_regime_bucket(c.trace.iterations);

        // RegimeChange: distinct (outcome_class, iter_bucket) regimes (especially high-iter, zero, huge, subnormal).
        let interesting_regime = c.trace.iterations >= 50
            || matches!(
                c.outcome_class.as_str(),
                "num:zero" | "num:subnormal" | "num:huge_r2" | "num:huge_r3" | "num:tiny_pos" | "num:tiny_neg"
            )
            || c.outcome_class.starts_with("err:");
        if interesting_regime {
            let entry = TextureCase {
                id: deterministic_texture_id(func, TextureKind::RegimeChange, &c.args),
                function: func.clone(),
                args: c.args.clone(),
                kind: TextureKind::RegimeChange,
                why: format!(
                    "regime exemplar: outcome={} iter_bucket={} (iters={})",
                    c.outcome_class, iter_b, c.trace.iterations
                ),
                recipe: make_recipe(&c.region),
                dna_calc_result: c.prediction.clone(),
                route_mask: format!("0x{:x}", c.trace.route_mask),
                iterations: c.trace.iterations,
                boundary_margin: c.trace.min_boundary_margin.is_finite().then_some(c.trace.min_boundary_margin),
                matched_excel: matched_status(&c.args, &c.prediction),
            };
            consider(
                TextureKind::RegimeChange,
                format!("oc:{}|ib:{iter_b}", c.outcome_class),
                entry,
            );
        }

        // NearTie: near-integer / near-halfway inputs or tight boundary margin.
        if let Some(reason) = has_near_tie_arg(&c.args) {
            let entry = TextureCase {
                id: deterministic_texture_id(func, TextureKind::NearTie, &c.args),
                function: func.clone(),
                args: c.args.clone(),
                kind: TextureKind::NearTie,
                why: reason,
                recipe: make_recipe(&c.region),
                dna_calc_result: c.prediction.clone(),
                route_mask: format!("0x{:x}", c.trace.route_mask),
                iterations: c.trace.iterations,
                boundary_margin: c.trace.min_boundary_margin.is_finite().then_some(c.trace.min_boundary_margin),
                matched_excel: matched_status(&c.args, &c.prediction),
            };
            consider(TextureKind::NearTie, format!("arg_tie:{}", c.outcome_class), entry);
        } else if c.trace.min_boundary_margin.is_finite() && c.trace.min_boundary_margin < 1e-4 {
            let label = c.trace.closest_boundary.unwrap_or("threshold");
            let entry = TextureCase {
                id: deterministic_texture_id(func, TextureKind::NearTie, &c.args),
                function: func.clone(),
                args: c.args.clone(),
                kind: TextureKind::NearTie,
                why: format!(
                    "narrow margin {:.3e} to boundary '{label}'",
                    c.trace.min_boundary_margin
                ),
                recipe: make_recipe(&c.region),
                dna_calc_result: c.prediction.clone(),
                route_mask: format!("0x{:x}", c.trace.route_mask),
                iterations: c.trace.iterations,
                boundary_margin: Some(c.trace.min_boundary_margin),
                matched_excel: matched_status(&c.args, &c.prediction),
            };
            consider(TextureKind::NearTie, format!("margin:{label}"), entry);
        }

        // ExtremeMagnitude: extreme normal inputs or huge/subnormal outputs.
        if let Some(reason) = has_extreme_magnitude_arg(&c.args) {
            let entry = TextureCase {
                id: deterministic_texture_id(func, TextureKind::ExtremeMagnitude, &c.args),
                function: func.clone(),
                args: c.args.clone(),
                kind: TextureKind::ExtremeMagnitude,
                why: reason,
                recipe: make_recipe(&c.region),
                dna_calc_result: c.prediction.clone(),
                route_mask: format!("0x{:x}", c.trace.route_mask),
                iterations: c.trace.iterations,
                boundary_margin: c.trace.min_boundary_margin.is_finite().then_some(c.trace.min_boundary_margin),
                matched_excel: matched_status(&c.args, &c.prediction),
            };
            consider(TextureKind::ExtremeMagnitude, format!("in:{}", c.outcome_class), entry);
        } else if matches!(c.outcome_class.as_str(), "num:subnormal" | "num:huge_r2" | "num:huge_r3") {
            let entry = TextureCase {
                id: deterministic_texture_id(func, TextureKind::ExtremeMagnitude, &c.args),
                function: func.clone(),
                args: c.args.clone(),
                kind: TextureKind::ExtremeMagnitude,
                why: format!("extreme output magnitude ({})", c.outcome_class),
                recipe: make_recipe(&c.region),
                dna_calc_result: c.prediction.clone(),
                route_mask: format!("0x{:x}", c.trace.route_mask),
                iterations: c.trace.iterations,
                boundary_margin: c.trace.min_boundary_margin.is_finite().then_some(c.trace.min_boundary_margin),
                matched_excel: matched_status(&c.args, &c.prediction),
            };
            consider(TextureKind::ExtremeMagnitude, format!("out:{}", c.outcome_class), entry);
        }
    }

    // Round-robin across the 6 TextureKind categories so no single category starves the others
    // under `max_entries`, while deduplicating by canonical (function, args).
    let mut by_kind: BTreeMap<TextureKind, Vec<TextureCase>> = BTreeMap::new();
    for ((kind, _), (_, entry)) in best_by_bucket {
        by_kind.entry(kind).or_default().push(entry);
    }
    let mut selected: Vec<TextureCase> = Vec::with_capacity(max_entries);
    let mut seen_canonical: BTreeSet<String> = BTreeSet::new();
    let mut pos = 0usize;
    while selected.len() < max_entries {
        let mut added_any = false;
        for list in by_kind.values() {
            if let Some(entry) = list.get(pos) {
                added_any = true;
                let key = canonical_case_key(&entry.function, &entry.args);
                if seen_canonical.insert(key) {
                    selected.push(entry.clone());
                    if selected.len() >= max_entries {
                        break;
                    }
                }
            }
        }
        if !added_any {
            break;
        }
        pos += 1;
    }

    selected.sort_by(|a, b| {
        a.function
            .cmp(&b.function)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.id.cmp(&b.id))
    });

    TextureCatalogue {
        format: TEXTURE_CATALOGUE_FORMAT.to_owned(),
        updated_utc: utc_timestamp_now(),
        entries: selected,
    }
}

/// Write a [`TextureCatalogue`] JSON file to `path`.
pub fn write_texture_catalogue(path: &Path, catalogue: &TextureCatalogue) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(catalogue).map_err(|e| e.to_string())? + "\n";
    fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

/// Load a [`TextureCatalogue`] JSON file from `path`.
pub fn read_texture_catalogue(path: &Path) -> Result<TextureCatalogue, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let cat: TextureCatalogue =
        serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;
    if cat.format != TEXTURE_CATALOGUE_FORMAT {
        return Err(format!(
            "{}: unsupported texture catalogue format {:?}",
            path.display(),
            cat.format
        ));
    }
    Ok(cat)
}

/// Merge `incoming` texture cases into an existing (or new) catalogue file at `path`,
/// deduplicating by canonical `(function, args)` in deterministic sorted order.
pub fn merge_texture_catalogue_file(
    path: &Path,
    incoming: &TextureCatalogue,
) -> Result<TextureCatalogue, String> {
    let mut by_key: BTreeMap<String, TextureCase> = BTreeMap::new();
    if path.is_file() {
        let existing = read_texture_catalogue(path)?;
        for entry in existing.entries {
            let key = canonical_case_key(&entry.function, &entry.args);
            by_key.insert(key, entry);
        }
    }
    for entry in &incoming.entries {
        let key = canonical_case_key(&entry.function, &entry.args);
        by_key
            .entry(key)
            .and_modify(|prev| {
                if !prev.why.contains(&entry.why) {
                    prev.why = format!("{}; {}", prev.why, entry.why);
                }
                if prev.matched_excel.is_none() && entry.matched_excel.is_some() {
                    prev.matched_excel = entry.matched_excel;
                }
            })
            .or_insert_with(|| entry.clone());
    }
    let mut merged: Vec<TextureCase> = by_key.into_values().collect();
    merged.sort_by(|a, b| {
        a.function
            .cmp(&b.function)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.id.cmp(&b.id))
    });
    let catalogue = TextureCatalogue {
        format: TEXTURE_CATALOGUE_FORMAT.to_owned(),
        updated_utc: utc_timestamp_now(),
        entries: merged,
    };
    write_texture_catalogue(path, &catalogue)?;
    Ok(catalogue)
}

/// Replay every case in a [`TextureCatalogue`] file through real `calc_func::invoke`
/// (`evaluate_traced`), checking determinism and comparing against recorded `dna_calc_result`.
pub fn validate_texture_catalogue_file(path: &Path) -> Result<TextureValidationReport, String> {
    let cat = read_texture_catalogue(path)?;
    let ctx = touchstone_bulk_context();
    let mut matched_recorded = 0usize;
    let mut drifted = Vec::new();
    let mut functions = BTreeSet::new();
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();

    for entry in &cat.entries {
        functions.insert(entry.function.clone());
        *by_kind.entry(entry.kind.as_str().to_owned()).or_insert(0) += 1;
        let (actual_1, _, _) = evaluate_traced(&entry.function, &entry.args, &ctx);
        let (actual_2, _, _) = evaluate_traced(&entry.function, &entry.args, &ctx);
        if actual_1 != actual_2 {
            drifted.push(format!(
                "{} ({}): nondeterministic replay ({actual_1} vs {actual_2})",
                entry.id, entry.function
            ));
            continue;
        }
        if actual_1 == entry.dna_calc_result {
            matched_recorded += 1;
        } else {
            drifted.push(format!(
                "{} ({}): recorded {} != actual {}",
                entry.id, entry.function, entry.dna_calc_result, actual_1
            ));
        }
    }

    Ok(TextureValidationReport {
        total: cat.entries.len(),
        matched_recorded,
        drifted,
        functions,
        by_kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuzz::{FuzzConfig, execute_dry_run};

    #[test]
    fn texture_catalogue_extracts_interesting_cases_deterministically_and_replays_cleanly() {
        let d1 = std::env::temp_dir().join("dnacalc-fuzz-texture-rate-1");
        let d2 = std::env::temp_dir().join("dnacalc-fuzz-texture-rate-2");
        let _ = fs::remove_dir_all(&d1);
        let _ = fs::remove_dir_all(&d2);

        let mut c1 = FuzzConfig::default_for("RATE");
        c1.seed = 20261124;
        c1.budget = 260;
        c1.round_size = 130;
        c1.max_frontiers = 8;
        c1.fractal_depth = 1;
        c1.frontier_window = 3;
        c1.dry_run = true;
        c1.out_dir = d1.clone();
        let mut c2 = c1.clone();
        c2.out_dir = d2.clone();

        let _ = execute_dry_run(&c1).expect("run 1");
        let _ = execute_dry_run(&c2).expect("run 2");

        let cat1 = read_texture_catalogue(&d1.join("textures.json")).expect("read cat 1");
        let cat2 = read_texture_catalogue(&d2.join("textures.json")).expect("read cat 2");
        assert_eq!(cat1.format, TEXTURE_CATALOGUE_FORMAT);
        assert_eq!(
            cat1.entries, cat2.entries,
            "texture catalogue entries must be deterministic from the same seed"
        );
        assert!(cat1.entries.len() >= 10, "expected >=10 texture entries, got {}", cat1.entries.len());

        let kinds: BTreeSet<TextureKind> = cat1.entries.iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&TextureKind::ErrorVsNumberNeighbour), "missing error_vs_number_neighbour in {kinds:?}");
        assert!(kinds.contains(&TextureKind::RouteSwitch), "missing route_switch in {kinds:?}");
        assert!(kinds.contains(&TextureKind::NearTie), "missing near_tie in {kinds:?}");
        assert!(kinds.contains(&TextureKind::RegimeChange), "missing regime_change in {kinds:?}");
        assert!(kinds.contains(&TextureKind::ExtremeMagnitude), "missing extreme_magnitude in {kinds:?}");

        for e in &cat1.entries {
            assert_eq!(e.function, "RATE");
            assert!(!e.args.is_empty());
            assert!(!e.why.is_empty());
            assert_eq!(e.recipe.seed, 20261124);
            assert_eq!(e.recipe.generator_version, GENERATOR_VERSION);
            assert!(!e.recipe.calc_func_commit.is_empty());
        }

        // Replay every catalogued texture case through real calc_func::invoke!
        let val = validate_texture_catalogue_file(&d1.join("textures.json")).expect("validate");
        assert_eq!(val.total, cat1.entries.len());
        assert_eq!(val.matched_recorded, val.total);
        assert!(val.drifted.is_empty(), "drifted: {:?}", val.drifted);

        let _ = fs::remove_dir_all(&d1);
        let _ = fs::remove_dir_all(&d2);
    }

    #[test]
    fn texture_catalogue_merges_across_functions_without_duplicates() {
        let tmp = std::env::temp_dir().join("dnacalc-fuzz-texture-merge-test");
        let _ = fs::remove_dir_all(&tmp);
        let shared = tmp.join("shared-textures.json");

        for func in ["PV", "RATE"] {
            let mut cfg = FuzzConfig::default_for(func);
            cfg.seed = 20261124;
            cfg.budget = 80;
            cfg.round_size = 80;
            cfg.max_frontiers = 4;
            cfg.fractal_depth = 1;
            cfg.frontier_window = 2;
            cfg.dry_run = true;
            cfg.out_dir = tmp.join(func.to_ascii_lowercase());
            cfg.texture_out = Some(shared.clone());
            let _ = execute_dry_run(&cfg).expect(func);
        }

        let merged = read_texture_catalogue(&shared).expect("merged catalogue");
        let funcs: BTreeSet<&str> = merged.entries.iter().map(|e| e.function.as_str()).collect();
        assert!(funcs.contains("PV") && funcs.contains("RATE"), "funcs={funcs:?}");

        let unique_keys: BTreeSet<String> = merged
            .entries
            .iter()
            .map(|e| canonical_case_key(&e.function, &e.args))
            .collect();
        assert_eq!(unique_keys.len(), merged.entries.len(), "no duplicate canonical texture entries");

        // Re-merging identical entries must be idempotent.
        let count_before = merged.entries.len();
        let merged_again = merge_texture_catalogue_file(&shared, &merged).expect("idempotent merge");
        assert_eq!(merged_again.entries.len(), count_before);

        let val = validate_texture_catalogue_file(&shared).expect("validate merged");
        assert_eq!(val.matched_recorded, val.total);
        assert!(val.drifted.is_empty());

        let _ = fs::remove_dir_all(&tmp);
    }
}
