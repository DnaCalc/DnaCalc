//! Unit tests of the corpus summary (`corpus_summary.rs`): the counts of one workbook's triage,
//! the grouping of its failing cells by root cause, and the totals and causes across workbooks.
//! The triage documents here are authored to exercise each rule; they are not Excel's values.

use super::*;
use serde_json::json;

fn number(decimal: f64) -> Value {
    json!({ "number": format!("{:016x}", decimal.to_bits()), "decimal": format!("{decimal:?}") })
}

fn call(spelling: &str, notation: &str) -> Value {
    json!({ "spelling": spelling, "notation": notation, "synthetic": false })
}

fn difference(
    function: Value,
    expected: Value,
    actual: Value,
    ulp: Option<u64>,
    agrees: Option<bool>,
) -> Value {
    json!({
        "function": function,
        "expected": expected,
        "actual": actual,
        "ulp": ulp,
        "agrees_to_15_significant_digits": agrees,
    })
}

/// A triage cell on sheet S.
fn cell(at: &str, verdict: &str, class: &str, key: &str, role: Option<&str>) -> Value {
    let mut cell =
        json!({ "sheet": "S", "cell": at, "verdict": verdict, "class": class, "key": key });
    if let Some(role) = role {
        cell["role"] = json!(role);
    }
    cell
}

fn with(mut cell: Value, member: &str, value: Value) -> Value {
    cell[member] = value;
    cell
}

/// Nine cells: one matched, a refusal root with a reader, a 1-ulp SUM root with a reader, an
/// operator root returning a number where Excel has an error, a mismatch with no call, a sheet
/// not loaded and a stale cell.
fn mixed_triage() -> Value {
    json!({
        "run": {
            "workbook_failure": null,
            "wall_time_s": { "open": 0.5, "step": 2.0, "project": 0.1, "judge": 0.25 },
        },
        "cells": [
            cell("A1", "matched", "matched", "matched", None),
            with(cell("A2", "not_modeled", "not-modelled", "NOT", Some("root")), "owner", json!("exactness dc-uar")),
            with(
                with(cell("A3", "not_modeled", "not-modelled", "NOT", Some("reads_root")), "owner", json!("exactness dc-uar")),
                "root",
                json!("S!A2"),
            ),
            with(
                cell("B1", "mismatched", "kernel-bits", "SUM", Some("root")),
                "difference",
                difference(call("SUM", "Call"), number(1.1), number(f64::from_bits(1.1f64.to_bits() + 1)), Some(1), Some(true)),
            ),
            with(
                with(
                    cell("B2", "mismatched", "kernel-bits", "SUM", Some("reads_root")),
                    "root",
                    json!("S!B1"),
                ),
                "difference",
                difference(call("+", "Infix"), number(2.2), number(2.3), Some(9), Some(false)),
            ),
            with(
                cell("B3", "mismatched", "kernel-value", "Infix +", Some("root")),
                "difference",
                difference(call("+", "Infix"), json!({ "error": "Div0" }), number(3.0), None, None),
            ),
            with(
                cell("D1", "mismatched", "formula", "value differs with no function call", Some("root")),
                "difference",
                difference(Value::Null, number(1.0), number(1.5), Some(1000), Some(false)),
            ),
            cell("C1", "missing", "doc", "sheet not loaded", Some("root")),
            cell("C2", "stale", "engine", "stale: Dirty", Some("root")),
        ],
    })
}

fn refusal(owner: &str, reason: &str) -> Cause {
    Cause::Refusal {
        owner: owner.to_owned(),
        reason: reason.to_owned(),
    }
}

fn mismatch(function: &str, shape: &str) -> Cause {
    Cause::Mismatch {
        function: function.to_owned(),
        shape: shape.to_owned(),
    }
}

fn other(class: &str, key: &str) -> Cause {
    Cause::Other {
        class: class.to_owned(),
        key: key.to_owned(),
    }
}

fn count(roots: u64, downstream: u64, whole_workbook: u64) -> CauseCount {
    CauseCount {
        roots,
        downstream,
        whole_workbook,
    }
}

#[test]
fn verdicts_fall_into_pass_mismatched_refused_and_failed_or_missing() {
    assert_eq!(bucket("matched"), Bucket::Pass);
    assert_eq!(bucket("mismatched"), Bucket::Mismatched);
    assert_eq!(bucket("not_modeled"), Bucket::Refused);
    for verdict in [
        "not_run",
        "unproven",
        "stale",
        "file_cached",
        "failed",
        "missing",
    ] {
        assert_eq!(bucket(verdict), Bucket::FailedOrMissing, "{verdict}");
    }
}

/// A refused formula is a failure: the counts of one workbook, and every failing cell counted
/// under its root's cause, the causes adding up to the fail count.
#[test]
fn one_workbook_is_counted_and_grouped_by_root_cause() {
    let compared = read_triage(&mixed_triage()).unwrap();
    let counts = &compared.counts;
    assert_eq!(
        (
            counts.formula_cells,
            counts.pass,
            counts.mismatched,
            counts.refused,
            counts.failed_or_missing,
            counts.fail()
        ),
        (9, 1, 4, 2, 2, 8)
    );
    assert_eq!(
        counts.failed_or_missing_by_verdict,
        BTreeMap::from([("missing".to_owned(), 1), ("stale".to_owned(), 1)])
    );
    let expected = BTreeMap::from([
        (refusal("exactness dc-uar", "not-modelled NOT"), count(1, 1, 0)),
        // B2's own difference is its + call's, but it is counted under its root, B1.
        (mismatch("SUM", "number, 1 ulp apart"), count(1, 1, 0)),
        (
            mismatch("Infix +", "error Div0 expected, number returned"),
            count(1, 0, 0),
        ),
        (
            mismatch("no call", "number, differs within 15 significant digits"),
            count(1, 0, 0),
        ),
        (other("doc", "sheet not loaded"), count(1, 0, 0)),
        (other("engine", "stale: Dirty"), count(1, 0, 0)),
    ]);
    assert_eq!(compared.causes, expected);
    let cells: u64 = compared.causes.values().map(CauseCount::cells).sum();
    assert_eq!(cells, counts.fail());
    assert_eq!(
        compared.times,
        Times {
            open: Some(0.5),
            recalculation: Some(2.0),
            project: Some(0.1),
            judge: Some(0.25),
            ..Times::default()
        }
    );
    assert_eq!(compared.workbook_failure, None);
}

/// A run that failed as a whole (calc-doc refused the package): every cell fails with it, under
/// one cause, counted as whole-workbook cells, never as refusals of their own.
#[test]
fn a_whole_run_failure_is_one_cause_of_every_cell() {
    let triage = json!({
        "run": {
            "workbook_failure": { "class": "doc", "key": "Workbook:Cell:UnsupportedType", "message": "m" },
            "wall_time_s": { "open": 0.01, "step": null, "project": null, "judge": 0.0 },
        },
        "cells": [
            cell("A1", "missing", "doc", "Workbook:Cell:UnsupportedType", Some("workbook")),
            cell("A2", "missing", "doc", "Workbook:Cell:UnsupportedType", Some("workbook")),
            cell("A3", "not_modeled", "doc", "Workbook:Cell:UnsupportedType", Some("workbook")),
        ],
    });
    let compared = read_triage(&triage).unwrap();
    assert_eq!(
        (
            compared.counts.pass,
            compared.counts.refused,
            compared.counts.failed_or_missing
        ),
        (0, 1, 2)
    );
    assert_eq!(
        compared.causes,
        BTreeMap::from([(
            other("doc", "Workbook:Cell:UnsupportedType"),
            count(0, 0, 3)
        )])
    );
    assert_eq!(
        compared.workbook_failure.as_deref(),
        Some("doc Workbook:Cell:UnsupportedType")
    );
    assert_eq!(compared.times.recalculation, None);
}

/// dc-ylr: a match that read another workbook's link caches is neither pass nor fail; a mismatch
/// that read them is a fail like any other.
#[test]
fn a_match_from_external_link_caches_is_counted_apart_from_pass() {
    let triage = json!({
        "run": {},
        "cells": [
            cell("A1", "matched", "matched", "matched", None),
            with(cell("A2", "matched", "matched", "matched", None), "external_links", json!(true)),
            with(
                with(cell("A3", "mismatched", "formula", "+", Some("root")), "external_links", json!(true)),
                "difference",
                difference(call("+", "Infix"), number(1.0), number(2.0), None, Some(false)),
            ),
        ],
    });
    let compared = read_triage(&triage).unwrap();
    let c = &compared.counts;
    assert_eq!(
        (c.formula_cells, c.pass, c.external_links, c.mismatched, c.fail()),
        (3, 1, 1, 1, 1)
    );
    let summary = Summary {
        run: json!({}),
        workbooks: vec![Workbook {
            name: "w.xlsx".to_owned(),
            sha256: None,
            capture: None,
            outcome: Ok(compared),
            times: Times::default(),
        }],
    };
    let markdown = summary.markdown();
    assert!(
        markdown.contains("pass 1; external links 1; fail 1 (mismatched 1"),
        "{markdown}"
    );
    assert_eq!(summary.json()["totals"]["counts"]["external_links"], 1);
}

#[test]
fn a_triage_naming_an_unlisted_root_is_refused() {
    let triage = json!({
        "run": {},
        "cells": [with(cell("A1", "not_modeled", "not-modelled", "NOT", Some("reads_root")), "root", json!("S!Z9"))],
    });
    let error = read_triage(&triage).unwrap_err();
    assert!(error.contains("S!Z9"), "{error}");
    assert!(read_triage(&json!({})).is_err());
}

#[test]
fn mismatch_shapes_name_the_kinds_and_the_distance() {
    let shape = |expected: Value, actual: Value, ulp: Option<u64>, agrees: Option<bool>| {
        mismatch_shape(Some(&difference(
            Value::Null,
            expected,
            actual,
            ulp,
            agrees,
        )))
    };
    assert_eq!(
        shape(number(1.0), number(1.0), Some(1), Some(true)),
        "number, 1 ulp apart"
    );
    assert_eq!(
        shape(number(1.0), number(1.0), Some(16), Some(true)),
        "number, 2 to 16 ulp apart"
    );
    assert_eq!(
        shape(number(1.0), number(1.0), Some(17), Some(true)),
        "number, equal to 15 significant digits"
    );
    assert_eq!(
        shape(number(1.0), number(2.0), Some(17), Some(false)),
        "number, differs within 15 significant digits"
    );
    assert_eq!(
        shape(
            json!({ "text": "a" }),
            json!({ "logical": true }),
            None,
            None
        ),
        "text expected, logical returned"
    );
    assert_eq!(mismatch_shape(None), "unknown");
    assert_eq!(mismatch_function(None), "no evaluator evidence");
    assert_eq!(
        mismatch_function(Some(&json!({ "function": call("NOT", "Call") }))),
        "NOT"
    );
    assert_eq!(
        mismatch_function(Some(&json!({ "function": call("%", "Postfix") }))),
        "Postfix %"
    );
}

/// Two compared workbooks and one that was not compared: the totals count only the compared
/// workbooks' cells, the pass rate is over all their formula cells, a cause occurring in both
/// is merged with two workbooks, and the causes add up to the total fail count.
#[test]
fn the_corpus_totals_and_causes_merge_the_workbooks() {
    let first = read_triage(&mixed_triage()).unwrap();
    let second = read_triage(&json!({
        "run": { "wall_time_s": { "open": 1.0, "step": 1.0, "project": 0.0, "judge": 1.0 } },
        "cells": [
            cell("A1", "matched", "matched", "matched", None),
            cell("A2", "matched", "matched", "matched", None),
            cell("A3", "matched", "matched", "matched", None),
            with(cell("A4", "not_modeled", "not-modelled", "NOT", Some("root")), "owner", json!("exactness dc-uar")),
        ],
    }))
    .unwrap();
    let summary = Summary {
        run: json!({ "command": "cargo xtask corpus test" }),
        workbooks: vec![
            Workbook {
                name: "one.xlsx".to_owned(),
                sha256: Some("aa".to_owned()),
                capture: Some("cache build 16.0.1".to_owned()),
                outcome: Ok(first),
                times: Times {
                    compare_process: Some(4.0),
                    ..Times::default()
                },
            },
            Workbook {
                name: "two.xlsx".to_owned(),
                sha256: Some("bb".to_owned()),
                capture: None,
                outcome: Ok(second),
                times: Times {
                    compare_process: Some(3.0),
                    capture: Some(60.0),
                    ..Times::default()
                },
            },
            Workbook {
                name: "three.xlsx".to_owned(),
                sha256: None,
                capture: None,
                outcome: Err(NotCompared {
                    stage: "capture".to_owned(),
                    reason: "Excel refused".to_owned(),
                }),
                times: Times::default(),
            },
        ],
    };
    let totals = summary.totals();
    assert_eq!(
        (totals.workbooks, totals.compared, totals.not_compared),
        (3, 2, 1)
    );
    let c = &totals.counts;
    assert_eq!(
        (
            c.formula_cells,
            c.pass,
            c.fail(),
            c.mismatched,
            c.refused,
            c.failed_or_missing
        ),
        (13, 4, 9, 4, 3, 2)
    );
    // Rounded down: 100.00% only when every cell passed.
    assert_eq!(rate(c.pass, c.formula_cells), "30.76%");
    assert_eq!(rate(32_879, 32_880), "99.99%");
    assert_eq!(rate(5, 5), "100.00%");
    assert_eq!(rate(0, 7), "0.00%");
    assert_eq!(rate(0, 0), "n/a");
    assert_eq!(totals.times.open, Some(1.5));
    assert_eq!(totals.times.compare_process, Some(7.0));
    assert_eq!(totals.times.capture, Some(60.0));

    let causes = summary.causes();
    assert_eq!(
        causes[0],
        (
            refusal("exactness dc-uar", "not-modelled NOT"),
            CorpusCause {
                count: count(2, 1, 0),
                workbooks: BTreeSet::from(["one.xlsx".to_owned(), "two.xlsx".to_owned()]),
            }
        )
    );
    let cells: u64 = causes.iter().map(|(_, cause)| cause.count.cells()).sum();
    assert_eq!(cells, c.fail());

    let summary_json = summary.json();
    assert_eq!(summary_json["totals"]["counts"]["pass"], json!(4));
    assert_eq!(summary_json["totals"]["counts"]["fail"], json!(9));
    assert_eq!(
        summary_json["workbooks"][2]["status"],
        json!("not compared")
    );
    assert_eq!(
        summary_json["workbooks"][2]["not_compared"]["stage"],
        json!("capture")
    );
    assert_eq!(
        summary_json["workbooks"][1]["wall_time_s"]["capture"],
        json!(60.0)
    );
    assert_eq!(
        summary_json["root_causes"]["refusals"][0],
        json!({
            "owner": "exactness dc-uar",
            "reason": "not-modelled NOT",
            "roots": 2,
            "downstream": 1,
            "whole_workbook": 0,
            "cells": 3,
            "workbooks": 2,
        })
    );
    assert_eq!(
        summary_json["root_causes"]["mismatches"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    // No headline over calculated cells: the only rate is over all formula cells.
    let text = summary_json.to_string();
    assert!(!text.contains("calculated_rate"), "{text}");

    let markdown = summary.markdown();
    for expected in [
        "- formula cells (compared workbooks): 13; pass 4; external links 0; fail 9 (mismatched 4, refused 3, failed or missing 2)",
        "- pass rate over all formula cells: 30.76%",
        "| one.xlsx | 9 | 1 | 0 | 8 | 4 | 2 | 2 | 11.11% | 0.500 | 2.000 | 0.250 |",
        "| three.xlsx (not compared) |",
        "- three.xlsx: capture: Excel refused",
        "| exactness dc-uar | not-modelled NOT | 2 | 1 | 0 | 3 | 2 |",
        "| SUM | number, 1 ulp apart | 1 | 1 | 0 | 2 | 1 |",
        "- 1 workbook(s) not compared hold formula cells not counted above",
    ] {
        assert!(
            markdown.contains(expected),
            "{expected:?} not in:\n{markdown}"
        );
    }
}
