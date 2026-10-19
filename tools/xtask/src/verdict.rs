//! `cargo xtask verdict --capture <file> --results <file> --evaluations <file> --out <file>
//! [--evidence <record>] [--exact-target <excel build>,<channel>,<compatibility>,<numeric
//! environment>]`: the repository command over the one verdict path (bead `infra-m1.1`).
//!
//! It reads a canonical CaptureRecord, a result set and the run's evaluation record (a separate
//! document the publisher of results does not write; bead `dc-qguw` produces it from instrumented
//! evaluator calls), judges them with calc-verdict's `judge` (the library entry point every
//! consumer calls) under Fast, or under the Exact target `--exact-target` names, with the legacy
//! evidence labels of the record `--evidence` names (`migration/evidence/oxfunc-smart-fuzzer.json`),
//! writes the CellVerdict report to `--out` (creating its directory) and prints the report's counts
//! on one line, with a line saying which evidence label was consulted when `--evidence` is given.
//! Paths are relative to the current directory.
//!
//! Exit codes: 0 when the report was written, whatever the report says; 1 when an input cannot be
//! read or is refused as inconsistent, or the report cannot be written; 2 on a usage error; 3 when
//! the capture record's formula inventory is empty, so that there is nothing to judge (no report is
//! written: a report of calculated 0 of 0 would say nothing).

use calc_verdict::value::{NumericMode, NumericTargetId};
use calc_verdict::{
    Acquisition, CaptureRecord, EvaluationRecord, EvidenceLabels, JudgeError, ResultSet,
    VerdictPolicy, VerdictReport, judge,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const USAGE: &str = "usage: cargo xtask verdict --capture <file> --results <file> --evaluations <file> --out <file> [--evidence <record>] [--exact-target <excel build>,<channel>,<compatibility>,<numeric environment>]";

const EXIT_REFUSED: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_EMPTY_INVENTORY: u8 = 3;

#[derive(Debug, PartialEq, Eq)]
struct Request {
    capture: PathBuf,
    results: PathBuf,
    evaluations: PathBuf,
    out: PathBuf,
    evidence: Option<PathBuf>,
    target: Option<NumericTargetId>,
}

/// Why no report was written, and the exit code that says so.
#[derive(Debug, PartialEq, Eq)]
struct Failure {
    code: u8,
    message: String,
}

impl Failure {
    fn refused(message: String) -> Failure {
        Failure {
            code: EXIT_REFUSED,
            message,
        }
    }
}

/// The failure for a refusal of `judge`: an empty inventory has its own exit code.
fn judge_failure(error: JudgeError) -> Failure {
    let code = match &error {
        JudgeError::EmptyInventory { .. } => EXIT_EMPTY_INVENTORY,
        JudgeError::Inconsistent { .. } => EXIT_REFUSED,
    };
    Failure {
        code,
        message: format!("the inputs are refused: {error}"),
    }
}

/// The Exact target `<excel build>,<channel>,<compatibility>,<numeric environment>`.
fn parse_target(text: &str) -> Result<NumericTargetId, String> {
    let parts: Vec<&str> = text.split(',').collect();
    match parts[..] {
        [build, channel, compatibility, environment] => {
            NumericTargetId::new(build, channel, compatibility, environment)
                .ok_or_else(|| format!("--exact-target {text:?} has an empty part"))
        }
        _ => Err(format!(
            "--exact-target {text:?} is not <excel build>,<channel>,<compatibility>,<numeric environment>"
        )),
    }
}

fn parse_args(args: &[String]) -> Result<Request, String> {
    let mut capture = None;
    let mut results = None;
    let mut evaluations = None;
    let mut out = None;
    let mut evidence = None;
    let mut target = None;
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let value = rest.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let slot = match flag.as_str() {
            "--capture" => &mut capture,
            "--results" => &mut results,
            "--evaluations" => &mut evaluations,
            "--out" => &mut out,
            "--evidence" => &mut evidence,
            "--exact-target" => &mut target,
            other => return Err(format!("unknown argument {other:?}")),
        };
        if slot.replace(value.clone()).is_some() {
            return Err(format!("{flag} is given twice"));
        }
    }
    Ok(Request {
        capture: PathBuf::from(capture.ok_or("--capture is required")?),
        results: PathBuf::from(results.ok_or("--results is required")?),
        evaluations: PathBuf::from(evaluations.ok_or("--evaluations is required")?),
        out: PathBuf::from(out.ok_or("--out is required")?),
        evidence: evidence.map(PathBuf::from),
        target: target.as_deref().map(parse_target).transpose()?,
    })
}

fn read(path: &Path, what: &str) -> Result<Vec<u8>, Failure> {
    fs::read(path)
        .map_err(|e| Failure::refused(format!("cannot read the {what} {}: {e}", path.display())))
}

/// What the evidence labels said about the capture record's acquisition: the label consulted for
/// a record converted from legacy evidence, or that none was consulted.
fn evidence_note(capture: &CaptureRecord, labels: &EvidenceLabels) -> String {
    match &capture.acquisition {
        Acquisition::LegacyEvidence { entry } => match labels.certifies_exact_target(entry) {
            Some(Some(label)) => format!("the evidence label of entry {entry:?} is {label}"),
            Some(None) => format!(
                "the evidence label of entry {entry:?} is null: its build is unknown and it certifies no Exact target"
            ),
            None => format!("the evidence record has no entry {entry:?}"),
        },
        Acquisition::LiveCapture | Acquisition::FileCacheReplay => format!(
            "the evidence record ({} labelled entries) was not consulted: the capture record was not converted from legacy evidence",
            labels.len()
        ),
    }
}

/// The report of the records in these bytes, judged under Fast or `target`, with the evidence
/// labels of `evidence` when given, and the note on the evidence label consulted.
fn judge_bytes(
    capture: &[u8],
    results: &[u8],
    evaluations: &[u8],
    evidence: Option<&[u8]>,
    target: Option<NumericTargetId>,
) -> Result<(VerdictReport, Option<String>), Failure> {
    let capture = CaptureRecord::decode(capture)
        .map_err(|e| Failure::refused(format!("the capture record is refused: {e}")))?;
    let results = ResultSet::decode(results)
        .map_err(|e| Failure::refused(format!("the result set is refused: {e}")))?;
    let evaluations = EvaluationRecord::decode(evaluations)
        .map_err(|e| Failure::refused(format!("the evaluation record is refused: {e}")))?;
    let labels = evidence
        .map(EvidenceLabels::decode)
        .transpose()
        .map_err(|e| Failure::refused(format!("the evidence record is refused: {e}")))?;
    let policy = VerdictPolicy {
        numeric_mode: target.map_or(NumericMode::Fast, NumericMode::Exact),
        evidence: labels.as_ref(),
    };
    let report = judge(&capture, &results, &evaluations, &policy).map_err(judge_failure)?;
    let note = labels
        .as_ref()
        .map(|labels| evidence_note(&capture, labels));
    Ok((report, note))
}

/// Reads the request's files, judges them and writes the report; the report on success.
fn verdict(request: Request) -> Result<VerdictReport, Failure> {
    let capture = read(&request.capture, "capture record")?;
    let results = read(&request.results, "result set")?;
    let evaluations = read(&request.evaluations, "evaluation record")?;
    let evidence = request
        .evidence
        .as_deref()
        .map(|path| read(path, "evidence record"))
        .transpose()?;
    let (report, note) = judge_bytes(
        &capture,
        &results,
        &evaluations,
        evidence.as_deref(),
        request.target,
    )?;
    if let Some(dir) = request
        .out
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
    {
        fs::create_dir_all(dir)
            .map_err(|e| Failure::refused(format!("cannot create {}: {e}", dir.display())))?;
    }
    fs::write(&request.out, report.encode())
        .map_err(|e| Failure::refused(format!("cannot write {}: {e}", request.out.display())))?;
    println!("xtask verdict: {}", report.summary());
    if let Some(note) = note {
        println!("xtask verdict: {note}");
    }
    println!("xtask verdict: report written to {}", request.out.display());
    Ok(report)
}

/// The command's exit code for `args`, after printing what it did or why it did not.
fn run_code(args: &[String]) -> u8 {
    let request = match parse_args(args) {
        Ok(request) => request,
        Err(e) => {
            eprintln!("xtask verdict: {e}\n{USAGE}");
            return EXIT_USAGE;
        }
    };
    match verdict(request) {
        Ok(_) => 0,
        Err(failure) => {
            eprintln!("xtask verdict: {}", failure.message);
            failure.code
        }
    }
}

pub fn run(args: &[String]) -> ExitCode {
    ExitCode::from(run_code(args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    fn workspace_file(path: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path)
    }

    const FIXTURES: &str = "tools/calc-verdict/tests/fixtures";
    const EVIDENCE: &str = "migration/evidence/oxfunc-smart-fuzzer.json";

    fn fixture(path: &str) -> PathBuf {
        workspace_file(&format!("{FIXTURES}/{path}"))
    }

    /// A directory of its own under the system temporary directory for one test.
    fn scratch(test: &str) -> PathBuf {
        std::env::temp_dir().join(format!("xtask-verdict-{}-{test}", std::process::id()))
    }

    #[test]
    fn arguments_name_each_input_once() {
        let request = parse_args(&args(
            "--capture c.json --results r.json --evaluations e.json --out o.json --exact-target 20430,ch,co,x86-64",
        ))
        .unwrap();
        assert_eq!(request.capture, PathBuf::from("c.json"));
        assert_eq!(request.evaluations, PathBuf::from("e.json"));
        assert_eq!(request.evidence, None);
        assert_eq!(
            request.target,
            NumericTargetId::new("20430", "ch", "co", "x86-64")
        );
        for (bad, refusal) in [
            (
                "--capture c.json --results r.json --evaluations e.json",
                "--out is required",
            ),
            (
                "--capture c.json --results r.json --out o.json",
                "--evaluations is required",
            ),
            (
                "--capture c.json --capture d.json --results r.json --evaluations e --out o",
                "given twice",
            ),
            (
                "--capture c.json --results r.json --evaluations e --out o --format x",
                "unknown argument",
            ),
            (
                "--capture c.json --results r.json --evaluations e --out",
                "needs a value",
            ),
            (
                "--capture c --results r --evaluations e --out o --exact-target 20430,ch,co",
                "is not <excel build>",
            ),
            (
                "--capture c --results r --evaluations e --out o --exact-target 20430,,co,x",
                "has an empty part",
            ),
        ] {
            let error = parse_args(&args(bad)).unwrap_err();
            assert!(error.contains(refusal), "{bad}: {error}");
        }
        assert_eq!(run_code(&args("--capture c.json")), EXIT_USAGE);
    }

    #[test]
    fn the_command_writes_the_libraries_report_for_the_fixtures() {
        let dir = scratch("trap");
        let out = dir.join("nested").join("trap.json");
        let report = verdict(Request {
            capture: fixture("cached_value_trap/capture.json"),
            results: fixture("cached_value_trap/results_file_cached.json"),
            evaluations: fixture("cached_value_trap/evaluations.json"),
            out: out.clone(),
            evidence: None,
            target: None,
        })
        .unwrap();
        let written: Value = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(written["counts"]["denominator"], 7);
        assert_eq!(written["counts"]["calculated"], 0);
        assert_eq!(written["counts"]["file_cached"], 7);
        assert_eq!(written["counts"]["not_calculated_equal_to_expectation"], 5);
        assert_eq!(written["counts"]["evaluator_calls"], 7);
        assert_eq!(report.counts.file_cached, 7);
        // The same calculated claims count only with the evaluation record behind them.
        for (evaluations, calculated, unproven) in [
            ("cached_value_trap/evaluations.json", 7, 0),
            ("cached_value_trap/evaluations_empty.json", 0, 7),
        ] {
            let report = verdict(Request {
                capture: fixture("cached_value_trap/capture.json"),
                results: fixture("cached_value_trap/results_calculated.json"),
                evaluations: fixture(evaluations),
                out: out.clone(),
                evidence: None,
                target: None,
            })
            .unwrap();
            assert_eq!(
                (report.counts.calculated, report.counts.unproven),
                (calculated, unproven),
                "{evaluations}"
            );
        }
        let missing = verdict(Request {
            capture: fixture("absent.json"),
            results: fixture("absent.json"),
            evaluations: fixture("absent.json"),
            out,
            evidence: None,
            target: None,
        })
        .unwrap_err();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(missing.code, EXIT_REFUSED);
        assert!(
            missing.message.contains("cannot read the capture record"),
            "{}",
            missing.message
        );
    }

    /// An empty formula inventory exits 3 and writes no report.
    #[test]
    fn an_empty_inventory_exits_three_without_a_report() {
        let dir = scratch("empty");
        let out = dir.join("empty.json");
        let failure = verdict(Request {
            capture: fixture("denominator_integrity/capture_empty.json"),
            results: fixture("denominator_integrity/results_omitting.json"),
            evaluations: fixture("denominator_integrity/evaluations.json"),
            out: out.clone(),
            evidence: None,
            target: None,
        })
        .unwrap_err();
        assert_eq!(failure.code, EXIT_EMPTY_INVENTORY);
        assert_eq!(
            failure.message,
            "the inputs are refused: the capture record of case \"infra_m1_1__denominator_integrity\" lists no formula cell, so there is nothing to judge"
        );
        assert!(!out.exists());
        let code = run_code(&[
            "--capture".to_owned(),
            fixture("denominator_integrity/capture_empty.json")
                .display()
                .to_string(),
            "--results".to_owned(),
            fixture("denominator_integrity/results_omitting.json")
                .display()
                .to_string(),
            "--evaluations".to_owned(),
            fixture("denominator_integrity/evaluations.json")
                .display()
                .to_string(),
            "--out".to_owned(),
            out.display().to_string(),
        ]);
        assert_eq!(code, 3);
        assert!(!out.exists());
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(
            judge_failure(JudgeError::Inconsistent {
                message: "m".to_owned()
            })
            .code,
            EXIT_REFUSED
        );
    }

    /// The certainty of the one cell of a report, as `<certainty>[/<reason>]`.
    fn certainty(report: &VerdictReport) -> String {
        let encoded: Value = serde_json::from_str(&report.encode()).unwrap();
        assert_eq!(encoded["cells"][0]["verdict"], "matched");
        format!(
            "{}{}",
            encoded["cells"][0]["certainty"].as_str().unwrap(),
            encoded["cells"][0]["uncertified"]
                .as_str()
                .map(|reason| format!("/{reason}"))
                .unwrap_or_default()
        )
    }

    /// The legacy-evidence fixtures through the command, with the committed evidence record and
    /// the full build string 16.0.20430.20000 as the target's build: the entry labelled
    /// excel-build-20430 certifies it, and the entry labelled excel-build-20131 does not.
    #[test]
    fn the_command_certifies_legacy_evidence_only_where_its_label_names_the_target() {
        let dir = scratch("legacy");
        let target =
            parse_target("16.0.20430.20000,fixture-channel,fixture-compatibility,fixture-x86-64")
                .unwrap();
        let run = |capture: &str| {
            verdict(Request {
                capture: fixture(&format!("legacy_evidence/{capture}")),
                results: fixture("legacy_evidence/results.json"),
                evaluations: fixture("legacy_evidence/evaluations.json"),
                out: dir.join(capture),
                evidence: Some(workspace_file(EVIDENCE)),
                target: Some(target.clone()),
            })
            .unwrap()
        };
        assert_eq!(
            certainty(&run("capture_label_names_target.json")),
            "strict_bits"
        );
        assert_eq!(
            certainty(&run("capture_label_not_target.json")),
            "uncertified/evidence_label_not_target"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// A one-cell record converted from legacy evidence `entry`, observed raw as 2 by Excel build
    /// 20430, a result set that calculated 2 for it, and the evaluation that proves it.
    fn legacy(entry: &str) -> (String, String, String) {
        let capture = format!(
            r#"{{"format": "dnacalc-capture-record", "version": 1, "case": "c", "source": "s",
                "acquisition": {{"kind": "legacy_evidence", "entry": "{entry}"}},
                "environment": {{"excel_build": "20430", "channel": "ch", "compatibility": "co", "numeric_environment": "x86-64", "settings": {{}}}},
                "sheets": [{{"name": "S", "visibility": "visible"}}],
                "formulas": [{{"sheet": "S", "row": 1, "column": 1, "kind": "ordinary"}}],
                "observations": [{{"sheet": "S", "row": 1, "column": 1, "excel_raw": {{"number": "4000000000000000"}}}}]}}"#
        );
        let results = r#"{"format": "dnacalc-result-set", "version": 1, "case": "c", "source": "s",
            "run": {"revision": 1, "context": "k"},
            "results": [{"sheet": "S", "row": 1, "column": 1, "provenance": {"calculated": {"tick": 1, "revision": 1, "context": "k"}}, "value": {"number": "4000000000000000"}}]}"#;
        let evaluations = r#"{"format": "dnacalc-evaluation-record", "version": 1, "case": "c", "source": "s",
            "evaluations": [{"sheet": "S", "row": 1, "column": 1, "tick": 1, "revision": 1, "context": "k"}]}"#;
        (capture, results.to_owned(), evaluations.to_owned())
    }

    /// The committed legacy evidence record certifies an entry only for the build its label names:
    /// `cache/oracle-20430/` (labelled excel-build-20430) only for build 20430, and
    /// `cache/w111-regenerated-baseline.json` (labelled null) for none.
    #[test]
    fn the_committed_evidence_record_decides_legacy_certification() {
        let record = fs::read(workspace_file(EVIDENCE)).unwrap();
        let judged = |entry: &str, build: &str, evidence: Option<&[u8]>| {
            let (capture, results, evaluations) = legacy(entry);
            let target = NumericTargetId::new(build, "ch", "co", "x86-64");
            let (report, note) = judge_bytes(
                capture.as_bytes(),
                results.as_bytes(),
                evaluations.as_bytes(),
                evidence,
                target,
            )
            .unwrap();
            (certainty(&report), note)
        };
        let record = Some(record.as_slice());
        assert_eq!(
            judged("cache/oracle-20430/", "20430", record),
            (
                "strict_bits".to_owned(),
                Some(
                    "the evidence label of entry \"cache/oracle-20430/\" is excel-build-20430"
                        .to_owned()
                )
            )
        );
        assert_eq!(
            judged("cache/oracle-20430/", "19929", record).0,
            "uncertified/environment_not_target"
        );
        assert_eq!(
            judged("cache/w111-regenerated-baseline.json", "20430", record),
            (
                "uncertified/evidence_label_not_target".to_owned(),
                Some("the evidence label of entry \"cache/w111-regenerated-baseline.json\" is null: its build is unknown and it certifies no Exact target".to_owned())
            )
        );
        assert_eq!(
            judged("cache/no-such-entry/", "20430", record),
            (
                "uncertified/evidence_entry_unknown".to_owned(),
                Some("the evidence record has no entry \"cache/no-such-entry/\"".to_owned())
            )
        );
        assert_eq!(
            judged("cache/oracle-20430/", "20430", None),
            ("uncertified/evidence_record_absent".to_owned(), None)
        );
        // An evidence record given for a record not converted from legacy evidence is reported as
        // not consulted.
        let capture = fs::read(fixture("denominator_integrity/capture.json")).unwrap();
        let results = fs::read(fixture("denominator_integrity/results_omitting.json")).unwrap();
        let evaluations = fs::read(fixture("denominator_integrity/evaluations.json")).unwrap();
        let (_, note) = judge_bytes(&capture, &results, &evaluations, record, None).unwrap();
        let note = note.unwrap();
        assert!(
            note.starts_with("the evidence record (")
                && note.ends_with(
                    "labelled entries) was not consulted: the capture record was not converted from legacy evidence"
                ),
            "{note}"
        );
    }
}
