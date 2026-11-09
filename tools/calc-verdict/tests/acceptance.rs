//! Acceptance cases owned by the calc-verdict package, each registered by its exact name in
//! `tools/calc-verdict/acceptance.json`. Every case is `#[test]` natively and `#[wasm_bindgen_test]`
//! on wasm32, with no browser or Node.js configuration of its own: `cargo xtask accept` forces the
//! browser with `WASM_BINDGEN_USE_BROWSER`.
//!
//! The fixtures under `tests/fixtures/` are authored canonical CaptureRecord, result-set and
//! evaluation-record JSON, read with `include_bytes!` so native and browser runs judge the same
//! bytes. Each case runs the library's own route (decode the three files, `judge`,
//! `VerdictReport::encode`), the route `cargo xtask verdict` runs, and compares the encoded
//! report's counts and per-cell verdicts with literal values written here. Each case also judges
//! variant fixtures on which its rule gives a different, equally literal, answer.
//!
//! The evaluation records here are authored too, in version 1, which carries no returned values;
//! the record made from instrumented evaluator calls (version 2, with what each evaluation
//! returned) is bead `dc-qguw`'s, judged in calc-engine's `dc_qguw` cases. What these cases show is
//! that the verdict counts a calculated claim only against that separate record, never against the
//! result set's own claim. Each report states that it was judged against a version 1 record and
//! counts every calculated cell in `calculated_without_value_check`; bead `dc-i1ro` moves these
//! fixtures to version 2.
//!
//! `dc_61sj__shared_ref_with_holes_judged` judges hand-built version 2 live-capture records under
//! `tests/fixtures/shared_ref_with_holes/` whose shared-formula ref covers cells that are not of
//! its group, both through `judge` and through `file_cache_replay`, the replay `cargo xtask
//! capture` runs before it writes a record.
//!
//! `infra_m1_2__capture_replays` reads the capture artifacts under `tests/captures/infra_m1_2/`,
//! which no one authors by hand: `cargo xtask capture` writes them on a Windows machine with
//! Excel (on the owner's machine any agent may run it). Until it has, each path holds the
//! absent-capture marker and the case reports `ACCEPT-NOT-RUN` with the library's typed
//! `MissingCapture`, which the runner records as not run.
//! Once it has, the case first ties the capture to its input as committed: a capture whose source
//! is not the SHA-256 of `platform/calc-oracle-excel/tests/fixtures/infra_m1_2/replay.xlsx` is
//! awaiting capture (reported as not run, never as passed). The worker that took it is provenance
//! (the capture-tool revision names its results version and hash), not a condition.
//! The replayed report must also count the COM add-ins the capture's environment lists as
//! connected while it was taken (`com_add_ins_connected`), counted here without the library.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_verdict::value::{NumericMode, NumericTargetId};
use calc_verdict::{
    CalculationContext, CaptureRecord, EvaluationRecord, JudgeError, MissingCapture, ResultSet,
    STAGE_ISOLATION, VerdictPolicy, file_cache_replay, judge, sha256_hex,
};
use serde_json::{Value, json};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test;

/// Prints a line the acceptance runner keeps: on stdout natively, on the browser console (which
/// wasm-bindgen-test-runner forwards under `--nocapture`) on wasm32.
macro_rules! note {
    ($($arg:tt)*) => {{
        let line = format!($($arg)*);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_test::console_log!("{}", line);
        #[cfg(not(target_arch = "wasm32"))]
        println!("{line}");
    }};
}

const TRAP_CAPTURE: &[u8] = include_bytes!("fixtures/cached_value_trap/capture.json");
const TRAP_FILE_CACHED: &[u8] =
    include_bytes!("fixtures/cached_value_trap/results_file_cached.json");
const TRAP_CALCULATED: &[u8] = include_bytes!("fixtures/cached_value_trap/results_calculated.json");
const TRAP_NOT_CALCULATED: &[u8] =
    include_bytes!("fixtures/cached_value_trap/results_not_calculated.json");
const TRAP_EVALUATIONS: &[u8] = include_bytes!("fixtures/cached_value_trap/evaluations.json");
const TRAP_NO_EVALUATIONS: &[u8] =
    include_bytes!("fixtures/cached_value_trap/evaluations_empty.json");
const TRAP_OTHER_EVALUATIONS: &[u8] =
    include_bytes!("fixtures/cached_value_trap/evaluations_mismatched.json");

const DENOMINATOR_CAPTURE: &[u8] = include_bytes!("fixtures/denominator_integrity/capture.json");
const DENOMINATOR_EMPTY: &[u8] =
    include_bytes!("fixtures/denominator_integrity/capture_empty.json");
const DENOMINATOR_COMPLETE: &[u8] =
    include_bytes!("fixtures/denominator_integrity/results_complete.json");
const DENOMINATOR_OMITTING: &[u8] =
    include_bytes!("fixtures/denominator_integrity/results_omitting.json");
const DENOMINATOR_EVALUATIONS: &[u8] =
    include_bytes!("fixtures/denominator_integrity/evaluations.json");

/// The judgement of `results` against `capture` with the evaluation record `evaluations`.
fn judged(
    capture: &[u8],
    results: &[u8],
    evaluations: &[u8],
    policy: &VerdictPolicy<'_>,
) -> Result<Value, JudgeError> {
    let capture = CaptureRecord::decode(capture).expect("the capture fixture decodes");
    let results = ResultSet::decode(results).expect("the result fixture decodes");
    let evaluations =
        EvaluationRecord::decode(evaluations).expect("the evaluation fixture decodes");
    let report = judge(&capture, &results, &evaluations, policy)?;
    let encoded = report.encode();
    Ok(serde_json::from_str(&encoded).expect("the encoded report is JSON"))
}

/// The encoded report of `results` judged against `capture` with `evaluations`, read back as JSON.
fn report(capture: &[u8], results: &[u8], evaluations: &[u8], policy: &VerdictPolicy<'_>) -> Value {
    judged(capture, results, evaluations, policy).expect("the fixtures are consistent")
}

fn text<'a>(value: &'a Value, member: &str) -> &'a str {
    value[member]
        .as_str()
        .unwrap_or_else(|| panic!("{member} is not a string in {value}"))
}

/// Each cell line of a report as `<sheet>!<cell> <kind>[(<group>)] <sheet visibility>
/// <verdict>[(<stale source>)][ <certainty>[/<reason>]][ tick=<tick>][ feature=<feature>]
/// [ fault=<fault>][ reason=<reason or null>][ equal=<bool>]`.
fn cell_lines(report: &Value) -> Vec<String> {
    report["cells"]
        .as_array()
        .expect("cells is an array")
        .iter()
        .map(|cell| {
            let mut line = format!(
                "{}!{} {}",
                text(cell, "sheet"),
                text(cell, "cell"),
                text(cell, "kind")
            );
            if let Some(group) = cell["group"].as_u64() {
                line.push_str(&format!("({group})"));
            }
            line.push_str(&format!(
                " {} {}",
                text(cell, "sheet_visibility"),
                text(cell, "verdict")
            ));
            if let Some(source) = cell["stale"].as_str() {
                line.push_str(&format!("({source})"));
            }
            if let Some(certainty) = cell["certainty"].as_str() {
                line.push_str(&format!(" {certainty}"));
            }
            if let Some(reason) = cell["uncertified"].as_str() {
                line.push_str(&format!("/{reason}"));
            }
            if let Some(tick) = cell["tick"].as_u64() {
                line.push_str(&format!(" tick={tick}"));
            }
            for member in ["feature", "fault"] {
                if let Some(value) = cell[member].as_str() {
                    line.push_str(&format!(" {member}={value}"));
                }
            }
            match cell.get("reason") {
                Some(Value::String(reason)) => line.push_str(&format!(" reason={reason}")),
                Some(Value::Null) => line.push_str(" reason=null"),
                _ => {}
            }
            if let Some(equal) = cell["value_equals_expectation"].as_bool() {
                line.push_str(&format!(" equal={equal}"));
            }
            line
        })
        .collect()
}

/// The counts a report states, with no category left out: every member of `counts` is compared.
/// Every report here is judged against a version 1 evaluation record, and says so.
fn assert_counts(report: &Value, expected: Value, what: &str) {
    assert_eq!(report["counts"], expected, "{what}: counts");
    assert_eq!(
        report["evaluation_record_version"], 1,
        "{what}: evaluation record version"
    );
}

/// Compares `cell_lines(report)` with `expected` line by line.
fn assert_cells(report: &Value, expected: &[&str], what: &str) {
    let lines = cell_lines(report);
    assert_eq!(lines, expected, "{what}: cell verdicts");
}

/// The Exact target the trap fixture's Excel environment is.
fn trap_target() -> NumericMode {
    NumericMode::Exact(
        NumericTargetId::new(
            "20430",
            "fixture-channel",
            "fixture-compatibility",
            "fixture-x86-64",
        )
        .expect("four non-empty parts"),
    )
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn infra_m1_1__cached_value_trap() {
    const CASE: &str = "infra_m1_1__cached_value_trap";
    let policy = VerdictPolicy {
        numeric_mode: trap_target(),
        evidence: None,
    };

    // Every result is the file's cache. Five of the seven equal Excel's observations exactly (a
    // number, a decimal saved cache, text, a logical and an error); A7 has no observation. The
    // evaluation record even lists an evaluation of every cell: the provenance still says the
    // value published is the file's, so none is calculated.
    let cached = report(TRAP_CAPTURE, TRAP_FILE_CACHED, TRAP_EVALUATIONS, &policy);
    assert_eq!(
        cached["headline"]["calculated_of_denominator"],
        json!([0, 7])
    );
    assert_eq!(cached["headline"]["matched_of_calculated"], json!([0, 0]));
    assert_counts(
        &cached,
        json!({
            "denominator": 7, "calculated": 0, "matched": 0, "mismatched": 0, "not_run": 0,
            "strict_certified": 0, "uncertified": 0, "unproven": 0, "stale": 0,
            "file_cached": 7, "not_modeled": 0, "failed": 0, "missing": 0,
            "not_calculated_equal_to_expectation": 5, "outside_inventory": 0,
            "evaluator_calls": 7,
            "calculated_without_value_check": 0
        }),
        "file-cached results",
    );
    assert_cells(
        &cached,
        &[
            "Sheet1!A1 ordinary visible file_cached equal=true",
            "Sheet1!A2 ordinary visible file_cached equal=true",
            "Sheet1!A3 ordinary visible file_cached equal=true",
            "Sheet1!A4 ordinary visible file_cached equal=true",
            "Sheet1!A5 ordinary visible file_cached equal=true",
            "Sheet1!A6 ordinary visible file_cached equal=false",
            "Sheet1!A7 ordinary visible file_cached",
        ],
        "file-cached results",
    );
    note!(
        "ACCEPT-NOTE: {CASE}: seven file-cached results, five equal to Excel's observations (raw number 2, saved-cache decimal 0.1, raw text \"ok\", raw TRUE, raw #DIV/0!), with an evaluation of every cell in the evaluation record: calculated 0 of 7, matched 0, file_cached 7, not_calculated_equal_to_expectation 5"
    );

    // Variant: the same values with calculated provenance, and the evaluation record holding an
    // evaluation of each cell at its tick, revision and context. The five equal values now match;
    // A6 (5 against Excel's 4) mismatches; A7, calculated but not observed, is not run. Raw
    // observations of the selected target are strict-bit certified; the decimal saved cache is not.
    let calculated = report(TRAP_CAPTURE, TRAP_CALCULATED, TRAP_EVALUATIONS, &policy);
    assert_eq!(
        calculated["headline"]["calculated_of_denominator"],
        json!([7, 7])
    );
    assert_eq!(
        calculated["headline"]["matched_of_calculated"],
        json!([5, 7])
    );
    assert_counts(
        &calculated,
        json!({
            "denominator": 7, "calculated": 7, "matched": 5, "mismatched": 1, "not_run": 1,
            "strict_certified": 5, "uncertified": 1, "unproven": 0, "stale": 0,
            "file_cached": 0, "not_modeled": 0, "failed": 0, "missing": 0,
            "not_calculated_equal_to_expectation": 0, "outside_inventory": 0,
            "evaluator_calls": 7,
            "calculated_without_value_check": 7
        }),
        "calculated results",
    );
    assert_cells(
        &calculated,
        &[
            "Sheet1!A1 ordinary visible matched strict_bits",
            "Sheet1!A2 ordinary visible matched uncertified/saved_cache",
            "Sheet1!A3 ordinary visible matched strict_bits",
            "Sheet1!A4 ordinary visible matched strict_bits",
            "Sheet1!A5 ordinary visible matched strict_bits",
            "Sheet1!A6 ordinary visible mismatched strict_bits",
            "Sheet1!A7 ordinary visible not_run",
        ],
        "calculated results",
    );
    assert_eq!(
        calculated["cells"][5]["mismatch"],
        json!({"class": "number", "ulp": 1125899906842624u64}),
        "A6 is 5 against Excel's 4: 2^50 binary64 steps apart"
    );
    assert_eq!(
        calculated["cells"][5]["expected"],
        json!({"number": "4010000000000000"})
    );
    assert_eq!(
        calculated["cells"][5]["actual"],
        json!({"number": "4014000000000000"})
    );
    assert_eq!(
        calculated["cells"][1]["cache"],
        json!({"t": "n", "v": "0.1"})
    );
    assert_eq!(calculated["cells"][6]["observation"], Value::Null);
    assert_eq!(
        calculated["uncertified_by_reason"],
        json!({"saved_cache": 1})
    );
    note!(
        "ACCEPT-NOTE: {CASE}: variant with calculated provenance and an evaluation of each cell in the evaluation record: calculated 7 of 7, matched 5 (strict_bits 4, saved_cache 1), mismatched 1 (A6: number, 1125899906842624 ulp), not_run 1 (A7, not observed)"
    );

    // Variant: the same calculated claims judged with an evaluation record that lists no
    // evaluation. The result set's claim alone is a status flag from the code under test, and
    // counts nothing.
    let unproven = report(TRAP_CAPTURE, TRAP_CALCULATED, TRAP_NO_EVALUATIONS, &policy);
    assert_eq!(
        unproven["headline"]["calculated_of_denominator"],
        json!([0, 7])
    );
    assert_counts(
        &unproven,
        json!({
            "denominator": 7, "calculated": 0, "matched": 0, "mismatched": 0, "not_run": 0,
            "strict_certified": 0, "uncertified": 0, "unproven": 7, "stale": 0,
            "file_cached": 0, "not_modeled": 0, "failed": 0, "missing": 0,
            "not_calculated_equal_to_expectation": 5, "outside_inventory": 0,
            "evaluator_calls": 0,
            "calculated_without_value_check": 0
        }),
        "calculated claims without evaluations",
    );
    assert_cells(
        &unproven,
        &[
            "Sheet1!A1 ordinary visible unproven tick=1 equal=true",
            "Sheet1!A2 ordinary visible unproven tick=2 equal=true",
            "Sheet1!A3 ordinary visible unproven tick=3 equal=true",
            "Sheet1!A4 ordinary visible unproven tick=4 equal=true",
            "Sheet1!A5 ordinary visible unproven tick=5 equal=true",
            "Sheet1!A6 ordinary visible unproven tick=6 equal=false",
            "Sheet1!A7 ordinary visible unproven tick=7",
        ],
        "calculated claims without evaluations",
    );
    note!(
        "ACCEPT-NOTE: {CASE}: variant with the same calculated claims and an empty evaluation record: calculated 0 of 7, unproven 7, not_calculated_equal_to_expectation 5"
    );

    // Variant: an evaluation record whose entries match only A1 and A7. A2's is at another tick,
    // A3's at another revision, A4's under another context, A5's for another cell (B5), and A6 has
    // none: each of those claims is unproven.
    let partly = report(
        TRAP_CAPTURE,
        TRAP_CALCULATED,
        TRAP_OTHER_EVALUATIONS,
        &policy,
    );
    assert_counts(
        &partly,
        json!({
            "denominator": 7, "calculated": 2, "matched": 1, "mismatched": 0, "not_run": 1,
            "strict_certified": 1, "uncertified": 0, "unproven": 5, "stale": 0,
            "file_cached": 0, "not_modeled": 0, "failed": 0, "missing": 0,
            "not_calculated_equal_to_expectation": 4, "outside_inventory": 0,
            "evaluator_calls": 6,
            "calculated_without_value_check": 2
        }),
        "evaluations that do not match the claims",
    );
    assert_cells(
        &partly,
        &[
            "Sheet1!A1 ordinary visible matched strict_bits",
            "Sheet1!A2 ordinary visible unproven tick=2 equal=true",
            "Sheet1!A3 ordinary visible unproven tick=3 equal=true",
            "Sheet1!A4 ordinary visible unproven tick=4 equal=true",
            "Sheet1!A5 ordinary visible unproven tick=5 equal=true",
            "Sheet1!A6 ordinary visible unproven tick=6 equal=false",
            "Sheet1!A7 ordinary visible not_run",
        ],
        "evaluations that do not match the claims",
    );
    note!(
        "ACCEPT-NOTE: {CASE}: variant whose evaluation record matches only A1 and A7 (A2 another tick, A3 another revision, A4 another context, A5 another cell, A6 none): calculated 2 of 7, matched 1, not_run 1, unproven 5"
    );

    // Variant: every other provenance that is not a calculation of this run, each counted in its
    // own category. A4 to A6 carry Excel's own values and still count nothing; only A7, proven
    // and unobserved, is calculated (not run).
    let others = report(TRAP_CAPTURE, TRAP_NOT_CALCULATED, TRAP_EVALUATIONS, &policy);
    assert_eq!(
        others["headline"]["calculated_of_denominator"],
        json!([1, 7])
    );
    assert_eq!(others["headline"]["matched_of_calculated"], json!([0, 1]));
    assert_counts(
        &others,
        json!({
            "denominator": 7, "calculated": 1, "matched": 0, "mismatched": 0, "not_run": 1,
            "strict_certified": 0, "uncertified": 0, "unproven": 0, "stale": 3,
            "file_cached": 0, "not_modeled": 1, "failed": 1, "missing": 1,
            "not_calculated_equal_to_expectation": 3, "outside_inventory": 0,
            "evaluator_calls": 7,
            "calculated_without_value_check": 1
        }),
        "not-calculated provenances",
    );
    assert_cells(
        &others,
        &[
            "Sheet1!A1 ordinary visible not_modeled feature=LAMBDA",
            "Sheet1!A2 ordinary visible failed fault=fixture-admission-refused",
            "Sheet1!A3 ordinary visible missing reason=fixture-nothing-published",
            "Sheet1!A4 ordinary visible stale(reported) equal=true",
            "Sheet1!A5 ordinary visible stale(revision_not_run) equal=true",
            "Sheet1!A6 ordinary visible stale(context_not_run) equal=true",
            "Sheet1!A7 ordinary visible not_run",
        ],
        "not-calculated provenances",
    );
    note!(
        "ACCEPT-NOTE: {CASE}: variant with not_modeled, failed, missing (with reason), stale (reported, other revision, other context: all three equal to Excel's values) and one proven unobserved cell: calculated 1 of 7 (not_run 1), matched 0, stale 3, not_modeled 1, failed 1, missing 1"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn infra_m1_1__denominator_integrity() {
    const CASE: &str = "infra_m1_1__denominator_integrity";
    let policy = VerdictPolicy::default();

    // The fixture's own inventory, read here without the library: seven formula cells, one on a
    // hidden sheet and one on a very hidden sheet, a shared group of a master and two children,
    // and two error-valued formulas.
    let fixture: Value = serde_json::from_slice(DENOMINATOR_CAPTURE).expect("fixture JSON");
    let formulas = fixture["formulas"].as_array().expect("formulas");
    let on = |sheet: &str| formulas.iter().filter(|f| f["sheet"] == sheet).count();
    let kind = |kind: &str| formulas.iter().filter(|f| f["kind"] == kind).count();
    assert_eq!(
        (
            formulas.len(),
            on("Hidden"),
            on("Archive"),
            kind("shared_master"),
            kind("shared_child")
        ),
        (7, 1, 1, 1, 2),
        "the fixture's inventory"
    );

    let expected_strata = json!({
        "visible_sheet": 5, "hidden_sheet": 1, "very_hidden_sheet": 1, "ordinary": 4,
        "shared_master": 1, "shared_child": 2,
        "legacy_array_anchor": 0, "legacy_array_member": 0, "observed": 7, "error_expected": 2
    });

    // Every inventory cell calculated, plus one result for a cell outside the inventory (Sheet1!E1),
    // which is counted apart and does not enter the denominator. The saved caches are of every
    // type: text (A1), numbers (the shared group), errors (D1, Archive!A1) and a logical
    // (Hidden!A1).
    let complete = report(
        DENOMINATOR_CAPTURE,
        DENOMINATOR_COMPLETE,
        DENOMINATOR_EVALUATIONS,
        &policy,
    );
    assert_eq!(
        complete["headline"]["calculated_of_denominator"],
        json!([7, 7])
    );
    assert_eq!(complete["headline"]["matched_of_calculated"], json!([7, 7]));
    assert_counts(
        &complete,
        json!({
            "denominator": 7, "calculated": 7, "matched": 7, "mismatched": 0, "not_run": 0,
            "strict_certified": 0, "uncertified": 7, "unproven": 0, "stale": 0,
            "file_cached": 0, "not_modeled": 0, "failed": 0, "missing": 0,
            "not_calculated_equal_to_expectation": 0, "outside_inventory": 1,
            "evaluator_calls": 8,
            "calculated_without_value_check": 7
        }),
        "complete results",
    );
    assert_eq!(
        complete["strata"], expected_strata,
        "complete results: strata"
    );
    assert_cells(
        &complete,
        &[
            "Sheet1!A1 ordinary visible matched uncertified/saved_cache",
            "Sheet1!C1 shared_master(0) visible matched uncertified/saved_cache",
            "Sheet1!C2 shared_child(0) visible matched uncertified/saved_cache",
            "Sheet1!C3 shared_child(0) visible matched uncertified/saved_cache",
            "Sheet1!D1 ordinary visible matched uncertified/saved_cache",
            "Hidden!A1 ordinary hidden matched uncertified/saved_cache",
            "Archive!A1 ordinary very_hidden matched uncertified/saved_cache",
        ],
        "complete results",
    );
    let caches: Vec<&Value> = complete["cells"]
        .as_array()
        .expect("cells")
        .iter()
        .map(|cell| &cell["cache"])
        .collect();
    assert_eq!(
        caches,
        [
            &json!({"t": "str", "v": "total"}),
            &json!({"t": "n", "v": "2"}),
            &json!({"t": "n", "v": "4"}),
            &json!({"t": "n", "v": "6"}),
            &json!({"t": "e", "v": "#DIV/0!"}),
            &json!({"t": "b", "v": "1"}),
            &json!({"t": "e", "v": "#N/A"}),
        ]
    );
    assert_eq!(
        complete["outside_inventory"],
        json!([{"sheet": "Sheet1", "row": 1, "column": 5, "cell": "E1", "provenance": "calculated"}])
    );
    note!(
        "ACCEPT-NOTE: {CASE}: complete result set: denominator 7 (hidden sheet 1, very hidden sheet 1, shared master 1, shared children 2, error-valued 2), calculated 7, matched 7 against saved caches of t str, n, e and b (saved_cache 7); the result for Sheet1!E1, outside the inventory, is counted apart (outside_inventory 1)"
    );

    // Variant: the result set leaves out the hidden-sheet formula, a shared child and an
    // error-valued formula, though the evaluation record lists an evaluation of each. The
    // denominator does not shrink: each is reported missing, with no result at all.
    let omitting = report(
        DENOMINATOR_CAPTURE,
        DENOMINATOR_OMITTING,
        DENOMINATOR_EVALUATIONS,
        &policy,
    );
    assert_eq!(
        omitting["headline"]["calculated_of_denominator"],
        json!([4, 7])
    );
    assert_eq!(omitting["headline"]["matched_of_calculated"], json!([4, 4]));
    assert_counts(
        &omitting,
        json!({
            "denominator": 7, "calculated": 4, "matched": 4, "mismatched": 0, "not_run": 0,
            "strict_certified": 0, "uncertified": 4, "unproven": 0, "stale": 0,
            "file_cached": 0, "not_modeled": 0, "failed": 0, "missing": 3,
            "not_calculated_equal_to_expectation": 0, "outside_inventory": 1,
            "evaluator_calls": 8,
            "calculated_without_value_check": 4
        }),
        "omitting results",
    );
    assert_eq!(
        omitting["strata"], expected_strata,
        "omitting results: strata"
    );
    assert_cells(
        &omitting,
        &[
            "Sheet1!A1 ordinary visible matched uncertified/saved_cache",
            "Sheet1!C1 shared_master(0) visible matched uncertified/saved_cache",
            "Sheet1!C2 shared_child(0) visible matched uncertified/saved_cache",
            "Sheet1!C3 shared_child(0) visible missing reason=null",
            "Sheet1!D1 ordinary visible missing reason=null",
            "Hidden!A1 ordinary hidden missing reason=null",
            "Archive!A1 ordinary very_hidden matched uncertified/saved_cache",
        ],
        "omitting results",
    );
    note!(
        "ACCEPT-NOTE: {CASE}: variant omitting Hidden!A1, the shared child Sheet1!C3 and the error-valued Sheet1!D1: denominator still 7, calculated 4, missing 3, each omitted cell listed missing with no result"
    );

    // Variant: a record of the same workbook whose inventory lists no formula cell. It is refused
    // with its own error rather than reported as calculated 0 of 0.
    let refusal = judged(
        DENOMINATOR_EMPTY,
        DENOMINATOR_OMITTING,
        DENOMINATOR_EVALUATIONS,
        &policy,
    )
    .expect_err("an empty inventory is refused");
    assert_eq!(
        refusal,
        JudgeError::EmptyInventory {
            case: CASE.to_owned()
        }
    );
    note!("ACCEPT-NOTE: {CASE}: variant with an empty formula inventory: refused ({refusal})");
}

const REPLAY_CAPTURE: &[u8] = include_bytes!("captures/infra_m1_2/replay.capture.json");
const REPLAY_VERDICT: &[u8] = include_bytes!("captures/infra_m1_2/replay.verdict.json");
/// The workbook the replay capture is of, as committed (kept byte for byte across checkouts by its
/// `.gitattributes`).
const REPLAY_WORKBOOK: &[u8] =
    include_bytes!("../../../platform/calc-oracle-excel/tests/fixtures/infra_m1_2/replay.xlsx");

/// Whether a saved cache (`t`, `v`) holds the canonical raw value `raw`, decided here without the
/// library: a number by the bits of the decimal's nearest binary64, a logical by `1` or `0`, an
/// error by its literal, text by its UTF-16 code units.
fn saved_equals_raw(saved: &Value, raw: &Value) -> bool {
    let v = saved["v"].as_str().expect("a saved cache has v");
    match saved["t"].as_str().expect("a saved cache has t") {
        "n" => match (raw["number"].as_str(), v.parse::<f64>()) {
            (Some(bits), Ok(number)) => format!("{:016x}", number.to_bits()) == bits,
            _ => false,
        },
        "b" => raw["logical"]
            .as_bool()
            .is_some_and(|b| v == if b { "1" } else { "0" }),
        "e" => {
            let literal = match raw["error"].as_str() {
                Some("null") => "#NULL!",
                Some("div0") => "#DIV/0!",
                Some("value") => "#VALUE!",
                Some("ref") => "#REF!",
                Some("name") => "#NAME?",
                Some("num") => "#NUM!",
                Some("na") => "#N/A",
                Some("busy") => "#BUSY!",
                Some("getting_data") => "#GETTING_DATA",
                Some("spill") => "#SPILL!",
                Some("calc") => "#CALC!",
                Some("field") => "#FIELD!",
                Some("blocked") => "#BLOCKED!",
                Some("connect") => "#CONNECT!",
                _ => return false,
            };
            v == literal
        }
        "str" => raw["text"].as_str().is_some_and(|units| {
            units
                == v.encode_utf16()
                    .map(|unit| format!("{unit:04x}"))
                    .collect::<String>()
        }),
        _ => false,
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn infra_m1_2__capture_replays() {
    const CASE: &str = "infra_m1_2__capture_replays";

    // The capture path: the record an Excel run of `cargo xtask capture` wrote, or the
    // absent-capture marker.
    if let Some(missing) = MissingCapture::recognize(REPLAY_CAPTURE) {
        assert_eq!(missing.case, CASE, "the marker names this case");
        note!("ACCEPT-NOT-RUN: {CASE}: {missing}");
        return;
    }
    let capture = CaptureRecord::decode(REPLAY_CAPTURE).expect("the committed capture decodes");
    assert_eq!(capture.case, CASE);
    let session = capture
        .session
        .as_ref()
        .expect("the committed capture is a version 2 record");
    if let Some(missing) = MissingCapture::recognize(REPLAY_VERDICT) {
        panic!("the capture is committed but its verdict record is not: {missing}");
    }

    // The capture is of the committed workbook; one of other bytes awaits a new capture. The
    // worker that took it is provenance (its tool revision), not a condition.
    let workbook = format!("sha256:{}", sha256_hex(REPLAY_WORKBOOK));
    if capture.source != workbook {
        note!(
            "ACCEPT-NOT-RUN: {CASE}: awaiting capture: the committed capture is of {}, and replay.xlsx as committed is {workbook}",
            capture.source
        );
        return;
    }
    note!(
        "ACCEPT-NOTE: {CASE}: the capture was taken by {}",
        session.tool
    );

    // The replay on this platform, through the library route `cargo xtask capture --verdict` took
    // on Windows when it wrote the verdict record: identical bytes.
    let report = file_cache_replay(&capture, &VerdictPolicy::default())
        .expect("the committed capture replays");
    let encoded = report.encode();
    assert!(
        encoded.as_bytes() == REPLAY_VERDICT,
        "the verdict record replayed here differs from the committed one\n--- replayed\n{encoded}\n--- committed\n{}",
        String::from_utf8_lossy(REPLAY_VERDICT)
    );

    // What the record says, read without the library.
    let raw: Value = serde_json::from_slice(REPLAY_CAPTURE).expect("capture JSON");
    let key = |entry: &Value| {
        (
            entry["sheet"].as_str().unwrap_or_default().to_owned(),
            entry["row"].as_u64().unwrap_or_default(),
            entry["column"].as_u64().unwrap_or_default(),
        )
    };
    let list = |name: &str| raw[name].as_array().cloned().unwrap_or_default();
    let formulas = list("formulas");
    let observed: std::collections::BTreeMap<_, Value> = list("observations")
        .iter()
        .map(|entry| (key(entry), entry["excel_raw"].clone()))
        .collect();
    let saved = list("saved");
    let equal = saved
        .iter()
        .filter(|entry| {
            observed
                .get(&key(entry))
                .is_some_and(|raw| saved_equals_raw(&entry["excel_saved_cache"], raw))
        })
        .count() as u64;
    let (denominator, saved_count) = (formulas.len() as u64, saved.len() as u64);
    assert_eq!(raw["acquisition"]["kind"], "live_capture");
    assert!(denominator > 0, "the capture lists formula cells");
    let replayed: Value = serde_json::from_str(&encoded).expect("report JSON");
    assert_counts(
        &replayed,
        json!({
            "denominator": denominator, "calculated": 0, "matched": 0, "mismatched": 0,
            "not_run": 0, "strict_certified": 0, "uncertified": 0, "unproven": 0, "stale": 0,
            "file_cached": saved_count, "not_modeled": 0, "failed": 0,
            "missing": denominator - saved_count,
            "not_calculated_equal_to_expectation": equal, "outside_inventory": 0,
            "evaluator_calls": 0,
            "calculated_without_value_check": 0
        }),
        "the file-cache replay of the capture",
    );
    assert_eq!(
        replayed["captured"],
        json!({"revision": session.revision, "context": session.context})
    );
    assert_eq!(
        replayed["capture_failures"]["total"],
        json!(session.failures.len()),
        "the replayed report counts every failure the capture records"
    );
    // `cargo xtask capture` refuses a capture with an isolation failure, so a committed one has
    // none.
    let isolation: Vec<_> = session.failures_at(STAGE_ISOLATION).collect();
    assert!(
        isolation.is_empty(),
        "the committed capture records isolation failures: {isolation:?}"
    );

    // The context fingerprint the capture recorded is the one its own settings give.
    let settings = &raw["environment"]["settings"];
    let setting = |name: &str| {
        settings[name]
            .as_str()
            .unwrap_or_else(|| panic!("the capture's settings have no {name}"))
    };
    let context = CalculationContext {
        date1904: setting("date1904") == "true",
        iteration: setting("iteration") == "true",
        max_iterations: setting("max_iterations").parse().expect("max_iterations"),
        max_change: f64::from_bits(
            u64::from_str_radix(setting("max_change"), 16).expect("max_change bits"),
        ),
        precision_as_displayed: setting("precision_as_displayed") == "true",
    };
    assert_eq!(session.context, context.fingerprint());

    // The COM add-ins the capture lists as connected (items separated by ';', a ';' within a
    // name escaped as %3B), counted here without the library: the replayed report counts the same
    // number, so a capture taken with an add-in connected is never reported as an isolated one.
    let connected = setting("com_add_ins_connected");
    let connected_count = connected.split(';').filter(|item| !item.is_empty()).count();
    assert_eq!(
        replayed["com_add_ins_connected"],
        json!(connected_count),
        "the replayed report counts the COM add-ins the capture lists as connected ({connected:?})"
    );

    note!(
        "ACCEPT-NOTE: {CASE}: the committed capture ({} formula cells, Excel {}, {} capture failures, {connected_count} COM add-ins connected: {connected:?}) replays to the committed verdict record byte for byte: calculated 0 of {denominator}, file_cached {saved_count}, missing {}, not_calculated_equal_to_expectation {equal}, com_add_ins_connected {}",
        denominator,
        raw["environment"]["excel_build"]
            .as_str()
            .unwrap_or("unknown"),
        session.failures.len(),
        denominator - saved_count,
        replayed["com_add_ins_connected"]
    );
}

const HOLES_CAPTURE: &[u8] = include_bytes!("fixtures/shared_ref_with_holes/capture.json");
const HOLES_RESULTS: &[u8] = include_bytes!("fixtures/shared_ref_with_holes/results.json");
const HOLES_EVALUATIONS: &[u8] = include_bytes!("fixtures/shared_ref_with_holes/evaluations.json");
const HOLES_SECOND_CAPTURE: &[u8] =
    include_bytes!("fixtures/shared_ref_with_holes/capture_second_group.json");
const HOLES_SECOND_RESULTS: &[u8] =
    include_bytes!("fixtures/shared_ref_with_holes/results_second_group.json");
const HOLES_SECOND_EVALUATIONS: &[u8] =
    include_bytes!("fixtures/shared_ref_with_holes/evaluations_second_group.json");

/// The file-cache replay of the capture record in `bytes`, decoded and replayed as `cargo xtask
/// capture` decodes and replays the record it has written before it writes `--out`.
fn replayed(bytes: &[u8]) -> Result<Value, JudgeError> {
    let capture = CaptureRecord::decode(bytes).expect("the capture fixture decodes");
    let report = file_cache_replay(&capture, &VerdictPolicy::default())?;
    Ok(serde_json::from_str(&report.encode()).expect("the encoded report is JSON"))
}

/// The cells of a fixture's formula inventory, read without the library, as
/// `<sheet>!<A1> <kind>[(<group>)]` in the record's order (columns A to Z only).
fn listed(fixture: &Value) -> Vec<String> {
    fixture["formulas"]
        .as_array()
        .expect("formulas")
        .iter()
        .map(|formula| {
            let column = u8::try_from(formula["column"].as_u64().expect("column"))
                .expect("a column within A to Z");
            let mut line = format!(
                "{}!{}{} {}",
                text(formula, "sheet"),
                char::from(b'A' + column - 1),
                formula["row"].as_u64().expect("row"),
                text(formula, "kind")
            );
            if let Some(group) = formula["group"].as_u64() {
                line.push_str(&format!("({group})"));
            }
            line
        })
        .collect()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_61sj__shared_ref_with_holes_judged() {
    const CASE: &str = "dc_61sj__shared_ref_with_holes_judged";
    const CONTEXT: &str = "excel-calc-context/1;date1904=false;iteration=false;max_iterations=100;max_change=3f50624dd2f1a9fc;precision_as_displayed=false";
    let policy = VerdictPolicy::default();

    // The hand-built record, read without the library: shared group 0's ref C1:C8 holds six cells
    // of the group, the ordinary formula C4 and C7, which holds no formula (the record lists no
    // cell there); seven formula cells in all.
    let fixture: Value = serde_json::from_slice(HOLES_CAPTURE).expect("fixture JSON");
    assert_eq!(
        fixture["shared_ranges"],
        json!([{"sheet": "Sheet1", "group": 0, "ref": "C1:C8"}])
    );
    assert_eq!(
        listed(&fixture),
        [
            "Sheet1!C1 shared_master(0)",
            "Sheet1!C2 shared_child(0)",
            "Sheet1!C3 shared_child(0)",
            "Sheet1!C4 ordinary",
            "Sheet1!C5 shared_child(0)",
            "Sheet1!C6 shared_child(0)",
            "Sheet1!C8 shared_child(0)",
        ],
        "the fixture's inventory"
    );
    let expected_strata = json!({
        "visible_sheet": 7, "hidden_sheet": 0, "very_hidden_sheet": 0, "ordinary": 1,
        "shared_master": 1, "shared_child": 5,
        "legacy_array_anchor": 0, "legacy_array_member": 0, "observed": 7, "error_expected": 0
    });

    // The route `cargo xtask capture` takes before it writes a record: the file-cache replay of
    // the record. It is judged, not refused, with all seven formula cells in the denominator and
    // none calculated.
    let replay = replayed(HOLES_CAPTURE).expect("a ref with holes is judged in the replay");
    assert_eq!(
        replay["headline"]["calculated_of_denominator"],
        json!([0, 7])
    );
    assert_counts(
        &replay,
        json!({
            "denominator": 7, "calculated": 0, "matched": 0, "mismatched": 0, "not_run": 0,
            "strict_certified": 0, "uncertified": 0, "unproven": 0, "stale": 0,
            "file_cached": 7, "not_modeled": 0, "failed": 0, "missing": 0,
            "not_calculated_equal_to_expectation": 7, "outside_inventory": 0,
            "evaluator_calls": 0,
            "calculated_without_value_check": 0
        }),
        "file-cache replay",
    );
    assert_eq!(
        replay["strata"], expected_strata,
        "file-cache replay: strata"
    );
    assert_cells(
        &replay,
        &[
            "Sheet1!C1 shared_master(0) visible file_cached equal=true",
            "Sheet1!C2 shared_child(0) visible file_cached equal=true",
            "Sheet1!C3 shared_child(0) visible file_cached equal=true",
            "Sheet1!C4 ordinary visible file_cached equal=true",
            "Sheet1!C5 shared_child(0) visible file_cached equal=true",
            "Sheet1!C6 shared_child(0) visible file_cached equal=true",
            "Sheet1!C8 shared_child(0) visible file_cached equal=true",
        ],
        "file-cache replay",
    );
    assert_eq!(
        replay["captured"],
        json!({"revision": 0, "context": CONTEXT})
    );
    assert_eq!(replay["capture_failures"]["total"], 0);
    note!(
        "ACCEPT-NOTE: {CASE}: shared ref Sheet1!C1:C8 listing 6 cells of group 0, the ordinary formula C4 and no formula at C7: the file-cache replay `cargo xtask capture` runs judges the record (denominator 7, calculated 0, file_cached 7, not_calculated_equal_to_expectation 7)"
    );

    // Every formula cell calculated, with an evaluation of each: all seven in the denominator,
    // the ordinary C4 among them, each matched.
    let calculated = report(HOLES_CAPTURE, HOLES_RESULTS, HOLES_EVALUATIONS, &policy);
    assert_eq!(
        calculated["headline"]["calculated_of_denominator"],
        json!([7, 7])
    );
    assert_eq!(
        calculated["headline"]["matched_of_calculated"],
        json!([7, 7])
    );
    assert_counts(
        &calculated,
        json!({
            "denominator": 7, "calculated": 7, "matched": 7, "mismatched": 0, "not_run": 0,
            "strict_certified": 0, "uncertified": 7, "unproven": 0, "stale": 0,
            "file_cached": 0, "not_modeled": 0, "failed": 0, "missing": 0,
            "not_calculated_equal_to_expectation": 0, "outside_inventory": 0,
            "evaluator_calls": 7,
            "calculated_without_value_check": 7
        }),
        "calculated results",
    );
    assert_eq!(
        calculated["strata"], expected_strata,
        "calculated results: strata"
    );
    assert_cells(
        &calculated,
        &[
            "Sheet1!C1 shared_master(0) visible matched uncertified/no_exact_target",
            "Sheet1!C2 shared_child(0) visible matched uncertified/no_exact_target",
            "Sheet1!C3 shared_child(0) visible matched uncertified/no_exact_target",
            "Sheet1!C4 ordinary visible matched uncertified/no_exact_target",
            "Sheet1!C5 shared_child(0) visible matched uncertified/no_exact_target",
            "Sheet1!C6 shared_child(0) visible matched uncertified/no_exact_target",
            "Sheet1!C8 shared_child(0) visible matched uncertified/no_exact_target",
        ],
        "calculated results",
    );
    note!(
        "ACCEPT-NOTE: {CASE}: the same record judged against seven calculated results with an evaluation of each: denominator 7 (shared master 1, shared children 5, ordinary 1), calculated 7, matched 7"
    );

    // Variant: C4 is instead the master of a second shared group (ref C4:D4, with the child D4),
    // so group 0's ref C1:C8 also covers a member of group 1. Judged in the replay and against
    // calculated results, with all eight formula cells in the denominator, each once.
    let second: Value = serde_json::from_slice(HOLES_SECOND_CAPTURE).expect("fixture JSON");
    assert_eq!(
        second["shared_ranges"],
        json!([
            {"sheet": "Sheet1", "group": 0, "ref": "C1:C8"},
            {"sheet": "Sheet1", "group": 1, "ref": "C4:D4"}
        ])
    );
    assert_eq!(
        listed(&second),
        [
            "Sheet1!C1 shared_master(0)",
            "Sheet1!C2 shared_child(0)",
            "Sheet1!C3 shared_child(0)",
            "Sheet1!C4 shared_master(1)",
            "Sheet1!D4 shared_child(1)",
            "Sheet1!C5 shared_child(0)",
            "Sheet1!C6 shared_child(0)",
            "Sheet1!C8 shared_child(0)",
        ],
        "the second-group fixture's inventory"
    );
    let second_strata = json!({
        "visible_sheet": 8, "hidden_sheet": 0, "very_hidden_sheet": 0, "ordinary": 0,
        "shared_master": 2, "shared_child": 6,
        "legacy_array_anchor": 0, "legacy_array_member": 0, "observed": 8, "error_expected": 0
    });
    let second_replay =
        replayed(HOLES_SECOND_CAPTURE).expect("a ref covering another group is judged");
    assert_eq!(
        second_replay["headline"]["calculated_of_denominator"],
        json!([0, 8])
    );
    assert_counts(
        &second_replay,
        json!({
            "denominator": 8, "calculated": 0, "matched": 0, "mismatched": 0, "not_run": 0,
            "strict_certified": 0, "uncertified": 0, "unproven": 0, "stale": 0,
            "file_cached": 8, "not_modeled": 0, "failed": 0, "missing": 0,
            "not_calculated_equal_to_expectation": 8, "outside_inventory": 0,
            "evaluator_calls": 0,
            "calculated_without_value_check": 0
        }),
        "second-group replay",
    );
    assert_eq!(
        second_replay["strata"], second_strata,
        "second-group replay: strata"
    );
    assert_cells(
        &second_replay,
        &[
            "Sheet1!C1 shared_master(0) visible file_cached equal=true",
            "Sheet1!C2 shared_child(0) visible file_cached equal=true",
            "Sheet1!C3 shared_child(0) visible file_cached equal=true",
            "Sheet1!C4 shared_master(1) visible file_cached equal=true",
            "Sheet1!D4 shared_child(1) visible file_cached equal=true",
            "Sheet1!C5 shared_child(0) visible file_cached equal=true",
            "Sheet1!C6 shared_child(0) visible file_cached equal=true",
            "Sheet1!C8 shared_child(0) visible file_cached equal=true",
        ],
        "second-group replay",
    );
    let second_calculated = report(
        HOLES_SECOND_CAPTURE,
        HOLES_SECOND_RESULTS,
        HOLES_SECOND_EVALUATIONS,
        &policy,
    );
    assert_counts(
        &second_calculated,
        json!({
            "denominator": 8, "calculated": 8, "matched": 8, "mismatched": 0, "not_run": 0,
            "strict_certified": 0, "uncertified": 8, "unproven": 0, "stale": 0,
            "file_cached": 0, "not_modeled": 0, "failed": 0, "missing": 0,
            "not_calculated_equal_to_expectation": 0, "outside_inventory": 0,
            "evaluator_calls": 8,
            "calculated_without_value_check": 8
        }),
        "second-group calculated results",
    );
    assert_eq!(
        second_calculated["strata"], second_strata,
        "second-group calculated results: strata"
    );
    assert_cells(
        &second_calculated,
        &[
            "Sheet1!C1 shared_master(0) visible matched uncertified/no_exact_target",
            "Sheet1!C2 shared_child(0) visible matched uncertified/no_exact_target",
            "Sheet1!C3 shared_child(0) visible matched uncertified/no_exact_target",
            "Sheet1!C4 shared_master(1) visible matched uncertified/no_exact_target",
            "Sheet1!D4 shared_child(1) visible matched uncertified/no_exact_target",
            "Sheet1!C5 shared_child(0) visible matched uncertified/no_exact_target",
            "Sheet1!C6 shared_child(0) visible matched uncertified/no_exact_target",
            "Sheet1!C8 shared_child(0) visible matched uncertified/no_exact_target",
        ],
        "second-group calculated results",
    );
    note!(
        "ACCEPT-NOTE: {CASE}: variant whose ref C1:C8 also covers C4, the master of group 1 (ref C4:D4): replay denominator 8, file_cached 8; calculated 8 of 8, matched 8 (shared masters 2, shared children 6)"
    );

    // Variant: the same record with group 0's ref declared C1:C6, so its member C8 lies outside
    // its ref. Still refused, by the replay and against calculated results, naming the cell.
    let source = std::str::from_utf8(HOLES_CAPTURE).expect("UTF-8 fixture");
    assert_eq!(source.matches("\"ref\": \"C1:C8\"").count(), 1);
    let narrowed = source.replace("\"ref\": \"C1:C8\"", "\"ref\": \"C1:C6\"");
    let outside = JudgeError::Inconsistent {
        message:
            "Sheet1!C8 belongs to shared group 0 and lies outside the group's range Sheet1!C1:C6"
                .to_owned(),
    };
    assert_eq!(
        replayed(narrowed.as_bytes()).expect_err("a member outside its ref is refused"),
        outside
    );
    assert_eq!(
        judged(
            narrowed.as_bytes(),
            HOLES_RESULTS,
            HOLES_EVALUATIONS,
            &policy
        )
        .expect_err("a member outside its ref is refused"),
        outside
    );
    note!("ACCEPT-NOTE: {CASE}: variant with group 0's ref narrowed to C1:C6: refused ({outside})");
}
