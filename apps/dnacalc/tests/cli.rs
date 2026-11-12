//! The `dnacalc` command on repository fixtures (dc-icq4.1), through the library entry the binary
//! calls (`dnacalc::run`), so the tests drive the route the command runs.
//!
//! Native-only: reads fixtures from the checkout, writes files and uses a large stack.
#![cfg(not(target_arch = "wasm32"))]

use serde_json::Value;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn inbook(name: &str) -> String {
    root()
        .join("crates/calc-vba/tests/corpus/inbook")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// Runs the command on a big stack, as the binary does; the JSON document it printed (or null) and
/// its exit code.
fn dnacalc(args: &[&str]) -> (Value, u8, String) {
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let code = dnacalc::run(&args, &mut out, &mut err);
            let out = String::from_utf8(out).unwrap();
            let json = serde_json::from_str(&out).unwrap_or(Value::Null);
            (json, code, out + &String::from_utf8(err).unwrap())
        })
        .unwrap()
        .join()
        .unwrap()
}

fn temp(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("dnacalc-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name).to_string_lossy().into_owned()
}

fn count(report: &Value, key: &str) -> u64 {
    report[key].as_u64().unwrap_or_else(|| panic!("{key} in {report}"))
}

#[test]
fn help_names_every_command_and_option() {
    let (_, code, text) = dnacalc(&["--help"]);
    assert_eq!(code, 0);
    for word in ["recalc", "diff", "get", "set", "run", "--macro", "--macro-level", "--json", "--out"] {
        assert!(text.contains(word), "{word} in the help");
    }
}

/// The corpus runner's recorded count for the in-book dna-simple project (`baseline.json`, route
/// xlsm: 109 matched, nothing else) is reproduced by `diff` over the same workbook, whose cached
/// values Excel saved.
#[test]
fn diff_reproduces_the_vba_corpus_baseline_for_dna_simple() {
    let baseline: Value = serde_json::from_slice(
        &std::fs::read(root().join("crates/calc-vba/tests/corpus/baseline.json")).unwrap(),
    )
    .unwrap();
    let recorded = &baseline["common"]["dna-simple/xlsm"];
    let (report, code, _) = dnacalc(&["diff", &inbook("dna-simple.xlsm"), "--macro-level", "safe", "--json"]);
    assert_eq!(code, 0, "{report}");
    for key in ["matched", "mismatched", "refused", "missing"] {
        assert_eq!(count(&report, key), recorded[key].as_u64().unwrap(), "{key}");
    }
    assert_eq!(count(&report, "formula_cells"), 109);
    assert_eq!(report["pass_rate"], 1.0);
    assert_eq!(report["cells"].as_array().unwrap().len(), 109);
}

/// With the macro level off (the default) the project does not run, so its functions are
/// unknown to the engine and every cell is a counted refusal, never a pass.
#[test]
fn diff_with_macros_off_refuses_every_project_function_cell() {
    let (report, code, _) = dnacalc(&["diff", &inbook("dna-simple.xlsm"), "--json"]);
    assert_eq!(code, 1);
    assert_eq!(count(&report, "matched"), 0);
    assert_eq!(count(&report, "refused"), 109);
    assert_eq!(report["macros"]["ran"], false);
    assert_eq!(report["pass_rate"], 0.0);
    assert_eq!(count(&report, "fail"), 109);
}

/// A differing cell is reported with both values and the comparison's class: dna-examples holds
/// `ThisWorkbook.Name`, which Excel cached as the name the file had when it was saved.
#[test]
fn diff_reports_a_differing_cell_with_both_values() {
    let (report, code, _) = dnacalc(&["diff", &inbook("dna-examples.xlsm"), "--macro-level", "safe", "--json"]);
    assert_eq!(code, 1);
    assert_eq!(count(&report, "mismatched"), 1);
    assert_eq!(count(&report, "matched") + count(&report, "mismatched"), count(&report, "formula_cells"));
    let row = report["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["status"] == "mismatched")
        .unwrap();
    assert_eq!(row["cached"]["value"], "Book30");
    assert_eq!(row["calculated"]["value"], "dna-examples.xlsm");
    assert_eq!(row["detail"], "Text");
}

#[test]
fn run_calls_a_project_function_under_each_macro_level() {
    let book = inbook("dna-simple.xlsm");
    let (report, code, _) = dnacalc(&["run", &book, "--macro", "DnaSimple.SimpleNeg", "--arg", "4", "--macro-level", "safe", "--json"]);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["result"]["type"], "number");
    assert_eq!(report["result"]["value"], -4.0);
    let (report, code, _) = dnacalc(&["run", &book, "--macro", "DnaSimple.SimpleUpper", "--arg", "abc", "--macro-level", "unguarded", "--json"]);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["result"]["value"], "ABC");
    assert_eq!(report["macros"]["policy"], "full");
    // The default level is off: a typed refusal, exit 2, and nothing runs.
    let (report, code, _) = dnacalc(&["run", &book, "--macro", "DnaSimple.SimpleNeg", "--arg", "4", "--json"]);
    assert_eq!(code, 2);
    assert!(report["error"].as_str().unwrap().contains("macro level is off"));
    // A function the project does not have is refused, not invented.
    let (report, code, _) = dnacalc(&["run", &book, "--macro", "DnaSimple.NoSuchProc", "--macro-level", "safe", "--json"]);
    assert_eq!(code, 2, "{report}");
}

#[test]
fn set_recalculates_dependents_and_saves_a_file_diff_reads_back() {
    let book = inbook("dna-simple.xlsm");
    let out = temp("set.xlsx");
    let (report, code, _) = dnacalc(&["set", &book, "A1", "=SimpleNeg(10)", "--macro-level", "safe", "--out", &out, "--json"]);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["cells"][0]["value"]["value"], -10.0);
    assert_eq!(report["cells"][0]["formula"], "SimpleNeg(10)");
    assert!(report["saved"]["bytes"].as_u64().unwrap() > 0);
    // The saved file carries the edited formula and its calculated value as the cache.
    let (read, code, _) = dnacalc(&["get", &out, "Sheet1!A1", "--macro-level", "safe", "--json"]);
    assert_eq!(code, 0, "{read}");
    assert_eq!(read["cells"][0]["cached"]["value"], -10.0);
    assert_eq!(read["cells"][0]["value"]["value"], -10.0);
}

#[test]
fn get_gives_formula_cached_and_calculated_values() {
    let (report, code, _) = dnacalc(&["get", &inbook("dna-simple.xlsm"), "A1", "Z99", "--macro-level", "safe", "--json"]);
    assert_eq!(code, 0);
    let a1 = &report["cells"][0];
    assert_eq!(a1["formula"], "SimpleNeg(2.5)");
    assert_eq!(a1["cached"]["value"], -2.5);
    assert_eq!(a1["value"]["value"], -2.5);
    assert_eq!(a1["provenance"], "calculated");
    assert_eq!(report["cells"][1]["provenance"], "empty");
}

#[test]
fn recalc_counts_and_saves() {
    let out = temp("recalc.xlsx");
    let (report, code, _) = dnacalc(&["recalc", &inbook("dna-simple.xlsm"), "--macro-level", "safe", "--out", &out, "--json"]);
    assert_eq!(code, 0, "{report}");
    assert_eq!(count(&report, "formula_cells"), 109);
    assert_eq!(count(&report, "calculated"), 109);
    assert!(Path::new(&out).metadata().unwrap().len() > 0);
}

#[test]
fn usage_errors_exit_two_and_text_mode_prints_lines() {
    assert_eq!(dnacalc(&["frobnicate", "x.xlsx"]).1, 2);
    assert_eq!(dnacalc(&["diff", "/nonexistent/book.xlsx"]).1, 2);
    assert_eq!(dnacalc(&["diff", &inbook("dna-simple.xlsm"), "--macro-level", "loud"]).1, 2);
    let (_, code, text) = dnacalc(&["diff", &inbook("dna-simple.xlsm"), "--macro-level", "safe"]);
    assert_eq!(code, 0);
    assert!(text.contains("matched: 109"), "{text}");
}

/// The corpus oracle's verdict for these two workbooks (`cargo xtask compare` against the
/// committed Excel capture: every formula cell calculated and matched Excel's answer) is
/// reproduced by `diff`, which compares with the cached values Excel saved in the same files.
/// A workbook whose cache is not Excel's own recalculation (a generated file with no cache, or
/// dc_y2o, whose circular chain Excel iterated past the saved values) is not a sample: `diff`
/// reports its missing and differing cells as such.
#[test]
fn diff_reproduces_the_oracle_verdict_on_a_corpus_sample() {
    for (krate, case) in [("calc-engine", "dc_10y"), ("calc-engine", "l3")] {
        let fixtures = root().join("crates").join(krate).join("tests");
        let capture: Value = serde_json::from_slice(
            &std::fs::read(fixtures.join("captures").join(case).join("oracle.capture.json")).unwrap(),
        )
        .unwrap();
        let formulas = capture["formulas"].as_array().unwrap().len() as u64;
        let book = fixtures.join("fixtures").join(case).join("oracle.xlsx");
        let (report, code, _) = dnacalc(&["diff", book.to_str().unwrap(), "--json"]);
        assert_eq!(code, 0, "{case}: {}", report["mismatched"]);
        assert_eq!(count(&report, "formula_cells"), formulas, "{case}");
        assert_eq!(count(&report, "matched"), formulas, "{case}");
        for key in ["mismatched", "refused", "missing", "failed"] {
            assert_eq!(count(&report, key), 0, "{case} {key}");
        }
    }
}

#[test]
fn diff_counts_a_workbook_without_cached_values_as_missing_not_passed() {
    let book = root().join("crates/calc-engine/tests/fixtures/eng15-overflow/oracle.xlsx");
    let (report, code, _) = dnacalc(&["diff", book.to_str().unwrap(), "--json"]);
    assert_eq!(code, 1);
    assert_eq!(count(&report, "matched"), 0);
    assert_eq!(count(&report, "missing"), count(&report, "formula_cells"));
    assert_eq!(report["pass_rate"], 0.0);
}
