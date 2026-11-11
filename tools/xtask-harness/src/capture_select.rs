//! `cargo xtask capture-select --set <name>=<files.jsonl>... [--covered <files.jsonl>]...
//! [--count <n>] [--repeat <n>] [--max-bytes <n>] [--max-formulas <n>] [--out <file.md>]`: picks the corpus files
//! for an Excel capture batch from the classifier's `files.jsonl` (bead dc-ztvv, `classify`).
//!
//! Excel answers are the scarce thing, so the files are chosen where one Excel minute buys the
//! most new (function, argument shape) pairs and formula features:
//!
//! 1. Excluded: files our reader refuses (blockers), files with a VBA project, a macro sheet or
//!    an `.xlsm`/`.xlsb` extension, files with external links, add-in or user-defined function
//!    calls (`_xll.`, `_xludf.`), live-data functions (WEBSERVICE, RTD, CUBE*, STOCKHISTORY), embedded
//!    ActiveX controls, data connections and query tables, files with no formula, files over the
//!    size limits, and repeats of one SHA-256.
//! 2. Gain of a file: 1.0 for each shape key no chosen file (and no `--covered` file) has, 0.25
//!    for a shape held by fewer than `--repeat` (default 5) chosen files, and 3.0 for each formula feature or formula kind
//!    nothing chosen has. The random functions' shapes carry no gain (their answers cannot be
//!    compared).
//! 3. Greedy: the file with the most gain per estimated Excel second is chosen, until `--count`
//!    files are chosen or none has gain left.
//!
//! The Excel time is a model, not a measurement: `OPEN_SECONDS` + bytes at `SECONDS_PER_MB` +
//! formula cells at `SECONDS_PER_CELL` (open, full rebuild, read back). The first measured batch
//! replaces it. Nothing here starts Excel or opens a corpus file.
//!
//! Exit codes: 0 when the output was written, 1 when it cannot be, 2 on a usage error.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::process::ExitCode;

pub const USAGE: &str = "usage: cargo xtask capture-select --set <name>=<files.jsonl>... [--covered <files.jsonl>]... [--count <n>] [--repeat <n>] [--max-bytes <n>] [--max-formulas <n>] [--out <file.md>]";

/// Fixed seconds per file: open, full rebuild, close.
const OPEN_SECONDS: f64 = 3.0;
const SECONDS_PER_MB: f64 = 1.5;
const SECONDS_PER_CELL: f64 = 0.0002;

const NEW_SHAPE: f64 = 1.0;
const FEW_SHAPE: f64 = 0.25;
/// Default for `--repeat`: files that must hold a shape before more of it gains nothing.
const COVERED_ENOUGH: u32 = 5;
const NEW_FEATURE: f64 = 3.0;

/// Functions that read live data at recalculation.
const LIVE: &[&str] = &["WEBSERVICE", "RTD", "STOCKHISTORY", "FILTERXML_NETWORK"];
/// Functions whose answers differ between two calculations.
const RANDOM: &[&str] = &["RAND", "RANDBETWEEN", "RANDARRAY"];

struct Args {
    sets: Vec<(String, String)>,
    covered: Vec<String>,
    count: usize,
    repeat: u32,
    max_bytes: u64,
    max_formulas: u64,
    out: Option<String>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut a = Args {
        sets: Vec::new(),
        covered: Vec::new(),
        count: 500,
        repeat: COVERED_ENOUGH,
        max_bytes: 8 * 1024 * 1024,
        max_formulas: 300_000,
        out: None,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--set" => {
                let v = value("--set")?;
                let (name, path) = v.split_once('=').ok_or("--set takes <name>=<files.jsonl>")?;
                a.sets.push((name.to_owned(), path.to_owned()));
            }
            "--covered" => a.covered.push(value("--covered")?),
            "--count" => a.count = value("--count")?.parse().map_err(|_| "--count is a number")?,
            "--repeat" => a.repeat = value("--repeat")?.parse().map_err(|_| "--repeat is a number")?,
            "--max-bytes" => a.max_bytes = value("--max-bytes")?.parse().map_err(|_| "--max-bytes is a number")?,
            "--max-formulas" => a.max_formulas = value("--max-formulas")?.parse().map_err(|_| "--max-formulas is a number")?,
            "--out" => a.out = Some(value("--out")?),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if a.sets.is_empty() {
        return Err("at least one --set is needed".into());
    }
    Ok(a)
}

#[derive(Debug, Clone)]
struct Candidate {
    set: String,
    sha: String,
    bytes: u64,
    formula_cells: u64,
    shapes: BTreeSet<String>,
    features: BTreeSet<String>,
    seconds: f64,
}

fn map_keys(v: Option<&Value>) -> impl Iterator<Item = &String> {
    v.and_then(Value::as_object).into_iter().flat_map(|o| o.keys())
}

fn function_of(shape: &str) -> &str {
    shape.split('(').next().unwrap_or(shape)
}

/// Why a file is not a candidate, or the candidate.
fn judge(set: &str, p: &Value, max_bytes: u64, max_formulas: u64) -> Result<Candidate, String> {
    let reader = &p["reader"];
    if reader["opened"].as_bool() != Some(true) {
        return Err("reader refuses the file (blocker)".into());
    }
    let extension = p["extension"].as_str().unwrap_or("").to_ascii_lowercase();
    if reader["has_vba_project"].as_bool() == Some(true)
        || reader["macro_sheets"].as_u64().unwrap_or(0) > 0
        || p["package"]["part_categories"].as_object().is_some_and(|c| c.keys().any(|k| k.contains("vbaProject")))
        || extension == "xlsm"
        || extension == "xlsb"
    {
        return Err("macros (VBA project, macro sheet, .xlsm or .xlsb)".into());
    }
    let categories: Vec<&String> = map_keys(p["package"].get("part_categories")).collect();
    let flags: BTreeSet<String> = map_keys(p.get("formula_features")).cloned().collect();
    if reader["links"].as_u64().unwrap_or(0) > 0
        || flags.contains("external_reference")
        || categories.iter().any(|k| k.contains("externalLinks"))
    {
        return Err("external links".into());
    }
    if flags.contains("add_in_call") || flags.contains("future_or_user_defined_function") {
        return Err("add-in or user-defined function calls".into());
    }
    let functions: Vec<&String> = map_keys(p.get("functions")).collect();
    if functions.iter().any(|f| LIVE.contains(&f.as_str()) || f.starts_with("CUBE")) {
        return Err("live-data functions (WEBSERVICE, RTD, CUBE*, STOCKHISTORY)".into());
    }
    if categories.iter().any(|k| k.contains("activeX") || k.contains("connections") || k.contains("queryTables")) {
        return Err("ActiveX controls, data connections or query tables".into());
    }
    let formula_cells = p["cells"]["formula_cells"].as_u64().unwrap_or(0);
    if formula_cells == 0 {
        return Err("no formula cells".into());
    }
    let bytes = p["bytes"].as_u64().unwrap_or(0);
    if bytes > max_bytes || formula_cells > max_formulas {
        return Err("over the size limits".into());
    }
    let shapes: BTreeSet<String> = map_keys(p.get("function_shapes"))
        .filter(|s| s.as_str() != "(other)" && !RANDOM.contains(&function_of(s)))
        .cloned()
        .collect();
    if shapes.is_empty() && p.get("function_shapes").is_none() {
        return Err("no function_shapes in the record (classify with a build that writes them)".into());
    }
    let mut features: BTreeSet<String> = flags;
    features.remove("volatile");
    features.extend(map_keys(p["cells"].get("formula_kinds")).filter(|k| k.as_str() != "normal").map(|k| format!("kind:{k}")));
    if reader["date_system"].as_str().is_some_and(|d| d != "Date1900") {
        features.insert("date1904".into());
    }
    if let Some(mode) = reader["calc_mode"].as_str().filter(|m| *m != "Automatic") {
        features.insert(format!("calc_mode:{mode}"));
    }
    let seconds = OPEN_SECONDS + bytes as f64 / 1_048_576.0 * SECONDS_PER_MB + formula_cells as f64 * SECONDS_PER_CELL;
    Ok(Candidate {
        set: set.to_owned(),
        sha: p["sha256"].as_str().unwrap_or("").to_owned(),
        bytes,
        formula_cells,
        shapes,
        features,
        seconds,
    })
}

fn read_lines(path: &str) -> Result<Vec<Value>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).map_err(|e| format!("{path}: {e}")))
        .collect()
}

/// One chosen file with what it adds.
#[derive(Debug)]
pub(crate) struct Chosen {
    pub(crate) candidate: Candidate,
    pub(crate) new_pairs: usize,
    pub(crate) new_features: usize,
}

/// The greedy choice (see the module's rules).
pub(crate) fn choose(
    candidates: Vec<Candidate>,
    covered_shapes: &BTreeSet<String>,
    count: usize,
    repeat: u32,
) -> Vec<Chosen> {
    let mut times: BTreeMap<&str, u32> = BTreeMap::new();
    let mut features_have: BTreeSet<&str> = BTreeSet::new();
    for s in covered_shapes {
        times.insert(s.as_str(), repeat);
    }
    let mut remaining: Vec<usize> = (0..candidates.len()).collect();
    let mut chosen = Vec::new();
    while chosen.len() < count {
        let gain = |c: &Candidate, times: &BTreeMap<&str, u32>, have: &BTreeSet<&str>| -> f64 {
            let shapes: f64 = c
                .shapes
                .iter()
                .map(|s| match times.get(s.as_str()).copied().unwrap_or(0) {
                    0 => NEW_SHAPE,
                    n if n < repeat => FEW_SHAPE,
                    _ => 0.0,
                })
                .sum();
            let features: f64 = c.features.iter().filter(|f| !have.contains(f.as_str())).count() as f64 * NEW_FEATURE;
            shapes + features
        };
        let mut best: Option<(usize, f64)> = None;
        for &i in &remaining {
            let c = &candidates[i];
            let g = gain(c, &times, &features_have);
            if g <= 0.0 {
                continue;
            }
            let score = g / c.seconds;
            if best.is_none_or(|(_, s)| score > s) {
                best = Some((i, score));
            }
        }
        let Some((index, _)) = best else { break };
        remaining.retain(|&i| i != index);
        let c = &candidates[index];
        let new_pairs = c.shapes.iter().filter(|s| !times.contains_key(s.as_str())).count();
        let new_features = c.features.iter().filter(|f| !features_have.contains(f.as_str())).count();
        for s in &c.shapes {
            *times.entry(s.as_str()).or_default() += 1;
        }
        for f in &c.features {
            features_have.insert(f.as_str());
        }
        chosen.push((index, new_pairs, new_features));
    }
    drop(times);
    drop(features_have);
    let mut by_index: BTreeMap<usize, Candidate> = candidates.into_iter().enumerate().collect();
    chosen
        .into_iter()
        .map(|(i, new_pairs, new_features)| Chosen {
            candidate: by_index.remove(&i).expect("a candidate index"),
            new_pairs,
            new_features,
        })
        .collect()
}

pub fn run(args: &[String]) -> ExitCode {
    let args = match parse_args(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("xtask capture-select: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match select(&args) {
        Ok(report) => match &args.out {
            Some(path) => match std::fs::write(path, report) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("xtask capture-select: {path}: {e}");
                    ExitCode::from(1)
                }
            },
            None => {
                print!("{report}");
                ExitCode::SUCCESS
            }
        },
        Err(e) => {
            eprintln!("xtask capture-select: {e}");
            ExitCode::from(1)
        }
    }
}

fn select(args: &Args) -> Result<String, String> {
    let mut covered = BTreeSet::new();
    for path in &args.covered {
        for p in read_lines(path)? {
            covered.extend(map_keys(p.get("function_shapes")).cloned());
        }
    }
    let mut excluded: BTreeMap<String, u64> = BTreeMap::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut candidates = Vec::new();
    let mut files_by_set: BTreeMap<String, u64> = BTreeMap::new();
    for (set, path) in &args.sets {
        for p in read_lines(path)? {
            *files_by_set.entry(set.clone()).or_default() += 1;
            match judge(set, &p, args.max_bytes, args.max_formulas) {
                Ok(c) => {
                    if !c.sha.is_empty() && !seen.insert(c.sha.clone()) {
                        *excluded.entry("same SHA-256 as an earlier candidate".into()).or_default() += 1;
                    } else {
                        candidates.push(c);
                    }
                }
                Err(why) => *excluded.entry(why).or_default() += 1,
            }
        }
    }
    let candidate_count = candidates.len();
    let (universe_len, new_universe) = {
        let universe: BTreeSet<&String> = candidates.iter().flat_map(|c| c.shapes.iter()).collect();
        (universe.len(), universe.iter().filter(|s| !covered.contains(s.as_str())).count())
    };
    let chosen = choose(candidates, &covered, args.count, args.repeat);
    let total_seconds: f64 = chosen.iter().map(|c| c.candidate.seconds).sum();
    let new_pairs: usize = chosen.iter().map(|c| c.new_pairs).sum();
    let new_features: usize = chosen.iter().map(|c| c.new_features).sum();
    let cells: u64 = chosen.iter().map(|c| c.candidate.formula_cells).sum();
    let mut out = String::new();
    let _ = writeln!(out, "# Capture batch 1 (bead dc-ztvv)\n");
    let _ = writeln!(
        out,
        "Written by `cargo xtask capture-select` from the classifier's `files.jsonl`. Hashes only: no file names, sheet names or contents. Nothing here ran Excel or opened a corpus file; the batch waits for the capture machine.\n"
    );
    let _ = writeln!(out, "## Result\n");
    let _ = writeln!(out, "* {} files chosen of {candidate_count} candidates ({} files read).", chosen.len(), files_by_set.values().sum::<u64>());
    let _ = writeln!(out, "* {new_pairs} new (function, argument shape) pairs, of {new_universe} pairs the candidates hold that no covered file has ({} pairs in all among candidates); {new_features} new formula features or kinds.", universe_len);
    let _ = writeln!(out, "* {cells} formula cells to capture.");
    let _ = writeln!(
        out,
        "* Estimated Excel time: {:.0} minutes ({:.1} hours), by the model {OPEN_SECONDS} s per file + {SECONDS_PER_MB} s per MB + {SECONDS_PER_CELL} s per formula cell. It is a model; the first measured batch replaces it.",
        total_seconds / 60.0,
        total_seconds / 3600.0
    );
    let _ = writeln!(out, "* Covered baseline: {} shape keys from {} file list(s).\n", covered.len(), args.covered.len());
    let _ = writeln!(out, "## Settings\n");
    let _ = writeln!(out, "Count {}, repeat {}, size limits {} bytes and {} formula cells, sets: {}.\n", args.count, args.repeat, args.max_bytes, args.max_formulas, args.sets.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", "));
    let _ = writeln!(out, "## Excluded\n\n| Reason | Files |\n|---|---:|");
    for (why, n) in &excluded {
        let _ = writeln!(out, "| {why} | {n} |");
    }
    let mut per_set: BTreeMap<&str, (u64, usize, f64)> = BTreeMap::new();
    for c in &chosen {
        let e = per_set.entry(c.candidate.set.as_str()).or_default();
        e.0 += 1;
        e.1 += c.new_pairs;
        e.2 += c.candidate.seconds;
    }
    let _ = writeln!(out, "\n## By source\n\n| Source | Files chosen | New pairs | Est. minutes |\n|---|---:|---:|---:|");
    for (set, (n, pairs, secs)) in &per_set {
        let _ = writeln!(out, "| {set} | {n} | {pairs} | {:.0} |", secs / 60.0);
    }
    let _ = writeln!(out, "\n## Files, in the order chosen\n\n| # | Source | SHA-256 | Bytes | Formula cells | New pairs | New features | Est. s | Cum. min |\n|---:|---|---|---:|---:|---:|---:|---:|---:|");
    let mut cumulative = 0.0;
    for (i, c) in chosen.iter().enumerate() {
        cumulative += c.candidate.seconds;
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} | {:.1} | {:.1} |",
            i + 1,
            c.candidate.set,
            c.candidate.sha,
            c.candidate.bytes,
            c.candidate.formula_cells,
            c.new_pairs,
            c.new_features,
            c.candidate.seconds,
            cumulative / 60.0
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn record(sha: &str, shapes: &[&str], extra: Value) -> Value {
        let mut v = json!({
            "sha256": sha, "extension": "xlsx", "bytes": 1000,
            "reader": {"opened": true, "links": 0, "macro_sheets": 0, "has_vba_project": false, "date_system": "Date1900", "calc_mode": "Automatic"},
            "package": {"part_categories": {"xl/worksheets": 1}},
            "cells": {"formula_cells": 10, "formula_kinds": {"normal": 10}},
            "formula_features": {},
            "functions": {"SUM": 1},
            "function_shapes": shapes.iter().map(|s| (s.to_string(), json!(1))).collect::<serde_json::Map<_, _>>(),
        });
        if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            for (k, x) in e {
                o.insert(k.clone(), x.clone());
            }
        }
        v
    }

    #[test]
    fn blockers_macros_links_and_live_data_are_excluded_with_a_reason() {
        let ok = |v: &Value| judge("s", v, 1 << 20, 1000);
        assert!(ok(&record("a", &["SUM(R)"], json!({}))).is_ok());
        let mut refused = record("b", &["SUM(R)"], json!({}));
        refused["reader"]["opened"] = json!(false);
        assert!(ok(&refused).unwrap_err().contains("blocker"));
        let mut vba = record("c", &["SUM(R)"], json!({}));
        vba["reader"]["has_vba_project"] = json!(true);
        assert!(ok(&vba).unwrap_err().contains("macros"));
        let linked = record("d", &["SUM(R)"], json!({"formula_features": {"external_reference": 1}}));
        assert!(ok(&linked).unwrap_err().contains("external"));
        let live = record("e", &["WEBSERVICE(S)"], json!({"functions": {"WEBSERVICE": 1}}));
        assert!(ok(&live).unwrap_err().contains("live-data"));
        let big = record("f", &["SUM(R)"], json!({"bytes": 5_000_000}));
        assert!(ok(&big).unwrap_err().contains("size"));
        let none = record("g", &[], json!({"cells": {"formula_cells": 0}}));
        assert!(ok(&none).unwrap_err().contains("no formula"));
    }

    #[test]
    fn greedy_takes_new_pairs_per_second_and_skips_covered_ones() {
        let cand = |sha: &str, shapes: &[&str], bytes: u64| {
            let mut c = judge("s", &record(sha, shapes, json!({"bytes": bytes})), u64::MAX, u64::MAX).unwrap();
            c.features.clear();
            c
        };
        let all = vec![
            cand("small", &["A(r)", "B(r)"], 1000),
            cand("same", &["A(r)", "B(r)"], 1000),
            cand("big", &["C(r)", "D(r)", "E(r)"], 50_000_000),
            cand("covered", &["Z(r)"], 1000),
        ];
        let covered: BTreeSet<String> = ["Z(r)".to_owned()].into();
        let chosen = choose(all, &covered, 10, 3);
        let order: Vec<&str> = chosen.iter().map(|c| c.candidate.sha.as_str()).collect();
        // The small file first (2 new pairs for 3 s), then the repeat of its pairs (a quarter of
        // the gain each, still cheap), then the big one (3 new pairs for 78 s); nothing for
        // "covered".
        assert_eq!(order, ["small", "same", "big"]);
        assert_eq!(chosen[0].new_pairs, 2);
        assert_eq!(chosen[1].new_pairs, 0);
        assert_eq!(chosen[2].new_pairs, 3);
    }
}
