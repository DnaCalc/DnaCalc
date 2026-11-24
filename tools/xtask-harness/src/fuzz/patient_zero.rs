//! Patient-zero infestation mapping & clean re-sweep protocol (`dc-ztvv.26.6`).
//!
//! Owner direction (2026-11-24): a failing case is patient zero of a possible infestation.
//! For each discrepancy:
//! 1. **Neighbourhood**: fractal boundary refinement (`Decade -> Binade -> ULP` + edges of edges)
//!    plus direct ULP/Binade neighbourhood probes around every numeric coordinate of patient zero;
//! 2. **Same route under other arguments**: code-steered generation targeting DNA Calc's same
//!    kernel `route_mask` (or signature when uninstrumented) under varied magnitudes, signs, and
//!    optional arguments;
//! 3. **Sibling functions sharing the kernel**: discovers sibling functions from `docs/fuzz/order.tsv`
//!    (`kernel_file` + `family`), transfers patient zero's exact bit patterns into matching parameter
//!    slots of each sibling, and sweeps each sibling's domain;
//! 4. **Re-sweep to close**: persists a deterministic `resweep-recipe.json`; a cluster closes
//!    (`can_close == true`) ONLY after a live re-sweep confirms `0` failures (`0` mismatched,
//!    `0` unread, `0` crashed) on patient zero, the primary region, AND every sibling function.

use super::boundary::{
    Frontier, expand_frontier_windows_with_refs, get_axis_bits, set_axis_f64,
};
use super::cluster::{DiscrepancyCluster, DiscrepancyRow, RegionStats, build_clusters};
use super::domain::{DomainSpec, ParamKind, mutate_case};
use super::fractal::{
    BoundarySignature, FractalConfig, FractalDiscoveryStats, binade_stencil, decade_stencil,
    fractal_discover_local, ulp_stencil,
};
use super::hints::{HintBank, load_default_hints};
use super::oracle::{
    ask_single_priority, connect_client, fractal_refine_coarse_discrepancies_excel,
    ingest_judged_cases,
};
use super::rng::{
    GENERATOR_VERSION, Rng, StreamCaseHasher, canonical_case_key, hex_f64, parse_hex_bits,
    step_ulps,
};
use super::steer::{FuzzCase, evaluate_traced, touchstone_bulk_context};
use super::{FuzzConfig, freeze_predictions, utc_timestamp_now};
use calc_verdict::WitnessSet;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use touchstone_client::Priority;

pub const PATIENT_ZERO_FORMAT: &str = "dnacalc-patient-zero/1";
pub const RESWEEP_RECIPE_FORMAT: &str = "dnacalc-resweep-recipe/1";
pub const RESWEEP_CASES_FORMAT: &str = "dnacalc-resweep-cases/1";
pub const RESWEEP_VERDICT_FORMAT: &str = "dnacalc-resweep-verdict/1";

/// Broad shared helper files where siblings must also match the `family` column in `order.tsv`.
const BROAD_SHARED_KERNELS: &[&str] = &[
    "crates/calc-func/src/ported/text_kernels.rs",
    "crates/calc-func/src/ported/variance_common.rs",
    "crates/calc-func/src/ported/excel_numeric/soft87.rs",
    "crates/calc-func/src/ported/excel_numeric/gammaln.rs",
    "crates/calc-func/src/ported/excel_numeric/mod.rs",
    "crates/calc-func/src/ported/a1_refs.rs",
];

/// One failing case entering the patient-zero protocol.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PatientZeroSeed {
    pub id: String,
    pub function: String,
    pub args: Vec<Value>,
    #[serde(default)]
    pub expected: Option<String>,
    #[serde(default)]
    pub cluster_key: Option<String>,
}

/// One sibling function sharing the kernel of the patient-zero function.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiblingInfo {
    pub function: String,
    pub family: String,
    pub kernel_file: String,
    pub rank: usize,
}

/// Tunables for patient-zero infestation mapping and re-sweeps.
#[derive(Clone, Debug)]
pub struct PatientZeroConfig {
    pub seed: u64,
    pub neighbourhood_budget: usize,
    pub same_route_budget: usize,
    pub sibling_budget: usize,
    pub max_siblings: usize,
    pub siblings_override: Vec<String>,
    pub fractal_depth: usize,
    pub max_frontiers: usize,
    pub frontier_window: i64,
}

impl PatientZeroConfig {
    pub fn from_fuzz_config(cfg: &FuzzConfig) -> Self {
        let primary = cfg.budget.clamp(40, 20_000);
        Self {
            seed: cfg.seed,
            neighbourhood_budget: (primary / 2).max(20),
            same_route_budget: (primary / 2).max(20),
            sibling_budget: (primary / 4).clamp(16, 2_000),
            max_siblings: cfg.max_siblings,
            siblings_override: cfg.pz_siblings.clone(),
            fractal_depth: cfg.fractal_depth,
            max_frontiers: cfg.max_frontiers.clamp(4, 32),
            frontier_window: cfg.frontier_window,
        }
    }
}

/// One persisted pre-fix input case saved alongside `resweep-recipe.json` so [`execute_resweep`]
/// replays the exact pre-fix suite verbatim without storing any Excel answers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistedResweepCase {
    pub id: String,
    pub region: String,
    pub args: Vec<Value>,
}

/// Persisted pre-fix primary and sibling case lists saved alongside `resweep-recipe.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResweepCaseList {
    pub format: String,
    pub function: String,
    pub primary_cases: Vec<PersistedResweepCase>,
    pub sibling_cases: BTreeMap<String, Vec<PersistedResweepCase>>,
    pub cases_sha256: String,
}

/// Reproducible re-sweep recipe written by [`execute_patient_zero`] and consumed by [`execute_resweep`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResweepRecipe {
    pub format: String,
    pub generator_version: String,
    pub created_utc: String,
    pub lane: String,
    pub function: String,
    pub patient_zero: PatientZeroSeed,
    pub target_route_mask: String,
    pub target_predicate: String,
    pub target_outcome_class: String,
    pub siblings: Vec<String>,
    pub seed: u64,
    pub neighbourhood_budget: usize,
    pub same_route_budget: usize,
    pub sibling_budget: usize,
    pub fractal_depth: usize,
    pub max_frontiers: usize,
    pub frontier_window: i64,
    pub initial_primary_asked: usize,
    pub initial_primary_discrepancies: usize,
    pub initial_sibling_discrepancies: BTreeMap<String, usize>,
    pub initial_clusters: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cases_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cases_sha256: Option<String>,
}

/// Per-function sweep statistics recorded in patient-zero reports and re-sweep verdicts.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FunctionSweepCounts {
    pub function: String,
    pub generated: usize,
    pub asked: usize,
    pub matched: usize,
    pub mismatched: usize,
    #[serde(default)]
    pub harness_fault: usize,
    pub unread: usize,
    pub crashed: usize,
    pub clusters: usize,
    pub predictions_sha256: String,
}

impl FunctionSweepCounts {
    pub fn failed(&self) -> usize {
        self.mismatched + self.harness_fault + self.unread + self.crashed
    }
}

/// Deterministic infestation suite built around one patient-zero case.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct PatientZeroSuite {
    pub seed: PatientZeroSeed,
    pub seed_case: FuzzCase,
    pub target_signature: BoundarySignature,
    pub primary_cases: Vec<FuzzCase>,
    pub primary_frontiers: Vec<Frontier>,
    pub fractal_stats: FractalDiscoveryStats,
    pub siblings: Vec<SiblingInfo>,
    pub sibling_cases: BTreeMap<String, Vec<FuzzCase>>,
    pub suite_sha256: String,
}

/// Result of running the patient-zero investigation protocol (`execute_patient_zero`).
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct PatientZeroReport {
    pub patient_zero: PatientZeroSeed,
    pub primary: FunctionSweepCounts,
    pub neighbourhood_cases: usize,
    pub same_route_cases: usize,
    pub frontiers_mapped: usize,
    pub siblings: BTreeMap<String, FunctionSweepCounts>,
    pub primary_clusters: Vec<DiscrepancyCluster>,
    pub suite_sha256: String,
    pub recipe_path: PathBuf,
    pub can_close: bool,
    pub closure_reason: String,
}

/// Result of running a closure re-sweep (`execute_resweep`).
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ResweepVerdict {
    pub function: String,
    pub patient_zero_id: String,
    pub patient_zero_matched: bool,
    pub primary: FunctionSweepCounts,
    pub siblings: BTreeMap<String, FunctionSweepCounts>,
    pub initial_primary_discrepancies: usize,
    pub initial_sibling_discrepancies: BTreeMap<String, usize>,
    pub can_close: bool,
    pub closure_reason: String,
}

fn order_tsv_path() -> PathBuf {
    let rel = PathBuf::from("docs/fuzz/order.tsv");
    if rel.is_file() {
        return rel;
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/fuzz/order.tsv")
}

/// Discover sibling worksheet functions sharing the same CalcFunc kernel as `function`
/// using `docs/fuzz/order.tsv` (`rank`, `function`, `family`, `kernel_file`).
pub fn discover_kernel_siblings(function: &str) -> Result<Vec<SiblingInfo>, String> {
    let target = function.trim().to_ascii_uppercase();
    let path = order_tsv_path();
    let text = fs::read_to_string(&path)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut rows: Vec<SiblingInfo> = Vec::new();
    let mut header_seen = false;
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !header_seen {
            header_seen = true;
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 19 {
            continue;
        }
        let Ok(rank) = cols[0].parse::<usize>() else {
            continue;
        };
        rows.push(SiblingInfo {
            function: cols[1].to_owned(),
            family: cols[2].to_owned(),
            kernel_file: cols[18].to_owned(),
            rank,
        });
    }
    let Some(me) = rows.iter().find(|r| r.function == target).cloned() else {
        return Ok(Vec::new());
    };
    if me.kernel_file == "-" || me.kernel_file.is_empty() {
        return Ok(Vec::new());
    }
    let require_family = BROAD_SHARED_KERNELS.contains(&me.kernel_file.as_str());
    let mut siblings: Vec<SiblingInfo> = rows
        .into_iter()
        .filter(|r| {
            r.function != target
                && r.kernel_file == me.kernel_file
                && (!require_family || r.family == me.family)
                && calc_catalogue::named(&r.function).is_some()
        })
        .collect();
    siblings.sort_by_key(|s| (s.rank, s.function.clone()));
    Ok(siblings)
}

/// Load one or more patient-zero seeds from a `dnacalc-witness-set/1` JSON file, a fuzz
/// `summary.json`, or a single-case JSON object (`{"function", "args", ...}`).
pub fn load_patient_zero_seeds(
    path: &Path,
    func_filter: Option<&str>,
) -> Result<Vec<PatientZeroSeed>, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let val: Value =
        serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;
    let filter_upper = func_filter.map(|f| f.trim().to_ascii_uppercase());

    // Case 1: WitnessSet JSON (`dnacalc-witness-set/1`)
    if val.get("format").and_then(Value::as_str) == Some(calc_verdict::WITNESS_SET_FORMAT) {
        let ws = WitnessSet::decode(raw.as_bytes())
            .map_err(|e| format!("decode WitnessSet {}: {e}", path.display()))?;
        let func = ws.function.to_ascii_uppercase();
        if let Some(ref f) = filter_upper
            && f != &func
        {
            return Err(format!(
                "{}: WitnessSet is for {func}, not requested {f}",
                path.display()
            ));
        }
        // Prefer failing rows (calc_func != expected); fall back to all witnesses if none marked.
        let mut failing = Vec::new();
        let mut all = Vec::new();
        for w in &ws.witnesses {
            let seed = PatientZeroSeed {
                id: w.id.clone(),
                function: func.clone(),
                args: w.args.clone(),
                expected: Some(w.expected.clone()),
                cluster_key: None,
            };
            if w.calc_func.as_deref().is_some_and(|c| {
                super::cluster::judge_pair(&w.expected, c).0 != calc_verdict::Severity::Exact
            }) {
                failing.push(seed.clone());
            }
            all.push(seed);
        }
        let chosen = if !failing.is_empty() { failing } else { all };
        if chosen.is_empty() {
            return Err(format!("{}: WitnessSet has no witnesses", path.display()));
        }
        return Ok(chosen);
    }

    // Case 2: Fuzz `summary.json` with `clusters[*].minimal_rep`
    if let Some(clusters) = val.get("clusters").and_then(Value::as_array) {
        let func = val
            .get("function")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{}: summary.json missing function", path.display()))?
            .to_ascii_uppercase();
        if let Some(ref f) = filter_upper
            && f != &func
        {
            return Err(format!(
                "{}: summary.json is for {func}, not requested {f}",
                path.display()
            ));
        }
        let mut out = Vec::new();
        for c in clusters {
            if let Some(rep) = c.get("minimal_rep")
                && let Some(args) = rep.get("args").and_then(Value::as_array)
            {
                out.push(PatientZeroSeed {
                    id: rep
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("cluster-rep")
                        .to_owned(),
                    function: func.clone(),
                    args: args.clone(),
                    expected: rep.get("excel").and_then(Value::as_str).map(str::to_owned),
                    cluster_key: c.get("key").and_then(Value::as_str).map(str::to_owned),
                });
            }
        }
        if out.is_empty() {
            return Err(format!(
                "{}: summary.json has no discrepancy clusters to trace",
                path.display()
            ));
        }
        return Ok(out);
    }

    // Case 3: Single-case JSON (`{"function": "...", "args": [...]}`)
    if let Some(args) = val.get("args").and_then(Value::as_array) {
        let func = val
            .get("function")
            .and_then(Value::as_str)
            .map(|s| s.to_ascii_uppercase())
            .or(filter_upper)
            .ok_or_else(|| format!("{}: missing function name", path.display()))?;
        return Ok(vec![PatientZeroSeed {
            id: val
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("patient-zero-001")
                .to_owned(),
            function: func,
            args: args.clone(),
            expected: val
                .get("expected_bits")
                .or_else(|| val.get("expected"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            cluster_key: val
                .get("cluster_key")
                .and_then(Value::as_str)
                .map(str::to_owned),
        }]);
    }

    Err(format!(
        "{}: unrecognized patient-zero JSON format (expected WitnessSet, summary.json, or single case)",
        path.display()
    ))
}

/// Transfer patient zero's argument bit patterns into matching parameter slots of a sibling function
/// (by normalized parameter name first, then by `ParamKind`), producing deterministic sibling probes
/// that carry the exact edge/extreme coordinates from patient zero across the shared kernel.
fn generate_sibling_transfer_args(
    primary_spec: &DomainSpec,
    primary_args: &[Value],
    sibling_spec: &DomainSpec,
    rng: &mut Rng,
) -> Vec<Value> {
    let base_region = rng.pick(&sibling_spec.regions).kind;
    let mut out = sibling_spec.generate(base_region, rng);
    for (s_idx, s_param) in sibling_spec.params.iter().enumerate() {
        if s_idx >= out.len() {
            break;
        }
        let s_name = s_param.name.to_ascii_lowercase();
        // 1. Exact parameter name match (e.g. nper, pmt, pv, fv, type, settlement, maturity, basis)
        let matched_primary = primary_spec
            .params
            .iter()
            .enumerate()
            .find(|(i, p)| *i < primary_args.len() && p.name.to_ascii_lowercase() == s_name)
            .map(|(i, _)| &primary_args[i])
            // 2. Fallback: matching ParamKind (e.g. PaymentOrAmount, Periods, Rate, DateSerial, ArrayNumeric)
            .or_else(|| {
                primary_spec
                    .params
                    .iter()
                    .enumerate()
                    .find(|(i, p)| {
                        *i < primary_args.len()
                            && p.kind == s_param.kind
                            && s_param.kind != ParamKind::RealGeneral
                    })
                    .map(|(i, _)| &primary_args[i])
            });
        if let Some(val) = matched_primary {
            // Optional slight ULP perturbation on 35% of transfers to probe sibling boundaries around the bit pattern.
            if let Some(bits) = parse_hex_bits(val)
                && rng.chance(0.35)
            {
                let delta = *rng.pick(&[-4i64, -2, -1, 1, 2, 4]);
                let perturbed = step_ulps(f64::from_bits(bits), delta, true)
                    .unwrap_or_else(|| f64::from_bits(bits));
                out[s_idx] = hex_f64(perturbed);
            } else {
                out[s_idx] = val.clone();
            }
        }
    }
    out
}

/// Build the complete deterministic infestation suite around one [`PatientZeroSeed`]:
/// 1. Patient zero itself,
/// 2. Fractal boundary refinement + ULP/Binade/Decade neighbourhood around every numeric axis,
/// 3. Same-route cases under different arguments (matching DNA Calc's `route_mask` or signature),
/// 4. Sibling functions sharing the kernel (parameter transfers + sibling domain cases).
pub fn build_patient_zero_suite(
    seed: &PatientZeroSeed,
    hints: &HintBank,
    pz_cfg: &PatientZeroConfig,
) -> Result<PatientZeroSuite, String> {
    let ctx = touchstone_bulk_context();
    let spec = DomainSpec::for_function_with_hints(&seed.function, hints)?;
    let mut rng = Rng::from_seed_and_tag(pz_cfg.seed, &format!("pz|{}|{}", seed.function, seed.id));
    let mut seen_keys: HashSet<String> = HashSet::new();
    let mut primary_cases: Vec<FuzzCase> = Vec::new();
    let mut next_id = 0usize;

    // 0. Evaluate patient zero itself.
    let (seed_pred, seed_trace, seed_oc) = evaluate_traced(&seed.function, &seed.args, &ctx);
    let target_sig = BoundarySignature::from_eval(&seed_pred, &seed_trace, &seed_oc);
    let seed_key = canonical_case_key(&seed.function, &seed.args);
    seen_keys.insert(seed_key);
    next_id += 1;
    let seed_case = FuzzCase {
        id: format!("pz-{}-{:06}", seed.function, next_id),
        region: "pz:seed".to_owned(),
        round: 0,
        args: seed.args.clone(),
        prediction: seed_pred,
        trace: seed_trace.clone(),
        outcome_class: seed_oc,
    };
    primary_cases.push(seed_case.clone());

    // 1. Fractal neighbourhood mapping around patient zero.
    let fcfg = FractalConfig::new(pz_cfg.max_frontiers, pz_cfg.fractal_depth);
    let fres = fractal_discover_local(&spec, std::slice::from_ref(&seed_case), &ctx, &fcfg);
    let primary_frontiers = fres.frontiers;
    let fractal_stats = fres.stats;

    let expanded = expand_frontier_windows_with_refs(
        &primary_frontiers,
        pz_cfg.frontier_window,
        &ctx,
        &mut seen_keys,
        0,
        &mut next_id,
    );
    for (_, mut c) in expanded.cases {
        if primary_cases.len() >= 1 + pz_cfg.neighbourhood_budget {
            break;
        }
        c.region = format!("pz:fractal_{}", c.region);
        primary_cases.push(c);
    }

    // Direct ULP + Binade + Decade probes along every numeric axis of patient zero so even smooth
    // DNA Calc interiors get a full multi-scale neighbourhood around patient zero.
    for (axis, param) in spec.params.iter().enumerate() {
        if primary_cases.len() >= 1 + pz_cfg.neighbourhood_budget {
            break;
        }
        let cell = if matches!(param.kind, ParamKind::ArrayNumeric | ParamKind::CellRange) {
            [(0, 0), (1, 0), (0, 1), (1, 1), (2, 0)]
                .into_iter()
                .find(|&rc| get_axis_bits(&seed.args, axis, Some(rc)).is_some())
                .or(Some((0, 0)))
        } else {
            None
        };
        let Some(base_bits) = get_axis_bits(&seed.args, axis, cell) else {
            continue;
        };
        let base_val = f64::from_bits(base_bits);
        if !base_val.is_finite() {
            continue;
        }
        let mut stencil = ulp_stencil(base_val);
        let lo_bin = step_ulps(base_val, -65_536, true).unwrap_or(base_val * 0.5);
        let hi_bin = step_ulps(base_val, 65_536, true).unwrap_or(base_val * 2.0);
        if lo_bin < hi_bin {
            stencil.extend(binade_stencil(lo_bin, hi_bin));
        }
        stencil.extend(decade_stencil(base_val, &[]));
        stencil.sort_by_key(|v| super::rng::f64_to_compact(*v));
        stencil.dedup_by_key(|v| super::rng::f64_to_compact(*v));

        for v in stencil {
            if primary_cases.len() >= 1 + pz_cfg.neighbourhood_budget {
                break;
            }
            let mut args = seed.args.clone();
            if !set_axis_f64(&mut args, axis, cell, v) {
                continue;
            }
            let key = canonical_case_key(&seed.function, &args);
            if !seen_keys.insert(key) {
                continue;
            }
            let (pred, trace, oc) = evaluate_traced(&seed.function, &args, &ctx);
            next_id += 1;
            primary_cases.push(FuzzCase {
                id: format!("pz-{}-nb-{:06}", seed.function, next_id),
                region: format!("pz:neighbourhood:{}", param.name),
                round: 0,
                args,
                prediction: pred,
                trace,
                outcome_class: oc,
            });
        }
    }

    // 2. Same route under other arguments (code-steered search matching patient zero's route_mask
    //    or coarse BoundarySignature across different magnitudes/signs/arities).
    let target_mask = seed_trace.route_mask;
    let same_route_end = primary_cases.len() + pz_cfg.same_route_budget;
    let max_attempts = pz_cfg.same_route_budget.saturating_mul(24).max(400);
    let mut partial_fallback: Vec<(u32, Vec<Value>, String, calc_func::trace::RouteTrace, String)> =
        Vec::new();
    for attempt in 0..max_attempts {
        if primary_cases.len() >= same_route_end {
            break;
        }
        let cand_args = if attempt % 3 == 0 {
            mutate_case(&spec, &seed.args, &mut rng)
        } else {
            let region = rng.pick(&spec.regions);
            spec.generate(region.kind, &mut rng)
        };
        let key = canonical_case_key(&seed.function, &cand_args);
        if seen_keys.contains(&key) {
            continue;
        }
        let (pred, trace, oc) = evaluate_traced(&seed.function, &cand_args, &ctx);
        let sig = BoundarySignature::from_eval(&pred, &trace, &oc);
        let exact_route = if target_mask != 0 {
            trace.route_mask == target_mask
        } else {
            sig.predicate == target_sig.predicate && sig.outcome_class == target_sig.outcome_class
        };
        if exact_route {
            if seen_keys.insert(key) {
                next_id += 1;
                primary_cases.push(FuzzCase {
                    id: format!("pz-{}-sr-{:06}", seed.function, next_id),
                    region: "pz:same_route".to_owned(),
                    round: 0,
                    args: cand_args,
                    prediction: pred,
                    trace,
                    outcome_class: oc,
                });
            }
        } else if target_mask != 0 && partial_fallback.len() < pz_cfg.same_route_budget * 2 {
            let overlap = (trace.route_mask & target_mask).count_ones();
            if overlap > 0 {
                partial_fallback.push((overlap, cand_args, pred, trace, oc));
            }
        }
    }
    if primary_cases.len() < same_route_end && !partial_fallback.is_empty() {
        partial_fallback.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, args, pred, trace, oc) in partial_fallback {
            if primary_cases.len() >= same_route_end {
                break;
            }
            let key = canonical_case_key(&seed.function, &args);
            if seen_keys.insert(key) {
                next_id += 1;
                primary_cases.push(FuzzCase {
                    id: format!("pz-{}-sr-{:06}", seed.function, next_id),
                    region: "pz:same_route_overlap".to_owned(),
                    round: 0,
                    args,
                    prediction: pred,
                    trace,
                    outcome_class: oc,
                });
            }
        }
    }

    // 3. Sibling functions sharing the kernel.
    let siblings: Vec<SiblingInfo> = if !pz_cfg.siblings_override.is_empty() {
        pz_cfg
            .siblings_override
            .iter()
            .filter_map(|raw| {
                let name = raw.trim().to_ascii_uppercase();
                (name != seed.function && calc_catalogue::named(&name).is_some()).then(|| SiblingInfo {
                    function: name,
                    family: "override".to_owned(),
                    kernel_file: "override".to_owned(),
                    rank: 0,
                })
            })
            .take(pz_cfg.max_siblings.max(pz_cfg.siblings_override.len()))
            .collect()
    } else {
        discover_kernel_siblings(&seed.function)?
            .into_iter()
            .take(pz_cfg.max_siblings)
            .collect()
    };

    let mut sibling_cases: BTreeMap<String, Vec<FuzzCase>> = BTreeMap::new();
    for sib in &siblings {
        let Ok(sib_spec) = DomainSpec::for_function_with_hints(&sib.function, hints) else {
            continue;
        };
        let mut sib_rng = Rng::from_seed_and_tag(
            pz_cfg.seed,
            &format!("pz-sib|{}->{}|{}", seed.function, sib.function, seed.id),
        );
        let mut sib_seen: HashSet<String> = HashSet::new();
        let mut cases: Vec<FuzzCase> = Vec::with_capacity(pz_cfg.sibling_budget);
        let transfer_quota = pz_cfg.sibling_budget / 2;
        let max_transfer_attempts = transfer_quota.saturating_mul(8).max(32);
        for _ in 0..max_transfer_attempts {
            if cases.len() >= transfer_quota {
                break;
            }
            let args = generate_sibling_transfer_args(&spec, &seed.args, &sib_spec, &mut sib_rng);
            let key = canonical_case_key(&sib.function, &args);
            if !sib_seen.insert(key) {
                continue;
            }
            let (pred, trace, oc) = evaluate_traced(&sib.function, &args, &ctx);
            let idx = cases.len() + 1;
            cases.push(FuzzCase {
                id: format!("pz-sib-{}-{:05}", sib.function, idx),
                region: "pz:sibling_transfer".to_owned(),
                round: 0,
                args,
                prediction: pred,
                trace,
                outcome_class: oc,
            });
        }
        let max_dom_attempts = pz_cfg.sibling_budget.saturating_mul(6).max(64);
        for _ in 0..max_dom_attempts {
            if cases.len() >= pz_cfg.sibling_budget {
                break;
            }
            let region = sib_rng.pick(&sib_spec.regions);
            let args = sib_spec.generate(region.kind, &mut sib_rng);
            let key = canonical_case_key(&sib.function, &args);
            if !sib_seen.insert(key) {
                continue;
            }
            let (pred, trace, oc) = evaluate_traced(&sib.function, &args, &ctx);
            let idx = cases.len() + 1;
            cases.push(FuzzCase {
                id: format!("pz-sib-{}-{:05}", sib.function, idx),
                region: format!("pz:sibling_domain:{}", region.name),
                round: 0,
                args,
                prediction: pred,
                trace,
                outcome_class: oc,
            });
        }
        sibling_cases.insert(sib.function.clone(), cases);
    }

    // Hash the entire deterministic suite (primary + all siblings in sorted order).
    let mut hasher = StreamCaseHasher::new(&seed.function);
    for c in &primary_cases {
        hasher.update_case(&c.region, &c.args);
    }
    for (sfunc, scases) in &sibling_cases {
        for c in scases {
            hasher.update_case(&format!("{sfunc}:{}", c.region), &c.args);
        }
    }
    let suite_sha256 = hasher.finish_hex();

    Ok(PatientZeroSuite {
        seed: seed.clone(),
        seed_case,
        target_signature: target_sig,
        primary_cases,
        primary_frontiers,
        fractal_stats,
        siblings,
        sibling_cases,
        suite_sha256,
    })
}

#[allow(clippy::too_many_arguments)]
fn eval_cases_on_touchstone(
    client: &touchstone_client::Client,
    function: &str,
    cases: &[FuzzCase],
    pred_path: &Path,
    batch_size: usize,
    in_flight: usize,
    cfg: &FuzzConfig,
    all_envs: &mut BTreeMap<String, Value>,
) -> Result<(FunctionSweepCounts, Vec<DiscrepancyRow>, HashMap<String, String>), String> {
    let pred_sha = freeze_predictions(pred_path, function, cases)?;
    let agg = ask_single_priority(
        client,
        function,
        cases,
        batch_size,
        in_flight,
        Priority::Bulk,
        Some(cfg),
    )?;
    for (k, v) in agg.envs {
        all_envs.entry(k).or_insert(v);
    }
    let mut excel_answers = HashMap::new();
    let mut region_stats: BTreeMap<String, RegionStats> = BTreeMap::new();
    let mut known_clusters = HashSet::new();
    let mut discrepancies = Vec::new();
    let mut dparents = Vec::new();
    let mut fseeds = Vec::new();
    let (mut asked, mut matched, mut mismatched, mut harness_fault, mut unread, mut crashed) =
        (0, 0, 0, 0, 0, 0);
    let mut harness_faults: BTreeMap<String, usize> = BTreeMap::new();
    let _ = ingest_judged_cases(
        function,
        cases,
        &agg.rows,
        &mut excel_answers,
        &mut region_stats,
        &mut known_clusters,
        &mut discrepancies,
        &mut dparents,
        &mut fseeds,
        &mut asked,
        &mut matched,
        &mut mismatched,
        &mut harness_fault,
        &mut harness_faults,
        &mut unread,
        &mut crashed,
    );
    let clusters = build_clusters(function, &discrepancies);
    Ok((
        FunctionSweepCounts {
            function: function.to_owned(),
            generated: cases.len(),
            asked,
            matched,
            mismatched,
            harness_fault,
            unread,
            crashed,
            clusters: clusters.len(),
            predictions_sha256: pred_sha,
        },
        discrepancies,
        excel_answers,
    ))
}

fn export_discrepancies_bank(
    bank_path: &Path,
    function: &str,
    lane: &str,
    source_rel: &str,
    selection: &str,
    envs: &BTreeMap<String, Value>,
    discrepancies: &[DiscrepancyRow],
) -> Result<(), String> {
    if discrepancies.is_empty() {
        return Ok(());
    }
    if let Some(parent) = bank_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let refs: Vec<&DiscrepancyRow> = discrepancies.iter().collect();
    let ts = utc_timestamp_now();
    let ctx = touchstone_bulk_context();
    let encoded = super::cluster::encode_witness_set_json(
        function, lane, source_rel, selection, &ts, envs, &ctx, &refs,
    )?;
    fs::write(bank_path, encoded).map_err(|e| format!("{}: {e}", bank_path.display()))
}

/// Execute the full patient-zero infestation mapping protocol (dry-run or live Touchstone).
pub fn execute_patient_zero(
    cfg: &FuzzConfig,
    seed_path: &Path,
) -> Result<PatientZeroReport, String> {
    let hints = match &cfg.hints_dir {
        Some(dir) => HintBank::load_dir(dir)?,
        None => load_default_hints()?,
    };
    let func_filter = (cfg.function != "ALL" && !cfg.function.is_empty()).then_some(cfg.function.as_str());
    let seeds = load_patient_zero_seeds(seed_path, func_filter)?;
    let seed = seeds[0].clone();
    let pz_cfg = PatientZeroConfig::from_fuzz_config(cfg);
    let mut suite = build_patient_zero_suite(&seed, &hints, &pz_cfg)?;

    fs::create_dir_all(&cfg.out_dir).map_err(|e| format!("{}: {e}", cfg.out_dir.display()))?;
    let neighbourhood_cases = suite
        .primary_cases
        .iter()
        .filter(|c| c.region.starts_with("pz:neighbourhood:") || c.region.starts_with("pz:fractal_"))
        .count();
    let same_route_cases = suite
        .primary_cases
        .iter()
        .filter(|c| c.region.starts_with("pz:same_route"))
        .count();

    let mut all_envs: BTreeMap<String, Value> = BTreeMap::new();
    let primary_counts: FunctionSweepCounts;
    let mut primary_discrepancies: Vec<DiscrepancyRow> = Vec::new();
    let mut sibling_counts: BTreeMap<String, FunctionSweepCounts> = BTreeMap::new();
    let mut sibling_disc_counts: BTreeMap<String, usize> = BTreeMap::new();

    if cfg.dry_run {
        let p_sha = freeze_predictions(
            &cfg.out_dir.join("pz-primary-predictions.jsonl"),
            &seed.function,
            &suite.primary_cases,
        )?;
        primary_counts = FunctionSweepCounts {
            function: seed.function.clone(),
            generated: suite.primary_cases.len(),
            asked: 0,
            matched: 0,
            mismatched: 0,
            harness_fault: 0,
            unread: 0,
            crashed: 0,
            clusters: 0,
            predictions_sha256: p_sha,
        };
        for (sfunc, scases) in &suite.sibling_cases {
            let s_sha = freeze_predictions(
                &cfg.out_dir
                    .join(format!("pz-sibling-{}-predictions.jsonl", sfunc.to_ascii_lowercase())),
                sfunc,
                scases,
            )?;
            sibling_counts.insert(
                sfunc.clone(),
                FunctionSweepCounts {
                    function: sfunc.clone(),
                    generated: scases.len(),
                    asked: 0,
                    matched: 0,
                    mismatched: 0,
                    harness_fault: 0,
                    unread: 0,
                    crashed: 0,
                    clusters: 0,
                    predictions_sha256: s_sha,
                },
            );
            sibling_disc_counts.insert(sfunc.clone(), 0);
        }
    } else {
        let client = connect_client(cfg.endpoint.as_deref())?;
        let (mut p_counts, mut p_discs, mut excel_answers) = eval_cases_on_touchstone(
            &client,
            &seed.function,
            &suite.primary_cases,
            &cfg.out_dir.join("pz-primary-predictions.jsonl"),
            cfg.batch_size,
            cfg.in_flight,
            cfg,
            &mut all_envs,
        )?;
        // Escalate primary discrepancies via coarse fractal Excel bisection.
        if !p_discs.is_empty() {
            let spec = DomainSpec::for_function_with_hints(&seed.function, &hints)?;
            let mut seen: HashSet<String> = suite
                .primary_cases
                .iter()
                .map(|c| canonical_case_key(&seed.function, &c.args))
                .collect();
            let mut hasher = StreamCaseHasher::new(&seed.function);
            let mut pred_hashes = Vec::new();
            let mut workers = BTreeMap::new();
            let mut rstats = BTreeMap::new();
            let mut known = HashSet::new();
            let mut dparents = Vec::new();
            let mut fseeds = Vec::new();
            let mut next_id = suite.primary_cases.len();
            let mut generated = p_counts.generated;
            let mut asked = p_counts.asked;
            let mut matched = p_counts.matched;
            let mut mismatched = p_counts.mismatched;
            let mut harness_fault = p_counts.harness_fault;
            let mut harness_faults: BTreeMap<String, usize> = BTreeMap::new();
            let mut unread = p_counts.unread;
            let mut crashed = p_counts.crashed;
            let mut excel_s = 0.0;
            let mut pz_cfg_out = cfg.clone();
            pz_cfg_out.function = seed.function.clone();
            let _ = fractal_refine_coarse_discrepancies_excel(
                &pz_cfg_out,
                &spec,
                &client,
                &mut suite.primary_frontiers,
                &mut suite.fractal_stats,
                &mut seen,
                &mut excel_answers,
                &mut hasher,
                &mut pred_hashes,
                &mut all_envs,
                &mut workers,
                &mut rstats,
                &mut known,
                &mut p_discs,
                &mut dparents,
                &mut fseeds,
                &mut next_id,
                &mut generated,
                &mut asked,
                &mut matched,
                &mut mismatched,
                &mut harness_fault,
                &mut harness_faults,
                &mut unread,
                &mut crashed,
                &mut excel_s,
                1,
                2,
            )?;
            p_counts.generated = generated;
            p_counts.asked = asked;
            p_counts.matched = matched;
            p_counts.mismatched = mismatched;
            p_counts.harness_fault = harness_fault;
            p_counts.unread = unread;
            p_counts.crashed = crashed;
        }
        primary_counts = p_counts;
        primary_discrepancies = p_discs;

        if let Some(bank_path) = &cfg.bank_rows_out {
            export_discrepancies_bank(
                bank_path,
                &seed.function,
                &cfg.lane,
                &format!("{}/pz-{}/patient-zero.json", cfg.lane, seed.function.to_ascii_lowercase()),
                &format!(
                    "patient-zero protocol for {} ({}): {} discrepancies of {} asked",
                    seed.function,
                    seed.id,
                    primary_discrepancies.len(),
                    primary_counts.asked
                ),
                &all_envs,
                &primary_discrepancies,
            )?;
        }

        // Sweep every sibling function sharing the kernel!
        for (sfunc, scases) in &suite.sibling_cases {
            let (s_counts, s_discs, _) = eval_cases_on_touchstone(
                &client,
                sfunc,
                scases,
                &cfg.out_dir
                    .join(format!("pz-sibling-{}-predictions.jsonl", sfunc.to_ascii_lowercase())),
                cfg.batch_size,
                cfg.in_flight,
                cfg,
                &mut all_envs,
            )?;
            sibling_disc_counts.insert(sfunc.clone(), s_discs.len());
            if let Some(bank_path) = &cfg.bank_rows_out
                && !s_discs.is_empty()
            {
                let sib_bank = bank_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(format!("{sfunc}.json"));
                export_discrepancies_bank(
                    &sib_bank,
                    sfunc,
                    &cfg.lane,
                    &format!("{}/pz-{}/patient-zero.json", cfg.lane, seed.function.to_ascii_lowercase()),
                    &format!(
                        "patient-zero sibling sweep {} (from {} {}): {} discrepancies of {} asked",
                        sfunc,
                        seed.function,
                        seed.id,
                        s_discs.len(),
                        s_counts.asked
                    ),
                    &all_envs,
                    &s_discs,
                )?;
            }
            sibling_counts.insert(sfunc.clone(), s_counts);
        }
    }

    let primary_clusters = build_clusters(&seed.function, &primary_discrepancies);
    let siblings_failed: usize = sibling_counts.values().map(FunctionSweepCounts::failed).sum();
    let (can_close, closure_reason) = if cfg.dry_run {
        (false, "dry-run-no-excel".to_owned())
    } else if primary_counts.asked == 0 {
        (false, "no-primary-cases-asked".to_owned())
    } else if primary_counts.failed() > 0 {
        (
            false,
            format!("primary-infestation-open ({} failed)", primary_counts.failed()),
        )
    } else if siblings_failed > 0 {
        (
            false,
            format!("sibling-infestation-open ({siblings_failed} failed)"),
        )
    } else {
        (true, "clean-primary-and-siblings".to_owned())
    };

    // Persist the exact pre-fix primary case list (including any coarse-escalation failing cases
    // and escalated frontier endpoints) and sibling case lists alongside `resweep-recipe.json` so
    // `execute_resweep` replays them verbatim after a fix changes routes or frontiers.
    let mut persisted_primary: Vec<PersistedResweepCase> = Vec::with_capacity(
        suite.primary_cases.len() + primary_discrepancies.len(),
    );
    let mut seen_primary: HashSet<String> = HashSet::new();
    for c in &suite.primary_cases {
        if seen_primary.insert(canonical_case_key(&seed.function, &c.args)) {
            persisted_primary.push(PersistedResweepCase {
                id: c.id.clone(),
                region: c.region.clone(),
                args: c.args.clone(),
            });
        }
    }
    for d in &primary_discrepancies {
        if seen_primary.insert(canonical_case_key(&seed.function, &d.case.args)) {
            persisted_primary.push(PersistedResweepCase {
                id: d.case.id.clone(),
                region: d.case.region.clone(),
                args: d.case.args.clone(),
            });
        }
    }
    for f in &suite.primary_frontiers {
        if f.scale != "coarse_discrepancy" {
            continue;
        }
        let mut endpoint_bits = vec![f.dna_lo_bits, f.dna_hi_bits];
        if let Some(b) = f.excel_lo_bits {
            endpoint_bits.push(b);
        }
        if let Some(b) = f.excel_hi_bits {
            endpoint_bits.push(b);
        }
        for (idx, bits) in endpoint_bits.into_iter().enumerate() {
            let mut args = f.base_args.clone();
            if set_axis_f64(&mut args, f.axis, f.axis_cell, f64::from_bits(bits))
                && seen_primary.insert(canonical_case_key(&seed.function, &args))
            {
                persisted_primary.push(PersistedResweepCase {
                    id: format!("{}-ep{idx}", f.id),
                    region: format!("fractal_coarse:{}", f.axis_name),
                    args,
                });
            }
        }
    }
    let mut persisted_siblings: BTreeMap<String, Vec<PersistedResweepCase>> = BTreeMap::new();
    for (sfunc, scases) in &suite.sibling_cases {
        persisted_siblings.insert(
            sfunc.clone(),
            scases
                .iter()
                .map(|c| PersistedResweepCase {
                    id: c.id.clone(),
                    region: c.region.clone(),
                    args: c.args.clone(),
                })
                .collect(),
        );
    }
    let mut cases_hasher = StreamCaseHasher::new(&seed.function);
    for c in &persisted_primary {
        cases_hasher.update_case(&c.region, &c.args);
    }
    for (sfunc, scases) in &persisted_siblings {
        for c in scases {
            cases_hasher.update_case(&format!("{sfunc}:{}", c.region), &c.args);
        }
    }
    let cases_sha256 = cases_hasher.finish_hex();
    let cases_filename = "resweep-cases.json";
    let cases_doc = ResweepCaseList {
        format: RESWEEP_CASES_FORMAT.to_owned(),
        function: seed.function.clone(),
        primary_cases: persisted_primary,
        sibling_cases: persisted_siblings,
        cases_sha256: cases_sha256.clone(),
    };
    let cases_path = cfg.out_dir.join(cases_filename);
    fs::write(
        &cases_path,
        serde_json::to_string_pretty(&cases_doc).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| format!("write {}: {e}", cases_path.display()))?;

    let recipe = ResweepRecipe {
        format: RESWEEP_RECIPE_FORMAT.to_owned(),
        generator_version: GENERATOR_VERSION.to_owned(),
        created_utc: utc_timestamp_now(),
        lane: cfg.lane.clone(),
        function: seed.function.clone(),
        patient_zero: seed.clone(),
        target_route_mask: format!("0x{:x}", suite.seed_case.trace.route_mask),
        target_predicate: suite.target_signature.predicate.clone(),
        target_outcome_class: suite.target_signature.outcome_class.clone(),
        siblings: suite.siblings.iter().map(|s| s.function.clone()).collect(),
        seed: pz_cfg.seed,
        neighbourhood_budget: pz_cfg.neighbourhood_budget,
        same_route_budget: pz_cfg.same_route_budget,
        sibling_budget: pz_cfg.sibling_budget,
        fractal_depth: pz_cfg.fractal_depth,
        max_frontiers: pz_cfg.max_frontiers,
        frontier_window: pz_cfg.frontier_window,
        initial_primary_asked: primary_counts.asked,
        initial_primary_discrepancies: primary_counts.mismatched,
        initial_sibling_discrepancies: sibling_disc_counts,
        initial_clusters: primary_clusters.iter().map(|c| c.key.clone()).collect(),
        cases_file: Some(cases_filename.to_owned()),
        cases_sha256: Some(cases_sha256),
    };
    let recipe_path = cfg.out_dir.join("resweep-recipe.json");
    fs::write(
        &recipe_path,
        serde_json::to_string_pretty(&recipe).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| format!("write {}: {e}", recipe_path.display()))?;

    let report_doc = json!({
        "format": PATIENT_ZERO_FORMAT,
        "generator_version": GENERATOR_VERSION,
        "function": seed.function,
        "patient_zero": seed,
        "target_signature": {
            "predicate": suite.target_signature.predicate,
            "route_mask": format!("0x{:x}", suite.target_signature.route_mask),
            "iter_bucket": suite.target_signature.iter_bucket,
            "outcome_class": suite.target_signature.outcome_class,
        },
        "suite_sha256": suite.suite_sha256,
        "neighbourhood_cases": neighbourhood_cases,
        "same_route_cases": same_route_cases,
        "frontiers_mapped": suite.primary_frontiers.len(),
        "fractal": suite.fractal_stats.to_json(),
        "primary": primary_counts,
        "siblings": sibling_counts,
        "primary_clusters": primary_clusters.iter().map(DiscrepancyCluster::to_json).collect::<Vec<_>>(),
        "resweep_recipe": recipe_path.file_name().and_then(|n| n.to_str()).unwrap_or("resweep-recipe.json"),
        "can_close": can_close,
        "closure_reason": closure_reason,
    });
    fs::write(
        cfg.out_dir.join("patient-zero.json"),
        serde_json::to_string_pretty(&report_doc).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| format!("write patient-zero.json: {e}"))?;

    Ok(PatientZeroReport {
        patient_zero: seed,
        primary: primary_counts,
        neighbourhood_cases,
        same_route_cases,
        frontiers_mapped: suite.primary_frontiers.len(),
        siblings: sibling_counts,
        primary_clusters,
        suite_sha256: suite.suite_sha256,
        recipe_path,
        can_close,
        closure_reason,
    })
}

/// Re-sweep the exact primary neighbourhood, same-route suite, and sibling functions recorded in a
/// `resweep-recipe.json` to decide whether the cluster can close (`can_close == true`).
///
/// Closure requires a live run with `0` failures (`0` mismatched, `0` unread, `0` crashed) across
/// patient zero itself, the primary suite, and every sibling function!
pub fn execute_resweep(cfg: &FuzzConfig, recipe_path: &Path) -> Result<ResweepVerdict, String> {
    let raw = fs::read_to_string(recipe_path)
        .map_err(|e| format!("read {}: {e}", recipe_path.display()))?;
    let recipe: ResweepRecipe = serde_json::from_str(&raw)
        .map_err(|e| format!("parse {}: {e}", recipe_path.display()))?;
    if recipe.format != RESWEEP_RECIPE_FORMAT {
        return Err(format!(
            "{}: unsupported resweep recipe format {:?}",
            recipe_path.display(),
            recipe.format
        ));
    }
    let (primary_cases, sibling_cases, seed_prediction) = if let Some(ref cases_rel) =
        recipe.cases_file
    {
        let cases_path = recipe_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(cases_rel);
        let raw_cases = fs::read_to_string(&cases_path)
            .map_err(|e| format!("read {}: {e}", cases_path.display()))?;
        let persisted: ResweepCaseList = serde_json::from_str(&raw_cases)
            .map_err(|e| format!("parse {}: {e}", cases_path.display()))?;
        if persisted.format != RESWEEP_CASES_FORMAT {
            return Err(format!(
                "{}: unsupported resweep cases format {:?}",
                cases_path.display(),
                persisted.format
            ));
        }
        if !persisted.function.eq_ignore_ascii_case(&recipe.function) {
            return Err(format!(
                "{}: resweep cases function {:?} does not match recipe function {:?}",
                cases_path.display(),
                persisted.function,
                recipe.function
            ));
        }
        let ctx = touchstone_bulk_context();
        let pz_key = canonical_case_key(&recipe.function, &recipe.patient_zero.args);
        let (seed_pred, seed_trace, seed_oc) =
            evaluate_traced(&recipe.function, &recipe.patient_zero.args, &ctx);
        let mut p_cases = Vec::with_capacity(persisted.primary_cases.len() + 1);
        let mut has_pz = false;
        for c in persisted.primary_cases {
            if canonical_case_key(&recipe.function, &c.args) == pz_key {
                has_pz = true;
            }
            let (prediction, trace, outcome_class) =
                evaluate_traced(&recipe.function, &c.args, &ctx);
            p_cases.push(FuzzCase {
                id: c.id,
                region: c.region,
                round: 0,
                args: c.args,
                prediction,
                trace,
                outcome_class,
            });
        }
        if !has_pz {
            p_cases.insert(
                0,
                FuzzCase {
                    id: format!("pz-{}-000001", recipe.function),
                    region: "pz:seed".to_owned(),
                    round: 0,
                    args: recipe.patient_zero.args.clone(),
                    prediction: seed_pred.clone(),
                    trace: seed_trace,
                    outcome_class: seed_oc,
                },
            );
        }
        let mut s_cases: BTreeMap<String, Vec<FuzzCase>> = BTreeMap::new();
        for (sfunc, cases) in persisted.sibling_cases {
            let mut evaled = Vec::with_capacity(cases.len());
            for c in cases {
                let (prediction, trace, outcome_class) = evaluate_traced(&sfunc, &c.args, &ctx);
                evaled.push(FuzzCase {
                    id: c.id,
                    region: c.region,
                    round: 0,
                    args: c.args,
                    prediction,
                    trace,
                    outcome_class,
                });
            }
            s_cases.insert(sfunc, evaled);
        }
        (p_cases, s_cases, seed_pred)
    } else {
        let hints = match &cfg.hints_dir {
            Some(dir) => HintBank::load_dir(dir)?,
            None => load_default_hints()?,
        };
        let pz_cfg = PatientZeroConfig {
            seed: recipe.seed,
            neighbourhood_budget: recipe.neighbourhood_budget,
            same_route_budget: recipe.same_route_budget,
            sibling_budget: recipe.sibling_budget,
            max_siblings: recipe.siblings.len().max(1),
            siblings_override: recipe.siblings.clone(),
            fractal_depth: recipe.fractal_depth,
            max_frontiers: recipe.max_frontiers,
            frontier_window: recipe.frontier_window,
        };
        let suite = build_patient_zero_suite(&recipe.patient_zero, &hints, &pz_cfg)?;
        (
            suite.primary_cases,
            suite.sibling_cases,
            suite.seed_case.prediction,
        )
    };
    fs::create_dir_all(&cfg.out_dir).map_err(|e| format!("{}: {e}", cfg.out_dir.display()))?;

    let mut all_envs: BTreeMap<String, Value> = BTreeMap::new();
    let primary_counts: FunctionSweepCounts;
    let mut sibling_counts: BTreeMap<String, FunctionSweepCounts> = BTreeMap::new();
    let patient_zero_matched: bool;

    if cfg.dry_run {
        let p_sha = freeze_predictions(
            &cfg.out_dir.join("resweep-primary-predictions.jsonl"),
            &recipe.function,
            &primary_cases,
        )?;
        primary_counts = FunctionSweepCounts {
            function: recipe.function.clone(),
            generated: primary_cases.len(),
            asked: 0,
            matched: 0,
            mismatched: 0,
            harness_fault: 0,
            unread: 0,
            crashed: 0,
            clusters: 0,
            predictions_sha256: p_sha,
        };
        for (sfunc, scases) in &sibling_cases {
            let s_sha = freeze_predictions(
                &cfg.out_dir.join(format!(
                    "resweep-sibling-{}-predictions.jsonl",
                    sfunc.to_ascii_lowercase()
                )),
                sfunc,
                scases,
            )?;
            sibling_counts.insert(
                sfunc.clone(),
                FunctionSweepCounts {
                    function: sfunc.clone(),
                    generated: scases.len(),
                    asked: 0,
                    matched: 0,
                    mismatched: 0,
                    harness_fault: 0,
                    unread: 0,
                    crashed: 0,
                    clusters: 0,
                    predictions_sha256: s_sha,
                },
            );
        }
        patient_zero_matched = false;
    } else {
        let client = connect_client(cfg.endpoint.as_deref())?;
        let (p_counts, p_discs, excel_answers) = eval_cases_on_touchstone(
            &client,
            &recipe.function,
            &primary_cases,
            &cfg.out_dir.join("resweep-primary-predictions.jsonl"),
            cfg.batch_size,
            cfg.in_flight,
            cfg,
            &mut all_envs,
        )?;
        let pz_key = canonical_case_key(&recipe.function, &recipe.patient_zero.args);
        patient_zero_matched = excel_answers.get(&pz_key).is_some_and(|ex| {
            super::cluster::judge_pair(ex, &seed_prediction).0 == calc_verdict::Severity::Exact
        });
        primary_counts = p_counts;

        if let Some(bank_path) = &cfg.bank_rows_out
            && !p_discs.is_empty()
        {
            export_discrepancies_bank(
                bank_path,
                &recipe.function,
                &cfg.lane,
                &format!(
                    "{}/resweep-{}/resweep-verdict.json",
                    cfg.lane,
                    recipe.function.to_ascii_lowercase()
                ),
                &format!(
                    "resweep remaining discrepancies for {} ({}): {} of {} asked",
                    recipe.function,
                    recipe.patient_zero.id,
                    p_discs.len(),
                    primary_counts.asked
                ),
                &all_envs,
                &p_discs,
            )?;
        }

        for (sfunc, scases) in &sibling_cases {
            let (s_counts, s_discs, _) = eval_cases_on_touchstone(
                &client,
                sfunc,
                scases,
                &cfg.out_dir.join(format!(
                    "resweep-sibling-{}-predictions.jsonl",
                    sfunc.to_ascii_lowercase()
                )),
                cfg.batch_size,
                cfg.in_flight,
                cfg,
                &mut all_envs,
            )?;
            if let Some(bank_path) = &cfg.bank_rows_out
                && !s_discs.is_empty()
            {
                let sib_bank = bank_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(format!("{sfunc}.json"));
                export_discrepancies_bank(
                    &sib_bank,
                    sfunc,
                    &cfg.lane,
                    &format!(
                        "{}/resweep-{}/resweep-verdict.json",
                        cfg.lane,
                        recipe.function.to_ascii_lowercase()
                    ),
                    &format!(
                        "resweep remaining sibling discrepancies for {} (from {}): {} of {} asked",
                        sfunc,
                        recipe.function,
                        s_discs.len(),
                        s_counts.asked
                    ),
                    &all_envs,
                    &s_discs,
                )?;
            }
            sibling_counts.insert(sfunc.clone(), s_counts);
        }
    }

    let siblings_failed: usize = sibling_counts.values().map(FunctionSweepCounts::failed).sum();
    let failing_siblings: Vec<String> = sibling_counts
        .iter()
        .filter(|(_, c)| c.failed() > 0)
        .map(|(f, c)| format!("{f}({} failed)", c.failed()))
        .collect();
    let (can_close, closure_reason) = if cfg.dry_run {
        (false, "dry-run-cannot-close".to_owned())
    } else if !patient_zero_matched {
        (false, "patient-zero-still-failing".to_owned())
    } else if primary_counts.asked == 0 {
        (false, "no-primary-cases-asked".to_owned())
    } else if primary_counts.failed() > 0 {
        (
            false,
            format!(
                "primary-region-not-clean ({} mismatched, {} harness_fault, {} unread, {} crashed)",
                primary_counts.mismatched,
                primary_counts.harness_fault,
                primary_counts.unread,
                primary_counts.crashed,
            ),
        )
    } else if siblings_failed > 0 {
        (
            false,
            format!("siblings-not-clean: {}", failing_siblings.join(", ")),
        )
    } else {
        (
            true,
            format!(
                "clean-resweep: primary {}/{} matched, {} siblings {}/{} matched",
                primary_counts.matched,
                primary_counts.asked,
                sibling_counts.len(),
                sibling_counts.values().map(|c| c.matched).sum::<usize>(),
                sibling_counts.values().map(|c| c.asked).sum::<usize>(),
            ),
        )
    };

    let verdict_doc = json!({
        "format": RESWEEP_VERDICT_FORMAT,
        "generator_version": GENERATOR_VERSION,
        "timestamp_utc": utc_timestamp_now(),
        "function": recipe.function,
        "patient_zero_id": recipe.patient_zero.id,
        "patient_zero_matched": patient_zero_matched,
        "initial_primary_discrepancies": recipe.initial_primary_discrepancies,
        "initial_sibling_discrepancies": recipe.initial_sibling_discrepancies,
        "primary": primary_counts,
        "siblings": sibling_counts,
        "can_close": can_close,
        "closure_reason": closure_reason,
    });
    fs::write(
        cfg.out_dir.join("resweep-verdict.json"),
        serde_json::to_string_pretty(&verdict_doc).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| format!("write resweep-verdict.json: {e}"))?;

    Ok(ResweepVerdict {
        function: recipe.function,
        patient_zero_id: recipe.patient_zero.id,
        patient_zero_matched,
        primary: primary_counts,
        siblings: sibling_counts,
        initial_primary_discrepancies: recipe.initial_primary_discrepancies,
        initial_sibling_discrepancies: recipe.initial_sibling_discrepancies,
        can_close,
        closure_reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::thread;

    fn read_frame(stream: &mut TcpStream) -> Option<(u8, Value)> {
        let mut size = [0u8; 4];
        stream.read_exact(&mut size).ok()?;
        let size = u32::from_be_bytes(size) as usize;
        let mut body = vec![0u8; size];
        stream.read_exact(&mut body).ok()?;
        Some((body[0], serde_json::from_slice(&body[1..]).unwrap()))
    }

    fn write_frame(stream: &mut TcpStream, kind: u8, body: &Value) {
        let text = serde_json::to_vec(body).unwrap();
        stream
            .write_all(&((text.len() + 1) as u32).to_be_bytes())
            .unwrap();
        stream.write_all(&[kind]).unwrap();
        stream.write_all(&text).unwrap();
    }

    #[test]
    fn kernel_sibling_discovery_finds_real_siblings_from_order_tsv() {
        let rate_sibs: Vec<String> = discover_kernel_siblings("RATE")
            .unwrap()
            .into_iter()
            .map(|s| s.function)
            .collect();
        assert!(rate_sibs.contains(&"PV".to_owned()), "RATE siblings: {rate_sibs:?}");
        assert!(rate_sibs.contains(&"FV".to_owned()), "RATE siblings: {rate_sibs:?}");
        assert!(rate_sibs.contains(&"PMT".to_owned()), "RATE siblings: {rate_sibs:?}");
        assert!(!rate_sibs.contains(&"RATE".to_owned()));

        let irr_sibs: Vec<String> = discover_kernel_siblings("IRR")
            .unwrap()
            .into_iter()
            .map(|s| s.function)
            .collect();
        assert!(irr_sibs.contains(&"XIRR".to_owned()), "IRR siblings: {irr_sibs:?}");
        assert!(irr_sibs.contains(&"XNPV".to_owned()), "IRR siblings: {irr_sibs:?}");

        let yield_sibs: Vec<String> = discover_kernel_siblings("YIELD")
            .unwrap()
            .into_iter()
            .map(|s| s.function)
            .collect();
        assert!(yield_sibs.contains(&"PRICE".to_owned()), "YIELD siblings: {yield_sibs:?}");
        assert!(yield_sibs.contains(&"DURATION".to_owned()), "YIELD siblings: {yield_sibs:?}");

        // Broad utility file variance_common.rs filters by family so ROUND gets rounding siblings only.
        let round_sibs: Vec<String> = discover_kernel_siblings("ROUND")
            .unwrap()
            .into_iter()
            .map(|s| s.function)
            .collect();
        assert!(round_sibs.contains(&"INT".to_owned()), "ROUND siblings: {round_sibs:?}");
        assert!(round_sibs.contains(&"MOD".to_owned()), "ROUND siblings: {round_sibs:?}");
        assert!(
            !round_sibs.contains(&"STDEV".to_owned()),
            "broad kernel file should not pull unrelated statistics siblings into ROUND: {round_sibs:?}"
        );
    }

    #[test]
    fn patient_zero_suite_maps_neighbourhood_same_route_and_siblings_deterministically() {
        let rep_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/calc-func/tests/witnesses/fuzz_representatives/RATE.json");
        let seeds = load_patient_zero_seeds(&rep_path, Some("RATE")).unwrap();
        assert!(!seeds.is_empty(), "expected failing RATE representatives");

        let hints = load_default_hints().unwrap();
        let pz_cfg = PatientZeroConfig {
            seed: 20261124,
            neighbourhood_budget: 60,
            same_route_budget: 30,
            sibling_budget: 24,
            max_siblings: 3,
            siblings_override: Vec::new(),
            fractal_depth: 1,
            max_frontiers: 6,
            frontier_window: 3,
        };
        let s1 = build_patient_zero_suite(&seeds[0], &hints, &pz_cfg).unwrap();
        let s2 = build_patient_zero_suite(&seeds[0], &hints, &pz_cfg).unwrap();

        assert_eq!(s1.suite_sha256, s2.suite_sha256, "patient-zero suite must be deterministic");
        assert!(
            s1.primary_cases
                .iter()
                .any(|c| c.region == "pz:seed"),
            "missing patient-zero seed case"
        );
        assert!(
            s1.primary_cases
                .iter()
                .any(|c| c.region.starts_with("pz:neighbourhood:") || c.region.starts_with("pz:fractal_")),
            "expected neighbourhood/fractal cases"
        );
        assert!(
            s1.primary_cases
                .iter()
                .any(|c| c.region.starts_with("pz:same_route")),
            "expected same-route cases under other arguments"
        );
        assert_eq!(s1.siblings.len(), 3, "expected 3 top-ranked RATE siblings");
        for sib in &s1.siblings {
            let scases = s1.sibling_cases.get(&sib.function).expect("sibling cases");
            assert!(!scases.is_empty());
            assert!(
                scases.iter().any(|c| c.region == "pz:sibling_transfer"),
                "expected parameter-transferred cases for sibling {}",
                sib.function
            );
        }
    }

    #[test]
    fn live_patient_zero_and_resweep_blocks_closure_until_primary_and_siblings_are_clean() {
        // Multi-function fake Touchstone broker with 3 controllable fix phases:
        //   phase 0: BOTH RATE (primary) and PV (sibling) differ from DNA Calc by +2 ULPs.
        //   phase 1: RATE is fixed (matches), but PV (sibling) STILL differs by +2 ULPs -> closure refused!
        //   phase 2: BOTH RATE and PV match DNA Calc bit-for-bit -> re-sweep closes cleanly!
        let phase = Arc::new(AtomicU8::new(0));
        let phase_srv = phase.clone();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let tmp = std::env::temp_dir().join(format!("dnacalc-fuzz-pz-test-{port}"));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let endpoint = tmp.join("endpoint.json");
        fs::write(
            &endpoint,
            json!({"host": "127.0.0.1", "port": port, "token": "t"}).to_string(),
        )
        .unwrap();

        let _server = thread::spawn(move || {
            let ctx = touchstone_bulk_context();
            // Accept 3 sequential client connections (phase 0, phase 1, phase 2).
            for _ in 0..3 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                while let Some((kind, body)) = read_frame(&mut stream) {
                    match kind {
                        b'H' => write_frame(
                            &mut stream,
                            b'H',
                            &json!({
                                "v": "touchstone/1",
                                "server": "fake-pz-broker",
                                "envs": {
                                    "e1": {
                                        "file_version": "16.0.20625.20108",
                                        "cpu": "Fake CPU",
                                        "machine": "x86-64"
                                    }
                                }
                            }),
                        ),
                        b'S' => {
                            let id = body["id"].as_str().unwrap().to_string();
                            let cur_phase = phase_srv.load(Ordering::SeqCst);
                            let mut answers = Vec::new();
                            for g in body["groups"].as_array().unwrap() {
                                let func = g["f"].as_str().unwrap().to_ascii_uppercase();
                                let tokens: Vec<&str> = g["k"].as_str().unwrap().split(',').collect();
                                let width = tokens.len();
                                let n = g["n"].as_u64().unwrap() as usize;
                                let flat = g["v"].as_array().unwrap();
                                let mut ty = String::new();
                                let mut out_v = Vec::new();
                                let mut ity = String::new();
                                let mut iv = Vec::new();
                                for r in 0..n {
                                    let mut args_json = Vec::with_capacity(width);
                                    for c in 0..width {
                                        let tok = tokens[c];
                                        let cell = &flat[r * width + c];
                                        match tok {
                                            "n" => {
                                                let h = cell.as_str().unwrap();
                                                args_json.push(Value::String(format!("0x{h}")));
                                                ity.push('n');
                                                iv.push(Value::String(h.to_owned()));
                                            }
                                            "s" | "S" => {
                                                args_json.push(cell.clone());
                                                ity.push('t');
                                                iv.push(cell.clone());
                                            }
                                            "b" | "B" => {
                                                args_json.push(cell.clone());
                                                ity.push('b');
                                                iv.push(cell.clone());
                                            }
                                            "e" => {
                                                let code = cell.as_str().unwrap();
                                                let var = match code {
                                                    "#NULL!" => "Null",
                                                    "#DIV/0!" => "Div0",
                                                    "#VALUE!" => "Value",
                                                    "#REF!" => "Ref",
                                                    "#NAME?" => "Name",
                                                    "#NUM!" => "Num",
                                                    "#N/A" => "NA",
                                                    _ => "Value",
                                                };
                                                args_json.push(json!({"error": var}));
                                                ity.push('e');
                                                iv.push(cell.clone());
                                            }
                                            _ => {
                                                args_json.push(Value::Null);
                                                ity.push('z');
                                                iv.push(Value::Null);
                                            }
                                        }
                                    }
                                    let (dna, _, _) = evaluate_traced(&func, &args_json, &ctx);
                                    let inject_bug = (func == "RATE" && cur_phase == 0)
                                        || (func == "PV" && cur_phase <= 1);
                                    if let Some(hex) = dna.strip_prefix("0x") {
                                        let mut bits = u64::from_str_radix(hex, 16).unwrap();
                                        if inject_bug {
                                            bits = step_ulps(f64::from_bits(bits), 2, true)
                                                .unwrap_or_else(|| f64::from_bits(bits))
                                                .to_bits();
                                        }
                                        ty.push('n');
                                        out_v.push(Value::String(format!("{bits:016x}")));
                                    } else if let Some(err) = dna.strip_prefix("error:") {
                                        let excel_err = match err {
                                            "Null" => "#NULL!",
                                            "Div0" => "#DIV/0!",
                                            "Value" => "#VALUE!",
                                            "Ref" => "#REF!",
                                            "Name" => "#NAME?",
                                            "Num" => "#NUM!",
                                            "NA" => "#N/A",
                                            _ => "#VALUE!",
                                        };
                                        ty.push('e');
                                        out_v.push(Value::String(excel_err.to_owned()));
                                    } else {
                                        ty.push('e');
                                        out_v.push(Value::String("#VALUE!".to_owned()));
                                    }
                                }
                                let mut g_out = json!({"ty": ty, "v": out_v, "ity": ity, "iv": iv});
                                if let Some(o) = g.get("o") {
                                    g_out["o"] = o.clone();
                                } else {
                                    g_out["i"] = g["i"].clone();
                                }
                                answers.push(g_out);
                            }
                            write_frame(
                                &mut stream,
                                b'A',
                                &json!({"id": id, "seq": body["seq"], "env": "e1", "w": "w1", "g": answers}),
                            );
                            if body["last"] == json!(true) {
                                write_frame(&mut stream, b'D', &json!({"id": id, "calls": 0}));
                            }
                        }
                        _ => {}
                    }
                }
            }
        });

        // Write a single patient-zero case for RATE (ordinary loan returning a finite number).
        let seed_file = tmp.join("pz-seed.json");
        fs::write(
            &seed_file,
            json!({
                "id": "pz-rate-001",
                "function": "RATE",
                "args": [hex_f64(10.0), hex_f64(-120.0), hex_f64(1000.0), hex_f64(0.0), hex_f64(0.0), hex_f64(0.1)]
            })
            .to_string(),
        )
        .unwrap();

        let bank_dir = tmp.join("bank-rows");
        let mut cfg = FuzzConfig::default_for("RATE");
        cfg.seed = 20261124;
        cfg.budget = 60;
        cfg.batch_size = 50;
        cfg.in_flight = 2;
        cfg.max_frontiers = 4;
        cfg.fractal_depth = 1;
        cfg.frontier_window = 2;
        cfg.pz_siblings = vec!["PV".to_owned(), "FV".to_owned()];
        cfg.max_siblings = 2;
        cfg.endpoint = Some(endpoint.clone());
        cfg.out_dir = tmp.join("pz-initial");
        cfg.bank_rows_out = Some(bank_dir.join("RATE.json"));

        // Phase 0: initial patient-zero investigation catches both RATE and sibling PV!
        let initial = execute_patient_zero(&cfg, &seed_file).expect("phase 0 patient-zero");
        assert!(!initial.can_close, "initial infested run must not close");
        assert!(initial.primary.mismatched > 0, "expected primary RATE mismatches");
        assert!(
            initial.siblings["PV"].mismatched > 0,
            "expected sibling PV mismatches"
        );
        assert_eq!(initial.siblings["FV"].mismatched, 0, "clean sibling FV should match");
        assert!(initial.recipe_path.is_file());
        // Verify discrepancies-only bank export for both RATE and sibling PV!
        let rate_bank = WitnessSet::decode(&fs::read(bank_dir.join("RATE.json")).unwrap()).unwrap();
        assert!(!rate_bank.witnesses.is_empty());
        assert!(rate_bank.witnesses.iter().all(|w| w.calc_func.as_deref() != Some(w.expected.as_str())));
        let pv_bank = WitnessSet::decode(&fs::read(bank_dir.join("PV.json")).unwrap()).unwrap();
        assert!(!pv_bank.witnesses.is_empty());
        assert!(pv_bank.witnesses.iter().all(|w| w.calc_func.as_deref() != Some(w.expected.as_str())));

        // Phase 1: primary RATE fixed, sibling PV still broken -> closure MUST be refused!
        phase.store(1, Ordering::SeqCst);
        let mut resweep_cfg = cfg.clone();
        resweep_cfg.out_dir = tmp.join("resweep-partial");
        resweep_cfg.bank_rows_out = None;
        let partial = execute_resweep(&resweep_cfg, &initial.recipe_path).expect("phase 1 resweep");
        assert!(partial.patient_zero_matched);
        assert_eq!(partial.primary.failed(), 0);
        assert!(partial.siblings["PV"].failed() > 0);
        assert!(
            !partial.can_close,
            "closure must be refused while sibling PV still has discrepancies"
        );
        assert!(partial.closure_reason.contains("PV"), "reason={}", partial.closure_reason);

        // Phase 2: primary RATE AND sibling PV both fixed -> closure succeeds!
        phase.store(2, Ordering::SeqCst);
        resweep_cfg.out_dir = tmp.join("resweep-clean");
        let clean = execute_resweep(&resweep_cfg, &initial.recipe_path).expect("phase 2 resweep");
        assert!(clean.patient_zero_matched);
        assert_eq!(clean.primary.failed(), 0);
        assert!(clean.primary.matched > 0);
        assert_eq!(clean.siblings["PV"].failed(), 0);
        assert_eq!(clean.siblings["FV"].failed(), 0);
        assert!(clean.can_close, "clean primary + sibling re-sweep should allow closure: {}", clean.closure_reason);
        assert!(resweep_cfg.out_dir.join("resweep-verdict.json").is_file());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn resweep_replays_persisted_pre_fix_and_coarse_escalation_cases_verbatim() {
        // Phase 0: patient zero (nper=10) mismatches by +2 ULPs, and Excel also has an
        // Excel-only #NUM! cutoff at nper > 37.25 so coarse escalation bisects into (37.25, 38.0)
        // and generates `fractal_coarse:nper` failing cases NOT produced by `build_patient_zero_suite`.
        // Phase 1: everything matches EXCEPT cases in (37.25, 38.0). Re-sweep MUST replay the
        // pre-fix escalated failing cases and refuse closure (`can_close == false`).
        // Phase 2: everything matches; even if `resweep-recipe.json`'s `seed` is altered (simulating
        // post-fix suite drift from route_mask/frontier changes), re-sweep MUST replay the exact
        // persisted primary & sibling case lists verbatim and close cleanly.
        let phase = Arc::new(AtomicU8::new(0));
        let phase_srv = phase.clone();
        let seen_in_phase1 = Arc::new(std::sync::Mutex::new(Vec::<f64>::new()));
        let seen_in_phase1_srv = seen_in_phase1.clone();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let tmp = std::env::temp_dir().join(format!("dnacalc-fuzz-pz-verbatim-{port}"));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let endpoint = tmp.join("endpoint.json");
        fs::write(
            &endpoint,
            json!({"host": "127.0.0.1", "port": port, "token": "t"}).to_string(),
        )
        .unwrap();

        let _server = thread::spawn(move || {
            let ctx = touchstone_bulk_context();
            for _ in 0..3 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                while let Some((kind, body)) = read_frame(&mut stream) {
                    match kind {
                        b'H' => write_frame(
                            &mut stream,
                            b'H',
                            &json!({
                                "v": "touchstone/1",
                                "server": "fake-pz-verbatim-broker",
                                "envs": {
                                    "e1": {
                                        "file_version": "16.0.20625.20108",
                                        "cpu": "Fake CPU",
                                        "machine": "x86-64"
                                    }
                                }
                            }),
                        ),
                        b'S' => {
                            let id = body["id"].as_str().unwrap().to_string();
                            let cur_phase = phase_srv.load(Ordering::SeqCst);
                            let mut answers = Vec::new();
                            for g in body["groups"].as_array().unwrap() {
                                let func = g["f"].as_str().unwrap().to_ascii_uppercase();
                                let tokens: Vec<&str> = g["k"].as_str().unwrap().split(',').collect();
                                let width = tokens.len();
                                let n = g["n"].as_u64().unwrap() as usize;
                                let flat = g["v"].as_array().unwrap();
                                let mut ty = String::new();
                                let mut out_v = Vec::new();
                                let mut ity = String::new();
                                let mut iv = Vec::new();
                                for r in 0..n {
                                    let mut args_json = Vec::with_capacity(width);
                                    for c in 0..width {
                                        let tok = tokens[c];
                                        let cell = &flat[r * width + c];
                                        match tok {
                                            "n" => {
                                                let h = cell.as_str().unwrap();
                                                args_json.push(Value::String(format!("0x{h}")));
                                                ity.push('n');
                                                iv.push(Value::String(h.to_owned()));
                                            }
                                            "s" | "S" => {
                                                args_json.push(cell.clone());
                                                ity.push('t');
                                                iv.push(cell.clone());
                                            }
                                            "b" | "B" => {
                                                args_json.push(cell.clone());
                                                ity.push('b');
                                                iv.push(cell.clone());
                                            }
                                            _ => {
                                                args_json.push(Value::Null);
                                                ity.push('z');
                                                iv.push(Value::Null);
                                            }
                                        }
                                    }
                                    let nper = args_json
                                        .first()
                                        .and_then(parse_hex_bits)
                                        .map(f64::from_bits)
                                        .unwrap_or(0.0);
                                    if cur_phase == 1 && func == "RATE" {
                                        seen_in_phase1_srv.lock().unwrap().push(nper);
                                    }
                                    let (dna, _, _) = evaluate_traced(&func, &args_json, &ctx);
                                    let force_num = func == "RATE"
                                        && ((cur_phase == 0 && nper > 37.25)
                                            || (cur_phase == 1 && nper > 37.25 && nper < 38.0));
                                    let bump_seed =
                                        func == "RATE" && cur_phase == 0 && (nper - 10.0).abs() < 1e-12;
                                    if force_num {
                                        ty.push('e');
                                        out_v.push(Value::String("#NUM!".to_owned()));
                                    } else if let Some(hex) = dna.strip_prefix("0x") {
                                        let mut bits = u64::from_str_radix(hex, 16).unwrap();
                                        if bump_seed {
                                            bits = step_ulps(f64::from_bits(bits), 2, true)
                                                .unwrap_or_else(|| f64::from_bits(bits))
                                                .to_bits();
                                        }
                                        ty.push('n');
                                        out_v.push(Value::String(format!("{bits:016x}")));
                                    } else if let Some(err) = dna.strip_prefix("error:") {
                                        let excel_err = match err {
                                            "Null" => "#NULL!",
                                            "Div0" => "#DIV/0!",
                                            "Value" => "#VALUE!",
                                            "Ref" => "#REF!",
                                            "Name" => "#NAME?",
                                            "Num" => "#NUM!",
                                            "NA" => "#N/A",
                                            _ => "#VALUE!",
                                        };
                                        ty.push('e');
                                        out_v.push(Value::String(excel_err.to_owned()));
                                    } else {
                                        ty.push('e');
                                        out_v.push(Value::String("#VALUE!".to_owned()));
                                    }
                                }
                                let mut g_out = json!({"ty": ty, "v": out_v, "ity": ity, "iv": iv});
                                if let Some(o) = g.get("o") {
                                    g_out["o"] = o.clone();
                                } else {
                                    g_out["i"] = g["i"].clone();
                                }
                                answers.push(g_out);
                            }
                            write_frame(
                                &mut stream,
                                b'A',
                                &json!({"id": id, "seq": body["seq"], "env": "e1", "w": "w1", "g": answers}),
                            );
                            if body["last"] == json!(true) {
                                write_frame(&mut stream, b'D', &json!({"id": id, "calls": 0}));
                            }
                        }
                        _ => {}
                    }
                }
            }
        });

        let seed_file = tmp.join("pz-seed.json");
        fs::write(
            &seed_file,
            json!({
                "id": "pz-rate-verbatim",
                "function": "RATE",
                "args": [hex_f64(10.0), hex_f64(-120.0), hex_f64(1000.0), hex_f64(0.0), hex_f64(0.0), hex_f64(0.1)]
            })
            .to_string(),
        )
        .unwrap();

        let mut cfg = FuzzConfig::default_for("RATE");
        cfg.seed = 20261124;
        cfg.budget = 60;
        cfg.batch_size = 50;
        cfg.in_flight = 2;
        cfg.max_frontiers = 4;
        cfg.fractal_depth = 1;
        cfg.frontier_window = 2;
        cfg.pz_siblings = vec!["PV".to_owned()];
        cfg.max_siblings = 1;
        cfg.endpoint = Some(endpoint);
        cfg.out_dir = tmp.join("pz-initial");

        // Phase 0: coarse escalation pinpoints the 37.25 Excel-only boundary and discovers
        // failing `fractal_coarse:nper` cases in (37.25, 38.0).
        let initial = execute_patient_zero(&cfg, &seed_file).expect("phase 0");
        assert!(!initial.can_close);
        assert!(initial.primary.mismatched > 0);

        // Mutate recipe.seed on disk to simulate post-fix suite drift (different RNG/route_mask
        // would produce a different suite if execute_resweep regenerated instead of replaying).
        let mut recipe_val: Value =
            serde_json::from_str(&fs::read_to_string(&initial.recipe_path).unwrap()).unwrap();
        recipe_val["seed"] = json!(99999999u64);
        fs::write(
            &initial.recipe_path,
            serde_json::to_string_pretty(&recipe_val).unwrap(),
        )
        .unwrap();

        // Phase 1: only the escalated (37.25, 38.0) failing cases still fail -> MUST NOT close!
        phase.store(1, Ordering::SeqCst);
        let mut resweep_cfg = cfg.clone();
        resweep_cfg.out_dir = tmp.join("resweep-phase1");
        let phase1 = execute_resweep(&resweep_cfg, &initial.recipe_path).expect("phase 1 resweep");
        assert!(
            seen_in_phase1
                .lock()
                .unwrap()
                .iter()
                .any(|&v| v > 37.25 && v < 38.0),
            "resweep must replay persisted coarse-escalation failing cases in (37.25, 38.0)"
        );
        assert!(
            phase1.primary.mismatched > 0,
            "still-failing pre-fix escalated cases must be caught in resweep"
        );
        assert!(
            !phase1.can_close,
            "resweep must refuse closure while pre-fix failing cases still fail"
        );

        // Phase 2: all persisted cases match -> closes cleanly!
        phase.store(2, Ordering::SeqCst);
        resweep_cfg.out_dir = tmp.join("resweep-phase2");
        let phase2 = execute_resweep(&resweep_cfg, &initial.recipe_path).expect("phase 2 resweep");
        assert!(phase2.can_close, "clean verbatim resweep should close: {}", phase2.closure_reason);
        assert_eq!(phase2.primary.asked, phase1.primary.asked);
        assert_eq!(phase2.siblings["PV"].asked, initial.siblings["PV"].asked);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn resweep_closure_reason_reports_harness_fault_count() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let tmp = std::env::temp_dir().join(format!("dnacalc-fuzz-pz-hf-reason-{port}"));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let endpoint = tmp.join("endpoint.json");
        fs::write(
            &endpoint,
            json!({"host": "127.0.0.1", "port": port, "token": "t"}).to_string(),
        )
        .unwrap();

        let _broker = thread::spawn(move || {
            let ctx = touchstone_bulk_context();
            let (mut stream, _) = listener.accept().unwrap();
            while let Some((kind, body)) = read_frame(&mut stream) {
                match kind {
                    b'H' => {
                        write_frame(
                            &mut stream,
                            b'H',
                            &json!({"v": "touchstone/1", "server": "fake-pz-hf", "envs": {"e1": {"engine": "excel"}}}),
                        );
                    }
                    b'S' => {
                        let id = body["id"].as_str().unwrap().to_owned();
                        let mut answers = Vec::new();
                        for g in body["groups"].as_array().unwrap() {
                            let n = g["n"].as_u64().unwrap() as usize;
                            let flat = g["v"].as_array().unwrap();
                            let mut ty = String::new();
                            let mut out_v = Vec::new();
                            let mut ity = String::new();
                            let mut iv = Vec::new();
                            for r in 0..n {
                                let text = flat[r].as_str().unwrap();
                                ity.push('t');
                                iv.push(Value::String(text.to_owned()));
                                let (dna, _, _) =
                                    evaluate_traced("VALUE", &[Value::String(text.to_owned())], &ctx);
                                if let Some(hex) = dna.strip_prefix("0x") {
                                    ty.push('n');
                                    out_v.push(Value::String(hex.to_owned()));
                                } else {
                                    ty.push('e');
                                    out_v.push(Value::String("#VALUE!".to_owned()));
                                }
                            }
                            let mut g_out = json!({"ty": ty, "v": out_v, "ity": ity, "iv": iv});
                            if let Some(o) = g.get("o") {
                                g_out["o"] = o.clone();
                            } else {
                                g_out["i"] = g["i"].clone();
                            }
                            answers.push(g_out);
                        }
                        write_frame(
                            &mut stream,
                            b'A',
                            &json!({"id": id, "seq": body["seq"], "env": "e1", "w": "w3", "g": answers}),
                        );
                        if body["last"] == json!(true) {
                            write_frame(&mut stream, b'D', &json!({"id": id, "calls": 0}));
                        }
                    }
                    _ => {}
                }
            }
        });

        // Persisted resweep suite for VALUE:
        // - patient_zero ("123") matches Excel (123.0)
        // - second primary case ("1,234") triggers DNA-side fault:NotImplemented (locale required)
        let recipe_dir = tmp.join("recipe");
        fs::create_dir_all(&recipe_dir).unwrap();
        let recipe_path = recipe_dir.join("resweep-recipe.json");
        let recipe = ResweepRecipe {
            format: RESWEEP_RECIPE_FORMAT.to_owned(),
            generator_version: GENERATOR_VERSION.to_owned(),
            created_utc: "2026-11-24T12:00:00Z".to_owned(),
            lane: "fz3".to_owned(),
            function: "VALUE".to_owned(),
            patient_zero: PatientZeroSeed {
                id: "pz-value-hf".to_owned(),
                function: "VALUE".to_owned(),
                args: vec![json!("123")],
                expected: None,
                cluster_key: None,
            },
            target_route_mask: "0x0".to_owned(),
            target_predicate: "number".to_owned(),
            target_outcome_class: "number".to_owned(),
            siblings: vec![],
            seed: 1,
            neighbourhood_budget: 2,
            same_route_budget: 0,
            sibling_budget: 0,
            fractal_depth: 0,
            max_frontiers: 0,
            frontier_window: 0,
            initial_primary_asked: 2,
            initial_primary_discrepancies: 1,
            initial_sibling_discrepancies: BTreeMap::new(),
            initial_clusters: vec![],
            cases_file: Some("resweep-cases.json".to_owned()),
            cases_sha256: None,
        };
        fs::write(&recipe_path, serde_json::to_string_pretty(&recipe).unwrap()).unwrap();
        let cases_doc = ResweepCaseList {
            format: RESWEEP_CASES_FORMAT.to_owned(),
            function: "VALUE".to_owned(),
            primary_cases: vec![
                PersistedResweepCase {
                    id: "pz-VALUE-000001".to_owned(),
                    region: "pz:seed".to_owned(),
                    args: vec![json!("123")],
                },
                PersistedResweepCase {
                    id: "pz-VALUE-000002".to_owned(),
                    region: "pz:locale".to_owned(),
                    args: vec![json!("1\u{00a0}234.5")],
                },
            ],
            sibling_cases: BTreeMap::new(),
            cases_sha256: "test".to_owned(),
        };
        fs::write(
            recipe_dir.join("resweep-cases.json"),
            serde_json::to_string_pretty(&cases_doc).unwrap(),
        )
        .unwrap();

        let mut cfg = FuzzConfig::default_for("VALUE");
        cfg.endpoint = Some(endpoint);
        cfg.out_dir = tmp.join("resweep-out");
        let verdict = execute_resweep(&cfg, &recipe_path).expect("resweep VALUE");
        assert!(!verdict.can_close);
        assert_eq!(verdict.primary.mismatched, 0);
        assert_eq!(verdict.primary.harness_fault, 1);
        assert!(
            verdict.closure_reason.contains("1 harness_fault"),
            "closure_reason must report harness_fault count, got {:?}",
            verdict.closure_reason
        );

        let _ = fs::remove_dir_all(&tmp);
    }
}
