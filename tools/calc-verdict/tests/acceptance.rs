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
//! The evaluation records are authored here too: producing one from instrumented evaluator calls
//! is bead `dc-qguw`. What these cases show is that the verdict counts a calculated claim only
//! against that separate record, never against the result set's own claim.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_verdict::value::{NumericMode, NumericTargetId};
use calc_verdict::{CaptureRecord, EvaluationRecord, JudgeError, ResultSet, VerdictPolicy, judge};
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
fn assert_counts(report: &Value, expected: Value, what: &str) {
    assert_eq!(report["counts"], expected, "{what}: counts");
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
            "evaluator_calls": 7
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
            "evaluator_calls": 7
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
            "evaluator_calls": 0
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
            "evaluator_calls": 6
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
            "evaluator_calls": 7
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
        "shared_master": 1, "shared_child": 2, "observed": 7, "error_expected": 2
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
            "evaluator_calls": 8
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
            "evaluator_calls": 8
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
