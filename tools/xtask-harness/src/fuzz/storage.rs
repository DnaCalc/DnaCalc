//! Sweep storage policy, recipe/provenance manifests, and read-only passing-bank retention
//! analysis (`dc-ztvv.26.7`).
//!
//! Owner direction (2026-11-24):
//! - Do NOT store matching Excel readings (re-reading is cheaper than storing).
//! - Store per sweep: fuzzer recipe, seed, generator version, CalcFunc commit, Excel build, CPU,
//!   counts, and discrepancies only (the bank rule).
//! - Passing-bank retention analysis joins `dc-jkf7s` (bank tiers): keep representatives of every
//!   discrepancy, edge class, route, regime, and boundary neighbour, record archive candidates and
//!   why alongside recreation recipes, and remove ZERO bank rows until `dc-jkf7s` is agreed.

use super::cluster::DiscrepancyRow;
use super::domain::DomainSpec;
use super::fractal::iteration_regime_bucket;
use super::rng::{GENERATOR_VERSION, Rng, parse_hex_bits};
use super::steer::{evaluate_traced, touchstone_bulk_context};
use super::{FuzzConfig, FuzzSummary, utc_timestamp_now};
use calc_verdict::{Sha256, WitnessSet};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

pub const SWEEP_RECIPE_FORMAT: &str = "dnacalc-fuzz-recipe/1";
pub const RETENTION_PLAN_FORMAT: &str = "dnacalc-bank-retention-plan/1";
pub const RETENTION_POLICY_NAME: &str = "discrepancies_only_no_matching_readings";

/// Environment summary recorded in [`SweepRecipe`] per Touchstone `env_id`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedEnvironment {
    pub excel_build: String,
    pub cpu: String,
    pub bitness: String,
    pub platform: String,
}

/// Counts summary stored in [`SweepRecipe`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SweepCounts {
    pub generated: usize,
    pub asked: usize,
    pub matched: usize,
    pub mismatched: usize,
    pub unread: usize,
    pub crashed: usize,
    pub failed: usize,
    pub pass_rate_pct: f64,
    pub clusters: usize,
    pub frontiers: usize,
    pub rounds_run: usize,
    pub stop_reason: String,
}

/// Tunable parameters needed to replay a sweep deterministically.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SweepParameters {
    pub budget: usize,
    pub min_budget: usize,
    pub round_size: usize,
    pub dry_rounds: usize,
    pub batch_size: usize,
    pub max_frontiers: usize,
    pub fractal_depth: usize,
    pub frontier_window: i64,
    pub dry_run: bool,
}

/// Explicit storage-policy attestation stored inside every [`SweepRecipe`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoragePolicyAttestation {
    pub policy: String,
    pub matching_readings_stored: bool,
    pub discrepancies_only: bool,
    pub discrepancy_rows_exported: usize,
    pub bank_rows_path: Option<String>,
}

/// Canonical per-sweep recipe and version provenance manifest (`recipe.json`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SweepRecipe {
    pub format: String,
    pub timestamp_utc: String,
    pub lane: String,
    pub function: String,
    pub seed: u64,
    pub generator_version: String,
    pub calc_func_commit: String,
    pub hint_family: Option<String>,
    pub hint_source_file: Option<String>,
    pub hint_files_sha256: String,
    pub parameters: SweepParameters,
    pub reproduce_command: String,
    pub excel_builds: Vec<String>,
    pub cpus: Vec<String>,
    pub environments: BTreeMap<String, RecordedEnvironment>,
    pub counts: SweepCounts,
    pub case_list_sha256: String,
    pub predictions_sha256: Vec<String>,
    pub storage_policy: StoragePolicyAttestation,
}

/// Result of auditing a sweep output directory + optional bank-rows file against the storage policy.
#[derive(Clone, Debug)]
pub struct StorageAuditReport {
    pub ok: bool,
    pub function: String,
    pub discrepancy_rows: usize,
    pub matching_rows_in_bank: usize,
    pub violations: Vec<String>,
}

/// Resolve the current `calc-func` git commit using the same convention as `witness.rs`.
pub fn current_calc_func_commit() -> String {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default()
    };
    let head = git(&["rev-parse", "--short=10", "HEAD"]);
    if head.is_empty() {
        return "unknown".to_owned();
    }
    let dirty = !git(&[
        "status",
        "--porcelain",
        "--",
        "crates/calc-func",
        "crates/calc-value",
    ])
    .is_empty();
    if dirty {
        format!("{head}+changes")
    } else {
        head
    }
}

/// Compute a deterministic SHA-256 over all `*.json` hint files in `hints_dir` (sorted by name).
pub fn hash_hints_dir(hints_dir: &Path) -> String {
    let mut sha = Sha256::new();
    if !hints_dir.is_dir() {
        sha.update(b"empty-hints\n");
        return sha.finish_hex();
    }
    let mut paths = Vec::new();
    if let Ok(entries) = fs::read_dir(hints_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("json"))
            {
                paths.push(p);
            }
        }
    }
    paths.sort();
    for p in paths {
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown.json");
        sha.update(name.as_bytes());
        sha.update(b"\n");
        if let Ok(bytes) = fs::read(&p) {
            sha.update(&bytes);
        }
        sha.update(b"\n");
    }
    sha.finish_hex()
}

fn extract_environment(env: &Value) -> RecordedEnvironment {
    let excel_build = env
        .get("file_version")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| env.get("build").and_then(Value::as_str).filter(|s| !s.is_empty()))
        .unwrap_or("unknown")
        .to_owned();
    let cpu = env
        .get("cpu")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| env.get("cpu_id").and_then(Value::as_str).filter(|s| !s.is_empty()))
        .unwrap_or("unknown")
        .to_owned();
    let bitness = env
        .get("machine")
        .and_then(Value::as_str)
        .map(|m| if m.contains("64") { "64-bit" } else { "32-bit" })
        .unwrap_or("64-bit")
        .to_owned();
    let platform = env
        .get("os")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("Windows")
        .to_owned();
    RecordedEnvironment {
        excel_build,
        cpu,
        bitness,
        platform,
    }
}

/// Build the canonical [`SweepRecipe`] for a completed fuzz run.
pub fn build_sweep_recipe(
    cfg: &FuzzConfig,
    spec: &DomainSpec,
    summary: &FuzzSummary,
    discrepancies: &[DiscrepancyRow],
    envs: &BTreeMap<String, Value>,
) -> SweepRecipe {
    let hints_dir = cfg
        .hints_dir
        .clone()
        .unwrap_or_else(super::hints::default_hints_dir);
    let hint_files_sha256 = hash_hints_dir(&hints_dir);

    let mut recorded_envs: BTreeMap<String, RecordedEnvironment> = BTreeMap::new();
    let mut build_set: BTreeSet<String> = BTreeSet::new();
    let mut cpu_set: BTreeSet<String> = BTreeSet::new();
    for (env_id, raw) in envs {
        let rec = extract_environment(raw);
        if rec.excel_build != "unknown" {
            build_set.insert(rec.excel_build.clone());
        }
        if rec.cpu != "unknown" {
            cpu_set.insert(rec.cpu.clone());
        }
        recorded_envs.insert(env_id.clone(), rec);
    }

    let total_failed = summary.total_mismatched + summary.total_unread + summary.total_crashed;
    let pass_rate_pct = if summary.total_asked == 0 {
        0.0
    } else {
        (summary.total_matched as f64) * 100.0 / (summary.total_asked as f64)
    };
    let mut cmd = format!(
        "cargo xtask fuzz --func {} --seed {} --budget {} --min-budget {} --round-size {} --dry-rounds {} --batch {} --frontiers {} --fractal-depth {} --window {}",
        cfg.function,
        cfg.seed,
        cfg.budget,
        cfg.min_budget,
        cfg.round_size,
        cfg.dry_rounds,
        cfg.batch_size,
        cfg.max_frontiers,
        cfg.fractal_depth,
        cfg.frontier_window,
    );
    if cfg.dry_run {
        cmd.push_str(" --dry-run");
    }

    SweepRecipe {
        format: SWEEP_RECIPE_FORMAT.to_owned(),
        timestamp_utc: utc_timestamp_now(),
        lane: cfg.lane.clone(),
        function: summary.function.clone(),
        seed: summary.seed,
        generator_version: GENERATOR_VERSION.to_owned(),
        calc_func_commit: current_calc_func_commit(),
        hint_family: spec.merged.hint_family.clone(),
        hint_source_file: spec.merged.hint_source_file.clone(),
        hint_files_sha256,
        parameters: SweepParameters {
            budget: cfg.budget,
            min_budget: cfg.min_budget,
            round_size: cfg.round_size,
            dry_rounds: cfg.dry_rounds,
            batch_size: cfg.batch_size,
            max_frontiers: cfg.max_frontiers,
            fractal_depth: cfg.fractal_depth,
            frontier_window: cfg.frontier_window,
            dry_run: cfg.dry_run,
        },
        reproduce_command: cmd,
        excel_builds: build_set.into_iter().collect(),
        cpus: cpu_set.into_iter().collect(),
        environments: recorded_envs,
        counts: SweepCounts {
            generated: summary.total_generated,
            asked: summary.total_asked,
            matched: summary.total_matched,
            mismatched: summary.total_mismatched,
            unread: summary.total_unread,
            crashed: summary.total_crashed,
            failed: total_failed,
            pass_rate_pct,
            clusters: summary.clusters.len(),
            frontiers: summary.frontiers.len(),
            rounds_run: summary.rounds_run,
            stop_reason: summary.stop_reason.clone(),
        },
        case_list_sha256: summary.case_list_sha256.clone(),
        predictions_sha256: summary.predictions_sha256.clone(),
        storage_policy: StoragePolicyAttestation {
            policy: RETENTION_POLICY_NAME.to_owned(),
            matching_readings_stored: false,
            discrepancies_only: true,
            discrepancy_rows_exported: if cfg.bank_rows_out.is_some() {
                discrepancies.len()
            } else {
                0
            },
            bank_rows_path: if !discrepancies.is_empty() {
                cfg.bank_rows_out.as_ref().map(|p| p.display().to_string())
            } else {
                None
            },
        },
    }
}

/// Write `recipe.json` into `out_dir`.
pub fn write_sweep_recipe(out_dir: &Path, recipe: &SweepRecipe) -> Result<(), String> {
    let path = out_dir.join("recipe.json");
    let text = serde_json::to_string_pretty(recipe).map_err(|e| e.to_string())? + "\n";
    fs::write(&path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

/// Audit a completed sweep output directory (and optional exported `bank-rows` WitnessSet)
/// against the `dc-ztvv.26.7` storage policy:
/// - `recipe.json` (`dnacalc-fuzz-recipe/1`) present with seed, generator version, CalcFunc commit,
///   counts, and (for live runs) Excel build + CPU;
/// - `matching_readings_stored == false` and `discrepancies_only == true`;
/// - no matching Excel answer files stored in `out_dir`;
/// - if `bank_rows_path` exists, every witness row differs (`calc_func != expected`).
pub fn verify_sweep_storage_policy(
    out_dir: &Path,
    bank_rows_path: Option<&Path>,
) -> Result<StorageAuditReport, String> {
    let recipe_path = out_dir.join("recipe.json");
    let raw = fs::read_to_string(&recipe_path)
        .map_err(|e| format!("read {}: {e}", recipe_path.display()))?;
    let recipe: SweepRecipe = serde_json::from_str(&raw)
        .map_err(|e| format!("parse {}: {e}", recipe_path.display()))?;

    let mut violations: Vec<String> = Vec::new();
    if recipe.format != SWEEP_RECIPE_FORMAT {
        violations.push(format!("unexpected recipe format {:?}", recipe.format));
    }
    if recipe.generator_version.trim().is_empty() {
        violations.push("missing generator_version".to_owned());
    }
    if recipe.calc_func_commit.trim().is_empty() {
        violations.push("missing calc_func_commit".to_owned());
    }
    if recipe.case_list_sha256.trim().is_empty() {
        violations.push("missing case_list_sha256".to_owned());
    }
    if recipe.reproduce_command.trim().is_empty() {
        violations.push("missing reproduce_command".to_owned());
    }
    if recipe.storage_policy.matching_readings_stored {
        violations.push("storage_policy.matching_readings_stored must be false".to_owned());
    }
    if !recipe.storage_policy.discrepancies_only {
        violations.push("storage_policy.discrepancies_only must be true".to_owned());
    }
    if !recipe.parameters.dry_run && recipe.counts.asked > 0 {
        if recipe.excel_builds.is_empty() {
            violations.push("live sweep recipe missing excel_builds".to_owned());
        }
        if recipe.cpus.is_empty() {
            violations.push("live sweep recipe missing cpus".to_owned());
        }
    }

    // Ensure no matching-reading archive files exist in `out_dir`.
    if let Ok(entries) = fs::read_dir(out_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name.contains("matched-readings")
                || name.contains("all-answers")
                || (name.starts_with("answers-") && name.ends_with(".json"))
            {
                violations.push(format!(
                    "forbidden matching-readings file found in sweep output: {name}"
                ));
            }
        }
    }

    let mut discrepancy_rows = 0usize;
    let mut matching_rows_in_bank = 0usize;
    let resolved_bank = bank_rows_path
        .map(Path::to_path_buf)
        .or_else(|| recipe.storage_policy.bank_rows_path.as_ref().map(Into::into));
    if let Some(bpath) = resolved_bank
        && bpath.is_file()
    {
        let bytes = fs::read(&bpath).map_err(|e| format!("read {}: {e}", bpath.display()))?;
        let ws = WitnessSet::decode(&bytes)
            .map_err(|e| format!("decode {}: {e}", bpath.display()))?;
        for w in &ws.witnesses {
            match w.calc_func.as_deref() {
                Some(cf) if cf != w.expected => discrepancy_rows += 1,
                _ => {
                    matching_rows_in_bank += 1;
                    violations.push(format!(
                        "bank-rows {} contains matching or unlabelled witness {}",
                        bpath.display(),
                        w.id
                    ));
                }
            }
        }
    }

    Ok(StorageAuditReport {
        ok: violations.is_empty(),
        function: recipe.function,
        discrepancy_rows,
        matching_rows_in_bank,
        violations,
    })
}

// ============================================================================
// Read-only passing-bank retention planner (joins dc-jkf7s; removes ZERO rows)
// ============================================================================

/// Retention decision for one existing witness row in a read-only audit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionTier {
    Keep,
    ArchiveCandidate,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RowRetentionEntry {
    pub id: String,
    pub tier: RetentionTier,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recreation_recipe: Option<String>,
}

/// Complete read-only retention plan for one WitnessSet file (`dc-ztvv.26.7` / `dc-jkf7s`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BankRetentionPlan {
    pub format: String,
    pub function: String,
    pub source_file: String,
    pub seed: u64,
    pub ordinary_sample_cap: usize,
    pub bank_modified: bool,
    pub rows_removed: usize,
    pub policy_note: String,
    pub total_rows: usize,
    pub keep_count: usize,
    pub archive_candidate_count: usize,
    pub keep_by_reason: BTreeMap<String, usize>,
    pub entries: Vec<RowRetentionEntry>,
}

fn is_edge_argument(arg: &Value) -> bool {
    match arg {
        Value::Null | Value::Bool(_) | Value::Object(_) | Value::Array(_) => true,
        Value::String(_) => {
            let Some(bits) = parse_hex_bits(arg) else {
                // Text argument is always a type-coercion / text edge class.
                return true;
            };
            let v = f64::from_bits(bits);
            if !v.is_finite() || v == 0.0 || v.is_subnormal() {
                return true;
            }
            let abs = v.abs();
            if abs == f64::MIN_POSITIVE
                || abs == f64::MAX
                || abs == 1.0
                || abs == 4_503_599_627_370_496.0 // 2^52
                || abs == 9_007_199_254_740_991.0 // 2^53 - 1
                || abs == 9_007_199_254_740_992.0 // 2^53
            {
                return true;
            }
            let exp = ((bits >> 52) & 0x7ff) as i32 - 1023;
            if exp <= -900 || exp >= 900 {
                return true;
            }
            // Exact power of 2 or 1 ULP adjacent to an integer / power of 2
            let mant = bits & ((1u64 << 52) - 1);
            if mant == 0 || mant == 1 || mant == ((1u64 << 52) - 1) {
                return true;
            }
            false
        }
        Value::Number(_) => false,
    }
}

/// Analyze an existing `dnacalc-witness-set/1` file and produce a deterministic, **strictly
/// read-only** retention plan (joining `dc-jkf7s`: NEVER modifies or removes any bank rows).
pub fn plan_bank_retention(
    witness_path: &Path,
    ordinary_sample_cap: usize,
    seed: u64,
) -> Result<BankRetentionPlan, String> {
    let bytes =
        fs::read(witness_path).map_err(|e| format!("read {}: {e}", witness_path.display()))?;
    let ws = WitnessSet::decode(&bytes)
        .map_err(|e| format!("decode {}: {e}", witness_path.display()))?;
    let ctx = touchstone_bulk_context();
    let func = ws.function.to_ascii_uppercase();

    let mut seen_signatures: BTreeSet<(u64, &'static str, String)> = BTreeSet::new();
    let mut kept_indices: BTreeMap<usize, String> = BTreeMap::new();
    let mut ordinary_indices: Vec<usize> = Vec::new();

    for (idx, w) in ws.witnesses.iter().enumerate() {
        let (dna_pred, trace, oc) = evaluate_traced(&func, &w.args, &ctx);
        let iter_b = iteration_regime_bucket(trace.iterations);

        // 1. Discrepancies (recorded or live) are ALWAYS kept.
        if w.calc_func.as_deref().is_some_and(|c| c != w.expected) || dna_pred != w.expected {
            kept_indices.insert(idx, "discrepancy".to_owned());
            seen_signatures.insert((trace.route_mask, iter_b, oc));
            continue;
        }
        // 2. Edge classes (signed zeros, subnormals, DBL_MAX, MIN_POSITIVE, 2^52/2^53, powers of 2,
        //    non-numeric types, arrays, blanks, errors) are ALWAYS kept.
        if w.args.iter().any(is_edge_argument) {
            kept_indices.insert(idx, "edge_class".to_owned());
            seen_signatures.insert((trace.route_mask, iter_b, oc));
            continue;
        }
        // 3. First representative of every distinct (route_mask, iter_bucket, outcome_class) signature.
        if seen_signatures.insert((trace.route_mask, iter_b, oc)) {
            kept_indices.insert(idx, "route_and_regime_representative".to_owned());
            continue;
        }
        // 4. Close boundary-predicate neighbours (texture).
        if trace.min_boundary_margin.is_finite() && trace.min_boundary_margin < 1e-2 {
            kept_indices.insert(idx, "boundary_neighbour".to_owned());
            continue;
        }
        ordinary_indices.push(idx);
    }

    // 5. Deterministic seeded sample of remaining ordinary passing rows.
    let mut rng = Rng::from_seed_and_tag(seed, &format!("bank-retain|{func}"));
    let mut perm = ordinary_indices.clone();
    // Deterministic Fisher-Yates shuffle on ordinary indices.
    for i in (1..perm.len()).rev() {
        let j = rng.gen_range(i + 1);
        perm.swap(i, j);
    }
    let sample_set: BTreeSet<usize> = perm.into_iter().take(ordinary_sample_cap).collect();

    let recreation = format!(
        "cargo xtask fuzz --func {func} --seed {seed} --budget {} ({GENERATOR_VERSION})",
        ws.witnesses.len().max(1_000)
    );
    let mut keep_by_reason: BTreeMap<String, usize> = BTreeMap::new();
    let mut entries: Vec<RowRetentionEntry> = Vec::with_capacity(ws.witnesses.len());
    for (idx, w) in ws.witnesses.iter().enumerate() {
        if let Some(reason) = kept_indices.get(&idx) {
            *keep_by_reason.entry(reason.clone()).or_insert(0) += 1;
            entries.push(RowRetentionEntry {
                id: w.id.clone(),
                tier: RetentionTier::Keep,
                reason: reason.clone(),
                recreation_recipe: None,
            });
        } else if sample_set.contains(&idx) {
            let reason = "seeded_ordinary_sample".to_owned();
            *keep_by_reason.entry(reason.clone()).or_insert(0) += 1;
            entries.push(RowRetentionEntry {
                id: w.id.clone(),
                tier: RetentionTier::Keep,
                reason,
                recreation_recipe: None,
            });
        } else {
            entries.push(RowRetentionEntry {
                id: w.id.clone(),
                tier: RetentionTier::ArchiveCandidate,
                reason: "redundant_ordinary_passing_row".to_owned(),
                recreation_recipe: Some(recreation.clone()),
            });
        }
    }

    let keep_count = entries
        .iter()
        .filter(|e| e.tier == RetentionTier::Keep)
        .count();
    let archive_candidate_count = entries.len().saturating_sub(keep_count);

    Ok(BankRetentionPlan {
        format: RETENTION_PLAN_FORMAT.to_owned(),
        function: func,
        source_file: witness_path.display().to_string(),
        seed,
        ordinary_sample_cap,
        bank_modified: false,
        rows_removed: 0,
        policy_note: "read-only retention analysis joining dc-jkf7s; no bank rows removed"
            .to_owned(),
        total_rows: entries.len(),
        keep_count,
        archive_candidate_count,
        keep_by_reason,
        entries,
    })
}

/// Write a [`BankRetentionPlan`] JSON report to `out_dir/retention-plan.json` without modifying
/// the inspected bank file.
pub fn write_retention_plan(out_dir: &Path, plan: &BankRetentionPlan) -> Result<Value, String> {
    fs::create_dir_all(out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let val = serde_json::to_value(plan).map_err(|e| e.to_string())?;
    let path = out_dir.join("retention-plan.json");
    fs::write(
        &path,
        serde_json::to_string_pretty(&val).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(json!({
        "path": path.display().to_string(),
        "total_rows": plan.total_rows,
        "keep_count": plan.keep_count,
        "archive_candidate_count": plan.archive_candidate_count,
        "rows_removed": plan.rows_removed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuzz::{FuzzConfig, execute_dry_run};
    use calc_verdict::sha256_hex;

    #[test]
    fn sweep_writes_versioned_recipe_and_passes_storage_policy_audit() {
        let tmp = std::env::temp_dir().join("dnacalc-fuzz-storage-recipe-test");
        let _ = fs::remove_dir_all(&tmp);
        let mut cfg = FuzzConfig::default_for("PV");
        cfg.seed = 20261124;
        cfg.budget = 60;
        cfg.round_size = 60;
        cfg.max_frontiers = 4;
        cfg.fractal_depth = 1;
        cfg.frontier_window = 2;
        cfg.dry_run = true;
        cfg.out_dir = tmp.clone();

        let _ = execute_dry_run(&cfg).expect("dry run");
        let recipe_path = tmp.join("recipe.json");
        assert!(recipe_path.is_file(), "expected recipe.json");
        let recipe: SweepRecipe =
            serde_json::from_str(&fs::read_to_string(&recipe_path).unwrap()).unwrap();
        assert_eq!(recipe.format, SWEEP_RECIPE_FORMAT);
        assert_eq!(recipe.function, "PV");
        assert_eq!(recipe.seed, 20261124);
        assert_eq!(recipe.generator_version, GENERATOR_VERSION);
        assert!(!recipe.calc_func_commit.is_empty());
        assert!(!recipe.hint_files_sha256.is_empty());
        assert!(!recipe.storage_policy.matching_readings_stored);
        assert!(recipe.storage_policy.discrepancies_only);
        assert!(recipe.reproduce_command.contains("--func PV --seed 20261124"));

        let audit = verify_sweep_storage_policy(&tmp, None).expect("audit");
        assert!(audit.ok, "unexpected violations: {:?}", audit.violations);
        assert_eq!(audit.matching_rows_in_bank, 0);

        // Injecting a forbidden matching-readings file must fail the storage audit.
        fs::write(tmp.join("all-answers.json"), "{}\n").unwrap();
        let bad_audit = verify_sweep_storage_policy(&tmp, None).expect("bad audit");
        assert!(!bad_audit.ok, "expected violation for all-answers.json");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn bank_retention_planner_is_read_only_and_preserves_discrepancies_edges_and_routes() {
        let iseven_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/calc-func/tests/witnesses/ISEVEN.json");
        let before_iseven_sha = sha256_hex(&fs::read(&iseven_path).unwrap());

        let plan1 = plan_bank_retention(&iseven_path, 20, 20261124).expect("plan ISEVEN");
        let plan2 = plan_bank_retention(&iseven_path, 20, 20261124).expect("plan ISEVEN 2");
        let after_iseven_sha = sha256_hex(&fs::read(&iseven_path).unwrap());
        assert_eq!(
            before_iseven_sha, after_iseven_sha,
            "retention planner must never modify bank files"
        );
        assert!(!plan1.bank_modified);
        assert_eq!(plan1.rows_removed, 0);
        assert_eq!(plan1.keep_count, plan2.keep_count, "retention plan must be deterministic");
        assert_eq!(plan1.archive_candidate_count, plan2.archive_candidate_count);
        assert!(plan1.keep_count > 0);
        assert!(plan1.archive_candidate_count > 0, "expected ordinary passing rows to be archive candidates on ISEVEN");
        assert!(plan1.keep_by_reason.contains_key("edge_class"));
        assert!(
            plan1
                .entries
                .iter()
                .filter(|e| e.tier == RetentionTier::ArchiveCandidate)
                .all(|e| e.recreation_recipe.is_some()),
            "every archive candidate must name a recreation recipe"
        );

        // On RATE fuzz_representatives (which contains open discrepancies), EVERY discrepancy must be Keep.
        let rate_rep_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/calc-func/tests/witnesses/fuzz_representatives/RATE.json");
        let before_rate_sha = sha256_hex(&fs::read(&rate_rep_path).unwrap());
        let rate_plan = plan_bank_retention(&rate_rep_path, 0, 20261124).expect("plan RATE reps");
        let after_rate_sha = sha256_hex(&fs::read(&rate_rep_path).unwrap());
        assert_eq!(before_rate_sha, after_rate_sha);
        let rate_ws = WitnessSet::decode(&fs::read(&rate_rep_path).unwrap()).unwrap();
        for (w, entry) in rate_ws.witnesses.iter().zip(&rate_plan.entries) {
            if w.calc_func.as_deref().is_some_and(|c| c != w.expected) {
                assert_eq!(
                    entry.tier,
                    RetentionTier::Keep,
                    "discrepancy row {} must NEVER be marked archive candidate",
                    w.id
                );
                assert_eq!(entry.reason, "discrepancy");
            }
        }
    }
}
