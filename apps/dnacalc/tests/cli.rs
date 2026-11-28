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
fn version_is_the_workspace_version() {
    let (_, code, text) = dnacalc(&["--version"]);
    assert_eq!(code, 0);
    assert_eq!(text.trim(), concat!("dnacalc ", env!("CARGO_PKG_VERSION")));
}

#[test]
fn portability_reports_source_procedures_for_all_host_profiles() {
    let book = inbook("dna-simple.xlsm");
    let (report, code, text) =
        dnacalc(&["portability", &book, "--host", "all", "--json"]);
    assert_eq!(code, 0, "{text}");
    let profiles = report["profiles"].as_array().expect("one report per profile");
    assert_eq!(profiles.len(), 7);
    let linux = profiles.iter().find(|entry| entry["host"] == "linux").unwrap();
    assert!(
        linux["vba"]["procedures"].as_array().unwrap().iter().any(|entry| {
            entry["module"] == "DnaSimple"
                && entry["procedure"] == "SimpleNeg"
                && entry["status"] == "runs"
        }),
        "{linux}"
    );
}

#[test]
fn help_names_every_command_and_option() {
    let (_, code, text) = dnacalc(&["--help"]);
    assert_eq!(code, 0);
    for word in ["recalc", "diff", "get", "set", "run", "--macro", "--macro-level", "guarded", "--json", "--out"] {
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
    let (report, code, _) = dnacalc(&["diff", &inbook("dna-simple.xlsm"), "--macro-level", "guarded", "--json"]);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["macros"]["level"], "guarded");
    assert_eq!(report["macros"]["policy"], "guarded");
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
    let (report, code, _) = dnacalc(&["diff", &inbook("dna-examples.xlsm"), "--macro-level", "guarded", "--json"]);
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
    let (report, code, _) = dnacalc(&["run", &book, "--macro", "DnaSimple.SimpleNeg", "--arg", "4", "--macro-level", "guarded", "--json"]);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["result"]["type"], "number");
    assert_eq!(report["result"]["value"], -4.0);
    assert_eq!(report["macros"]["level"], "guarded");
    assert_eq!(report["macros"]["policy"], "guarded");
    // The former name `safe` remains accepted as an alias for `guarded` (dc-8ovd).
    let (report, code, _) = dnacalc(&["run", &book, "--macro", "DnaSimple.SimpleNeg", "--arg", "4", "--macro-level", "safe", "--json"]);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["result"]["value"], -4.0);
    assert_eq!(report["macros"]["level"], "guarded");
    assert_eq!(report["macros"]["policy"], "guarded");
    let (report, code, _) = dnacalc(&["run", &book, "--macro", "DnaSimple.SimpleUpper", "--arg", "abc", "--macro-level", "unguarded", "--json"]);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["result"]["value"], "ABC");
    assert_eq!(report["macros"]["policy"], "full");
    // The default level is off: a typed refusal, exit 2, and nothing runs.
    let (report, code, _) = dnacalc(&["run", &book, "--macro", "DnaSimple.SimpleNeg", "--arg", "4", "--json"]);
    assert_eq!(code, 2);
    assert!(report["error"].as_str().unwrap().contains("macro level is off"));
    // A function the project does not have is refused, not invented.
    let (report, code, _) = dnacalc(&["run", &book, "--macro", "DnaSimple.NoSuchProc", "--macro-level", "guarded", "--json"]);
    assert_eq!(code, 2, "{report}");
}

fn dialog_workbook(file: &str, source: &str) -> String {
    let path = temp(file);
    let bytes = calc_doc::source_vba_package(&[calc_doc::SourceModule {
        name: "Dialogs",
        kind: calc_doc::SourceModuleKind::Standard,
        source,
    }])
    .unwrap();
    std::fs::write(&path, bytes).unwrap();
    path
}

#[cfg(not(windows))]
#[test]
fn mounted_workbook_paths_round_trip_through_vba_file_io() {
    let path = dialog_workbook(
        "mount-persona.xlsm",
        r#"Public Function ReadPersona() As String
    Dim contents As String
    Open ThisWorkbook.Path & "\mount-persona.txt" For Input As #1
    Line Input #1, contents
    Close #1
    ReadPersona = ThisWorkbook.FullName & "|" & Application.PathSeparator & "|" & Application.DefaultFilePath & "|" & CurDir & "|" & contents
End Function

Public Function DeniedWrite() As String
    On Error Resume Next
    Open ThisWorkbook.Path & "\mount-persona-write.txt" For Output As #1
    DeniedWrite = CStr(Err.Number)
End Function
"#,
    );
    std::fs::write(temp("mount-persona.txt"), b"read through the mount\r\n").unwrap();
    let config = temp("mount-persona.toml");
    let folder = Path::new(&path).parent().unwrap();
    std::fs::write(
        &config,
        format!("[[mount]]\nwindows = 'D:\\Models'\nhost = '{}'\nmode = 'read'\n", folder.display()),
    )
    .unwrap();
    let (report, code, text) = dnacalc(&[
        "run", &path, "--mounts", &config, "--macro", "Dialogs.ReadPersona",
        "--macro-level", "guarded", "--json",
    ]);
    assert_eq!(code, 0, "{text}");
    assert_eq!(
        report["result"]["value"],
        r"D:\Models\mount-persona.xlsm|\|D:\Models|D:\Models|read through the mount",
    );
    let (report, code, text) = dnacalc(&[
        "run", &path, "--mounts", &config, "--macro", "Dialogs.DeniedWrite",
        "--macro-level", "guarded", "--json",
    ]);
    assert_eq!(code, 0, "{text}");
    assert_eq!(report["result"]["value"], "75");
    assert!(!folder.join("mount-persona-write.txt").exists());
    let (report, code, text) = dnacalc(&[
        "set", &path, "B1", r#"=CELL("filename",A1)"#, "C1", r#"=INFO("directory")"#,
        "--mounts", &config, "--json",
    ]);
    assert_eq!(code, 0, "{text}");
    let cells = report["cells"].as_array().unwrap();
    assert_eq!(cells[0]["value"]["value"], r"D:\Models\[mount-persona.xlsm]Sheet1");
    assert_eq!(cells[1]["value"]["value"], r"D:\Models\");
}

#[cfg(windows)]
#[test]
fn native_workbook_paths_use_the_windows_location() {
    let path = dialog_workbook("native-persona.xlsm", "Public Sub Unused()\r\nEnd Sub\r\n");
    let (report, code, text) = dnacalc(&[
        "set", &path, "B1", r#"=CELL("filename",A1)"#, "--json",
    ]);
    assert_eq!(code, 0, "{text}");
    let directory = Path::new(&path).parent().unwrap().display();
    assert_eq!(
        report["cells"][0]["value"]["value"],
        format!(r"{directory}\[native-persona.xlsm]Sheet1"),
    );
    let (_, code, text) = dnacalc(&["get", &path, "A1", "--mounts", "unused.toml", "--json"]);
    assert_eq!(code, 2, "{text}");
    assert!(text.contains("--mounts is available on macOS and Linux"), "{text}");
}

#[test]
fn headless_dialogs_use_explicit_answers_and_record_each_call() {
    let book = dialog_workbook(
        "dialog-answers.xlsm",
        "Public Function Reply() As String\r\n\
         Reply = CStr(MsgBox(\"Continue?\", 4)) & \"/\" & InputBox(\"Name?\", , \"Default\")\r\n\
         End Function\r\n",
    );
    let answers = temp("dialog-answers.json");
    std::fs::write(&answers, r#"{"msgbox":{"Continue?":6},"inputbox":{"Name?":"Alice"}}"#).unwrap();
    let (report, code, text) = dnacalc(&[
        "run", &book, "--macro", "Dialogs.Reply", "--macro-level", "guarded", "--answers", &answers, "--json",
    ]);
    assert_eq!(code, 0, "{text}");
    assert_eq!(report["result"]["value"], "6/Alice");
    assert_eq!(report["invocation"]["allowed"], 2);
    assert_eq!(report["invocation"]["denials"], serde_json::json!([]));
    let dialogs = report["invocation"]["dialogs"].as_array().unwrap();
    assert_eq!(dialogs.len(), 2);
    assert_eq!(dialogs[0]["member"], "MsgBox");
    assert_eq!(dialogs[0]["answer"], 6);
    assert_eq!(dialogs[1]["member"], "InputBox");
    assert_eq!(dialogs[1]["answer"], "Alice");

    let (all, code, text) = dnacalc(&[
        "run", &book, "--all", "--macro-level", "guarded", "--answers", &answers, "--json",
    ]);
    assert_eq!(code, 0, "{text}");
    assert_eq!(all["success"], 1);
    assert_eq!(all["invocations"][0]["dialogs"], report["invocation"]["dialogs"]);
}

#[test]
fn headless_dialogs_report_an_unanswered_call_even_when_vba_handles_it() {
    let book = dialog_workbook(
        "dialog-unanswered.xlsm",
        "Public Function Reply() As String\r\n\
         On Error Resume Next\r\n\
         Reply = InputBox(\"Unanswered?\")\r\n\
         End Function\r\n",
    );
    let (report, code, text) = dnacalc(&[
        "run", &book, "--macro", "Dialogs.Reply", "--macro-level", "guarded", "--json",
    ]);
    assert_eq!(code, 1, "{text}");
    assert_eq!(report["invocation"]["outcome"]["kind"], "not-available-here");
    assert_eq!(report["invocation"]["outcome"]["need"], "ui.dialog");
    assert_eq!(report["invocation"]["dialogs"][0]["member"], "InputBox");

    let (all, code, text) = dnacalc(&["run", &book, "--all", "--macro-level", "guarded", "--json"]);
    assert_eq!(code, 1, "{text}");
    assert_eq!(all["success"], 0);
    assert_eq!(all["refused"], 1);
    assert_eq!(all["invocations"][0]["outcome"], report["invocation"]["outcome"]);
}

#[test]
fn set_recalculates_dependents_and_saves_a_file_diff_reads_back() {
    let book = inbook("dna-simple.xlsm");
    let out = temp("set.xlsx");
    let (report, code, _) = dnacalc(&["set", &book, "A1", "=SimpleNeg(10)", "--macro-level", "guarded", "--out", &out, "--json"]);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["cells"][0]["value"]["value"], -10.0);
    assert_eq!(report["cells"][0]["formula"], "SimpleNeg(10)");
    assert!(report["saved"]["bytes"].as_u64().unwrap() > 0);
    // The saved file carries the edited formula and its calculated value as the cache.
    let (read, code, _) = dnacalc(&["get", &out, "Sheet1!A1", "--macro-level", "guarded", "--json"]);
    assert_eq!(code, 0, "{read}");
    assert_eq!(read["cells"][0]["cached"]["value"], -10.0);
    assert_eq!(read["cells"][0]["value"]["value"], -10.0);
}

#[test]
fn get_gives_formula_cached_and_calculated_values() {
    let (report, code, _) = dnacalc(&["get", &inbook("dna-simple.xlsm"), "A1", "Z99", "--macro-level", "guarded", "--json"]);
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
    let (report, code, _) = dnacalc(&["recalc", &inbook("dna-simple.xlsm"), "--macro-level", "guarded", "--out", &out, "--json"]);
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
    let (_, code, text) = dnacalc(&["diff", &inbook("dna-simple.xlsm"), "--macro-level", "guarded"]);
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

/// `run --all` gives one outcome per public entry, each in its own session, with the counts.
#[test]
fn run_all_reports_one_outcome_per_entry() {
    let book = root().join("crates/calc-doc/tests/fixtures/vba_udf_names/local.xlsm");
    let book = book.to_str().unwrap();
    let (report, code, _) = dnacalc(&["run", book, "--all", "--macro-level", "guarded", "--json"]);
    assert!(code <= 1, "{report}");
    let rows = report["invocations"].as_array().unwrap();
    let my_func = rows.iter().find(|row| row["entry"] == "Module1.MyFunc").unwrap();
    assert_eq!(my_func["procedure"], "function");
    assert_eq!(my_func["outcome"], serde_json::json!({
        "kind": "success", "result": {"type": "number", "value": 101.0}
    }));
    assert!(!rows.iter().any(|row| row["entry"] == "Module1.SecretFunc"));
    assert_eq!(report["entries"].as_u64().unwrap(), rows.len() as u64);
    let counted = ["success", "runtime_error", "refused", "failed", "needs_arguments"]
        .iter()
        .map(|k| report[*k].as_u64().unwrap())
        .sum::<u64>();
    assert_eq!(counted, rows.len() as u64);
    assert!(rows.iter().all(|r| r["outcome"]["kind"].is_string() && r["invocation"] == "command"));
    // Without a macro level nothing runs.
    let (_, code, _) = dnacalc(&["run", book, "--all", "--json"]);
    assert_eq!(code, 2);
}

/// `--steps <n>` sets the most steps one VBA call may take (default 50,000,000): a long Monte Carlo
/// entry finishes with a larger budget, and a small one ends the entry with the typed budget failure.
/// (dc-re3qu)
#[test]
fn run_all_takes_a_step_budget_per_entry() {
    let book = root().join("crates/calc-doc/tests/fixtures/vba_udf_names/local.xlsm");
    let book = book.to_str().unwrap();
    let (report, _, _) = dnacalc(&["run", book, "--all", "--steps", "1", "--macro-level", "guarded", "--json"]);
    let rows = report["invocations"].as_array().unwrap();
    let my_func = rows.iter().find(|row| row["entry"] == "Module1.MyFunc").unwrap();
    assert_eq!(my_func["outcome"], serde_json::json!({
        "kind": "failed", "reason": "the step budget was exhausted"
    }));
    let (report, _, _) = dnacalc(&["run", book, "--all", "--steps", "1000000", "--macro-level", "guarded", "--json"]);
    let rows = report["invocations"].as_array().unwrap();
    let my_func = rows.iter().find(|row| row["entry"] == "Module1.MyFunc").unwrap();
    assert_eq!(my_func["outcome"]["kind"], "success");
    let (_, code, _) = dnacalc(&["run", book, "--all", "--steps", "many", "--macro-level", "guarded", "--json"]);
    assert_eq!(code, 2);
}

#[test]
fn skin_validate_accepts_the_dark_audit_manifest_and_checks_contrast() {
    let path = temp("dark-audit.dnaskin.json");
    let mut manifest = serde_json::json!({
        "schema": "dna.skin/1", "id": "calm-dark-audit", "name": "Calm Dark Audit",
        "extends": "dna:default",
        "theme": {
            "base": "cockpit-dark", "density": "working",
            "overrides": {
                "ink.secondary": "#A4B8C2", "ink-3": "#A4B8C2",
                "surface.panel": "#22343C", "formula.font.size": "15.5px"
            }
        },
        "shell": { "deck": "compact", "panels": { "right": [{"id": "inspector.trace", "width_px": 320}] } },
        "keymap": { "profile": "excel-365-win-en-us", "bindings": [] }
    });
    let save = |manifest: &Value| std::fs::write(&path, serde_json::to_vec_pretty(manifest).unwrap()).unwrap();
    save(&manifest);
    let (result, code, text) = dnacalc(&["skin", "validate", &path, "--json"]);
    assert_eq!(code, 0, "{text}");
    assert_eq!(result["schema"], "dna.skin/1");
    assert_eq!(result["theme"], "cockpit-dark");
    assert_eq!(result["contrast_pairs"], 17);
    manifest["theme"]["overrides"]["ink.secondary"] = serde_json::json!("#7D97A0");
    save(&manifest);
    let (_, code, text) = dnacalc(&["skin", "validate", &path]);
    assert_eq!(code, 2);
    assert!(text.contains("ink-2 on paper-2"), "{text}");
    assert!(text.contains("contrast"), "{text}");
}

#[test]
fn skin_validate_checks_reserved_and_overlapping_keys_and_page_bindings() {
    let path = temp("manifest-key-or-bind.json");
    let mut manifest = serde_json::json!({
        "schema": "dna.skin/1", "id": "review", "name": "Review",
        "theme": { "base": "cockpit-light", "density": "working" },
        "keymap": {
            "profile": "excel-365-win-en-us",
            "bindings": [{"chord": "Escape", "when": "mode:ready", "command": "view.classic"}]
        }
    });
    let save = |manifest: &Value| std::fs::write(&path, serde_json::to_vec(manifest).unwrap()).unwrap();
    save(&manifest);
    let (_, code, text) = dnacalc(&["skin", "validate", &path]);
    assert_eq!(code, 2);
    assert!(text.contains("Escape is reserved"), "{text}");
    manifest["keymap"]["bindings"] = serde_json::json!([
        {"chord": "Ctrl+Shift+E", "when": "view:classic", "command": "view.classic"},
        {"chord": "Shift+Ctrl+E", "when": "mode:ready", "command": "view.app"}
    ]);
    save(&manifest);
    let (_, code, text) = dnacalc(&["skin", "validate", &path]);
    assert_eq!(code, 2);
    assert!(text.contains("Overlapping skin bindings for Ctrl+Shift+E"), "{text}");

    save(&serde_json::json!({
        "schema": "dna.page/1", "title": "Loan",
        "root": {"kind": "Part", "id": "rate", "component": "input", "bind": {
            "value": {"anchor": "00000000-0000-0000-0000-000000000001", "address": "Sheet1!B3"}
        }}
    }));
    let workbook = temp("skin-bindings.xlsx");
    std::fs::write(&workbook, calc_host::blank_package()).unwrap();
    let (result, code, text) = dnacalc(&["skin", "validate", &path, "--json"]);
    assert_eq!(code, 0, "{text}");
    assert_eq!(result["bindings"], 1);
    assert!(result["bindings_checked"].is_null());
    let (_, code, text) = dnacalc(&["skin", "validate", &path, "--workbook", &workbook]);
    assert_eq!(code, 2);
    assert!(text.contains("Sheet1!B3: Source missing"), "{text}");
}
