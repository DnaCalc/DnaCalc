//! The summary of a `cargo xtask corpus` run (bead `dc-vcs`): per workbook and in total, what
//! matched Excel and what did not, and the failures grouped by root cause across the corpus.
//!
//! The headline follows the owner's ruling of 2026-10-23: a refused formula is a failure in
//! every statistic. Over all formula cells of a workbook's capture inventory,
//!
//! - pass is a cell whose verdict is `matched` (calculated, and equal to Excel's value) and whose
//!   calculation read only the workbook's own inputs;
//! - external links is a matched cell whose calculation read values another workbook's link caches
//!   in the file hold, directly or through the formulas it read (`external_links` in
//!   `triage.json`, dc-ylr): calculated, but from the file's cache of the other book, so it is
//!   neither a pass nor a fail and is reported apart beside them;
//! - circular rule is a matched cell whose value is the one the circular-reference rule holds
//!   with iteration off (`circular_rule` in `triage.json`, dc-y2o.1): the formula is on a circular
//!   reference, or reads one, and keeps what its cell held, as Excel keeps it, without being
//!   evaluated, so it is neither a pass nor a fail and is reported apart beside them (a cell that
//!   is both is counted here);
//! - fail is every other cell: `mismatched` (a mismatch that read link caches, or of a value the
//!   circular rule held, included), refused
//!   (`not_modeled`, a typed not-modelled result), and failed or missing (`failed`, `missing`,
//!   `stale`, `file_cached`, `unproven`, `not_run`, each counted by name);
//! - awaiting capture is a formula cell of an oracle workbook (or of a section of one) that has no
//!   committed capture of its current content (`oracle_cases.rs`): it is not compared, so it is
//!   neither a pass nor a fail, and `cargo xtask capture --pending` takes its capture. Such cells
//!   are counted from the workbook's package;
//!
//! so formula cells = pass + external links + circular rule + awaiting capture + fail, and the
//! pass rate is pass over all formula cells, awaiting ones included. No rate over calculated
//! cells is given.
//!
//! A workbook that could not be compared at all (it could not be read or captured, or `cargo
//! xtask compare` refused it or ended abnormally) is listed with its stage and reason, counted
//! among the failed workbooks, and holds no cell counts, because its formula cells are not known.
//!
//! Root causes are read from each workbook's `triage.json` (`compare_triage.rs`), where every
//! failing cell is either a root or reads a root (`reads_root`), or fails with the whole run
//! (`workbook`). A failing cell is counted under the cause of its root:
//!
//! - a refusal (the root's verdict is `not_modeled`): by the refusal's owner and its reason, the
//!   triage's class and key;
//! - a mismatch (the root's verdict is `mismatched`, classed `kernel-value`, `kernel-bits` or
//!   `formula`): by the function at the root (the call the triage takes for the one whose result
//!   the formula returned, which is inferred) and the shape of the difference (the kinds of the
//!   two values and, for two numbers, how far apart they are);
//! - anything else: by the triage's class and key.
//!
//! Each cause counts its roots, the cells reading them (downstream) and the cells failing with
//! the whole run, and the workbooks it occurs in. The causes' cells add up to the fail count.
//!
//! The causes' keys hold function names, refusal kinds and value kinds, never a cell's text or a
//! sheet's name; a reason a workbook could not be compared is the tool's own message, which may
//! name a sheet or a cell.

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The most causes of each kind `summary.md` lists; `summary.json` lists them all.
const MARKDOWN_CAUSES: usize = 40;

/// What a cell's verdict counts as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Bucket {
    Pass,
    Mismatched,
    Refused,
    FailedOrMissing,
}

/// The bucket of a verdict name of calc-verdict's `Verdict::name`.
pub(crate) fn bucket(verdict: &str) -> Bucket {
    match verdict {
        "matched" => Bucket::Pass,
        "mismatched" => Bucket::Mismatched,
        "not_modeled" => Bucket::Refused,
        _ => Bucket::FailedOrMissing,
    }
}

/// The root cause a failing cell is counted under.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Cause {
    Refusal { owner: String, reason: String },
    Mismatch { function: String, shape: String },
    Other { class: String, key: String },
}

/// The cells of one cause: roots, cells reading a root, and cells failing with the whole run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CauseCount {
    pub(crate) roots: u64,
    pub(crate) downstream: u64,
    pub(crate) whole_workbook: u64,
}

impl CauseCount {
    pub(crate) fn cells(&self) -> u64 {
        self.roots + self.downstream + self.whole_workbook
    }

    fn add(&mut self, other: &CauseCount) {
        self.roots += other.roots;
        self.downstream += other.downstream;
        self.whole_workbook += other.whole_workbook;
    }
}

/// The cell counts of a compared workbook.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) formula_cells: u64,
    pub(crate) pass: u64,
    /// Matched cells whose calculation read another workbook's link caches: neither pass nor
    /// fail (dc-ylr).
    pub(crate) external_links: u64,
    /// Matched cells whose value the circular-reference rule held with iteration off: neither
    /// pass nor fail (dc-y2o.1).
    pub(crate) circular_rule: u64,
    /// Formula cells with no committed capture of their current content: neither pass nor fail.
    pub(crate) awaiting_capture: u64,
    pub(crate) mismatched: u64,
    pub(crate) refused: u64,
    pub(crate) failed_or_missing: u64,
    /// The failed-or-missing cells by verdict name.
    pub(crate) failed_or_missing_by_verdict: BTreeMap<String, u64>,
}

impl Counts {
    pub(crate) fn fail(&self) -> u64 {
        self.mismatched + self.refused + self.failed_or_missing
    }

    pub(crate) fn add(&mut self, other: &Counts) {
        self.formula_cells += other.formula_cells;
        self.pass += other.pass;
        self.external_links += other.external_links;
        self.circular_rule += other.circular_rule;
        self.awaiting_capture += other.awaiting_capture;
        self.mismatched += other.mismatched;
        self.refused += other.refused;
        self.failed_or_missing += other.failed_or_missing;
        for (verdict, count) in &other.failed_or_missing_by_verdict {
            *self
                .failed_or_missing_by_verdict
                .entry(verdict.clone())
                .or_default() += count;
        }
    }
}

/// Wall times in seconds; a stage that did not run has none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Times {
    pub(crate) open: Option<f64>,
    pub(crate) recalculation: Option<f64>,
    pub(crate) project: Option<f64>,
    pub(crate) judge: Option<f64>,
    /// The whole `cargo xtask compare` process, reading the capture record included.
    pub(crate) compare_process: Option<f64>,
    /// An Excel capture taken in this run.
    pub(crate) capture: Option<f64>,
}

/// What `triage.json` says of one workbook: its counts, causes, stage times and the failure of
/// the whole run, if any.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Compared {
    pub(crate) counts: Counts,
    pub(crate) causes: BTreeMap<Cause, CauseCount>,
    pub(crate) times: Times,
    /// The class and key of a failure of the whole run (calc-doc refused the package, the
    /// recalculation failed).
    pub(crate) workbook_failure: Option<String>,
}

impl Compared {
    /// What a workbook's parts compared one by one (its sections' capture sessions) say together:
    /// the counts and causes added, the stage times added, the first whole-run failure.
    pub(crate) fn add(&mut self, other: &Compared) {
        self.counts.add(&other.counts);
        for (cause, count) in &other.causes {
            self.causes.entry(cause.clone()).or_default().add(count);
        }
        add_time(&mut self.times.open, other.times.open);
        add_time(&mut self.times.recalculation, other.times.recalculation);
        add_time(&mut self.times.project, other.times.project);
        add_time(&mut self.times.judge, other.times.judge);
        add_time(&mut self.times.compare_process, other.times.compare_process);
        add_time(&mut self.times.capture, other.times.capture);
        if self.workbook_failure.is_none() {
            self.workbook_failure.clone_from(&other.workbook_failure);
        }
    }
}

/// Why a workbook was not compared: the stage that failed and the reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NotCompared {
    pub(crate) stage: String,
    pub(crate) reason: String,
}

/// One workbook of the corpus.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Workbook {
    pub(crate) name: String,
    pub(crate) sha256: Option<String>,
    /// Where the capture it was compared with came from.
    pub(crate) capture: Option<String>,
    pub(crate) outcome: Result<Compared, NotCompared>,
    /// The capture and compare-process times, which the outcome's own times do not hold when the
    /// workbook was not compared.
    pub(crate) times: Times,
}

/// The kind of a value as `triage.json` writes it (`compare_triage::value_json`).
fn value_kind(value: &Value) -> String {
    let Some(object) = value.as_object() else {
        return "no value".to_owned();
    };
    if object.contains_key("number") {
        return "number".to_owned();
    }
    if let Some(code) = object.get("error").and_then(Value::as_str) {
        return format!("error {code}");
    }
    object
        .keys()
        .next()
        .cloned()
        .unwrap_or_else(|| "no value".to_owned())
}

/// The function at a mismatched root, from its `difference` in `triage.json`: the named call, or
/// an operator's notation and spelling.
pub(crate) fn mismatch_function(difference: Option<&Value>) -> String {
    let Some(difference) = difference else {
        return "no evaluator evidence".to_owned();
    };
    let call = &difference["function"];
    if call.is_null() {
        return "no call".to_owned();
    }
    let spelling = call["spelling"].as_str().unwrap_or("?");
    match call["notation"].as_str() {
        Some("Call") | None => spelling.to_owned(),
        Some(notation) => format!("{notation} {spelling}"),
    }
}

/// The shape of a mismatched root's difference: two numbers by how far apart they are, other
/// values by their kinds.
pub(crate) fn mismatch_shape(difference: Option<&Value>) -> String {
    let Some(difference) = difference else {
        return "unknown".to_owned();
    };
    let expected = value_kind(&difference["expected"]);
    let actual = value_kind(&difference["actual"]);
    if expected == "number" && actual == "number" {
        let agrees = difference["agrees_to_15_significant_digits"].as_bool();
        return match (agrees, difference["ulp"].as_u64()) {
            (Some(true), Some(1)) => "number, 1 ulp apart".to_owned(),
            (Some(true), Some(ulp)) if ulp <= 16 => "number, 2 to 16 ulp apart".to_owned(),
            (Some(true), _) => "number, equal to 15 significant digits".to_owned(),
            _ => "number, differs within 15 significant digits".to_owned(),
        };
    }
    format!("{expected} expected, {actual} returned")
}

/// The classes whose mismatched root is counted by function and shape.
const MISMATCH_CLASSES: [&str; 3] = ["kernel-value", "kernel-bits", "formula"];

fn seconds(value: &Value) -> Option<f64> {
    value.as_f64()
}

/// The counts, causes and times of one workbook's `triage.json`, or why it cannot be read.
pub(crate) fn read_triage(triage: &Value) -> Result<Compared, String> {
    let cells = triage["cells"]
        .as_array()
        .ok_or("triage.json has no cells")?;
    let text = |cell: &Value, member: &str| -> Result<String, String> {
        cell[member]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("a cell of triage.json has no {member}"))
    };
    let place = |cell: &Value| -> Option<String> {
        Some(format!(
            "{}!{}",
            cell["sheet"].as_str()?,
            cell["cell"].as_str()?
        ))
    };
    let index: HashMap<String, usize> = cells
        .iter()
        .enumerate()
        .filter_map(|(i, cell)| Some((place(cell)?, i)))
        .collect();

    let mut compared = Compared::default();
    let counts = &mut compared.counts;
    for cell in cells {
        let verdict = text(cell, "verdict")?;
        counts.formula_cells += 1;
        match bucket(&verdict) {
            Bucket::Pass if cell["circular_rule"].as_bool() == Some(true) => {
                counts.circular_rule += 1;
                continue;
            }
            Bucket::Pass if cell["external_links"].as_bool() == Some(true) => {
                counts.external_links += 1;
                continue;
            }
            Bucket::Pass => {
                counts.pass += 1;
                continue;
            }
            Bucket::Mismatched => counts.mismatched += 1,
            Bucket::Refused => counts.refused += 1,
            Bucket::FailedOrMissing => {
                counts.failed_or_missing += 1;
                *counts
                    .failed_or_missing_by_verdict
                    .entry(verdict.clone())
                    .or_default() += 1;
            }
        }
        let class = text(cell, "class")?;
        let key = text(cell, "key")?;
        let role = cell["role"].as_str().unwrap_or("root");
        let root = match role {
            "reads_root" => {
                let root = cell["root"]
                    .as_str()
                    .ok_or("a cell reading a root names none")?;
                &cells[*index
                    .get(root)
                    .ok_or_else(|| format!("a cell reads the root {root}, which is not listed"))?]
            }
            _ => cell,
        };
        let root_verdict = root["verdict"].as_str().unwrap_or_default();
        let cause = if root_verdict == "not_modeled" && role != "workbook" {
            Cause::Refusal {
                owner: cell["owner"]
                    .as_str()
                    .or_else(|| root["owner"].as_str())
                    .unwrap_or("no owner recorded")
                    .to_owned(),
                reason: format!("{class} {key}"),
            }
        } else if root_verdict == "mismatched"
            && role != "workbook"
            && MISMATCH_CLASSES.contains(&root["class"].as_str().unwrap_or_default())
        {
            let difference = root.get("difference");
            Cause::Mismatch {
                function: mismatch_function(difference),
                shape: mismatch_shape(difference),
            }
        } else {
            Cause::Other { class, key }
        };
        let count = compared.causes.entry(cause).or_default();
        match role {
            "reads_root" => count.downstream += 1,
            "workbook" => count.whole_workbook += 1,
            _ => count.roots += 1,
        }
    }
    let run = &triage["run"];
    let wall = &run["wall_time_s"];
    compared.times = Times {
        open: seconds(&wall["open"]),
        recalculation: seconds(&wall["step"]),
        project: seconds(&wall["project"]),
        judge: seconds(&wall["judge"]),
        ..Times::default()
    };
    compared.workbook_failure = run["workbook_failure"].as_object().map(|failure| {
        format!(
            "{} {}",
            failure.get("class").and_then(Value::as_str).unwrap_or("?"),
            failure.get("key").and_then(Value::as_str).unwrap_or("?")
        )
    });
    Ok(compared)
}

/// A pass rate as a percentage rounded down to two decimals, so that 100.00% means every cell
/// passed, or `n/a` over no cells.
pub(crate) fn rate(pass: u64, cells: u64) -> String {
    if cells == 0 {
        return "n/a".to_owned();
    }
    let basis_points = u128::from(pass) * 10_000 / u128::from(cells);
    format!("{}.{:02}%", basis_points / 100, basis_points % 100)
}

fn rate_json(pass: u64, cells: u64) -> Value {
    if cells == 0 {
        Value::Null
    } else {
        json!(pass as f64 / cells as f64)
    }
}

/// One cause across the corpus: its counts and the workbooks it occurs in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CorpusCause {
    pub(crate) count: CauseCount,
    pub(crate) workbooks: BTreeSet<String>,
}

/// The totals of the corpus.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Totals {
    pub(crate) workbooks: u64,
    pub(crate) compared: u64,
    pub(crate) not_compared: u64,
    /// Compared workbooks whose whole run failed (every cell fails with it).
    pub(crate) whole_run_failed: u64,
    pub(crate) counts: Counts,
    pub(crate) times: Times,
}

fn add_time(total: &mut Option<f64>, time: Option<f64>) {
    if let Some(time) = time {
        *total = Some(total.unwrap_or(0.0) + time);
    }
}

/// The summary of a corpus run: its workbooks and what describes the run.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Summary {
    /// What describes the run (command, repository revision, Excel build, cache), as given.
    pub(crate) run: Value,
    pub(crate) workbooks: Vec<Workbook>,
}

impl Summary {
    pub(crate) fn totals(&self) -> Totals {
        let mut totals = Totals {
            workbooks: self.workbooks.len() as u64,
            ..Totals::default()
        };
        for workbook in &self.workbooks {
            let times = &mut totals.times;
            add_time(&mut times.capture, workbook.times.capture);
            add_time(&mut times.compare_process, workbook.times.compare_process);
            match &workbook.outcome {
                Ok(compared) => {
                    totals.compared += 1;
                    if compared.workbook_failure.is_some() {
                        totals.whole_run_failed += 1;
                    }
                    totals.counts.add(&compared.counts);
                    add_time(&mut times.open, compared.times.open);
                    add_time(&mut times.recalculation, compared.times.recalculation);
                    add_time(&mut times.project, compared.times.project);
                    add_time(&mut times.judge, compared.times.judge);
                }
                Err(_) => totals.not_compared += 1,
            }
        }
        totals
    }

    /// Every cause across the corpus, most cells first.
    pub(crate) fn causes(&self) -> Vec<(Cause, CorpusCause)> {
        let mut causes: BTreeMap<Cause, CorpusCause> = BTreeMap::new();
        for workbook in &self.workbooks {
            let Ok(compared) = &workbook.outcome else {
                continue;
            };
            for (cause, count) in &compared.causes {
                let entry = causes.entry(cause.clone()).or_default();
                entry.count.add(count);
                entry.workbooks.insert(workbook.name.clone());
            }
        }
        let mut causes: Vec<(Cause, CorpusCause)> = causes.into_iter().collect();
        causes.sort_by(|a, b| {
            b.1.count
                .cells()
                .cmp(&a.1.count.cells())
                .then_with(|| a.0.cmp(&b.0))
        });
        causes
    }

    /// `summary.json`.
    pub(crate) fn json(&self) -> Value {
        let totals = self.totals();
        let counts_json = |counts: &Counts| {
            json!({
                "formula_cells": counts.formula_cells,
                "pass": counts.pass,
                "external_links": counts.external_links,
                "circular_rule": counts.circular_rule,
                "awaiting_capture": counts.awaiting_capture,
                "fail": counts.fail(),
                "mismatched": counts.mismatched,
                "refused": counts.refused,
                "failed_or_missing": counts.failed_or_missing,
                "failed_or_missing_by_verdict": counts.failed_or_missing_by_verdict,
                "pass_rate": rate_json(counts.pass, counts.formula_cells),
            })
        };
        let times_json = |times: &Times| {
            json!({
                "open": times.open,
                "recalculation": times.recalculation,
                "project": times.project,
                "judge": times.judge,
                "compare_process": times.compare_process,
                "capture": times.capture,
            })
        };
        let workbooks: Vec<Value> = self
            .workbooks
            .iter()
            .map(|workbook| {
                let mut entry = json!({
                    "name": workbook.name,
                    "sha256": workbook.sha256,
                    "capture": workbook.capture,
                });
                match &workbook.outcome {
                    Ok(compared) => {
                        let mut times = compared.times.clone();
                        times.capture = workbook.times.capture;
                        times.compare_process = workbook.times.compare_process;
                        entry["status"] = json!("compared");
                        entry["counts"] = counts_json(&compared.counts);
                        entry["whole_run_failure"] = json!(compared.workbook_failure);
                        entry["wall_time_s"] = times_json(&times);
                    }
                    Err(not) => {
                        entry["status"] = json!("not compared");
                        entry["not_compared"] = json!({ "stage": not.stage, "reason": not.reason });
                        entry["wall_time_s"] = times_json(&workbook.times);
                    }
                }
                entry
            })
            .collect();
        let cause_json = |count: &CorpusCause| {
            json!({
                "roots": count.count.roots,
                "downstream": count.count.downstream,
                "whole_workbook": count.count.whole_workbook,
                "cells": count.count.cells(),
                "workbooks": count.workbooks.len(),
            })
        };
        let (mut refusals, mut mismatches, mut other) = (Vec::new(), Vec::new(), Vec::new());
        for (cause, count) in self.causes() {
            let mut entry = cause_json(&count);
            match cause {
                Cause::Refusal { owner, reason } => {
                    entry["owner"] = json!(owner);
                    entry["reason"] = json!(reason);
                    refusals.push(entry);
                }
                Cause::Mismatch { function, shape } => {
                    entry["function"] = json!(function);
                    entry["shape"] = json!(shape);
                    mismatches.push(entry);
                }
                Cause::Other { class, key } => {
                    entry["class"] = json!(class);
                    entry["key"] = json!(key);
                    other.push(entry);
                }
            }
        }
        json!({
            "format": "dnacalc-corpus-summary",
            "version": 1,
            "rule": "pass = matched Excel from the workbook's own inputs; external_links = matched Excel from values another workbook's link caches hold (neither pass nor fail, dc-ylr); circular_rule = matched Excel with the value the circular-reference rule holds with iteration off (neither pass nor fail, dc-y2o.1); awaiting_capture = formula cells of an oracle workbook or section with no committed capture of its current content (neither pass nor fail; cargo xtask capture --pending takes them); fail = mismatched + refused (not modelled) + failed or missing; the pass rate is pass over all formula cells, awaiting ones included (owner ruling 2026-10-23)",
            "run": self.run,
            "totals": {
                "workbooks": totals.workbooks,
                "compared": totals.compared,
                "not_compared": totals.not_compared,
                "whole_run_failed": totals.whole_run_failed,
                "counts": counts_json(&totals.counts),
                "wall_time_s": times_json(&totals.times),
            },
            "workbooks": workbooks,
            "root_causes": {
                "refusals": refusals,
                "mismatches": mismatches,
                "other": other,
            },
        })
    }

    /// `summary.md`.
    pub(crate) fn markdown(&self) -> String {
        let totals = self.totals();
        let mut out = String::new();
        let line = |out: &mut String, text: &str| {
            out.push_str(text);
            out.push('\n');
        };
        let time = |time: Option<f64>| time.map_or_else(|| "-".to_owned(), |t| format!("{t:.3}"));
        line(&mut out, "# Corpus summary");
        line(&mut out, "");
        line(
            &mut out,
            "Pass = matched Excel from the workbook's own inputs. External links = matched Excel from values another workbook's link caches hold, counted apart (neither pass nor fail, dc-ylr). Circular rule = matched Excel with the value a formula on a circular reference, or one reading it, keeps with iteration off, counted apart (neither pass nor fail, dc-y2o.1). Awaiting capture = formula cells of an oracle workbook or section with no committed capture of its current content, counted apart (neither pass nor fail; `cargo xtask capture --pending` takes them). Fail = mismatched + refused (not modelled) + failed or missing. The pass rate is pass over all formula cells, awaiting ones included (owner ruling 2026-10-23).",
        );
        line(&mut out, "");
        if let Some(run) = self.run.as_object() {
            for (name, value) in run {
                let shown = match value {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
                line(&mut out, &format!("- {name}: {shown}"));
            }
            line(&mut out, "");
        }
        let c = &totals.counts;
        line(&mut out, "## Totals");
        line(&mut out, "");
        line(
            &mut out,
            &format!(
                "- workbooks: {} ({} compared, {} not compared; {} compared with the whole run failed)",
                totals.workbooks, totals.compared, totals.not_compared, totals.whole_run_failed
            ),
        );
        line(
            &mut out,
            &format!(
                "- formula cells (compared workbooks): {}; pass {}; external links {}; circular rule {}; awaiting capture {}; fail {} (mismatched {}, refused {}, failed or missing {})",
                c.formula_cells,
                c.pass,
                c.external_links,
                c.circular_rule,
                c.awaiting_capture,
                c.fail(),
                c.mismatched,
                c.refused,
                c.failed_or_missing
            ),
        );
        line(
            &mut out,
            &format!(
                "- pass rate over all formula cells: {}",
                rate(c.pass, c.formula_cells)
            ),
        );
        if totals.not_compared > 0 {
            line(
                &mut out,
                &format!(
                    "- {} workbook(s) not compared hold formula cells not counted above, since their inventories are not known",
                    totals.not_compared
                ),
            );
        }
        if !c.failed_or_missing_by_verdict.is_empty() {
            let parts: Vec<String> = c
                .failed_or_missing_by_verdict
                .iter()
                .map(|(verdict, count)| format!("{verdict} {count}"))
                .collect();
            line(
                &mut out,
                &format!("- failed or missing by verdict: {}", parts.join(", ")),
            );
        }
        let t = &totals.times;
        line(
            &mut out,
            &format!(
                "- wall time (s): open {}, recalculation {}, project {}, judge {}, compare processes {}, captures {}",
                time(t.open),
                time(t.recalculation),
                time(t.project),
                time(t.judge),
                time(t.compare_process),
                time(t.capture)
            ),
        );
        line(&mut out, "");
        line(&mut out, "## Workbooks");
        line(&mut out, "");
        line(
            &mut out,
            "| Workbook | Formula cells | Pass | External links | Circular rule | Awaiting capture | Fail | Mismatched | Refused | Failed or missing | Pass rate | Open s | Recalc s | Judge s |",
        );
        line(
            &mut out,
            "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
        );
        let mut not_compared = Vec::new();
        for workbook in &self.workbooks {
            match &workbook.outcome {
                Ok(compared) => {
                    let c = &compared.counts;
                    let failure = compared
                        .workbook_failure
                        .as_ref()
                        .map(|failure| format!(" (whole run failed: {failure})"))
                        .unwrap_or_default();
                    line(
                        &mut out,
                        &format!(
                            "| {}{failure} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                            workbook.name,
                            c.formula_cells,
                            c.pass,
                            c.external_links,
                            c.circular_rule,
                            c.awaiting_capture,
                            c.fail(),
                            c.mismatched,
                            c.refused,
                            c.failed_or_missing,
                            rate(c.pass, c.formula_cells),
                            time(compared.times.open),
                            time(compared.times.recalculation),
                            time(compared.times.judge)
                        ),
                    );
                }
                Err(not) => {
                    line(
                        &mut out,
                        &format!(
                            "| {} (not compared) | - | - | - | - | - | - | - | - | - | - | - | - | - |",
                            workbook.name
                        ),
                    );
                    not_compared.push((workbook.name.as_str(), not));
                }
            }
        }
        line(
            &mut out,
            &format!(
                "| **Total** | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                c.formula_cells,
                c.pass,
                c.external_links,
                c.circular_rule,
                c.awaiting_capture,
                c.fail(),
                c.mismatched,
                c.refused,
                c.failed_or_missing,
                rate(c.pass, c.formula_cells),
                time(t.open),
                time(t.recalculation),
                time(t.judge)
            ),
        );
        line(&mut out, "");
        if !not_compared.is_empty() {
            line(&mut out, "### Not compared");
            line(&mut out, "");
            for (name, not) in not_compared {
                line(
                    &mut out,
                    &format!("- {name}: {}: {}", not.stage, one_line(&not.reason)),
                );
            }
            line(&mut out, "");
        }
        let captures: Vec<String> = self
            .workbooks
            .iter()
            .filter_map(|workbook| {
                Some(format!(
                    "- {}: {}",
                    workbook.name,
                    workbook.capture.as_ref()?
                ))
            })
            .collect();
        if !captures.is_empty() {
            line(&mut out, "### Captures");
            line(&mut out, "");
            for capture in captures {
                line(&mut out, &capture);
            }
            line(&mut out, "");
        }

        let causes = self.causes();
        let table = |out: &mut String,
                     title: &str,
                     head: [&str; 2],
                     rows: Vec<(String, String, &CorpusCause)>| {
            line(out, &format!("### {title}"));
            line(out, "");
            if rows.is_empty() {
                line(out, "None.");
                line(out, "");
                return;
            }
            let roots: u64 = rows.iter().map(|row| row.2.count.roots).sum();
            let cells: u64 = rows.iter().map(|row| row.2.count.cells()).sum();
            line(
                out,
                &format!("{} causes, {roots} roots, {cells} cells.", rows.len()),
            );
            line(out, "");
            line(
                out,
                &format!(
                    "| {} | {} | Roots | Downstream | Whole workbook | Cells | Workbooks |",
                    head[0], head[1]
                ),
            );
            line(out, "|---|---|---:|---:|---:|---:|---:|");
            let shown = rows.len().min(MARKDOWN_CAUSES);
            for (a, b, cause) in rows.iter().take(shown) {
                line(
                    out,
                    &format!(
                        "| {} | {} | {} | {} | {} | {} | {} |",
                        cell_text(a),
                        cell_text(b),
                        cause.count.roots,
                        cause.count.downstream,
                        cause.count.whole_workbook,
                        cause.count.cells(),
                        cause.workbooks.len()
                    ),
                );
            }
            if rows.len() > shown {
                line(
                    out,
                    &format!("\nand {} more in summary.json", rows.len() - shown),
                );
            }
            line(out, "");
        };
        line(&mut out, "## Failures by root cause");
        line(&mut out, "");
        let mut refusals = Vec::new();
        let mut mismatches = Vec::new();
        let mut other = Vec::new();
        for (cause, count) in &causes {
            match cause {
                Cause::Refusal { owner, reason } => {
                    refusals.push((owner.clone(), reason.clone(), count))
                }
                Cause::Mismatch { function, shape } => {
                    mismatches.push((function.clone(), shape.clone(), count))
                }
                Cause::Other { class, key } => other.push((class.clone(), key.clone(), count)),
            }
        }
        table(
            &mut out,
            "Refusals (not modelled), by owner and reason",
            ["Owner", "Reason"],
            refusals,
        );
        table(
            &mut out,
            "Mismatches, by function and shape at the root",
            ["Function", "Shape"],
            mismatches,
        );
        table(
            &mut out,
            "Other failures, by class and key",
            ["Class", "Key"],
            other,
        );
        out
    }
}

/// `text` on one line, at most 400 characters.
fn one_line(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.chars().count() > 400 {
        let cut: String = joined.chars().take(400).collect();
        format!("{cut}...")
    } else {
        joined
    }
}

/// `text` as the content of a markdown table cell.
fn cell_text(text: &str) -> String {
    one_line(text).replace('|', "\\|")
}

#[cfg(test)]
#[path = "corpus_summary_tests.rs"]
mod tests;
