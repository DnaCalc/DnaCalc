//! Unit tests of CaptureRecord version 2: the codec, the judge's version 2 rules (stale against the
//! capture's revision and context, the shared ranges, the round-trip lists, the capture's
//! failures), the file-cache replay and the absent-capture marker.

use crate::evaluations::{Evaluation, EvaluationRecord, InstrumentRecord, InstrumentedEvaluation};
use crate::result::{CellResult, Provenance, ResultSet, RunIdentity};
use crate::value::{CalcValue, NumericMode, NumericTargetId, WorksheetErrorCode};
use crate::{
    Acquisition, CalculationContext, CaptureFailure, CaptureFailureCounts, CaptureRecord,
    CellAddress, CellObservation, Certainty, JudgeError, MissingCapture, Observation, SharedRange,
    StaleSource, UncertifiedReason, Verdict, VerdictPolicy, file_cache_replay, judge,
};

const CONTEXT: &str = "excel-calc-context/1;date1904=false;iteration=false;max_iterations=100;max_change=3f50624dd2f1a9fc;precision_as_displayed=false";

/// A version 2 live capture of revision 1: Sheet1!A1 (=1+1), the shared group 0 of Sheet1!C1:C3
/// and Hidden!A1 (=1/0). C3 has no saved cache, C2's saved cache (1) differs from its raw value
/// (0), and only A1 and C1 were read back after reopening.
const RECORD: &str = r##"{"format": "dnacalc-capture-record", "version": 2, "case": "c", "source": "sha256:in",
 "acquisition": {"kind": "live_capture"},
 "environment": {"excel_build": "16.0.19127.20154", "channel": "current", "compatibility": "calc-191029",
   "numeric_environment": "excel-x86-64/cpu-x86-64", "settings": {"date_system": "1900", "calculation": "automatic"}},
 "session": {"revision": 1, "operations": ["set Sheet1!B1 3"],
   "context": "excel-calc-context/1;date1904=false;iteration=false;max_iterations=100;max_change=3f50624dd2f1a9fc;precision_as_displayed=false",
   "tool": "calc-oracle-excel test", "packages": {"input": "sha256:in", "output": "sha256:out"},
   "failures": [{"stage": "isolation", "message": "m"},
                {"stage": "value", "sheet": "Sheet1", "row": 1, "column": 1, "message": "n"}]},
 "sheets": [{"name": "Sheet1", "visibility": "visible"}, {"name": "Hidden", "visibility": "hidden"}],
 "formulas": [{"sheet": "Sheet1", "row": 1, "column": 1, "kind": "ordinary", "formula": "=1+1"},
              {"sheet": "Sheet1", "row": 1, "column": 3, "kind": "shared_master", "group": 0, "formula": "=A1*2"},
              {"sheet": "Sheet1", "row": 2, "column": 3, "kind": "shared_child", "group": 0, "formula": "=A2*2"},
              {"sheet": "Sheet1", "row": 3, "column": 3, "kind": "shared_child", "group": 0},
              {"sheet": "Hidden", "row": 1, "column": 1, "kind": "ordinary", "formula": "=1/0"}],
 "shared_ranges": [{"sheet": "Sheet1", "group": 0, "ref": "C1:C3"}],
 "observations": [{"sheet": "Sheet1", "row": 1, "column": 1, "excel_raw": {"number": "4000000000000000"}},
                  {"sheet": "Sheet1", "row": 1, "column": 3, "excel_raw": {"number": "4010000000000000"}},
                  {"sheet": "Sheet1", "row": 2, "column": 3, "excel_raw": {"number": "0000000000000000"}},
                  {"sheet": "Sheet1", "row": 3, "column": 3, "excel_raw": {"number": "0000000000000000"}},
                  {"sheet": "Hidden", "row": 1, "column": 1, "excel_raw": {"error": "div0"}}],
 "saved": [{"sheet": "Sheet1", "row": 1, "column": 1, "excel_saved_cache": {"t": "n", "v": "2"}},
           {"sheet": "Sheet1", "row": 1, "column": 3, "excel_saved_cache": {"t": "n", "v": "4"}},
           {"sheet": "Sheet1", "row": 2, "column": 3, "excel_saved_cache": {"t": "n", "v": "1"}},
           {"sheet": "Hidden", "row": 1, "column": 1, "excel_saved_cache": {"t": "e", "v": "#DIV/0!"}}],
 "reopened": [{"sheet": "Sheet1", "row": 1, "column": 1, "excel_raw": {"number": "4000000000000000"}},
              {"sheet": "Sheet1", "row": 1, "column": 3, "excel_raw": {"number": "4010000000000000"}}]}"##;

fn record() -> CaptureRecord {
    CaptureRecord::decode(RECORD.as_bytes()).unwrap()
}

fn cell(sheet: &str, row: u32, column: u32) -> CellAddress {
    CellAddress::new(sheet, row, column).unwrap()
}

/// The inventory cells of [`RECORD`] in order, with the value each is calculated as.
fn calculated_values() -> Vec<(CellAddress, CalcValue)> {
    vec![
        (cell("Sheet1", 1, 1), CalcValue::number(2.0)),
        (cell("Sheet1", 1, 3), CalcValue::number(4.0)),
        (cell("Sheet1", 2, 3), CalcValue::number(0.0)),
        (cell("Sheet1", 3, 3), CalcValue::number(0.0)),
        (
            cell("Hidden", 1, 1),
            CalcValue::error(WorksheetErrorCode::Div0),
        ),
    ]
}

/// A run at `run` whose every inventory cell is calculated at tick `i + 1`, revision `revision`
/// and context `context`, with the evaluation record listing each of those evaluations.
fn calculated(run: (u64, &str), revision: u64, context: &str) -> (ResultSet, EvaluationRecord) {
    let cells = calculated_values();
    let results = ResultSet {
        case: "c".to_owned(),
        source: "sha256:in".to_owned(),
        run: RunIdentity {
            revision: run.0,
            context: run.1.to_owned(),
        },
        results: cells
            .iter()
            .enumerate()
            .map(|(i, (cell, value))| CellResult {
                cell: cell.clone(),
                provenance: Provenance::Calculated {
                    tick: i as u64 + 1,
                    revision,
                    context: context.to_owned(),
                },
                value: Some(value.clone()),
            })
            .collect(),
    };
    let evaluations = EvaluationRecord {
        case: "c".to_owned(),
        source: "sha256:in".to_owned(),
        evaluations: cells
            .iter()
            .enumerate()
            .map(|(i, (cell, _))| Evaluation {
                cell: cell.clone(),
                tick: i as u64 + 1,
                revision,
                context: context.to_owned(),
            })
            .collect(),
    };
    (results, evaluations)
}

fn verdict_names(
    capture: &CaptureRecord,
    results: &ResultSet,
    proof: &EvaluationRecord,
) -> Vec<String> {
    let report = judge(capture, results, proof, &VerdictPolicy::default()).unwrap();
    report
        .cells
        .iter()
        .map(|cell| match &cell.verdict {
            Verdict::Stale {
                source: StaleSource::RevisionNotRun { revision },
            } => format!("stale(revision_not_run {revision})"),
            Verdict::Stale {
                source: StaleSource::ContextNotRun { context },
            } => format!("stale(context_not_run {context})"),
            other => other.name().to_owned(),
        })
        .collect()
}

fn refused(capture: &CaptureRecord, needle: &str) {
    let (results, proof) = calculated((1, CONTEXT), 1, CONTEXT);
    let error = judge(capture, &results, &proof, &VerdictPolicy::default())
        .err()
        .unwrap_or_else(|| panic!("judge accepted a record that should be refused for {needle:?}"));
    assert!(
        matches!(error, JudgeError::Inconsistent { .. }),
        "{error:?}"
    );
    assert!(
        error.to_string().contains(needle),
        "{error} lacks {needle:?}"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn version_2_decodes_every_member_and_round_trips_exactly() {
    let record = record();
    let session = record.session.as_ref().unwrap();
    assert_eq!(session.revision, 1);
    assert_eq!(session.operations, ["set Sheet1!B1 3"]);
    assert_eq!(session.context, CONTEXT);
    assert_eq!(session.tool, "calc-oracle-excel test");
    assert_eq!(session.input_package, "sha256:in");
    assert_eq!(session.output_package.as_deref(), Some("sha256:out"));
    assert_eq!(session.failures.len(), 2);
    assert_eq!(session.failures[0].cell, None);
    assert_eq!(session.failures[1].cell, Some(cell("Sheet1", 1, 1)));
    assert_eq!(
        session.shared_ranges,
        [SharedRange::new("Sheet1", 0, "C1:C3").unwrap()]
    );
    assert_eq!(session.saved.len(), 4);
    assert!(
        session
            .saved
            .iter()
            .all(|saved| matches!(saved.observation, Observation::ExcelSavedCache(_)))
    );
    assert_eq!(session.reopened.len(), 2);
    assert_eq!(record.formulas[0].formula.as_deref(), Some("=1+1"));
    assert_eq!(record.formulas[3].formula, None);

    let written = record.encode().unwrap();
    let again = CaptureRecord::decode(written.as_bytes()).unwrap();
    assert_eq!(again, record);
    assert_eq!(again.encode().unwrap(), written);
    // Members in the documented order, version 2.
    let order = [
        "\"format\"",
        "\"version\": 2",
        "\"case\"",
        "\"source\"",
        "\"acquisition\"",
        "\"environment\"",
        "\"session\"",
        "\"sheets\"",
        "\"formulas\"",
        "\"shared_ranges\"",
        "\"observations\"",
        "\"saved\"",
        "\"reopened\"",
    ];
    let positions: Vec<usize> = order
        .iter()
        .map(|member| {
            written
                .find(member)
                .unwrap_or_else(|| panic!("{member} in {written}"))
        })
        .collect();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "{written}"
    );
    assert!(
        written.contains(
            "{\"sheet\":\"Sheet1\",\"row\":1,\"column\":3,\"kind\":\"shared_master\",\"group\":0,\"formula\":\"=A1*2\"}"
        ),
        "{written}"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn version_1_is_read_and_never_written() {
    let v1 = r#"{"format": "dnacalc-capture-record", "version": 1, "case": "c", "source": "s",
        "acquisition": {"kind": "live_capture"}, "environment": null, "sheets": [], "formulas": [],
        "observations": []}"#;
    let record = CaptureRecord::decode(v1.as_bytes()).unwrap();
    assert_eq!(record.session, None);
    let error = record.encode().err().unwrap();
    assert!(
        error
            .message
            .contains("version 1 record is read, never written"),
        "{error}"
    );

    let mut unrepresentable = self::record();
    unrepresentable.observations[0].observation = Observation::ExcelRaw(CalcValue::missing());
    let error = unrepresentable.encode().err().unwrap();
    assert!(
        error
            .message
            .contains("Sheet1!A1 holds a value no cell holds"),
        "{error}"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn malformed_version_2_records_are_refused_with_their_path() {
    for (from, to, path) in [
        ("\"version\": 2", "\"version\": 3", "capture.version"),
        (
            "\"format\": \"dnacalc-capture-record\"",
            "\"format\": \"dnacalc-capture-absent\"",
            "capture.format",
        ),
        // Version 1 has neither a session nor formula text.
        ("\"version\": 2", "\"version\": 1", "capture"),
        (
            r#" "session": {"revision": 1,"#,
            r#" "sessions": {"revision": 1,"#,
            "capture",
        ),
        (
            r#""operations": ["set Sheet1!B1 3"]"#,
            r#""operations": [""]"#,
            "capture.session.operations[0]",
        ),
        (
            r#""packages": {"input": "sha256:in", "output": "sha256:out"}"#,
            r#""packages": {"input": "sha256:in"}"#,
            "capture.session.packages",
        ),
        (
            r#"{"stage": "value", "sheet": "Sheet1", "row": 1, "column": 1, "message": "n"}"#,
            r#"{"stage": "value", "sheet": "Sheet1", "message": "n"}"#,
            "capture.session.failures[1]",
        ),
        (
            r#""ref": "C1:C3""#,
            r#""ref": "C3:C1""#,
            "capture.shared_ranges[0].ref",
        ),
        (
            r#""ref": "C1:C3""#,
            r#""ref": "$C$1:C3""#,
            "capture.shared_ranges[0].ref",
        ),
        (
            r#""ref": "C1:C3""#,
            r#""ref": "C1:C1048577""#,
            "capture.shared_ranges[0].ref",
        ),
        (
            r#""saved": [{"sheet": "Sheet1", "row": 1, "column": 1, "excel_saved_cache": {"t": "n", "v": "2"}}"#,
            r#""saved": [{"sheet": "Sheet1", "row": 1, "column": 1, "excel_raw": {"number": "4000000000000000"}}"#,
            "capture.saved[0]",
        ),
        (
            r#""reopened": [{"sheet": "Sheet1", "row": 1, "column": 1, "excel_raw": {"number": "4000000000000000"}}"#,
            r#""reopened": [{"sheet": "Sheet1", "row": 1, "column": 1, "excel_saved_cache": {"t": "n", "v": "2"}}"#,
            "capture.reopened[0]",
        ),
        (
            r#""formula": "=1+1""#,
            r#""formula": """#,
            "capture.formulas[0].formula",
        ),
    ] {
        let text = RECORD.replacen(from, to, 1);
        assert_ne!(text, RECORD, "{from} not found");
        let error = CaptureRecord::decode(text.as_bytes()).err().unwrap();
        assert_eq!(error.path, path, "{from} -> {to}: {error}");
    }
}

/// A dynamic-array formula over its own cell is an ordinary inventory cell flagged as dynamic: the
/// flag round-trips, is written only when true, and is refused when given as false or on a shared
/// formula. The file-cache replay counts the cell like any other.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_dynamic_array_formula_is_an_ordinary_cell_flagged_as_dynamic() {
    let plain = r#""kind": "ordinary", "formula": "=1+1"}"#;
    let text = RECORD.replacen(
        plain,
        r#""kind": "ordinary", "formula": "=1+1", "dynamic_array": true}"#,
        1,
    );
    assert_ne!(text, RECORD);
    let flagged = CaptureRecord::decode(text.as_bytes()).unwrap();
    assert!(flagged.formulas[0].dynamic_array);
    assert_eq!(flagged.formulas[0].kind, crate::FormulaKind::Ordinary);
    assert!(flagged.formulas[1..].iter().all(|f| !f.dynamic_array));
    let encoded = flagged.encode().unwrap();
    assert!(
        encoded.contains(r#""formula":"=1+1","dynamic_array":true}"#),
        "{encoded}"
    );
    assert_eq!(encoded.matches("dynamic_array").count(), 1);
    assert_eq!(CaptureRecord::decode(encoded.as_bytes()).unwrap(), flagged);
    assert!(!record().encode().unwrap().contains("dynamic_array"));
    let policy = VerdictPolicy::default();
    assert_eq!(
        file_cache_replay(&flagged, &policy).unwrap().encode(),
        file_cache_replay(&record(), &policy).unwrap().encode()
    );
    for (to, path, message) in [
        (
            r#""kind": "ordinary", "formula": "=1+1", "dynamic_array": false}"#,
            "capture.formulas[0].dynamic_array",
            "is false; the member is written only when true",
        ),
        (
            r#""kind": "ordinary", "formula": "=1+1", "dynamic_array": 1}"#,
            "capture.formulas[0].dynamic_array",
            "",
        ),
    ] {
        let error = CaptureRecord::decode(RECORD.replacen(plain, to, 1).as_bytes())
            .err()
            .unwrap();
        assert_eq!(error.path, path, "{to}: {error}");
        assert!(error.message.contains(message), "{to}: {error}");
    }
    let shared = RECORD.replacen(
        r#""group": 0, "formula": "=A1*2"}"#,
        r#""group": 0, "formula": "=A1*2", "dynamic_array": true}"#,
        1,
    );
    let error = CaptureRecord::decode(shared.as_bytes()).err().unwrap();
    assert_eq!(
        (error.path.as_str(), error.message.as_str()),
        (
            "capture.formulas[1].dynamic_array",
            "is given for a shared_master formula"
        )
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_run_at_another_revision_or_context_than_the_capture_is_stale() {
    let capture = record();
    // The run, its cells and the capture all at revision 1 under the captured context.
    let (results, proof) = calculated((1, CONTEXT), 1, CONTEXT);
    assert_eq!(
        verdict_names(&capture, &results, &proof),
        ["matched", "matched", "matched", "matched", "matched"]
    );
    let report = judge(&capture, &results, &proof, &VerdictPolicy::default()).unwrap();
    let encoded: serde_json::Value = serde_json::from_str(&report.encode()).unwrap();
    assert_eq!(
        encoded["captured"],
        serde_json::json!({"revision": 1, "context": CONTEXT})
    );
    assert_eq!(encoded["run"]["revision"], 1);

    // A result set at revision 0, consistent in itself and with its evaluation record, judged
    // against the capture of revision 1: every calculated cell is stale.
    let (results, proof) = calculated((0, CONTEXT), 0, CONTEXT);
    assert_eq!(
        verdict_names(&capture, &results, &proof),
        vec!["stale(revision_not_run 0)"; 5]
    );
    // The run declares the capture's revision, but its cells were calculated at revision 0.
    let (mut results, proof) = calculated((0, CONTEXT), 0, CONTEXT);
    results.run.revision = 1;
    assert_eq!(
        verdict_names(&capture, &results, &proof),
        vec!["stale(revision_not_run 0)"; 5]
    );
    // Another context than the captured one, run and cells agreeing with each other.
    let (results, proof) = calculated((1, "other"), 1, "other");
    assert_eq!(
        verdict_names(&capture, &results, &proof),
        vec!["stale(context_not_run other)"; 5]
    );
    let report = judge(&capture, &results, &proof, &VerdictPolicy::default()).unwrap();
    assert_eq!(report.counts.calculated, 0);
    assert_eq!(report.counts.stale, 5);

    // A version 1 record carries no revision of its own: the run's declaration is all there is.
    let mut v1 = capture.clone();
    v1.session = None;
    let (results, proof) = calculated((0, CONTEXT), 0, CONTEXT);
    assert_eq!(
        verdict_names(&v1, &results, &proof),
        ["matched", "matched", "matched", "matched", "matched"]
    );
    let report = judge(&v1, &results, &proof, &VerdictPolicy::default()).unwrap();
    let encoded: serde_json::Value = serde_json::from_str(&report.encode()).unwrap();
    assert!(encoded.get("captured").is_none());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn shared_ranges_must_hold_the_cells_of_their_group() {
    // The inventory lists no formula at C3 (nor its observation), as for a package whose C3 holds
    // a constant: the range C1:C3 covers a cell that is not of its group, as Excel saves refs, and
    // the record is judged with the four formula cells it lists in the denominator.
    let mut dropped = record();
    dropped.formulas.remove(3);
    dropped.observations.remove(3);
    let (mut results, mut proof) = calculated((1, CONTEXT), 1, CONTEXT);
    results.results.remove(3);
    proof.evaluations.remove(3);
    let report = judge(&dropped, &results, &proof, &VerdictPolicy::default()).unwrap();
    assert_eq!(
        (
            report.counts.denominator,
            report.counts.calculated,
            report.counts.matched
        ),
        (4, 4, 4)
    );
    assert_eq!(
        report
            .cells
            .iter()
            .map(|verdict| format!(
                "{}!{}",
                verdict.formula.cell.sheet(),
                verdict.formula.cell.a1()
            ))
            .collect::<Vec<_>>(),
        ["Sheet1!A1", "Sheet1!C1", "Sheet1!C2", "Hidden!A1"]
    );
    // Had C3 held a formula the package did not list, the worker's cross-check against Excel's
    // SpecialCells would have recorded an inventory failure there, and the record is refused.
    dropped
        .session
        .as_mut()
        .unwrap()
        .failures
        .push(CaptureFailure {
            stage: "inventory".to_owned(),
            cell: Some(cell("Sheet1", 3, 3)),
            message: "Excel reports a formula here that the package does not hold".to_owned(),
        });
    refused(
        &dropped,
        "records an inventory failure at Sheet1!C3 (Excel reports a formula here that the package does not hold), so its formula inventory, the denominator, is not known to be complete",
    );
    // The range declared smaller than the group.
    let mut small = record();
    small.session.as_mut().unwrap().shared_ranges =
        vec![SharedRange::new("Sheet1", 0, "C1:C2").unwrap()];
    refused(
        &small,
        "Sheet1!C3 belongs to shared group 0 and lies outside the group's range Sheet1!C1:C2",
    );
    let mut none = record();
    none.session.as_mut().unwrap().shared_ranges.clear();
    refused(
        &none,
        "shared formula group 0 on sheet \"Sheet1\" has no shared range",
    );
    let mut twice = record();
    let ranges = &mut twice.session.as_mut().unwrap().shared_ranges;
    ranges.push(ranges[0].clone());
    refused(
        &twice,
        "lists two shared ranges for group 0 on sheet \"Sheet1\"",
    );
    let mut orphan = record();
    orphan
        .session
        .as_mut()
        .unwrap()
        .shared_ranges
        .push(SharedRange::new("Sheet1", 7, "E1:E2").unwrap());
    refused(
        &orphan,
        "names group 7, to which no formula cell of the inventory belongs",
    );
    let mut elsewhere = record();
    elsewhere.session.as_mut().unwrap().shared_ranges =
        vec![SharedRange::new("Other", 0, "C1:C3").unwrap()];
    refused(
        &elsewhere,
        "is on sheet \"Other\", which the capture record does not list",
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn saved_and_reopened_entries_are_inventory_cells_listed_once() {
    let outside = |list: &str| {
        let mut capture = record();
        let session = capture.session.as_mut().unwrap();
        let entries = if list == "saved" {
            &mut session.saved
        } else {
            &mut session.reopened
        };
        entries[0].cell = cell("Sheet1", 9, 9);
        capture
    };
    refused(
        &outside("saved"),
        "saved list holds Sheet1!I9, which is not in its formula inventory",
    );
    refused(
        &outside("reopened"),
        "reopened list holds Sheet1!I9, which is not in its formula inventory",
    );
    let mut twice = record();
    let saved = &mut twice.session.as_mut().unwrap().saved;
    saved.push(saved[0].clone());
    refused(&twice, "saved list holds Sheet1!A1 twice");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn the_file_cache_replay_counts_no_cell_as_calculated() {
    let capture = record();
    let report = file_cache_replay(&capture, &VerdictPolicy::default()).unwrap();
    let names: Vec<&str> = report
        .cells
        .iter()
        .map(|cell| cell.verdict.name())
        .collect();
    assert_eq!(
        names,
        [
            "file_cached",
            "file_cached",
            "file_cached",
            "missing",
            "file_cached"
        ]
    );
    assert_eq!(
        report
            .cells
            .iter()
            .map(|cell| cell.value_equals_expectation)
            .collect::<Vec<_>>(),
        [Some(true), Some(true), Some(false), None, Some(true)]
    );
    let c = &report.counts;
    assert_eq!(
        (
            c.denominator,
            c.calculated,
            c.matched,
            c.file_cached,
            c.missing,
            c.not_calculated_equal_to_expectation,
            c.evaluator_calls
        ),
        (5, 0, 0, 4, 1, 3, 0)
    );
    assert_eq!(
        report.cells[0].verdict,
        Verdict::FileCached {
            source: "sha256:out".to_owned()
        }
    );
    assert_eq!(report.run, report.captured.clone().unwrap());

    let mut not_live = capture.clone();
    not_live.acquisition = Acquisition::FileCacheReplay;
    not_live.observations.clear();
    let error = file_cache_replay(&not_live, &VerdictPolicy::default())
        .err()
        .unwrap();
    assert!(
        error.to_string().contains("is not a live capture"),
        "{error}"
    );
    let mut v1 = capture.clone();
    v1.session = None;
    let error = file_cache_replay(&v1, &VerdictPolicy::default())
        .err()
        .unwrap();
    assert!(error.to_string().contains("is version 1"), "{error}");
    let mut unsaved = capture.clone();
    unsaved.session.as_mut().unwrap().output_package = None;
    let error = file_cache_replay(&unsaved, &VerdictPolicy::default())
        .err()
        .unwrap();
    assert!(
        error.to_string().contains("names no saved package"),
        "{error}"
    );
    let mut raw_saved = capture;
    raw_saved.session.as_mut().unwrap().saved[0] = CellObservation {
        cell: cell("Sheet1", 1, 1),
        observation: Observation::ExcelRaw(CalcValue::number(2.0)),
    };
    let error = file_cache_replay(&raw_saved, &VerdictPolicy::default())
        .err()
        .unwrap();
    assert!(
        error.to_string().contains("is not a saved cache"),
        "{error}"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn the_absent_capture_marker_is_recognized_and_nothing_else_is() {
    let marker = br#"{"format": "dnacalc-capture-absent", "version": 1, "case": "k",
        "artifact": "capture record", "fixture": "f.xlsx", "command": "cargo xtask capture --fixture f.xlsx --out c.json"}"#;
    let missing = MissingCapture::recognize(marker).unwrap();
    assert_eq!(missing.case, "k");
    assert_eq!(missing.artifact, "capture record");
    assert_eq!(
        missing.to_string(),
        "MissingCapture: no capture of f.xlsx has been taken for case k; this path will hold its capture record once an Excel run of `cargo xtask capture --fixture f.xlsx --out c.json` writes it"
    );
    assert_eq!(MissingCapture::recognize(RECORD.as_bytes()), None);
    let text = std::str::from_utf8(marker).unwrap();
    for (from, to) in [
        ("\"version\": 1", "\"version\": 2"),
        (r#""case": "k","#, ""),
        (r#""case": "k","#, r#""case": "k", "extra": 1,"#),
    ] {
        let malformed = text.replacen(from, to, 1);
        assert_ne!(malformed, text);
        assert_eq!(
            MissingCapture::recognize(malformed.as_bytes()),
            None,
            "{malformed}"
        );
        assert!(CaptureRecord::decode(malformed.as_bytes()).is_err());
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn the_calculation_context_fingerprint_names_every_setting() {
    let context = CalculationContext {
        date1904: false,
        iteration: false,
        max_iterations: 100,
        max_change: 0.001,
        precision_as_displayed: false,
    };
    assert_eq!(context.fingerprint(), CONTEXT);
    assert_eq!(
        CalculationContext::without_iteration(false, false).fingerprint(),
        CONTEXT
    );
    for changed in [
        CalculationContext {
            date1904: true,
            ..context
        },
        CalculationContext {
            iteration: true,
            ..context
        },
        CalculationContext {
            precision_as_displayed: true,
            ..context
        },
    ] {
        assert_ne!(changed.fingerprint(), CONTEXT, "{changed:?}");
    }
    // While iteration is off the limits change no value, so they do not change the fingerprint;
    // while it is on, each limit is part of it.
    for limits in [
        (101, 0.001),
        (100, f64::from_bits(0.001f64.to_bits() + 1)),
        (1, 0.5),
    ] {
        let off = CalculationContext {
            max_iterations: limits.0,
            max_change: limits.1,
            ..context
        };
        assert_eq!(off.fingerprint(), CONTEXT, "{off:?}");
        let on = CalculationContext {
            iteration: true,
            ..off
        };
        let on_defaults = CalculationContext {
            iteration: true,
            ..context
        };
        assert_ne!(on.fingerprint(), on_defaults.fingerprint(), "{on:?}");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn shared_ranges_parse_relative_a1_refs() {
    let range = SharedRange::new("S", 3, "B2:D4").unwrap();
    assert_eq!(
        (range.sheet(), range.group(), range.reference()),
        ("S", 3, "B2:D4")
    );
    assert!(range.contains(&cell("S", 2, 2)));
    assert!(range.contains(&cell("S", 4, 4)));
    assert!(!range.contains(&cell("S", 1, 2)));
    assert!(!range.contains(&cell("S", 2, 5)));
    assert!(!range.contains(&cell("S", 3, 1)));
    assert!(range.contains(&cell("S", 3, 3)));
    assert!(!range.contains(&cell("T", 3, 3)));
    assert!(!range.contains(&cell("S", 5, 3)));
    let single = SharedRange::new("S", 0, "XFD1048576").unwrap();
    assert!(single.contains(&cell("S", 1_048_576, 16_384)));
    assert!(!single.contains(&cell("S", 1_048_575, 16_384)));
    assert!(!single.contains(&cell("S", 1_048_576, 16_383)));
    for bad in [
        "", "B2:", ":B2", "b2", "B02", "B0", "XFE1", "AAAA1", "B2:A1", "B2:B1", "B2 :C3",
    ] {
        assert_eq!(SharedRange::new("S", 0, bad), None, "{bad:?}");
    }
    assert_eq!(SharedRange::new("", 0, "A1"), None);
}

/// The Exact target [`RECORD`]'s Excel environment is.
fn record_target() -> VerdictPolicy<'static> {
    VerdictPolicy {
        numeric_mode: NumericMode::Exact(
            NumericTargetId::new(
                "16.0.19127.20154",
                "current",
                "calc-191029",
                "excel-x86-64/cpu-x86-64",
            )
            .unwrap(),
        ),
        evidence: None,
    }
}

/// [`RECORD`] with its session's failures replaced by `failures`, each as (stage, cell).
fn with_failures(failures: &[(&str, Option<CellAddress>)]) -> CaptureRecord {
    let mut capture = record();
    capture.session.as_mut().unwrap().failures = failures
        .iter()
        .map(|(stage, cell)| CaptureFailure {
            stage: (*stage).to_owned(),
            cell: cell.clone(),
            message: format!("{stage} went wrong"),
        })
        .collect();
    capture
}

/// The certainty of each compared cell of `capture`, every inventory cell calculated as the
/// capture observed it, under the Exact target of the record's environment.
fn certainties(capture: &CaptureRecord) -> Vec<Certainty> {
    let (results, proof) = calculated((1, CONTEXT), 1, CONTEXT);
    let report = judge(capture, &results, &proof, &record_target()).unwrap();
    report
        .cells
        .iter()
        .map(|cell| match &cell.verdict {
            Verdict::Matched { certainty } | Verdict::Mismatched { certainty, .. } => *certainty,
            other => panic!("{other:?}"),
        })
        .collect()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn capture_failures_are_counted_in_the_report() {
    // RECORD's own failures: one isolation failure, one value failure at Sheet1!A1.
    let report = file_cache_replay(&record(), &VerdictPolicy::default()).unwrap();
    let mut by_stage = std::collections::BTreeMap::new();
    by_stage.insert("isolation".to_owned(), 1);
    by_stage.insert("value".to_owned(), 1);
    assert_eq!(
        report.capture_failures,
        Some(CaptureFailureCounts { total: 2, by_stage })
    );
    let encoded: serde_json::Value = serde_json::from_str(&report.encode()).unwrap();
    assert_eq!(encoded["version"], 5);
    assert_eq!(
        encoded["capture_failures"],
        serde_json::json!({"total": 2, "by_stage": {"isolation": 1, "value": 1}})
    );
    // The member follows `captured`, which follows `run`.
    let text = report.encode();
    let (run, captured, failures, headline) = (
        text.find("\"run\"").unwrap(),
        text.find("\"captured\"").unwrap(),
        text.find("\"capture_failures\"").unwrap(),
        text.find("\"headline\"").unwrap(),
    );
    assert!(
        run < captured && captured < failures && failures < headline,
        "{text}"
    );
    assert!(
        report
            .summary()
            .ends_with("; 2 capture failures (isolation 1, value 1)"),
        "{}",
        report.summary()
    );

    // A capture without failures counts zero; a version 1 record has no such member at all.
    let clean = with_failures(&[]);
    let report = file_cache_replay(&clean, &VerdictPolicy::default()).unwrap();
    assert_eq!(
        report.capture_failures,
        Some(CaptureFailureCounts::default())
    );
    let encoded: serde_json::Value = serde_json::from_str(&report.encode()).unwrap();
    assert_eq!(
        encoded["capture_failures"],
        serde_json::json!({"total": 0, "by_stage": {}})
    );
    assert!(report.summary().ends_with("; 0 capture failures"));
    let mut v1 = clean;
    v1.session = None;
    let (results, proof) = calculated((1, CONTEXT), 1, CONTEXT);
    let report = judge(&v1, &results, &proof, &VerdictPolicy::default()).unwrap();
    assert_eq!(report.capture_failures, None);
    let encoded: serde_json::Value = serde_json::from_str(&report.encode()).unwrap();
    assert!(encoded.get("capture_failures").is_none());
    assert!(!report.summary().contains("capture failures"));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn an_isolation_failure_certifies_no_cell() {
    let strict = vec![Certainty::StrictBits; 5];
    let failed = vec![Certainty::Uncertified(UncertifiedReason::CaptureFailure); 5];
    // Without failures every raw comparison of the selected target is strict-bit certified, and a
    // failure about one cell's value leaves the certainty of every comparison alone.
    assert_eq!(certainties(&with_failures(&[])), strict);
    assert_eq!(
        certainties(&with_failures(&[("value", Some(cell("Sheet1", 1, 1)))])),
        strict
    );
    // One isolation failure, with or without a cell, and nothing is certified.
    assert_eq!(certainties(&with_failures(&[("isolation", None)])), failed);
    assert_eq!(
        certainties(&with_failures(&[
            ("value", None),
            ("isolation", Some(cell("Hidden", 1, 1)))
        ])),
        failed
    );
    let (results, proof) = calculated((1, CONTEXT), 1, CONTEXT);
    let report = judge(
        &with_failures(&[("isolation", None)]),
        &results,
        &proof,
        &record_target(),
    )
    .unwrap();
    assert_eq!(
        (report.counts.strict_certified, report.counts.uncertified),
        (0, 5)
    );
    let encoded: serde_json::Value = serde_json::from_str(&report.encode()).unwrap();
    assert_eq!(
        encoded["uncertified_by_reason"],
        serde_json::json!({"capture_failure": 5})
    );
    assert_eq!(encoded["cells"][0]["uncertified"], "capture_failure");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn an_environment_failure_certifies_no_cell() {
    let failed = vec![Certainty::Uncertified(UncertifiedReason::CaptureFailure); 5];
    assert_eq!(
        certainties(&with_failures(&[("environment", None)])),
        failed
    );
    // The reason is the capture's failure even where the policy alone would leave the comparison
    // uncertified for another reason.
    let (results, proof) = calculated((1, CONTEXT), 1, CONTEXT);
    let report = judge(
        &with_failures(&[("environment", None)]),
        &results,
        &proof,
        &VerdictPolicy::default(),
    )
    .unwrap();
    let mut by_reason = std::collections::BTreeMap::new();
    by_reason.insert(UncertifiedReason::CaptureFailure, 5);
    assert_eq!(report.uncertified_by_reason(), by_reason);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn an_inventory_failure_refuses_the_record() {
    refused(
        &with_failures(&[("inventory", None)]),
        "records an inventory failure (inventory went wrong), so its formula inventory, the denominator, is not known to be complete",
    );
    refused(
        &with_failures(&[("value", None), ("inventory", Some(cell("Sheet1", 9, 9)))]),
        "records an inventory failure at Sheet1!I9 (inventory went wrong)",
    );
    // The file-cache replay goes through the same judge and is refused alike.
    let error = file_cache_replay(
        &with_failures(&[("inventory", None)]),
        &VerdictPolicy::default(),
    )
    .err()
    .unwrap();
    assert!(
        error.to_string().contains("records an inventory failure"),
        "{error}"
    );
}

// --- legacy array formulas ------------------------------------------------------------------------

/// A version 2 live capture whose sheet Calc holds a legacy array formula over L1:L3 (anchor L1,
/// members L2 and L3, each showing its own element: 10, 20, 30), a legacy array formula over its
/// one cell N1 (6) and an ordinary formula O1 (1).
const LEGACY: &str = r##"{"format": "dnacalc-capture-record", "version": 2, "case": "c", "source": "sha256:in",
 "acquisition": {"kind": "live_capture"}, "environment": null,
 "session": {"revision": 1, "operations": [],
   "context": "excel-calc-context/1;date1904=false;iteration=false;max_iterations=100;max_change=3f50624dd2f1a9fc;precision_as_displayed=false",
   "tool": "calc-oracle-excel test", "packages": {"input": "sha256:in", "output": null}, "failures": []},
 "sheets": [{"name": "Calc", "visibility": "visible"}],
 "formulas": [{"sheet": "Calc", "row": 1, "column": 12, "kind": "legacy_array_anchor", "ref": "L1:L3", "formula": "=Data!A1:A3*10"},
              {"sheet": "Calc", "row": 1, "column": 14, "kind": "legacy_array_anchor", "ref": "N1", "formula": "=SUM(Data!A1:A3)"},
              {"sheet": "Calc", "row": 1, "column": 15, "kind": "ordinary", "formula": "=1"},
              {"sheet": "Calc", "row": 2, "column": 12, "kind": "legacy_array_member", "anchor": "L1", "formula": "=Data!A1:A3*10"},
              {"sheet": "Calc", "row": 3, "column": 12, "kind": "legacy_array_member", "anchor": "L1", "formula": "=Data!A1:A3*10"}],
 "shared_ranges": [],
 "observations": [{"sheet": "Calc", "row": 1, "column": 12, "excel_raw": {"number": "4024000000000000"}},
                  {"sheet": "Calc", "row": 1, "column": 14, "excel_raw": {"number": "4018000000000000"}},
                  {"sheet": "Calc", "row": 1, "column": 15, "excel_raw": {"number": "3ff0000000000000"}},
                  {"sheet": "Calc", "row": 2, "column": 12, "excel_raw": {"number": "4034000000000000"}},
                  {"sheet": "Calc", "row": 3, "column": 12, "excel_raw": {"number": "403e000000000000"}}],
 "saved": [], "reopened": []}"##;

fn legacy() -> CaptureRecord {
    CaptureRecord::decode(LEGACY.as_bytes()).unwrap()
}

/// A run at revision 1 under [`CONTEXT`] that calculated each cell of `values`, with the
/// evaluation record listing each evaluation.
fn calculated_cells(values: &[(CellAddress, f64)]) -> (ResultSet, EvaluationRecord) {
    let results = ResultSet {
        case: "c".to_owned(),
        source: "sha256:in".to_owned(),
        run: RunIdentity {
            revision: 1,
            context: CONTEXT.to_owned(),
        },
        results: values
            .iter()
            .enumerate()
            .map(|(i, (cell, value))| CellResult {
                cell: cell.clone(),
                provenance: Provenance::Calculated {
                    tick: i as u64 + 1,
                    revision: 1,
                    context: CONTEXT.to_owned(),
                },
                value: Some(CalcValue::number(*value)),
            })
            .collect(),
    };
    let evaluations = EvaluationRecord {
        case: "c".to_owned(),
        source: "sha256:in".to_owned(),
        evaluations: values
            .iter()
            .enumerate()
            .map(|(i, (cell, _))| Evaluation {
                cell: cell.clone(),
                tick: i as u64 + 1,
                revision: 1,
                context: CONTEXT.to_owned(),
            })
            .collect(),
    };
    (results, evaluations)
}

/// The cells of [`LEGACY`] in its inventory order, each with Excel's value.
fn legacy_values() -> Vec<(CellAddress, f64)> {
    vec![
        (cell("Calc", 1, 12), 10.0),
        (cell("Calc", 1, 14), 6.0),
        (cell("Calc", 1, 15), 1.0),
        (cell("Calc", 2, 12), 20.0),
        (cell("Calc", 3, 12), 30.0),
    ]
}

/// In a version 2 record a legacy array formula is evaluated once, at its anchor, and its range's
/// cells are proven by that evaluation's array as the range shows it (dc-nam): L1:L3 publish 10,
/// 20 and 30 from {10;20;30}; a member publishing another value stays unproven.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn legacy_array_cells_are_proven_by_their_anchors_evaluation() {
    let capture = legacy();
    let results_of = |values: &[(CellAddress, f64)]| ResultSet {
        case: "c".to_owned(),
        source: "sha256:in".to_owned(),
        run: RunIdentity {
            revision: 1,
            context: CONTEXT.to_owned(),
        },
        results: values
            .iter()
            .map(|(cell, value)| CellResult {
                cell: cell.clone(),
                provenance: Provenance::Calculated {
                    tick: 1,
                    revision: 1,
                    context: CONTEXT.to_owned(),
                },
                value: Some(CalcValue::number(*value)),
            })
            .collect(),
    };
    let column = CalcValue::array(
        crate::value::CalcArray::from_rows(vec![
            vec![CalcValue::number(10.0)],
            vec![CalcValue::number(20.0)],
            vec![CalcValue::number(30.0)],
        ])
        .unwrap(),
    );
    let evaluated = |cell: CellAddress, returned: CalcValue| InstrumentedEvaluation {
        evaluation: Evaluation {
            cell,
            tick: 1,
            revision: 1,
            context: CONTEXT.to_owned(),
        },
        returned: Ok(returned),
    };
    let record = InstrumentRecord {
        case: "c".to_owned(),
        source: "sha256:in".to_owned(),
        evaluations: vec![
            evaluated(cell("Calc", 1, 12), column),
            evaluated(cell("Calc", 1, 14), CalcValue::number(6.0)),
            evaluated(cell("Calc", 1, 15), CalcValue::number(1.0)),
        ],
    };
    let report = judge(&capture, &results_of(&legacy_values()), &record, &VerdictPolicy::default())
        .unwrap();
    assert!(
        report
            .cells
            .iter()
            .all(|cell| matches!(cell.verdict, Verdict::Matched { .. })),
        "{:?}",
        report.cells.iter().map(|cell| &cell.verdict).collect::<Vec<_>>()
    );
    let mut wrong = legacy_values();
    wrong[4].1 = 10.0;
    let report = judge(&capture, &results_of(&wrong), &record, &VerdictPolicy::default()).unwrap();
    assert!(matches!(report.cells[4].verdict, Verdict::Unproven { .. }));
}

/// The legacy array kinds decode from their JSON form (an anchor with its range, a member naming
/// its anchor) and the record round-trips exactly.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn legacy_array_kinds_decode_and_round_trip() {
    use crate::FormulaKind;
    let record = legacy();
    let kinds: Vec<(String, FormulaKind)> = record
        .formulas
        .iter()
        .map(|f| (f.cell.a1(), f.kind))
        .collect();
    let member = FormulaKind::LegacyArrayMember {
        anchor_row: 1,
        anchor_column: 12,
    };
    assert_eq!(
        kinds,
        [
            (
                "L1".to_owned(),
                FormulaKind::LegacyArrayAnchor {
                    rows: 3,
                    columns: 1
                }
            ),
            (
                "N1".to_owned(),
                FormulaKind::LegacyArrayAnchor {
                    rows: 1,
                    columns: 1
                }
            ),
            ("O1".to_owned(), FormulaKind::Ordinary),
            ("L2".to_owned(), member),
            ("L3".to_owned(), member),
        ]
    );
    assert!(record.formulas.iter().all(|f| !f.dynamic_array));
    let encoded = record.encode().unwrap();
    for written in [
        r#""column":12,"kind":"legacy_array_anchor","ref":"L1:L3","formula":"=Data!A1:A3*10"}"#,
        r#""column":14,"kind":"legacy_array_anchor","ref":"N1","formula""#,
        r#""row":2,"column":12,"kind":"legacy_array_member","anchor":"L1","formula""#,
    ] {
        assert!(encoded.contains(written), "{written} not in {encoded}");
    }
    let decoded = CaptureRecord::decode(encoded.as_bytes()).unwrap();
    assert_eq!(decoded, record);
    assert_eq!(decoded.encode().unwrap(), encoded);
}

/// A malformed legacy array entry is refused with its path: a range that does not start at the
/// anchor, runs up or left, is not a range, or is not written in its one form; a member whose
/// anchor is not above or left of it; a member without its anchor, an anchor without its range;
/// and a member that belongs to another kind (a ref on an ordinary formula, a group on an anchor,
/// a dynamic-array flag on a legacy cell). A version 1 record has no legacy kinds.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn malformed_legacy_array_entries_are_refused_with_their_path() {
    let anchor = r#""kind": "legacy_array_anchor", "ref": "L1:L3""#;
    let member = r#""kind": "legacy_array_member", "anchor": "L1""#;
    let ordinary = r#""kind": "ordinary""#;
    for (from, to, path, message) in [
        (
            anchor,
            r#""kind": "legacy_array_anchor", "ref": "M1:M3""#,
            "capture.formulas[0].ref",
            "\"M1:M3\" does not start at the anchor L1",
        ),
        (
            anchor,
            r#""kind": "legacy_array_anchor", "ref": "L1:K3""#,
            "capture.formulas[0].ref",
            "does not start at the anchor L1 and extend down and right from it",
        ),
        (
            anchor,
            r#""kind": "legacy_array_anchor", "ref": "L1:""#,
            "capture.formulas[0].ref",
            "is not a range of relative A1 references",
        ),
        (
            anchor,
            r#""kind": "legacy_array_anchor", "ref": "$L$1:$L$3""#,
            "capture.formulas[0].ref",
            "is not a range of relative A1 references",
        ),
        (
            r#""kind": "legacy_array_anchor", "ref": "N1""#,
            r#""kind": "legacy_array_anchor", "ref": "N1:N1""#,
            "capture.formulas[1].ref",
            "is \"N1:N1\"; this range is written \"N1\"",
        ),
        (
            anchor,
            r#""kind": "legacy_array_anchor""#,
            "capture.formulas[0].kind",
            "is legacy_array_anchor and has no ref",
        ),
        (
            anchor,
            r#""kind": "legacy_array_anchor", "ref": "L1:L3", "group": 0"#,
            "capture.formulas[0].group",
            "is given for a legacy_array_anchor formula",
        ),
        (
            anchor,
            r#""kind": "legacy_array_anchor", "ref": "L1:L3", "anchor": "L1""#,
            "capture.formulas[0].anchor",
            "is given for a legacy_array_anchor formula",
        ),
        (
            member,
            r#""kind": "legacy_array_member", "anchor": "L3""#,
            "capture.formulas[3].anchor",
            "\"L3\" is not above or left of the member L2",
        ),
        (
            member,
            r#""kind": "legacy_array_member", "anchor": "L2""#,
            "capture.formulas[3].anchor",
            "\"L2\" is not above or left of the member L2",
        ),
        (
            member,
            r#""kind": "legacy_array_member", "anchor": "l1""#,
            "capture.formulas[3].anchor",
            "is not a relative A1 reference",
        ),
        (
            member,
            r#""kind": "legacy_array_member""#,
            "capture.formulas[3].kind",
            "is legacy_array_member and has no anchor",
        ),
        (
            member,
            r#""kind": "legacy_array_member", "anchor": "L1", "ref": "L1:L3""#,
            "capture.formulas[3].ref",
            "is given for a legacy_array_member formula",
        ),
        (
            ordinary,
            r#""kind": "ordinary", "ref": "O1""#,
            "capture.formulas[2].ref",
            "is given for an ordinary formula",
        ),
        (
            anchor,
            r#""kind": "legacy_array", "ref": "L1:L3""#,
            "capture.formulas[0].kind",
            "\"legacy_array\" is not one of ordinary, shared_master, shared_child, legacy_array_anchor, legacy_array_member",
        ),
        (
            r#""ref": "N1", "formula": "=SUM(Data!A1:A3)"}"#,
            r#""ref": "N1", "formula": "=SUM(Data!A1:A3)", "dynamic_array": true}"#,
            "capture.formulas[1].dynamic_array",
            "is given for a legacy_array_anchor formula",
        ),
    ] {
        let text = LEGACY.replacen(from, to, 1);
        assert_ne!(text, LEGACY, "{from} not found");
        let error = CaptureRecord::decode(text.as_bytes()).err().unwrap();
        assert_eq!(error.path, path, "{to}: {error}");
        assert!(error.message.contains(message), "{to}: {error}");
    }
    // A version 1 record admits no ref or anchor member, so no legacy kind.
    let v1 = r#"{"format": "dnacalc-capture-record", "version": 1, "case": "c", "source": "s",
        "acquisition": {"kind": "live_capture"}, "environment": null,
        "sheets": [{"name": "Calc", "visibility": "visible"}],
        "formulas": [{"sheet": "Calc", "row": 1, "column": 14, "kind": "legacy_array_anchor", "ref": "N1"}],
        "observations": []}"#;
    let error = CaptureRecord::decode(v1.as_bytes()).err().unwrap();
    assert_eq!(
        (
            error.path.as_str(),
            error.message.contains("unknown member \"ref\"")
        ),
        ("capture.formulas[0]", true),
        "{error}"
    );
}

/// Each cell of a legacy array formula is judged like an ordinary formula cell, by its own value:
/// the anchor and each member matched when calculated as Excel shows it, and a member calculated
/// as another value mismatched. The strata count the anchors and members, and each cell of the
/// report names its range or its anchor.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn legacy_array_cells_are_judged_each_by_its_own_value() {
    let capture = legacy();
    let (results, proof) = calculated_cells(&legacy_values());
    assert_eq!(
        verdict_names(&capture, &results, &proof),
        vec!["matched"; 5]
    );
    let report = judge(&capture, &results, &proof, &VerdictPolicy::default()).unwrap();
    assert_eq!(report.counts.denominator, 5);
    assert_eq!(
        (
            report.strata.ordinary,
            report.strata.legacy_array_anchor,
            report.strata.legacy_array_member
        ),
        (1, 2, 2)
    );
    let encoded: serde_json::Value = serde_json::from_str(&report.encode()).unwrap();
    assert_eq!(encoded["strata"]["legacy_array_anchor"], 2);
    assert_eq!(encoded["strata"]["legacy_array_member"], 2);
    let cells: Vec<String> = encoded["cells"]
        .as_array()
        .unwrap()
        .iter()
        .map(|cell| {
            format!(
                "{} {} ref={} anchor={}",
                cell["cell"].as_str().unwrap(),
                cell["kind"].as_str().unwrap(),
                cell["ref"],
                cell["anchor"]
            )
        })
        .collect();
    assert_eq!(
        cells,
        [
            "L1 legacy_array_anchor ref=\"L1:L3\" anchor=null",
            "N1 legacy_array_anchor ref=\"N1\" anchor=null",
            "O1 ordinary ref=null anchor=null",
            "L2 legacy_array_member ref=null anchor=\"L1\"",
            "L3 legacy_array_member ref=null anchor=\"L1\"",
        ]
    );

    // L3 calculated as the anchor's value, as a scalar reading of the array would give it.
    let mut wrong = legacy_values();
    wrong[4].1 = 10.0;
    let (results, proof) = calculated_cells(&wrong);
    assert_eq!(
        verdict_names(&capture, &results, &proof),
        ["matched", "matched", "matched", "matched", "mismatched"]
    );
    // A member with no result is missing, never dropped from the denominator.
    let mut omitted = legacy_values();
    omitted.remove(3);
    let (results, proof) = calculated_cells(&omitted);
    let report = judge(&capture, &results, &proof, &VerdictPolicy::default()).unwrap();
    assert_eq!((report.counts.denominator, report.counts.missing), (5, 1));
}

/// The inventory must list every cell of each legacy array range once, the anchor and each member
/// naming it: a record with a member left out, listed as another kind, naming another anchor, or
/// outside its anchor's range or naming a cell that is no anchor, or with an anchor whose range is
/// empty or leaves the grid, is refused.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_legacy_array_must_list_every_cell_of_its_range() {
    use crate::{FormulaCell, FormulaKind};
    let member_of = |anchor_row, anchor_column| FormulaKind::LegacyArrayMember {
        anchor_row,
        anchor_column,
    };
    let mut dropped = legacy();
    dropped.formulas.remove(4);
    dropped.observations.remove(4);
    refused(
        &dropped,
        "Calc!L3 lies in the range of the legacy array formula at Calc!L1 and is not in the formula inventory",
    );
    let mut ordinary = legacy();
    ordinary.formulas[4].kind = FormulaKind::Ordinary;
    refused(
        &ordinary,
        "Calc!L3 lies in the range of the legacy array formula at Calc!L1 and the formula inventory lists it as ordinary",
    );
    let mut elsewhere = legacy();
    elsewhere.formulas[4].kind = member_of(2, 12);
    refused(
        &elsewhere,
        "Calc!L3 lies in the range of the legacy array formula at Calc!L1 and the formula inventory lists it as legacy_array_member, not as a member of that array",
    );
    // A member below the anchor's range, which names the anchor.
    let mut beyond = legacy();
    beyond.formulas.push(FormulaCell {
        cell: cell("Calc", 4, 12),
        kind: member_of(1, 12),
        formula: None,
        dynamic_array: false,
    });
    refused(
        &beyond,
        "Calc!L4 is a member of a legacy array formula anchored at Calc!L1, which the formula inventory does not list as the anchor of a range covering it",
    );
    // A member naming an ordinary formula as its anchor.
    let mut not_anchor = legacy();
    not_anchor.formulas.push(FormulaCell {
        cell: cell("Calc", 2, 15),
        kind: member_of(1, 15),
        formula: None,
        dynamic_array: false,
    });
    refused(
        &not_anchor,
        "Calc!O2 is a member of a legacy array formula anchored at Calc!O1, which the formula inventory does not list as the anchor of a range covering it",
    );
    for (rows, columns) in [
        (0, 1),
        (1, 0),
        (crate::value::MAX_ROWS + 1, 1),
        (1, 2 << 14),
    ] {
        let mut empty = legacy();
        empty.formulas[1].kind = FormulaKind::LegacyArrayAnchor { rows, columns };
        refused(
            &empty,
            &format!(
                "the legacy array formula at Calc!N1 has a range of {rows} by {columns} cells, which is not a range of Excel's grid starting at the cell"
            ),
        );
    }
}
