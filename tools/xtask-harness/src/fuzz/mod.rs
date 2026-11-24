//! `cargo xtask fuzz`: deterministic, code-steered, Excel-compared smart fuzzer (`dc-ztvv.21`).
//!
//! Extends the witness tooling (`calc_func::invoke` via `witness::calculate_with_context`,
//! `calc_func::trace`, `calc_verdict::Severity` and `WitnessSet`, and `touchstone-client`)
//! rather than duplicating any evaluator, verdict path or format.

pub mod boundary;
pub mod cluster;
pub mod domain;
pub mod hints;
pub mod oracle;
pub mod rng;
pub mod spec_gen;
pub mod steer;

use boundary::{Frontier, discover_local_frontiers, expand_frontier_windows};
use cluster::{DiscrepancyCluster, DiscrepancyRow, RegionStats};
use domain::DomainSpec;
use rng::{GENERATOR_VERSION, Rng, hash_case_stream};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use steer::{CoverageTracker, FuzzCase, generate_steered_round, touchstone_bulk_context};

pub const USAGE: &str = "cargo xtask fuzz (--func <F> | --all-catalogue) [--seed <N>] [--budget <N>] [--min-budget <N>] [--out <dir>] [--hints-dir <dir>] [--round-size <N>] [--dry-rounds <K>] [--batch <N>] [--in-flight <N>] [--frontiers <N>] [--window <W>] [--lane <name>] [--endpoint <path>] [--bank-rows-out <json>] [--pin-out <json>] [--dry-run]";

#[derive(Clone, Debug)]
pub struct FuzzConfig {
    pub function: String,
    pub all_catalogue: bool,
    pub hints_dir: Option<PathBuf>,
    pub seed: u64,
    pub budget: usize,
    pub min_budget: usize,
    pub out_dir: PathBuf,
    pub round_size: usize,
    pub dry_rounds: usize,
    pub batch_size: usize,
    pub in_flight: usize,
    pub max_frontiers: usize,
    pub frontier_window: i64,
    pub lane: String,
    pub endpoint: Option<PathBuf>,
    pub bank_rows_out: Option<PathBuf>,
    pub pin_out: Option<PathBuf>,
    pub dry_run: bool,
}

impl FuzzConfig {
    pub fn default_for(function: &str) -> Self {
        let lane = std::env::var("DNACALC_LANE")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                let log = std::env::var("DNACALC_AGENT_LOG").ok()?;
                let (_, path) = log.split_once(':')?;
                let norm = path.replace('\\', "/");
                let parts: Vec<&str> = norm.split('/').collect();
                (parts.len() >= 3 && parts[parts.len() - 3] == "codex-runs")
                    .then(|| parts[parts.len() - 2].to_owned())
            })
            .unwrap_or_else(|| "local".to_owned());
        Self {
            function: function.to_ascii_uppercase(),
            all_catalogue: false,
            hints_dir: None,
            seed: 20261123,
            budget: 10_000,
            min_budget: 0,
            out_dir: PathBuf::from(format!("target/fuzz/{}", function.to_ascii_lowercase())),
            round_size: 20_000,
            dry_rounds: 3,
            batch_size: 5_000,
            in_flight: 4,
            max_frontiers: 40,
            frontier_window: 8,
            lane,
            endpoint: None,
            bank_rows_out: None,
            pin_out: None,
            dry_run: false,
        }
    }
}

pub fn parse_args(raw: &[String]) -> Result<FuzzConfig, String> {
    let mut function: Option<String> = None;
    let mut all_catalogue = false;
    let mut hints_dir: Option<PathBuf> = None;
    let mut seed: Option<u64> = None;
    let mut budget: Option<usize> = None;
    let mut min_budget: Option<usize> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut round_size: Option<usize> = None;
    let mut dry_rounds: Option<usize> = None;
    let mut batch_size: Option<usize> = None;
    let mut in_flight: Option<usize> = None;
    let mut max_frontiers: Option<usize> = None;
    let mut frontier_window: Option<i64> = None;
    let mut lane: Option<String> = None;
    let mut endpoint: Option<PathBuf> = None;
    let mut bank_rows_out: Option<PathBuf> = None;
    let mut pin_out: Option<PathBuf> = None;
    let mut dry_run = false;

    let mut it = raw.iter();
    while let Some(arg) = it.next() {
        let mut next_val = || it.next().cloned().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--func" => function = Some(next_val()?.to_ascii_uppercase()),
            "--all-catalogue" => all_catalogue = true,
            "--hints-dir" => hints_dir = Some(PathBuf::from(next_val()?)),
            "--seed" => {
                seed = Some(
                    next_val()?
                        .parse()
                        .map_err(|_| "--seed needs an unsigned integer")?,
                );
            }
            "--budget" => {
                budget = Some(
                    next_val()?
                        .parse()
                        .map_err(|_| "--budget needs a positive integer")?,
                );
            }
            "--min-budget" => {
                min_budget = Some(
                    next_val()?
                        .parse()
                        .map_err(|_| "--min-budget needs an integer")?,
                );
            }
            "--out" => out_dir = Some(PathBuf::from(next_val()?)),
            "--round-size" => {
                round_size = Some(
                    next_val()?
                        .parse()
                        .map_err(|_| "--round-size needs a positive integer")?,
                );
            }
            "--dry-rounds" => {
                dry_rounds = Some(
                    next_val()?
                        .parse()
                        .map_err(|_| "--dry-rounds needs an integer")?,
                );
            }
            "--batch" => {
                batch_size = Some(
                    next_val()?
                        .parse()
                        .map_err(|_| "--batch needs a positive integer")?,
                );
            }
            "--in-flight" => {
                in_flight = Some(
                    next_val()?
                        .parse()
                        .map_err(|_| "--in-flight needs a positive integer")?,
                );
            }
            "--frontiers" => {
                max_frontiers = Some(
                    next_val()?
                        .parse()
                        .map_err(|_| "--frontiers needs an integer")?,
                );
            }
            "--window" => {
                frontier_window = Some(
                    next_val()?
                        .parse()
                        .map_err(|_| "--window needs an integer")?,
                );
            }
            "--lane" => lane = Some(next_val()?),
            "--endpoint" => endpoint = Some(PathBuf::from(next_val()?)),
            "--bank-rows-out" => bank_rows_out = Some(PathBuf::from(next_val()?)),
            "--pin-out" => pin_out = Some(PathBuf::from(next_val()?)),
            "--dry-run" => dry_run = true,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let function = match (function, all_catalogue) {
        (Some(f), false) => f,
        (None, true) => "ALL".to_owned(),
        (Some(_), true) => return Err("cannot combine --func with --all-catalogue".into()),
        (None, false) => return Err("--func or --all-catalogue is required".into()),
    };
    let mut cfg = FuzzConfig::default_for(&function);
    cfg.all_catalogue = all_catalogue;
    cfg.hints_dir = hints_dir;
    if all_catalogue && out_dir.is_none() {
        cfg.out_dir = PathBuf::from("target/fuzz/all-catalogue");
    }
    if all_catalogue && budget.is_none() {
        cfg.budget = 1_000;
    }
    if let Some(v) = seed {
        cfg.seed = v;
    }
    if let Some(v) = budget {
        if v == 0 {
            return Err("--budget must be > 0".into());
        }
        cfg.budget = v;
    }
    if let Some(v) = min_budget {
        cfg.min_budget = v.min(cfg.budget);
    }
    if let Some(v) = out_dir {
        cfg.out_dir = v;
    }
    if let Some(v) = round_size {
        if v == 0 {
            return Err("--round-size must be > 0".into());
        }
        cfg.round_size = v;
    } else {
        // Default round size scales sensibly with budget: ~5 adaptive rounds.
        cfg.round_size = (cfg.budget / 5).clamp(500, 25_000).min(cfg.budget);
    }
    if let Some(v) = dry_rounds {
        cfg.dry_rounds = v.max(1);
    }
    if let Some(v) = batch_size {
        cfg.batch_size = v.clamp(1, 20_000);
    }
    if let Some(v) = in_flight {
        cfg.in_flight = v.clamp(1, 16);
    }
    if let Some(v) = max_frontiers {
        cfg.max_frontiers = v;
    }
    if let Some(v) = frontier_window {
        cfg.frontier_window = v.clamp(1, 64);
    }
    if let Some(v) = lane {
        cfg.lane = v;
    }
    cfg.endpoint = endpoint;
    cfg.bank_rows_out = bank_rows_out;
    cfg.pin_out = pin_out;
    cfg.dry_run = dry_run;
    Ok(cfg)
}

/// Write frozen predictions JSONL and return its SHA-256 hex digest BEFORE asking Excel.
pub fn freeze_predictions(path: &Path, function: &str, cases: &[FuzzCase]) -> Result<String, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let file = fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut writer = BufWriter::new(file);
    let mut sha = calc_verdict::Sha256::new();
    for c in cases {
        let line = serde_json::to_string(&json!({
            "id": c.id,
            "function": function,
            "region": c.region,
            "round": c.round,
            "args": c.args,
            "prediction": c.prediction,
            "iterations": c.trace.iterations,
            "route_mask": format!("0x{:x}", c.trace.route_mask),
        }))
        .map_err(|e| e.to_string())?;
        let bytes = format!("{line}\n");
        sha.update(bytes.as_bytes());
        writer
            .write_all(bytes.as_bytes())
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    writer
        .flush()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(sha.finish_hex())
}

/// ISO-8601 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`) using only `std::time`.
pub fn utc_timestamp_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let hour = rem / 3600;
    let min = (rem % 3600) / 60;
    let sec = rem % 60;
    // Civil date from Unix days (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + i64::from(m <= 2);
    format!("{year:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}Z")
}

/// Summary of one completed fuzz run (dry-run or live Excel-compared).
#[derive(Clone, Debug)]
pub struct FuzzSummary {
    pub function: String,
    pub seed: u64,
    pub budget: usize,
    pub dry_run: bool,
    pub total_generated: usize,
    pub total_asked: usize,
    pub total_matched: usize,
    pub total_mismatched: usize,
    pub total_unread: usize,
    pub total_crashed: usize,
    pub rounds_run: usize,
    pub stop_reason: String,
    pub case_list_sha256: String,
    pub predictions_sha256: Vec<String>,
    pub elapsed_seconds: f64,
    pub excel_seconds: f64,
    pub worker_calls: BTreeMap<String, usize>,
    pub region_stats: BTreeMap<String, RegionStats>,
    pub clusters: Vec<DiscrepancyCluster>,
    pub frontiers: Vec<Frontier>,
    pub tracker: CoverageTracker,
}

/// Format a human-readable Markdown report with region coverage, route coverage, throughput,
/// frontier map, and discrepancy clusters.
pub fn render_markdown_report(summary: &FuzzSummary) -> String {
    let mut md = String::new();
    let total_fail = summary.total_mismatched + summary.total_unread + summary.total_crashed;
    let pass_rate = if summary.total_asked == 0 {
        0.0
    } else {
        (summary.total_matched as f64) * 100.0 / (summary.total_asked as f64)
    };
    md.push_str(&format!(
        "# Smart Fuzzer Report — `{}` (seed `{}`)\n\n",
        summary.function, summary.seed
    ));
    md.push_str(&format!("- Generator: `{GENERATOR_VERSION}`\n"));
    md.push_str(&format!("- Mode: `{}`\n", if summary.dry_run { "dry-run (DNA Calc local only)" } else { "live Excel via Touchstone" }));
    md.push_str(&format!("- Budget: `{}` | Generated: `{}` | Rounds: `{}` (`{}`)\n", summary.budget, summary.total_generated, summary.rounds_run, summary.stop_reason));
    md.push_str(&format!("- Case list SHA-256: `{}`\n", summary.case_list_sha256));
    if !summary.dry_run {
        let rate_s = if summary.excel_seconds > 0.0 {
            summary.total_asked as f64 / summary.excel_seconds
        } else {
            0.0
        };
        md.push_str(&format!(
            "- Verdict: **{} matched / {} asked ({:.4}%)**, **{} failed** ({} mismatched, {} unread, {} crashed)\n",
            summary.total_matched,
            summary.total_asked,
            pass_rate,
            total_fail,
            summary.total_mismatched,
            summary.total_unread,
            summary.total_crashed
        ));
        md.push_str(&format!(
            "- Throughput: `{:.1}` calls/s over `{:.2}s` Excel time (`{:.2}s` total wall); workers: `{:?}`\n",
            rate_s, summary.excel_seconds, summary.elapsed_seconds, summary.worker_calls
        ));
    }
    let (rhit, rtot) = summary.tracker.exercised_kernel_routes();
    md.push_str(&format!(
        "- Code route coverage: `{rhit}/{rtot}` kernel routes exercised; `{}` outcome classes; `{}` frontiers mapped\n\n",
        summary.tracker.outcome_hits.len(),
        summary.frontiers.len()
    ));

    md.push_str("## Per-Region Coverage\n\n");
    md.push_str("| Region | Asked | Matched | Mismatched | Unread | Crashed | Pass % | Worst ULP |\n");
    md.push_str("|---|---:|---:|---:|---:|---:|---:|---:|\n");
    for (region, st) in &summary.region_stats {
        let ulp = st.worst_ulp.map_or("-".to_owned(), |u| u.to_string());
        md.push_str(&format!(
            "| `{region}` | {} | {} | {} | {} | {} | {:.3}% | {ulp} |\n",
            st.asked, st.matched, st.mismatched, st.unread, st.crashed, st.pass_rate()
        ));
    }

    md.push_str("\n## Discrepancy Clusters\n\n");
    if summary.clusters.is_empty() {
        md.push_str("_None._\n");
    } else {
        md.push_str("| Cluster | Kind | Count | Worst | Worst ULP | Key | Minimal Rep ID |\n");
        md.push_str("|---|---|---:|---|---:|---|---|\n");
        for c in &summary.clusters {
            let ulp = c.worst_ulp.map_or("-".to_owned(), |u| u.to_string());
            md.push_str(&format!(
                "| `{}` | `{}` | {} | `{}` | {} | `{}` | `{}` |\n",
                c.id,
                c.kind,
                c.count,
                c.worst_severity.name(),
                ulp,
                c.key,
                c.minimal_rep.case.id
            ));
        }
    }

    if !summary.frontiers.is_empty() {
        md.push_str("\n## Mapped Frontiers (first 25)\n\n");
        md.push_str("| ID | Axis | Flip | DNA lo..hi | Excel lo..hi | Shift (ULP) |\n");
        md.push_str("|---|---|---|---|---|---:|\n");
        for f in summary.frontiers.iter().take(25) {
            let ex = match (f.excel_lo_bits, f.excel_hi_bits) {
                (Some(l), Some(h)) => format!("`0x{l:016x}`..`0x{h:016x}`"),
                _ => "-".to_owned(),
            };
            let sh = f.shifted_ulps.map_or("-".to_owned(), |s| s.to_string());
            md.push_str(&format!(
                "| `{}` | `{}` | `{}` | `0x{:016x}`..`0x{:016x}` | {} | {} |\n",
                f.id, f.axis_name, f.kind, f.dna_lo_bits, f.dna_hi_bits, ex, sh
            ));
        }
    }
    md
}

/// Shared budget reservation for boundary windows & Excel bisection, used identically by both
/// `--dry-run` (`execute_dry_run`) and live runs (`execute_live_run`) so Round 0 matches
/// bit-for-bit for identical configs.
pub fn main_round_budget(cfg: &FuzzConfig) -> usize {
    let per_frontier = (cfg.frontier_window.max(0) as usize)
        .saturating_mul(2)
        .saturating_add(8);
    let boundary_reserve = (cfg.budget / 12)
        .min(cfg.max_frontiers.saturating_mul(per_frontier));
    cfg.budget.saturating_sub(boundary_reserve).max(1)
}

/// Run a deterministic local dry-run (generates strategic cases, runs code-guided steering,
/// maps DNA Calc frontiers, freezes predictions + SHA-256, and writes spec/report without Excel).
pub fn execute_dry_run(cfg: &FuzzConfig) -> Result<FuzzSummary, String> {
    let started = Instant::now();
    let hints = match &cfg.hints_dir {
        Some(dir) => hints::HintBank::load_dir(dir)?,
        None => hints::load_default_hints()?,
    };
    let spec = DomainSpec::for_function_with_hints(&cfg.function, &hints)?;
    let ctx = touchstone_bulk_context();
    fs::create_dir_all(&cfg.out_dir).map_err(|e| format!("{}: {e}", cfg.out_dir.display()))?;

    let mut rng = Rng::from_seed_and_tag(cfg.seed, &cfg.function);
    let mut tracker = CoverageTracker::new(&cfg.function);
    let mut seen_keys: HashSet<String> = HashSet::with_capacity(cfg.budget);
    let weights: Vec<f64> = spec.regions.iter().map(|r| r.base_weight).collect();
    let mut all_cases: Vec<FuzzCase> = Vec::with_capacity(cfg.budget);
    let mut pred_hashes = Vec::new();
    let mut next_id = 0usize;
    let mut round = 0usize;

    let main_budget = main_round_budget(cfg);

    while all_cases.len() < main_budget {
        let want = cfg.round_size.min(main_budget - all_cases.len());
        let batch = generate_steered_round(
            &spec,
            &weights,
            &[],
            &mut tracker,
            &mut seen_keys,
            &ctx,
            round,
            want,
            &mut next_id,
            &mut rng,
        );
        if batch.is_empty() {
            break;
        }
        let pred_path = cfg.out_dir.join(format!("round-{round:02}-predictions.jsonl"));
        pred_hashes.push(freeze_predictions(&pred_path, &cfg.function, &batch)?);
        all_cases.extend(batch);
        round += 1;
    }

    let frontiers = discover_local_frontiers(&spec, &all_cases, &ctx, cfg.max_frontiers, &mut rng);
    if !frontiers.is_empty() && all_cases.len() < cfg.budget {
        let expanded = expand_frontier_windows(
            &frontiers,
            cfg.frontier_window,
            &ctx,
            &mut seen_keys,
            round,
            &mut next_id,
        );
        let boundary_cases: Vec<FuzzCase> = expanded
            .into_iter()
            .map(|(_, c)| c)
            .take(cfg.budget - all_cases.len())
            .collect();
        if !boundary_cases.is_empty() {
            let pred_path = cfg.out_dir.join(format!("round-{round:02}-predictions.jsonl"));
            pred_hashes.push(freeze_predictions(&pred_path, &cfg.function, &boundary_cases)?);
            all_cases.extend(boundary_cases);
            round += 1;
        }
    }

    // If fewer boundary points were unique than reserved, top up to the requested budget.
    if all_cases.len() < cfg.budget {
        let want = cfg.budget - all_cases.len();
        let topup = generate_steered_round(
            &spec,
            &weights,
            &[],
            &mut tracker,
            &mut seen_keys,
            &ctx,
            round,
            want,
            &mut next_id,
            &mut rng,
        );
        if !topup.is_empty() {
            let pred_path = cfg.out_dir.join(format!("round-{round:02}-predictions.jsonl"));
            pred_hashes.push(freeze_predictions(&pred_path, &cfg.function, &topup)?);
            all_cases.extend(topup);
            round += 1;
        }
    }

    let case_list_sha256 = hash_case_stream(
        &cfg.function,
        all_cases.iter().map(|c| (c.region.as_str(), c.args.as_slice())),
    );
    let mut region_stats: BTreeMap<String, RegionStats> = BTreeMap::new();
    for c in &all_cases {
        region_stats.entry(c.region.clone()).or_default().asked += 1;
    }

    let summary = FuzzSummary {
        function: cfg.function.clone(),
        seed: cfg.seed,
        budget: cfg.budget,
        dry_run: true,
        total_generated: all_cases.len(),
        total_asked: 0,
        total_matched: 0,
        total_mismatched: 0,
        total_unread: 0,
        total_crashed: 0,
        rounds_run: round,
        stop_reason: "dry-run-complete".to_owned(),
        case_list_sha256,
        predictions_sha256: pred_hashes,
        elapsed_seconds: started.elapsed().as_secs_f64(),
        excel_seconds: 0.0,
        worker_calls: BTreeMap::new(),
        region_stats,
        clusters: Vec::new(),
        frontiers,
        tracker,
    };
    write_outputs(cfg, &spec, &summary, &[], &BTreeMap::new())?;
    Ok(summary)
}

/// Write `spec.json`, `summary.json`, `frontiers.json`, `report.md`, and (when discrepancies exist)
/// `bank-rows` and `pinned-representatives` WitnessSets.
pub fn write_outputs(
    cfg: &FuzzConfig,
    spec: &DomainSpec,
    summary: &FuzzSummary,
    discrepancies: &[DiscrepancyRow],
    envs: &BTreeMap<String, Value>,
) -> Result<(), String> {
    fs::create_dir_all(&cfg.out_dir).map_err(|e| format!("{}: {e}", cfg.out_dir.display()))?;
    let spec_doc = json!({
        "format": "dnacalc-fuzz-spec/1",
        "generator_version": GENERATOR_VERSION,
        "function": cfg.function,
        "seed": cfg.seed,
        "budget": cfg.budget,
        "round_size": cfg.round_size,
        "dry_rounds": cfg.dry_rounds,
        "batch_size": cfg.batch_size,
        "max_frontiers": cfg.max_frontiers,
        "frontier_window": cfg.frontier_window,
        "has_custom_hints": spec.has_custom_hints,
        "has_hint_file": spec.has_hint_file,
        "hint_family": spec.merged.hint_family,
        "merged_spec": spec.merged.to_json(),
        "regions": spec.regions.iter().map(|r| json!({"name": r.name, "weight": r.base_weight, "description": r.description})).collect::<Vec<_>>(),
        "case_list_sha256": summary.case_list_sha256,
        "predictions_sha256": summary.predictions_sha256,
    });
    fs::write(
        cfg.out_dir.join("spec.json"),
        serde_json::to_string_pretty(&spec_doc).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| format!("write spec.json: {e}"))?;

    let region_json: BTreeMap<&str, Value> = summary
        .region_stats
        .iter()
        .map(|(k, st)| {
            (
                k.as_str(),
                json!({
                    "asked": st.asked,
                    "matched": st.matched,
                    "mismatched": st.mismatched,
                    "unread": st.unread,
                    "crashed": st.crashed,
                    "failed": st.total_failed(),
                    "pass_rate_pct": st.pass_rate(),
                    "worst_ulp": st.worst_ulp,
                }),
            )
        })
        .collect();
    let summary_doc = json!({
        "function": summary.function,
        "seed": summary.seed,
        "budget": summary.budget,
        "dry_run": summary.dry_run,
        "total_generated": summary.total_generated,
        "total_asked": summary.total_asked,
        "total_matched": summary.total_matched,
        "total_mismatched": summary.total_mismatched,
        "total_unread": summary.total_unread,
        "total_crashed": summary.total_crashed,
        "total_failed": summary.total_mismatched + summary.total_unread + summary.total_crashed,
        "pass_rate_pct": if summary.total_asked == 0 { 0.0 } else { (summary.total_matched as f64) * 100.0 / (summary.total_asked as f64) },
        "rounds_run": summary.rounds_run,
        "stop_reason": summary.stop_reason,
        "case_list_sha256": summary.case_list_sha256,
        "predictions_sha256": summary.predictions_sha256,
        "elapsed_seconds": summary.elapsed_seconds,
        "excel_seconds": summary.excel_seconds,
        "calls_per_second": if summary.excel_seconds > 0.0 { summary.total_asked as f64 / summary.excel_seconds } else { 0.0 },
        "worker_calls": summary.worker_calls,
        "regions": region_json,
        "route_coverage": summary.tracker.to_json(),
        "cluster_count": summary.clusters.len(),
        "clusters": summary.clusters.iter().map(DiscrepancyCluster::to_json).collect::<Vec<_>>(),
        "frontier_count": summary.frontiers.len(),
    });
    fs::write(
        cfg.out_dir.join("summary.json"),
        serde_json::to_string_pretty(&summary_doc).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| format!("write summary.json: {e}"))?;

    let frontiers_doc = json!({
        "function": summary.function,
        "seed": summary.seed,
        "frontiers": summary.frontiers.iter().map(Frontier::to_json).collect::<Vec<_>>(),
    });
    fs::write(
        cfg.out_dir.join("frontiers.json"),
        serde_json::to_string_pretty(&frontiers_doc).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| format!("write frontiers.json: {e}"))?;

    fs::write(cfg.out_dir.join("report.md"), render_markdown_report(summary))
        .map_err(|e| format!("write report.md: {e}"))?;

    if !discrepancies.is_empty() {
        let ts = utc_timestamp_now();
        let source_rel = format!(
            "{}/{}-fuzz-seed-{}/spec.json",
            cfg.lane,
            cfg.function.to_ascii_lowercase(),
            cfg.seed
        );
        let ctx = touchstone_bulk_context();
        if let Some(bank_path) = &cfg.bank_rows_out {
            if let Some(parent) = bank_path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            let refs: Vec<&DiscrepancyRow> = discrepancies.iter().collect();
            let selection = format!(
                "cargo xtask fuzz --func {} --seed {} --budget {}: {} discrepancies of {} Excel-compared calls (discrepancies only)",
                cfg.function, cfg.seed, cfg.budget, discrepancies.len(), summary.total_asked
            );
            let encoded = cluster::encode_witness_set_json(
                &cfg.function,
                &cfg.lane,
                &source_rel,
                &selection,
                &ts,
                envs,
                &ctx,
                &refs,
            )?;
            fs::write(bank_path, encoded).map_err(|e| format!("{}: {e}", bank_path.display()))?;
        }
        if let Some(pin_path) = &cfg.pin_out {
            if let Some(parent) = pin_path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            let mut reps: Vec<&DiscrepancyRow> = Vec::new();
            let mut seen_ids = HashSet::new();
            for c in &summary.clusters {
                if seen_ids.insert(c.minimal_rep.case.id.clone()) {
                    reps.push(&c.minimal_rep);
                }
                if seen_ids.insert(c.worst_rep.case.id.clone()) {
                    reps.push(&c.worst_rep);
                }
            }
            let selection = format!(
                "cargo xtask fuzz cluster representatives for {} (seed {}): {} representatives across {} clusters",
                cfg.function, cfg.seed, reps.len(), summary.clusters.len()
            );
            let encoded = cluster::encode_witness_set_json(
                &cfg.function,
                &cfg.lane,
                &source_rel,
                &selection,
                &ts,
                envs,
                &ctx,
                &reps,
            )?;
            fs::write(pin_path, encoded).map_err(|e| format!("{}: {e}", pin_path.display()))?;
        }
    }
    Ok(())
}

pub fn run(raw: &[String]) -> ExitCode {
    let cfg = match parse_args(raw) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("xtask fuzz: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if cfg.all_catalogue {
        if !cfg.dry_run {
            eprintln!("xtask fuzz: --all-catalogue requires --dry-run\n{USAGE}");
            return ExitCode::from(2);
        }
        let hints = match &cfg.hints_dir {
            Some(dir) => hints::HintBank::load_dir(dir),
            None => hints::load_default_hints(),
        };
        let hints = match hints {
            Ok(h) => h,
            Err(e) => {
                eprintln!("xtask fuzz: {e}");
                return ExitCode::from(1);
            }
        };
        match spec_gen::run_all_catalogue_dry_run(&hints, cfg.budget, cfg.seed) {
            Ok(report) => {
                if let Err(e) = fs::create_dir_all(&cfg.out_dir) {
                    eprintln!("xtask fuzz: {}: {e}", cfg.out_dir.display());
                    return ExitCode::from(1);
                }
                let summary_path = cfg.out_dir.join("summary.json");
                let encoded = match serde_json::to_string_pretty(&report.to_json()) {
                    Ok(s) => s + "\n",
                    Err(e) => {
                        eprintln!("xtask fuzz: serialize summary: {e}");
                        return ExitCode::from(1);
                    }
                };
                if let Err(e) = fs::write(&summary_path, encoded) {
                    eprintln!("xtask fuzz: write {}: {e}", summary_path.display());
                    return ExitCode::from(1);
                }
                println!(
                    "xtask fuzz --all-catalogue (dry-run): {} functions ({} with hint files), {} cases/func, {} total cases validated, out {}",
                    report.functions_total,
                    report.functions_with_hints,
                    report.cases_per_function,
                    report.total_cases_generated,
                    summary_path.display()
                );
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!("xtask fuzz: {e}");
                return ExitCode::from(1);
            }
        }
    }
    let res = if cfg.dry_run {
        execute_dry_run(&cfg)
    } else {
        oracle::execute_live_run(&cfg)
    };
    match res {
        Ok(summary) => {
            let (hit, total) = summary.tracker.exercised_kernel_routes();
            if summary.dry_run {
                println!(
                    "xtask fuzz {} (dry-run): generated {} cases (sha256 {}), routes {}/{}, frontiers {}, out {}",
                    summary.function,
                    summary.total_generated,
                    &summary.case_list_sha256[..16],
                    hit,
                    total,
                    summary.frontiers.len(),
                    cfg.out_dir.display()
                );
            } else {
                let fail = summary.total_mismatched + summary.total_unread + summary.total_crashed;
                let pct = if summary.total_asked == 0 {
                    0.0
                } else {
                    (summary.total_matched as f64) * 100.0 / (summary.total_asked as f64)
                };
                let cps = if summary.excel_seconds > 0.0 {
                    summary.total_asked as f64 / summary.excel_seconds
                } else {
                    0.0
                };
                println!(
                    "xtask fuzz {}: {}/{} matched ({:.4}%), {} failed ({} mismatched, {} unread, {} crashed), {} clusters, routes {}/{}, frontiers {}, {:.1} calls/s, workers {:?}, out {}",
                    summary.function,
                    summary.total_matched,
                    summary.total_asked,
                    pct,
                    fail,
                    summary.total_mismatched,
                    summary.total_unread,
                    summary.total_crashed,
                    summary.clusters.len(),
                    hit,
                    total,
                    summary.frontiers.len(),
                    cps,
                    summary.worker_calls,
                    cfg.out_dir.display()
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("xtask fuzz: {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_is_bit_identical_from_same_seed_across_rate_irr_and_yield() {
        for func in ["RATE", "IRR", "YIELD"] {
            let d1 = std::env::temp_dir().join(format!("dnacalc-fuzz-test-{func}-1"));
            let d2 = std::env::temp_dir().join(format!("dnacalc-fuzz-test-{func}-2"));
            let _ = fs::remove_dir_all(&d1);
            let _ = fs::remove_dir_all(&d2);
            let mut c1 = FuzzConfig::default_for(func);
            c1.seed = 99991;
            c1.budget = 400;
            c1.round_size = 200;
            c1.max_frontiers = 6;
            c1.frontier_window = 3;
            c1.dry_run = true;
            c1.out_dir = d1.clone();
            let mut c2 = c1.clone();
            c2.out_dir = d2.clone();

            let s1 = execute_dry_run(&c1).expect("run 1");
            let s2 = execute_dry_run(&c2).expect("run 2");
            assert_eq!(s1.case_list_sha256, s2.case_list_sha256, "{func} deterministic hash");
            assert_eq!(s1.predictions_sha256, s2.predictions_sha256, "{func} frozen prediction hash");
            assert_eq!(s1.total_generated, 400);
            assert!(d1.join("spec.json").is_file());
            assert!(d1.join("summary.json").is_file());
            assert!(d1.join("report.md").is_file());
            let _ = fs::remove_dir_all(&d1);
            let _ = fs::remove_dir_all(&d2);
        }
    }
}
