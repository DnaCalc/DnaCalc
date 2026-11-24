//! Per-function fuzz coverage aggregation from sweep records (`dc-ztvv.26.3`).
//!
//! Reads completed live sweep records written by `cargo xtask fuzz` (standalone runs) and
//! `cargo xtask fuzz sweep` (numbered run directories with `done.json` + `<attempt>/summary.json`),
//! resolves open vs closed cluster status against `.beads/issues.jsonl` using the same
//! `dnacalc-fuzz:<sha256>` identity as `fuzz::sweep`, and writes `crates/calc-func/fuzz-status.tsv`.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

pub const USAGE: &str = "cargo xtask fuzz coverage --records <dir> [--records <dir>...] [--beads <issues.jsonl>] [--out <tsv>] [--merge]";
pub const FUZZ_STATUS_PATH: &str = "crates/calc-func/fuzz-status.tsv";
pub const FUZZ_STATUS_HEADER: &str = "function\tsweeps\tcases_swept\tmatched\tfailed\tregions_covered\tregions_total\tfrontiers_mapped\topen_clusters\tlast_sweep_date\tlast_sweep_commit";

/// One per-function row in `crates/calc-func/fuzz-status.tsv`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuzzCoverageRow {
    pub function: String,
    pub sweeps: u64,
    pub cases_swept: u64,
    pub matched: u64,
    pub failed: u64,
    pub regions_covered: u64,
    pub regions_total: u64,
    pub frontiers_mapped: u64,
    pub open_clusters: u64,
    pub last_sweep_date: String,
    pub last_sweep_commit: String,
}

#[derive(Debug)]
struct Options {
    records: Vec<PathBuf>,
    beads: Option<PathBuf>,
    out: PathBuf,
    merge: bool,
}

impl Options {
    fn parse(raw: &[String]) -> Result<Self, String> {
        let mut records = Vec::new();
        let mut beads = None;
        let mut out = PathBuf::from(FUZZ_STATUS_PATH);
        let mut merge = false;
        let mut it = raw.iter();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--merge" => merge = true,
                "--records" => {
                    let val = it.next().ok_or("--records needs a directory")?;
                    records.push(PathBuf::from(val));
                }
                "--beads" => {
                    let val = it.next().ok_or("--beads needs a path")?;
                    beads = Some(PathBuf::from(val));
                }
                "--out" => {
                    let val = it.next().ok_or("--out needs a path")?;
                    out = PathBuf::from(val);
                }
                other => return Err(format!("unknown coverage option {other}")),
            }
        }
        if records.is_empty() {
            return Err("at least one --records <dir> is required".into());
        }
        Ok(Self {
            records,
            beads,
            out,
            merge,
        })
    }
}

/// Stable external reference for a fuzz cluster, matching `fuzz::sweep::new_cluster_issues`.
pub fn cluster_external_ref(function: &str, key: &str) -> String {
    format!(
        "dnacalc-fuzz:{}",
        calc_verdict::sha256_hex(format!("{function}\n{key}").as_bytes())
    )
}

/// Parse `crates/calc-func/fuzz-status.tsv` into sorted rows.
pub fn parse_tsv(contents: &str) -> Result<Vec<FuzzCoverageRow>, String> {
    let mut lines = contents
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'));
    let header_line = lines.next().ok_or("missing fuzz-status.tsv header")?;
    let headers: Vec<&str> = header_line.split('\t').collect();
    let col = |name: &str| {
        headers
            .iter()
            .position(|h| *h == name)
            .ok_or_else(|| format!("fuzz-status.tsv: missing {name} column"))
    };
    let idx_fn = col("function")?;
    let idx_sweeps = col("sweeps")?;
    let idx_swept = col("cases_swept")?;
    let idx_matched = col("matched")?;
    let idx_failed = col("failed")?;
    let idx_reg_cov = col("regions_covered")?;
    let idx_reg_tot = col("regions_total")?;
    let idx_frontiers = col("frontiers_mapped")?;
    let idx_clusters = col("open_clusters")?;
    let idx_date = col("last_sweep_date")?;
    let idx_commit = col("last_sweep_commit")?;

    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    for (line_no, line) in lines.enumerate() {
        let fields: Vec<&str> = line.split('\t').collect();
        let str_at = |idx: usize, label: &str| -> Result<String, String> {
            fields
                .get(idx)
                .map(|s| (*s).trim().to_owned())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("fuzz-status.tsv line {}: missing {label}", line_no + 2))
        };
        let u64_at = |idx: usize, label: &str| -> Result<u64, String> {
            str_at(idx, label)?
                .parse::<u64>()
                .map_err(|e| format!("fuzz-status.tsv line {}: invalid {label}: {e}", line_no + 2))
        };
        let function = str_at(idx_fn, "function")?.to_ascii_uppercase();
        if !seen.insert(function.clone()) {
            return Err(format!("fuzz-status.tsv: duplicate function {function}"));
        }
        let cases_swept = u64_at(idx_swept, "cases_swept")?;
        let matched = u64_at(idx_matched, "matched")?;
        let failed = u64_at(idx_failed, "failed")?;
        if matched.saturating_add(failed) != cases_swept {
            return Err(format!(
                "fuzz-status.tsv line {} ({function}): matched ({matched}) + failed ({failed}) != cases_swept ({cases_swept})",
                line_no + 2
            ));
        }
        let regions_covered = u64_at(idx_reg_cov, "regions_covered")?;
        let regions_total = u64_at(idx_reg_tot, "regions_total")?;
        if regions_covered > regions_total {
            return Err(format!(
                "fuzz-status.tsv line {} ({function}): regions_covered ({regions_covered}) > regions_total ({regions_total})",
                line_no + 2
            ));
        }
        rows.push(FuzzCoverageRow {
            function,
            sweeps: u64_at(idx_sweeps, "sweeps")?,
            cases_swept,
            matched,
            failed,
            regions_covered,
            regions_total,
            frontiers_mapped: u64_at(idx_frontiers, "frontiers_mapped")?,
            open_clusters: u64_at(idx_clusters, "open_clusters")?,
            last_sweep_date: str_at(idx_date, "last_sweep_date")?,
            last_sweep_commit: str_at(idx_commit, "last_sweep_commit")?,
        });
    }
    rows.sort_by(|a, b| a.function.cmp(&b.function));
    Ok(rows)
}

/// Format rows deterministically in canonical TSV order.
pub fn format_tsv(rows: &[FuzzCoverageRow]) -> String {
    let mut sorted = rows.to_vec();
    sorted.sort_by(|a, b| a.function.cmp(&b.function));
    let mut out = String::from(FUZZ_STATUS_HEADER);
    out.push('\n');
    for r in &sorted {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            r.function,
            r.sweeps,
            r.cases_swept,
            r.matched,
            r.failed,
            r.regions_covered,
            r.regions_total,
            r.frontiers_mapped,
            r.open_clusters,
            r.last_sweep_date,
            r.last_sweep_commit,
        ));
    }
    out
}

#[derive(Default)]
struct BeadsIndex {
    status_by_id: BTreeMap<String, String>,
    status_by_ext_ref: BTreeMap<String, String>,
    open_fuzz_refs_by_func: BTreeMap<String, BTreeSet<String>>,
}

impl BeadsIndex {
    fn load(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut index = Self::default();
        for (line_no, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let item: Value = serde_json::from_str(trimmed)
                .map_err(|e| format!("{}:{}: {e}", path.display(), line_no + 1))?;
            let status = item["status"].as_str().unwrap_or("open").to_owned();
            let is_open = !matches!(status.as_str(), "closed" | "tombstone");
            if let Some(id) = item["id"].as_str() {
                index.status_by_id.insert(id.to_owned(), status.clone());
            }
            if let Some(ext) = item["external_ref"].as_str() {
                let replace = match index.status_by_ext_ref.get(ext) {
                    None => true,
                    Some(prev) => matches!(prev.as_str(), "closed" | "tombstone") && is_open,
                };
                if replace {
                    index.status_by_ext_ref.insert(ext.to_owned(), status.clone());
                }
                if is_open
                    && ext.starts_with("dnacalc-fuzz:")
                    && item["labels"]
                        .as_array()
                        .is_some_and(|labels| labels.iter().any(|l| l.as_str() == Some("fuzz-sweep-cluster")))
                    && let Some(title) = item["title"].as_str()
                    && let Some(rest) = title.strip_prefix("Fuzz discrepancy: ")
                    && let Some(func) = rest.split_whitespace().next()
                {
                    index
                        .open_fuzz_refs_by_func
                        .entry(func.to_ascii_uppercase())
                        .or_default()
                        .insert(ext.to_owned());
                }
            }
        }
        Ok(index)
    }

    fn is_closed(&self, external_ref: &str, explicit_bead: Option<&str>) -> bool {
        if let Some(status) = self.status_by_ext_ref.get(external_ref) {
            return matches!(status.as_str(), "closed" | "tombstone");
        }
        if let Some(bead_id) = explicit_bead
            && let Some(status) = self.status_by_id.get(bead_id)
        {
            return matches!(status.as_str(), "closed" | "tombstone");
        }
        false
    }
}

struct DiscoveredRecord {
    summary_path: PathBuf,
    spec_path: Option<PathBuf>,
    frontiers_path: Option<PathBuf>,
    attempt_path: Option<PathBuf>,
    done_finished_utc: Option<String>,
}

fn discover_records(dir: &Path, out: &mut Vec<DiscoveredRecord>) -> Result<(), String> {
    if !dir.is_dir() {
        return Err(format!("{}: not a directory", dir.display()));
    }
    let done_path = dir.join("done.json");
    if done_path.is_file() {
        let done_text =
            fs::read_to_string(&done_path).map_err(|e| format!("{}: {e}", done_path.display()))?;
        let done: Value = serde_json::from_str(&done_text)
            .map_err(|e| format!("{}: {e}", done_path.display()))?;
        let attempt = done["attempt"]
            .as_u64()
            .ok_or_else(|| format!("{}: missing numeric attempt", done_path.display()))?;
        let attempt_dir = dir.join(format!("{attempt:08}"));
        let summary_path = attempt_dir.join("summary.json");
        if !summary_path.is_file() {
            return Err(format!(
                "{}: completed sweep missing {}",
                done_path.display(),
                summary_path.display()
            ));
        }
        let spec_path = attempt_dir.join("spec.json");
        let frontiers_path = attempt_dir.join("frontiers.json");
        let attempt_path = attempt_dir.join("attempt.json");
        out.push(DiscoveredRecord {
            summary_path,
            spec_path: spec_path.is_file().then_some(spec_path),
            frontiers_path: frontiers_path.is_file().then_some(frontiers_path),
            attempt_path: attempt_path.is_file().then_some(attempt_path),
            done_finished_utc: done["finished_utc"].as_str().map(str::to_owned),
        });
        return Ok(());
    }
    // Incomplete scheduler run directory: skip attempts until done.json exists.
    if dir.join("recipe.json").is_file() {
        return Ok(());
    }
    let summary_path = dir.join("summary.json");
    if summary_path.is_file() {
        let spec_path = dir.join("spec.json");
        let frontiers_path = dir.join("frontiers.json");
        let attempt_path = dir.join("attempt.json");
        out.push(DiscoveredRecord {
            summary_path,
            spec_path: spec_path.is_file().then_some(spec_path),
            frontiers_path: frontiers_path.is_file().then_some(frontiers_path),
            attempt_path: attempt_path.is_file().then_some(attempt_path),
            done_finished_utc: None,
        });
    }
    let mut subdirs = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            subdirs.push(entry.path());
        }
    }
    subdirs.sort();
    for sub in subdirs {
        discover_records(&sub, out)?;
    }
    Ok(())
}

fn date_from_timestamp(ts: &str) -> Option<String> {
    let trimmed = ts.trim();
    if trimmed.len() >= 10 {
        let prefix = &trimmed[..10];
        let b = prefix.as_bytes();
        if b[0..4].iter().all(u8::is_ascii_digit)
            && b[4] == b'-'
            && b[5..7].iter().all(u8::is_ascii_digit)
            && b[7] == b'-'
            && b[8..10].iter().all(u8::is_ascii_digit)
        {
            return Some(prefix.to_owned());
        }
    }
    None
}

fn date_from_mtime(path: &Path) -> String {
    let secs = fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        });
    let days = (secs / 86_400) as i64;
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
    format!("{year:04}-{m:02}-{d:02}")
}

fn short_commit(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty() || s == "-" {
        "-".to_owned()
    } else if s.len() > 10 {
        s[..10].to_owned()
    } else {
        s.to_owned()
    }
}

#[derive(Default)]
struct FunctionAcc {
    sweeps: u64,
    cases_swept: u64,
    matched: u64,
    failed: u64,
    regions_covered: BTreeSet<String>,
    regions_total: BTreeSet<String>,
    unique_frontiers: BTreeSet<String>,
    unindexed_frontiers: u64,
    open_cluster_refs: BTreeSet<String>,
    last_sweep_ts: String,
    last_sweep_date: String,
    last_sweep_commit: String,
}

/// Scan sweep record directories and aggregate per-function fuzz coverage rows.
pub fn collect_from_records(
    record_dirs: &[PathBuf],
    beads_path: Option<&Path>,
) -> Result<Vec<FuzzCoverageRow>, String> {
    let beads = match beads_path {
        Some(path) if path.is_file() => BeadsIndex::load(path)?,
        _ => BeadsIndex::default(),
    };
    let mut discovered = Vec::new();
    for dir in record_dirs {
        discover_records(dir, &mut discovered)?;
    }

    let mut seen_runs: BTreeSet<(String, u64, String, u64)> = BTreeSet::new();
    let mut by_func: BTreeMap<String, FunctionAcc> = BTreeMap::new();

    for rec in discovered {
        let text = fs::read_to_string(&rec.summary_path)
            .map_err(|e| format!("{}: {e}", rec.summary_path.display()))?;
        let summary: Value = serde_json::from_str(&text)
            .map_err(|e| format!("{}: {e}", rec.summary_path.display()))?;

        // Ignore --all-catalogue dry-run reports and any local-only dry runs.
        if summary.get("functions_total").is_some() {
            continue;
        }
        if summary["dry_run"].as_bool().unwrap_or(false) {
            continue;
        }
        let total_asked = summary["total_asked"].as_u64().unwrap_or(0);
        if total_asked == 0 {
            continue;
        }
        let function = summary["function"]
            .as_str()
            .ok_or_else(|| format!("{}: missing function", rec.summary_path.display()))?
            .to_ascii_uppercase();
        let seed = summary["seed"].as_u64().unwrap_or(0);
        let case_hash = summary["case_list_sha256"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        if !case_hash.is_empty() && !seen_runs.insert((function.clone(), seed, case_hash, total_asked))
        {
            continue;
        }

        let total_matched = summary["total_matched"].as_u64().unwrap_or(0);
        let total_failed = match (
            summary["total_mismatched"].as_u64(),
            summary["total_unread"].as_u64(),
            summary["total_crashed"].as_u64(),
        ) {
            (Some(m), Some(u), Some(c)) => m.saturating_add(u).saturating_add(c),
            _ => summary["total_failed"]
                .as_u64()
                .unwrap_or_else(|| total_asked.saturating_sub(total_matched)),
        };
        if total_matched.saturating_add(total_failed) != total_asked {
            return Err(format!(
                "{}: matched ({total_matched}) + failed ({total_failed}) != total_asked ({total_asked})",
                rec.summary_path.display()
            ));
        }

        let attempt_json: Option<Value> = match &rec.attempt_path {
            Some(path) => {
                let raw = fs::read_to_string(path)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                Some(serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", path.display()))?)
            }
            None => None,
        };

        let timestamp = rec
            .done_finished_utc
            .clone()
            .or_else(|| summary["finished_utc"].as_str().map(str::to_owned))
            .or_else(|| {
                attempt_json
                    .as_ref()
                    .and_then(|a| a["started_utc"].as_str().map(str::to_owned))
            })
            .unwrap_or_else(|| format!("{}T00:00:00Z", date_from_mtime(&rec.summary_path)));
        let sweep_date = date_from_timestamp(&timestamp)
            .unwrap_or_else(|| date_from_mtime(&rec.summary_path));

        let sweep_commit = attempt_json
            .as_ref()
            .and_then(|a| a["versions"]["calc_func_commit"].as_str())
            .or_else(|| summary["calc_func_commit"].as_str())
            .map(short_commit)
            .unwrap_or_else(|| "-".to_owned());

        let acc = by_func.entry(function.clone()).or_default();
        acc.sweeps += 1;
        acc.cases_swept += total_asked;
        acc.matched += total_matched;
        acc.failed += total_failed;

        if let Some(regions) = summary["regions"].as_object() {
            for (name, stats) in regions {
                acc.regions_total.insert(name.clone());
                if stats["asked"].as_u64().unwrap_or(0) > 0 {
                    acc.regions_covered.insert(name.clone());
                }
            }
        }
        if let Some(spec_path) = &rec.spec_path {
            let spec_text = fs::read_to_string(spec_path)
                .map_err(|e| format!("{}: {e}", spec_path.display()))?;
            let spec: Value = serde_json::from_str(&spec_text)
                .map_err(|e| format!("{}: {e}", spec_path.display()))?;
            if let Some(regions) = spec["regions"].as_array() {
                for region in regions {
                    if let Some(name) = region["name"].as_str() {
                        acc.regions_total.insert(name.to_owned());
                    }
                }
            }
        }

        let mut loaded_frontiers = false;
        if let Some(frontiers_path) = &rec.frontiers_path {
            let ftext = fs::read_to_string(frontiers_path)
                .map_err(|e| format!("{}: {e}", frontiers_path.display()))?;
            let fdoc: Value = serde_json::from_str(&ftext)
                .map_err(|e| format!("{}: {e}", frontiers_path.display()))?;
            if let Some(arr) = fdoc["frontiers"].as_array() {
                loaded_frontiers = true;
                for f in arr {
                    let key = format!(
                        "{}|{}|{}|{}",
                        f["axis_name"].as_str().unwrap_or(""),
                        f["kind"].as_str().unwrap_or(""),
                        f["dna_lo_bits"].as_str().unwrap_or(""),
                        f["dna_hi_bits"].as_str().unwrap_or(""),
                    );
                    acc.unique_frontiers.insert(key);
                }
            }
        }
        if !loaded_frontiers {
            acc.unindexed_frontiers += summary["frontier_count"].as_u64().unwrap_or(0);
        }

        if let Some(clusters) = summary["clusters"].as_array() {
            for cluster in clusters {
                if let Some(key) = cluster["key"].as_str() {
                    let ext_ref = cluster_external_ref(&function, key);
                    let explicit_bead = cluster["bead"].as_str();
                    if !beads.is_closed(&ext_ref, explicit_bead) {
                        acc.open_cluster_refs.insert(ext_ref);
                    }
                }
            }
        }

        if timestamp >= acc.last_sweep_ts {
            acc.last_sweep_ts = timestamp;
            acc.last_sweep_date = sweep_date;
            if sweep_commit != "-" || acc.last_sweep_commit.is_empty() {
                acc.last_sweep_commit = sweep_commit;
            }
        }
    }

    // Union any open fuzz-sweep-cluster beads for functions that were swept.
    for (function, acc) in &mut by_func {
        if let Some(open_refs) = beads.open_fuzz_refs_by_func.get(function) {
            for r in open_refs {
                acc.open_cluster_refs.insert(r.clone());
            }
        }
    }

    let rows = by_func
        .into_iter()
        .map(|(function, acc)| {
            let regions_covered = acc.regions_covered.len() as u64;
            let regions_total = (acc.regions_total.len() as u64).max(regions_covered);
            let frontiers_mapped =
                (acc.unique_frontiers.len() as u64).saturating_add(acc.unindexed_frontiers);
            FuzzCoverageRow {
                function,
                sweeps: acc.sweeps,
                cases_swept: acc.cases_swept,
                matched: acc.matched,
                failed: acc.failed,
                regions_covered,
                regions_total,
                frontiers_mapped,
                open_clusters: acc.open_cluster_refs.len() as u64,
                last_sweep_date: acc.last_sweep_date,
                last_sweep_commit: if acc.last_sweep_commit.is_empty() {
                    "-".to_owned()
                } else {
                    acc.last_sweep_commit
                },
            }
        })
        .collect();
    Ok(rows)
}

fn write_atomic(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension("tsv.tmp");
    fs::write(&tmp, content)
        .and_then(|()| fs::rename(&tmp, path))
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn execute(options: &Options) -> Result<Vec<FuzzCoverageRow>, String> {
    let default_beads = PathBuf::from(".beads/issues.jsonl");
    let beads_path = options
        .beads
        .as_deref()
        .or_else(|| default_beads.is_file().then_some(default_beads.as_path()));
    let mut fresh = collect_from_records(&options.records, beads_path)?;
    if options.merge && options.out.is_file() {
        let existing_text = fs::read_to_string(&options.out)
            .map_err(|e| format!("{}: {e}", options.out.display()))?;
        let mut by_fn: BTreeMap<String, FuzzCoverageRow> = parse_tsv(&existing_text)?
            .into_iter()
            .map(|r| (r.function.clone(), r))
            .collect();
        for row in fresh {
            by_fn.insert(row.function.clone(), row);
        }
        fresh = by_fn.into_values().collect();
    }
    let rendered = format_tsv(&fresh);
    write_atomic(&options.out, &rendered)?;
    Ok(fresh)
}

pub(super) fn run(raw: &[String]) -> ExitCode {
    if raw == ["--help"] {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let options = match Options::parse(raw) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("fuzz coverage: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match execute(&options) {
        Ok(rows) => {
            let swept: u64 = rows.iter().map(|r| r.cases_swept).sum();
            let matched: u64 = rows.iter().map(|r| r.matched).sum();
            let failed: u64 = rows.iter().map(|r| r.failed).sum();
            let frontiers: u64 = rows.iter().map(|r| r.frontiers_mapped).sum();
            let clusters: u64 = rows.iter().map(|r| r.open_clusters).sum();
            println!(
                "xtask fuzz coverage: {} functions, {} cases swept ({} matched, {} failed), {} frontiers, {} open clusters -> {}",
                rows.len(),
                swept,
                matched,
                failed,
                frontiers,
                clusters,
                options.out.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("fuzz coverage: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuzz::{
        FuzzConfig, FuzzSummary,
        boundary::Frontier,
        cluster::{DiscrepancyCluster, DiscrepancyRow, RegionStats},
        domain::DomainSpec,
        fractal::FractalDiscoveryStats,
        steer::{CoverageTracker, FuzzCase},
        write_outputs,
    };
    use calc_func::trace::RouteTrace;
    use calc_verdict::Severity;
    use serde_json::json;

    fn sample_cluster(function: &str, id: &str, key: &str, count: usize) -> DiscrepancyCluster {
        let rep = DiscrepancyRow {
            case: FuzzCase {
                id: format!("{function}-{id}-0"),
                region: "special_values".to_owned(),
                round: 0,
                args: vec![json!("0x3ff0000000000000")],
                prediction: "error:Num".to_owned(),
                trace: RouteTrace::default(),
                outcome_class: "error:Num".to_owned(),
            },
            expected: "0x3ff0000000000000".to_owned(),
            severity: Severity::Structural,
            ulp: None,
            env_id: "env-test".to_owned(),
            cluster_key: key.to_owned(),
        };
        DiscrepancyCluster {
            id: id.to_owned(),
            key: key.to_owned(),
            function: function.to_owned(),
            kind: "error_vs_number".to_owned(),
            region_group: "special_values".to_owned(),
            count,
            worst_severity: Severity::Structural,
            worst_ulp: None,
            regions: BTreeMap::from([("special_values".to_owned(), count)]),
            minimal_rep: rep.clone(),
            worst_rep: rep,
        }
    }

    fn sample_frontier(function: &str, id: &str, lo: u64, hi: u64) -> Frontier {
        Frontier {
            id: id.to_owned(),
            function: function.to_owned(),
            axis: 0,
            axis_cell: None,
            axis_name: "nper".to_owned(),
            kind: "number_vs_error".to_owned(),
            depth: 0,
            scale: "ulp".to_owned(),
            transition_kind: "number<->error:Num".to_owned(),
            parent_id: None,
            base_args: vec![json!("0x4000000000000000")],
            dna_lo_bits: lo,
            dna_hi_bits: hi,
            dna_lo_outcome: "number".to_owned(),
            dna_hi_outcome: "error:Num".to_owned(),
            excel_lo_bits: Some(lo),
            excel_hi_bits: Some(hi),
            excel_lo_outcome: Some("number".to_owned()),
            excel_hi_outcome: Some("error:Num".to_owned()),
            shifted_ulps: Some(0),
        }
    }

    #[test]
    fn coverage_aggregates_standalone_and_scheduler_sweeps_filters_dry_runs_and_resolves_beads() {
        let base = std::env::temp_dir().join(format!("dnacalc-fuzz-cov-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        // 1. Standalone live RATE sweep written through the real write_outputs path.
        let rate_dir = base.join("standalone-rate");
        let mut rate_cfg = FuzzConfig::default_for("RATE");
        rate_cfg.out_dir = rate_dir.clone();
        rate_cfg.seed = 20261123;
        rate_cfg.budget = 100;
        let rate_spec = DomainSpec::for_function("RATE").unwrap();
        let mut rate_regions = BTreeMap::new();
        rate_regions.insert(
            "special_values".to_owned(),
            RegionStats {
                asked: 40,
                matched: 36,
                mismatched: 4,
                unread: 0,
                crashed: 0,
                worst_ulp: None,
                clusters_seen: 2,
            },
        );
        rate_regions.insert(
            "near_integers".to_owned(),
            RegionStats {
                asked: 60,
                matched: 60,
                mismatched: 0,
                unread: 0,
                crashed: 0,
                worst_ulp: None,
                clusters_seen: 0,
            },
        );
        let rate_summary = FuzzSummary {
            function: "RATE".to_owned(),
            seed: 20261123,
            budget: 100,
            dry_run: false,
            total_generated: 100,
            total_asked: 100,
            total_matched: 96,
            total_mismatched: 4,
            total_unread: 0,
            total_crashed: 0,
            rounds_run: 2,
            stop_reason: "budget-reached".to_owned(),
            case_list_sha256: "aaaa1111".to_owned(),
            predictions_sha256: vec!["pred1".to_owned()],
            elapsed_seconds: 1.5,
            excel_seconds: 1.0,
            worker_calls: BTreeMap::from([("w1".to_owned(), 100)]),
            region_stats: rate_regions,
            clusters: vec![
                sample_cluster("RATE", "C01", "cluster-open-rate", 3),
                sample_cluster("RATE", "C02", "cluster-closed-rate", 1),
            ],
            frontiers: vec![
                sample_frontier("RATE", "F01", 0x3ff0_0000_0000_0000, 0x3ff0_0000_0000_0001),
                sample_frontier("RATE", "F02", 0x4000_0000_0000_0000, 0x4000_0000_0000_0001),
            ],
            fractal_stats: FractalDiscoveryStats::default(),
            tracker: CoverageTracker::new("RATE"),
        };
        write_outputs(&rate_cfg, &rate_spec, &rate_summary, &[], &BTreeMap::new()).unwrap();

        // 2. Dry-run sweep for ABS must be ignored (0 Excel calls).
        let abs_dry_dir = base.join("dry-abs");
        let mut abs_cfg = FuzzConfig::default_for("ABS");
        abs_cfg.out_dir = abs_dry_dir.clone();
        abs_cfg.budget = 25;
        abs_cfg.dry_run = true;
        crate::fuzz::execute_dry_run(&abs_cfg).unwrap();

        // 3. Scheduler run directory (00000001) with interrupted attempt 00000001 and completed attempt 00000002.
        let sched_run = base.join("sweep-state/00000001");
        let att1 = sched_run.join("00000001");
        let att2 = sched_run.join("00000002");
        fs::create_dir_all(&att1).unwrap();
        fs::create_dir_all(&att2).unwrap();
        // Interrupted attempt summary must NOT be counted because done.json points to attempt 2.
        fs::write(
            att1.join("summary.json"),
            serde_json::to_string_pretty(&json!({
                "function": "IRR",
                "seed": 77,
                "dry_run": false,
                "total_asked": 999,
                "total_matched": 999,
                "total_mismatched": 0,
                "total_unread": 0,
                "total_crashed": 0,
                "case_list_sha256": "stale-attempt"
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            att2.join("attempt.json"),
            serde_json::to_string_pretty(&json!({
                "started_utc": "2026-11-24T08:00:00Z",
                "versions": {"calc_func_commit": "abcdef1234567890"}
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            att2.join("spec.json"),
            serde_json::to_string_pretty(&json!({
                "regions": [
                    {"name": "special_values"},
                    {"name": "sign_patterns"},
                    {"name": "log_uniform"}
                ]
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            att2.join("summary.json"),
            serde_json::to_string_pretty(&json!({
                "function": "IRR",
                "seed": 77,
                "dry_run": false,
                "total_asked": 50,
                "total_matched": 45,
                "total_mismatched": 4,
                "total_unread": 1,
                "total_crashed": 0,
                "case_list_sha256": "bbbb2222",
                "frontier_count": 3,
                "regions": {
                    "special_values": {"asked": 20, "matched": 18, "mismatched": 2, "unread": 0, "crashed": 0},
                    "sign_patterns": {"asked": 30, "matched": 27, "mismatched": 2, "unread": 1, "crashed": 0},
                    "log_uniform": {"asked": 0, "matched": 0, "mismatched": 0, "unread": 0, "crashed": 0}
                },
                "clusters": [
                    {"id": "C01", "key": "irr-cluster-open", "count": 4}
                ]
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            sched_run.join("done.json"),
            serde_json::to_string_pretty(&json!({
                "attempt": 2,
                "finished_utc": "2026-11-24T08:15:00Z"
            }))
            .unwrap(),
        )
        .unwrap();

        // Incomplete scheduler run 00000002 (no done.json) must be skipped completely.
        let sched_incomplete = base.join("sweep-state/00000002");
        fs::create_dir_all(sched_incomplete.join("00000001")).unwrap();
        fs::write(sched_incomplete.join("recipe.json"), r#"{"function":"YIELD"}"#).unwrap();
        fs::write(
            sched_incomplete.join("00000001/summary.json"),
            r#"{"function":"YIELD","seed":1,"dry_run":false,"total_asked":10,"total_matched":10,"total_mismatched":0,"total_unread":0,"total_crashed":0}"#,
        )
        .unwrap();

        // 4. Tracker issues.jsonl marking cluster-closed-rate as closed and cluster-open-rate as open.
        let beads_path = base.join("issues.jsonl");
        let closed_ref = cluster_external_ref("RATE", "cluster-closed-rate");
        let open_ref = cluster_external_ref("RATE", "cluster-open-rate");
        fs::write(
            &beads_path,
            format!(
                "{}\n{}\n",
                json!({
                    "id": "dc-test.1",
                    "title": "Fuzz discrepancy: RATE error_vs_number",
                    "status": "closed",
                    "labels": ["fuzz-sweep-cluster"],
                    "external_ref": closed_ref
                }),
                json!({
                    "id": "dc-test.2",
                    "title": "Fuzz discrepancy: RATE error_vs_number",
                    "status": "open",
                    "labels": ["fuzz-sweep-cluster"],
                    "external_ref": open_ref
                }),
            ),
        )
        .unwrap();

        let out_tsv = base.join("fuzz-status.tsv");
        let args = [
            "--records",
            base.to_str().unwrap(),
            "--beads",
            beads_path.to_str().unwrap(),
            "--out",
            out_tsv.to_str().unwrap(),
        ]
        .map(str::to_owned);
        assert_eq!(run(&args), ExitCode::SUCCESS);

        let parsed = parse_tsv(&fs::read_to_string(&out_tsv).unwrap()).unwrap();
        assert_eq!(parsed.len(), 2, "expected only IRR and RATE: {parsed:?}");

        let irr = &parsed[0];
        assert_eq!(irr.function, "IRR");
        assert_eq!(irr.sweeps, 1);
        assert_eq!(irr.cases_swept, 50);
        assert_eq!(irr.matched, 45);
        assert_eq!(irr.failed, 5);
        assert_eq!(irr.regions_covered, 2);
        assert_eq!(irr.regions_total, 3);
        assert_eq!(irr.frontiers_mapped, 3);
        assert_eq!(irr.open_clusters, 1);
        assert_eq!(irr.last_sweep_date, "2026-11-24");
        assert_eq!(irr.last_sweep_commit, "abcdef1234");

        let rate = &parsed[1];
        assert_eq!(rate.function, "RATE");
        assert_eq!(rate.sweeps, 1);
        assert_eq!(rate.cases_swept, 100);
        assert_eq!(rate.matched, 96);
        assert_eq!(rate.failed, 4);
        assert_eq!(rate.regions_covered, 2);
        assert_eq!(rate.regions_total, rate_spec.regions.len() as u64);
        assert_eq!(rate.frontiers_mapped, 2);
        assert_eq!(rate.open_clusters, 1, "closed cluster bead must be excluded");
        assert_eq!(rate.last_sweep_date.len(), 10);

        // Round-trip format/parse invariant.
        assert_eq!(parse_tsv(&format_tsv(&parsed)).unwrap(), parsed);

        // Scanning the same records twice must deduplicate rather than double-count.
        let dup_tsv = base.join("fuzz-status-dup.tsv");
        let dup_args = [
            "--records",
            base.to_str().unwrap(),
            "--records",
            base.to_str().unwrap(),
            "--beads",
            beads_path.to_str().unwrap(),
            "--out",
            dup_tsv.to_str().unwrap(),
        ]
        .map(str::to_owned);
        assert_eq!(run(&dup_args), ExitCode::SUCCESS);
        assert_eq!(
            parse_tsv(&fs::read_to_string(&dup_tsv).unwrap()).unwrap(),
            parsed
        );

        // --merge preserves previously recorded functions not in the new batch.
        let merged_tsv = base.join("fuzz-status-merged.tsv");
        let existing_row = FuzzCoverageRow {
            function: "SIN".to_owned(),
            sweeps: 1,
            cases_swept: 200,
            matched: 200,
            failed: 0,
            regions_covered: 4,
            regions_total: 4,
            frontiers_mapped: 1,
            open_clusters: 0,
            last_sweep_date: "2026-11-22".to_owned(),
            last_sweep_commit: "1111222233".to_owned(),
        };
        fs::write(&merged_tsv, format_tsv(&[existing_row.clone()])).unwrap();
        let merge_args = [
            "--records",
            base.to_str().unwrap(),
            "--beads",
            beads_path.to_str().unwrap(),
            "--out",
            merged_tsv.to_str().unwrap(),
            "--merge",
        ]
        .map(str::to_owned);
        assert_eq!(run(&merge_args), ExitCode::SUCCESS);
        let merged = parse_tsv(&fs::read_to_string(&merged_tsv).unwrap()).unwrap();
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[2], existing_row);

        let _ = fs::remove_dir_all(&base);
    }
}
