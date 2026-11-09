//! Unit tests of the worker script, Windows only, as the worker is. None starts Excel.
//!
//! - The package reader and the pre-open refusals: the embedded worker script is dot-sourced into
//!   Windows PowerShell, which defines its functions and starts nothing, and `Read-Package` and
//!   `Get-PackageRefusals` are run on the fixture packages under `tests/fixtures/`.
//! - The main flow: the script is called with the two parameters the adapter passes it
//!   (`-Request`, `-Result`, with request and result files of the adapter's form) from a harness
//!   that first shadows `New-Object` with a guard; [`crate::capture`]'s own start of the worker
//!   (`powershell.exe -File` under [`crate::run_isolated`]) is not part of these tests. The guard
//!   passes every .NET object through and creates no COM object: `New-Object -ComObject` returns
//!   the test's stand-in Application when the test gives one, and throws otherwise, so a
//!   regression that let a refused package reach Excel's start fails the test instead of starting
//!   Excel. The script creates its Excel through `New-Object -ComObject` only, which a test here
//!   checks. The harness and the fuller stand-in Excel are `tests/fixtures/standin-harness.ps1`
//!   and `tests/fixtures/standin-excel.ps1`.
//! - The spill scenarios (the section of that name below): the main flow against the stand-in
//!   Excel on `worker/spill.xlsx`, each result converted with the conversion [`crate::capture`]
//!   applies and judged by calc-verdict. The acceptance case `dc_xb26__spill_shape` runs them by
//!   name.
//! - The legacy array scenarios (the section of that name below): the package reader on
//!   `worker/legacy-array.xlsx` and `worker/legacy-array-conflict.xlsx`, and the main flow against
//!   the stand-in Excel on `worker/legacy-array.xlsx`, converted and judged like the spill
//!   scenarios.
//! - Text checks of the script: the empty catches, and the settings it assigns.
//! - The fixtures are the output of their committed generator, `tests/fixtures/make-fixtures.ps1`.

use crate::convert::{self, Frame};
use crate::{STAGE_SPILL, WORKER_SCRIPT, windows_powershell};
use calc_verdict::value::CalcValue;
use calc_verdict::{
    CaptureRecord, CellAddress, CellObservation, CellResult, Evaluation, EvaluationRecord,
    FormulaKind, Observation, Provenance, ResultSet, RunIdentity, STAGE_INVENTORY, VerdictPolicy,
    judge,
};
use serde_json::{Value, json};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// CREATE_NO_WINDOW.
const NO_WINDOW: u32 = 0x0800_0000;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn fixture(path: &str) -> PathBuf {
    fixtures().join(path)
}

/// A fresh directory for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "calc-oracle-excel-worker-tests-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A PowerShell single-quoted string of `text`.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn powershell(args: &[&str]) -> std::process::Output {
    Command::new(windows_powershell())
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
        ])
        .args(args)
        .stdin(Stdio::null())
        .creation_flags(NO_WINDOW)
        .output()
        .unwrap()
}

/// Native-only: invokes Windows PowerShell to exercise the production value reader.
#[test]
fn raw_value_metadata_keeps_numeric_nan_and_error_distinct_with_one_getter_read() {
    let dir = scratch("raw-value-metadata");
    let script = dir.join("capture-worker.ps1");
    std::fs::write(&script, WORKER_SCRIPT).unwrap();
    let command = format!(
        r#". {}; $script:reads=0; $script:nan=[BitConverter]::Int64BitsToDouble([Convert]::ToInt64('7ff800000000000a',16)); $cell=[pscustomobject]@{{}}; $cell|Add-Member ScriptProperty Value2 {{$script:reads++; return $script:nan}}; $kind=$null; $number=Read-Value $cell ([ref]$kind); $numericReads=$script:reads; $errorKind=$null; $errorValue=Read-Value ([pscustomobject]@{{Value2=[int]2036}}) ([ref]$errorKind); $oldCaller=Read-Value $cell; [Console]::Out.Write((ConvertTo-Json -Compress -Depth 8 -InputObject @{{number=$number;runtime_type=$kind;numeric_reads=$numericReads;error=$errorValue;error_runtime_type=$errorKind;old_caller=$oldCaller;total_reads=$script:reads}}))"#,
        quoted(&script.to_string_lossy())
    );
    let out = powershell(&["-Command", &command]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        json!({
            "number":{"number":"7ff800000000000a"},
            "runtime_type":"System.Double",
            "numeric_reads":1,
            "error":{"error_code":2036},
            "error_runtime_type":"System.Int32",
            "old_caller":{"number":"7ff800000000000a"},
            "total_reads":2
        })
    );
}

#[test]
fn the_witness_worker_parses_and_reads_a_one_based_matrix_in_windows_powershell() {
    let dir = scratch("witness-parser");
    let script = dir.join("witness-worker.ps1");
    std::fs::write(&script, crate::WITNESS_SCRIPT).unwrap();
    // Parse the complete production script without running its Excel startup. Exercise the
    // extracted column reader on the same one-based object[,] shape that Range.Value2 returns.
    let command = format!(
        "$tokens=$null; $errors=$null; $ast=[System.Management.Automation.Language.Parser]::ParseFile({},[ref]$tokens,[ref]$errors); if($errors.Count){{throw ($errors | Out-String)}}; $fn=$ast.Find({{param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Read-Column'}},$true); Invoke-Expression $fn.Extent.Text; function Get-ComProperty($Object,$Name){{return ,($Object.$Name)}}; $matrix=[Array]::CreateInstance([object],[int[]]@(2,1),[int[]]@(1,1)); $matrix.SetValue(1.25,1,1); $matrix.SetValue(-3,2,1); $range=[pscustomobject]@{{Value2=$matrix}}; $read=Read-Column $range 2; if($read.Count -ne 2){{throw 'wrong column size'}}; [Console]::Out.Write((ConvertTo-Json -InputObject @($read[0],$read[1]) -Compress))",
        quoted(&script.to_string_lossy())
    );
    let out = powershell(&["-Command", &command]);
    assert!(
        out.status.success(),
        "witness parser/column reader: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        json!([1.25, -3])
    );
}

/// What the worker's package reader reads from `package`, the refusals it gives, and the failures
/// it recorded, as JSON.
fn read_package(package: &Path) -> Value {
    let dir = scratch(&format!(
        "read-{}",
        package.file_name().unwrap().to_string_lossy()
    ));
    let script = dir.join("capture-worker.ps1");
    std::fs::write(&script, WORKER_SCRIPT).unwrap();
    let command = format!(
        ". {}; $p = Read-Package {}; $r = Get-PackageRefusals $p; [Console]::Out.Write((ConvertTo-Json -Compress -Depth 8 -InputObject ([ordered]@{{ sheets = @($p.sheets); formulas = @($p.formulas); connections = @($p.connections); links = @($p.links); roots = @($p.roots); external_calls = @($p.external_calls); refusals = @($r); failures = @($Failures) }})))",
        quoted(&script.to_string_lossy()),
        quoted(&package.to_string_lossy())
    );
    let out = powershell(&["-Command", &command]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "the dot-sourced worker failed: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "the worker's output is not JSON ({e}): {}",
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

/// The root elements the package reader found with refreshOnLoad set, as "element part".
fn refreshing_roots(read: &Value) -> Vec<String> {
    read["roots"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|root| root["refresh_on_load"] == true)
        .map(|root| {
            format!(
                "{} {}",
                root["element"].as_str().unwrap(),
                root["part"].as_str().unwrap()
            )
        })
        .collect()
}

/// A macro sheet, whose relationship type and document element are in Microsoft's namespace, is
/// inventoried as a worksheet is (dc-xuw): its formula cells are inventory cells beside the
/// worksheet's, and no failure is recorded.
#[test]
fn a_macro_sheet_is_inventoried_as_a_worksheet() {
    let read = read_package(&fixture("worker/macro-sheet.xlsm"));
    assert_eq!(
        read["sheets"],
        json!([
            {"name": "Sheet1", "visibility": "visible"},
            {"name": "Macro1", "visibility": "visible"}
        ])
    );
    assert_eq!(
        read["formulas"],
        json!([
            {"sheet": "Sheet1", "row": 1, "column": 1, "type": "normal", "si": null, "ref": null, "cm": null, "dynamic_array_metadata": false},
            {"sheet": "Macro1", "row": 1, "column": 1, "type": "normal", "si": null, "ref": null, "cm": null, "dynamic_array_metadata": false}
        ])
    );
    assert_eq!(read["failures"], json!([]));
    assert_eq!(read["refusals"], json!([]));
}

/// The data connections and external link parts a package declares are read, and the ones Excel
/// must never open (a connection that refreshes on load, a DDE link) are refused; a link to
/// another workbook and a connection that does not refresh on load are not.
#[test]
fn connections_that_refresh_on_load_and_dde_links_are_refused_before_excel_starts() {
    let read = read_package(&fixture("worker/external-content.xlsx"));
    assert_eq!(
        read["connections"],
        json!([
            {"name": "Refreshing", "type": "1", "model": false, "refresh_on_load": true},
            {"name": "Quiet", "type": "1", "model": false, "refresh_on_load": false}
        ])
    );
    assert_eq!(
        read["links"],
        json!([
            {"part": "xl/externalLinks/externalLink1.xml", "kind": "ddeLink"},
            {"part": "xl/externalLinks/externalLink2.xml", "kind": "externalBook"}
        ])
    );
    assert_eq!(
        read["refusals"],
        json!([
            "data connection 'Refreshing' refreshes on load",
            "external link part xl/externalLinks/externalLink1.xml is a ddeLink link"
        ])
    );
    assert_eq!(read["failures"], json!([]));

    // The untrusted-link fixture's link is to another workbook: read, and not refused.
    let link = read_package(&fixture("infra_m1_2/untrusted-link.xlsx"));
    assert_eq!(
        link["links"],
        json!([{"part": "xl/externalLinks/externalLink1.xml", "kind": "externalBook"}])
    );
    assert_eq!(link["refusals"], json!([]));
    assert_eq!(link["connections"], json!([]));
}

/// A pivot cache definition that refreshes on load is refused before Excel starts, found by its
/// root element; one that does not (kept at a part name outside the usual convention) is read and
/// not refused.
#[test]
fn a_pivot_cache_that_refreshes_on_load_is_refused_before_excel_starts() {
    let read = read_package(&fixture("worker/pivot-refresh.xlsx"));
    let pivots: Vec<(String, bool)> = read["roots"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|root| root["element"] == "pivotCacheDefinition")
        .map(|root| {
            (
                root["part"].as_str().unwrap().to_owned(),
                root["refresh_on_load"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        pivots,
        [
            ("xl/pivotCache/pivotCacheDefinition1.xml".to_owned(), true),
            ("xl/pivotCache/quiet-cache.xml".to_owned(), false)
        ]
    );
    assert_eq!(
        read["refusals"],
        json!(["pivot cache definition xl/pivotCache/pivotCacheDefinition1.xml refreshes on load"])
    );
    assert_eq!(read["connections"], json!([]));
}

/// A query table that refreshes on load is refused before Excel starts, although its data
/// connection does not refresh on load: the query table's own flag is read.
#[test]
fn a_query_table_that_refreshes_on_load_is_refused_before_excel_starts() {
    let read = read_package(&fixture("worker/query-table-refresh.xlsx"));
    assert_eq!(
        read["connections"],
        json!([{"name": "WebQuery", "type": "4", "model": false, "refresh_on_load": false}])
    );
    assert_eq!(
        refreshing_roots(&read),
        ["queryTable xl/queryTables/queryTable1.xml"]
    );
    assert_eq!(
        read["refusals"],
        json!(["query table xl/queryTables/queryTable1.xml refreshes on load"])
    );
}

/// A formula that calls a function the worker lists as reaching external data is refused before
/// Excel starts, wherever the package holds it (a cell, a conditional-formatting rule, a
/// data-validation rule, a defined name), with or without the _xlfn. prefix and in any case; a
/// function name inside a string literal, or at the end of a longer name, is not a call.
#[test]
fn formulas_calling_external_data_functions_are_refused_before_excel_starts() {
    let read = read_package(&fixture("worker/external-functions.xlsx"));
    assert_eq!(
        read["external_calls"],
        json!([
            {"where": "the formula of Sheet1!A1", "functions": ["RTD"]},
            {"where": "the formula of Sheet1!A2", "functions": ["WEBSERVICE"]},
            {"where": "the formula of Sheet1!A3", "functions": ["IMAGE"]},
            {"where": "the formula of Sheet1!A4", "functions": ["STOCKHISTORY"]},
            {"where": "the formula of Sheet1!A5", "functions": ["CUBEVALUE"]},
            {"where": "a cfRule formula of sheet Sheet1", "functions": ["RTD"]},
            {"where": "a dataValidation formula1 of sheet Sheet1", "functions": ["WEBSERVICE"]},
            {"where": "the defined name Feed", "functions": ["WEBSERVICE"]}
        ])
    );
    let refusals: Vec<&str> = read["refusals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();
    assert_eq!(refusals.len(), 8, "{refusals:?}");
    assert_eq!(
        refusals[1],
        "the formula of Sheet1!A2 calls WEBSERVICE, which reaches external data or another application"
    );
    // A6 (names in string literals), A7 (MYRTD) and A8 (SUM) are read as formulas and not refused.
    assert_eq!(read["formulas"].as_array().unwrap().len(), 8);
    assert!(
        !refusals
            .iter()
            .any(|r| r.contains("!A6") || r.contains("!A7") || r.contains("!A8"))
    );
}

/// Data-model mode: a workbook that declares the in-file data model (and no other connection than
/// the worksheet connections that feed it) may call the CUBE functions, which then read that
/// model; the same call in a workbook without the model stays refused (the test above).
///
/// Native-only: it runs the capture worker's package reader under PowerShell.
#[test]
fn cube_calls_over_the_in_file_data_model_are_not_refused() {
    let oracle = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("calc-host")
        .join("tests")
        .join("fixtures")
        .join("pivot_cube")
        .join("oracle.xlsx");
    let read = read_package(&oracle);
    let connections = read["connections"].as_array().unwrap();
    assert!(
        connections.iter().any(|c| c["model"] == json!(true)),
        "{connections:?}"
    );
    assert!(
        read["external_calls"].as_array().unwrap().len() > 100,
        "the workbook's CUBE calls are still read"
    );
    assert_eq!(read["refusals"], json!([]));
}

/// The matching itself, on formula texts: every listed function, with prefixes, spaces before
/// the parenthesis and any case; never inside a string literal or as the end of a longer name.
#[test]
fn the_external_function_match_reads_calls_only() {
    let dir = scratch("external-calls");
    let script = dir.join("capture-worker.ps1");
    std::fs::write(&script, WORKER_SCRIPT).unwrap();
    let formulas = [
        "RTD(\"a\",,\"b\")",
        "_xlfn._xlws.PY(\"x\",0)",
        "_xlfn.COPILOT(\"q\")",
        "register.id(\"lib\",\"f\")",
        "SQL.REQUEST(\"dsn\")",
        "CALL (\"lib\",\"f\",\"J\")",
        "_xlfn.TRANSLATE(A1,\"en\",\"nl\")+LEN(_xlfn.DETECTLANGUAGE(A1))",
        "CUBEMEMBER(\"c\",\"m\")&CUBESET(\"c\",\"s\")",
        "_xlfn.IMPORTCSV(\"f.csv\")&_xlfn.IMPORTTEXT(\"f.txt\")",
        "\"RTD(\"\"x\"\")\"&\"WEBSERVICE(\"",
        "MYRTD(1)+X.IMAGE(2)+RTDX(3)+_RTD(4)",
        "SUM(1,2)",
        "IMAGE",
    ];
    let list = formulas
        .iter()
        .map(|f| quoted(f))
        .collect::<Vec<_>>()
        .join(",");
    let command = format!(
        ". {}; [Console]::Out.Write((ConvertTo-Json -Compress -InputObject @(foreach ($f in @({list})) {{ , (Get-ExternalCalls $f) }})))",
        quoted(&script.to_string_lossy())
    );
    let out = powershell(&["-Command", &command]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let calls: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        calls,
        json!([
            ["RTD"],
            ["PY"],
            ["COPILOT"],
            ["REGISTER.ID"],
            ["SQL.REQUEST"],
            ["CALL"],
            ["TRANSLATE", "DETECTLANGUAGE"],
            ["CUBEMEMBER", "CUBESET"],
            ["IMPORTCSV", "IMPORTTEXT"],
            [],
            [],
            [],
            []
        ])
    );
}

/// The formula hex of `text`, as the worker writes it.
fn units(text: &str) -> String {
    text.encode_utf16()
        .map(|unit| format!("{unit:04x}"))
        .collect()
}

/// The converter on the package reader's own output for `worker/dynamic-array.xlsx`: Sheet1!B1,
/// saved as Excel with dynamic arrays saves a single-cell formula (t="array", ref B1, cm="1", which
/// the reader resolves to the metadata part's dynamic-array metadata XLDAPR), is an ordinary
/// formula flagged as dynamic; C1 is an ordinary formula, not flagged.
#[test]
fn a_package_with_a_single_cell_dynamic_array_formula_converts() {
    let read = read_package(&fixture("worker/dynamic-array.xlsx"));
    assert_eq!(
        read["formulas"],
        json!([
            {"sheet": "Sheet1", "row": 1, "column": 2, "type": "array", "si": null, "ref": "B1", "cm": "1", "dynamic_array_metadata": true},
            {"sheet": "Sheet1", "row": 1, "column": 3, "type": "normal", "si": null, "ref": null, "cm": null, "dynamic_array_metadata": false}
        ])
    );
    assert_eq!(read["refusals"], json!([]));
    let result = json!({
        "format": convert::RESULT_FORMAT, "version": convert::RESULT_VERSION, "excel_pid": 4, "fatal": null,
        "environment": {
            "excel_build": "16.0.1.2", "channel": "current", "compatibility": "calc-1",
            "numeric_environment": "excel-x86-64/cpu-x86-64",
            "settings": [["date1904", "false"], ["iteration", "false"], ["max_iterations", "100"],
                         ["max_change", "3f50624dd2f1a9fc"], ["precision_as_displayed", "false"]]
        },
        "sheets": read["sheets"],
        "formulas": read["formulas"],
        "cells": [
            {"sheet": "Sheet1", "row": 1, "column": 2, "formula": units("=SUM(A1:A2*2)"), "value": {"number": format!("{:016x}", 6f64.to_bits())}, "spill": null},
            {"sheet": "Sheet1", "row": 1, "column": 3, "formula": units("=A1+1"), "value": {"number": format!("{:016x}", 2f64.to_bits())}, "spill": null}
        ],
        "saved": [], "saved_formulas": read["formulas"], "spill_members": [], "reopened": [], "failures": []
    });
    let record = convert::convert(
        &result,
        Frame {
            case: "k".to_owned(),
            source: "sha256:in".to_owned(),
            tool: "t".to_owned(),
            revision: 0,
            operations: Vec::new(),
            output_package: None,
            failures: Vec::new(),
        },
    )
    .unwrap();
    let kinds: Vec<(String, FormulaKind, bool)> = record
        .formulas
        .iter()
        .map(|f| (f.cell.a1(), f.kind, f.dynamic_array))
        .collect();
    assert_eq!(
        kinds,
        [
            ("B1".to_owned(), FormulaKind::Ordinary, true),
            ("C1".to_owned(), FormulaKind::Ordinary, false)
        ]
    );
}

// --- the main flow ------------------------------------------------------------------------------

/// The harness every main-flow test runs, `tests/fixtures/standin-harness.ps1`: it shadows
/// `New-Object` with the guard (module documentation), defines the stand-in builders, dot-sources
/// the file `$Setup`, then calls the worker with `-Request` and `-Result`, and exits with the
/// worker's exit code. Lines the guard and the stand-ins print start with `STANDIN:`.
const HARNESS: &str = include_str!("../tests/fixtures/standin-harness.ps1");

/// What the worker's main flow did with a copy of `package`, run from [`HARNESS`] after `setup`:
/// its exit code, its standard output lines and its result file.
fn run_main_flow(name: &str, package: &Path, setup: &str) -> (Option<i32>, Vec<String>, Value) {
    let dir = scratch(&format!("main-{name}"));
    let input = dir.join("in").join(package.file_name().unwrap());
    std::fs::create_dir_all(input.parent().unwrap()).unwrap();
    std::fs::copy(package, &input).unwrap();
    let output = dir.join("out").join(package.file_name().unwrap());
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    let script = dir.join("capture-worker.ps1");
    std::fs::write(&script, WORKER_SCRIPT).unwrap();
    let harness = dir.join("harness.ps1");
    std::fs::write(&harness, HARNESS).unwrap();
    let setup_file = dir.join("setup.ps1");
    std::fs::write(&setup_file, setup).unwrap();
    let request = dir.join("request.json");
    let result = dir.join("result.json");
    std::fs::write(
        &request,
        json!({
            "format": "dnacalc-capture-request", "version": 1,
            "input": input.to_string_lossy(), "output": output.to_string_lossy(),
            "calculation": {"mode": "automatic", "iteration": false, "max_iterations": 100, "max_change": "3f50624dd2f1a9fc"}
        })
        .to_string(),
    )
    .unwrap();
    let out = powershell(&[
        "-File",
        &harness.to_string_lossy(),
        "-Worker",
        &script.to_string_lossy(),
        "-Request",
        &request.to_string_lossy(),
        "-Result",
        &result.to_string_lossy(),
        "-Setup",
        &setup_file.to_string_lossy(),
    ]);
    let written: Value = std::fs::read(&result)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null);
    let lines: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect();
    assert!(
        written != Value::Null,
        "the worker wrote no result: {lines:?}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
    (out.status.code(), lines, written)
}

fn failure_messages(result: &Value, stage: &str) -> Vec<String> {
    result["failures"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|failure| failure["stage"] == stage)
        .map(|failure| failure["message"].as_str().unwrap().to_owned())
        .collect()
}

/// A package the worker refuses never reaches Excel's start: called with a request for it, the
/// worker prints no excel-pid line and asks for no COM object (the guard would print it), writes
/// its result with the refusals as isolation failures and a fatal error, prints done and exits 1.
#[test]
fn a_refused_package_never_starts_excel() {
    for (fixture_path, refusals) in [
        ("worker/external-content.xlsx", 2),
        ("worker/pivot-refresh.xlsx", 1),
        ("worker/query-table-refresh.xlsx", 1),
        ("worker/external-functions.xlsx", 8),
    ] {
        let (code, lines, result) = run_main_flow(
            &fixture_path.replace(['/', '.'], "-"),
            &fixture(fixture_path),
            "",
        );
        assert_eq!(code, Some(1), "{fixture_path}: {lines:?}");
        assert!(
            !lines.iter().any(|line| line.contains("excel-pid")),
            "{fixture_path}: the worker reported an Excel: {lines:?}"
        );
        assert!(
            !lines.iter().any(|line| line.starts_with("STANDIN:")),
            "{fixture_path}: the worker asked for a COM object: {lines:?}"
        );
        assert_eq!(
            lines.last().map(String::as_str),
            Some("dnacalc-capture-worker/1 done"),
            "{fixture_path}"
        );
        assert_eq!(result["excel_pid"], Value::Null);
        let fatal = result["fatal"].as_str().unwrap();
        assert!(
            fatal.starts_with("refused before Excel was started:"),
            "{fixture_path}: {fatal}"
        );
        let isolation = failure_messages(&result, "isolation");
        assert_eq!(isolation.len(), refusals, "{fixture_path}: {isolation:?}");
        assert!(
            isolation
                .iter()
                .all(|m| m.ends_with("; Excel was not started, so the workbook was never opened"))
        );
        assert_eq!(result["cells"], json!([]));
    }
}

/// An Excel with an add-in open in its instance is refused before the workbook is opened, nothing
/// switched off: a stand-in Application with an add-in open in the instance and a DisplayAlerts
/// that cannot be read back makes the worker report the stand-in's PID, refuse the capture with
/// both named (the add-in by its file name, no part of its directory anywhere in the result),
/// never call Workbooks.Open, and quit the stand-in. A connected COM add-in, an idle one and one
/// installed in the profile but not open refuse nothing. A COM add-in whose Connect cannot be read
/// and a real COM object (a closed ADODB.Recordset, which has neither ProgId nor Connect) in the
/// COM add-in list are environment failures naming each, never a state that is off: a read that
/// fails, through PowerShell's property read or through COM, never passes for one.
#[test]
fn an_excel_with_an_add_in_open_is_refused_before_the_workbook_opens() {
    let setup = "$recordset = Microsoft.PowerShell.Utility\\New-Object -ComObject ADODB.Recordset\n$global:StandinApplication = New-StandinApplication @((New-StandinComAddIn 'Contoso.Loaded' $true), (New-StandinComAddIn 'Contoso.Idle' $false), (New-StandinComAddIn 'Contoso.Broken' $null), $recordset) @((New-StandinAddIn 'C:\\AddIns\\Tool.xlam' $true $true), (New-StandinAddIn 'C:\\AddIns\\Installed.xll' $false $true), (New-StandinAddIn 'C:\\AddIns\\Idle.xlam' $false $false)) @('DisplayAlerts')";
    let (code, lines, result) = run_main_flow("add-ins", &fixture("infra_m1_2/replay.xlsx"), setup);
    assert_eq!(code, Some(1), "{lines:?}");
    assert!(
        lines.contains(&"STANDIN: New-Object -ComObject Excel.Application".to_owned()),
        "{lines:?}"
    );
    let reported = lines
        .iter()
        .find_map(|line| line.strip_prefix("dnacalc-capture-worker/1 excel-pid "))
        .expect("the worker reports the stand-in's PID");
    assert_eq!(result["excel_pid"].to_string(), reported);
    assert!(
        !lines
            .iter()
            .any(|line| line.starts_with("STANDIN: Workbooks.Open")),
        "the workbook was opened: {lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("COMAddIn.Connect")),
        "an add-in was switched: {lines:?}"
    );
    assert!(lines.contains(&"STANDIN: Quit".to_owned()), "{lines:?}");
    let fatal = result["fatal"].as_str().unwrap();
    assert!(
        fatal.starts_with("refused before the workbook was opened: this Excel instance is not isolated (a capture needs an Excel instance with no add-in open in it, and nothing was switched off)"),
        "{fatal}"
    );
    let isolation = failure_messages(&result, "isolation");
    assert_eq!(
        isolation,
        [
            "DisplayAlerts reads back as nothing after it was set to False; the workbook was not opened",
            "add-in Tool.xlam is open; the workbook was not opened"
        ]
    );
    assert!(
        !result.to_string().contains("AddIns\\\\"),
        "the result names an add-in's directory: {result}"
    );
    let environment = failure_messages(&result, "environment");
    assert_eq!(environment.len(), 2, "{environment:?}");
    assert_eq!(
        environment[0],
        "the state of COM add-in Contoso.Broken cannot be read, so the record may omit it from com_add_ins_connected: Connect reads as nothing, not a boolean"
    );
    assert!(
        environment[1].starts_with("the state of COM add-in 4 cannot be read, so the record may omit it from com_add_ins_connected: ")
            && environment[1].contains("Unknown name"),
        "{}",
        environment[1]
    );
    for name in ["Contoso", "Idle", "Installed.xll"] {
        assert!(!fatal.contains(name), "{name}: {fatal}");
    }
    assert_eq!(result["cells"], json!([]));
}

/// A connected COM add-in is recorded and the capture completes (the orchestrator's rulings of
/// 2026-10-21 under AGENTS.md, Excel on this machine; calc-verdict counts it): against the full
/// stand-in Excel with two connected COM add-ins (one whose Description holds `|`, `;` and `%`),
/// an idle one, three installed add-ins (one under the user's profile directory, one whose file
/// name holds `;` and `%`) and one that is not installed, the worker opens the workbook, rebuilds,
/// saves and reopens, exits 0 with no fatal error and no isolation failure, writes no add-in's
/// Connect, and records the connected ones as `ProgId|Description` in `com_add_ins_connected` and
/// the installed ones by file name in `add_ins_installed`, each field escaped (`%25`, `%3B`,
/// `%7C`), so the lists split into exactly their items. No directory of an add-in, and not the
/// user's profile directory, appears anywhere in the result. The result converts to a
/// CaptureRecord that carries both settings.
#[test]
fn a_connected_com_add_in_is_recorded_and_the_capture_completes() {
    let setup = format!(
        "{FULL_STANDIN}\n$profileAddIn = Join-Path $env:USERPROFILE 'AppData\\Roaming\\Microsoft\\AddIns\\Tool.xlam'\n$app.COMAddIns = @((New-StandinComAddIn 'VS15ExcelAdaptor' $true 'Visual Studio Tools for Office Design-Time Adaptor for Excel'), (New-StandinComAddIn 'Contoso.Idle' $false 'Idle'), (New-StandinComAddIn 'Contoso.Odd' $true 'Tools | odd; 100%'))\n$app.AddIns = @((New-StandinAddIn 'C:\\AddIns\\Installed.xll' $false $true), (New-StandinAddIn $profileAddIn $false $true), (New-StandinAddIn 'C:\\AddIns\\Idle.xlam' $false $false), (New-StandinAddIn 'C:\\AddIns\\odd;name%.xll' $false $true))\n$app.AddIns2 = $app.AddIns\n"
    );
    let (code, lines, result) = run_main_flow(
        "connected-com-add-in",
        &fixture("infra_m1_2/untrusted-link.xlsx"),
        &setup,
    );
    assert_eq!(code, Some(0), "{lines:?}\n{result}");
    assert_eq!(result["fatal"], Value::Null, "{result}");
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.starts_with("STANDIN: Workbooks.Open untrusted-link.xlsx from "))
            .count(),
        2,
        "{lines:?}"
    );
    assert!(lines.contains(&"STANDIN: SaveAs 51 from untrusted-link.xlsx".to_owned()));
    assert!(
        !lines.iter().any(|line| line.contains("COMAddIn.Connect")),
        "an add-in was switched: {lines:?}"
    );
    assert_eq!(failure_messages(&result, "isolation"), Vec::<String>::new());
    let record = convert::convert(
        &result,
        Frame {
            case: "k".to_owned(),
            source: "sha256:in".to_owned(),
            tool: "t".to_owned(),
            revision: 0,
            operations: Vec::new(),
            output_package: None,
            failures: Vec::new(),
        },
    )
    .unwrap_or_else(|e| panic!("{e}\n{result}"));
    let settings = &record.environment.as_ref().unwrap().settings;
    let setting = |name: &str| {
        settings
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    };
    let connected = setting("com_add_ins_connected").unwrap();
    assert_eq!(
        connected,
        "VS15ExcelAdaptor|Visual Studio Tools for Office Design-Time Adaptor for Excel;Contoso.Odd|Tools %7C odd%3B 100%25"
    );
    let installed = setting("add_ins_installed").unwrap();
    assert_eq!(installed, "Installed.xll;Tool.xlam;odd%3Bname%25.xll");
    // The lists split into their items and fields, and each field unescapes to the name as read.
    let unescape = |field: &str| {
        field
            .replace("%7C", "|")
            .replace("%3B", ";")
            .replace("%25", "%")
    };
    let items: Vec<Vec<String>> = connected
        .split(';')
        .map(|item| item.split('|').map(unescape).collect())
        .collect();
    assert_eq!(
        items,
        [
            vec![
                "VS15ExcelAdaptor".to_owned(),
                "Visual Studio Tools for Office Design-Time Adaptor for Excel".to_owned()
            ],
            vec!["Contoso.Odd".to_owned(), "Tools | odd; 100%".to_owned()]
        ]
    );
    let names: Vec<String> = installed.split(';').map(unescape).collect();
    assert_eq!(names, ["Installed.xll", "Tool.xlam", "odd;name%.xll"]);
    assert_eq!(
        setting("add_ins"),
        Some(
            "COM add-in VS15ExcelAdaptor connect=true;COM add-in Contoso.Idle connect=false;COM add-in Contoso.Odd connect=true;add-in Installed.xll open=false installed=true;add-in Tool.xlam open=false installed=true;add-in Idle.xlam open=false installed=false;add-in odd%3Bname%25.xll open=false installed=true"
        )
    );
    let profile = std::env::var("USERPROFILE").unwrap();
    let text = result.to_string().to_lowercase();
    for directory in [
        profile.replace('\\', "\\\\").to_lowercase(),
        "c:\\\\addins".to_owned(),
        "%userprofile%".to_owned(),
    ] {
        assert!(
            !text.contains(&directory),
            "the result names {directory}: {result}"
        );
    }
    let session = record.session.as_ref().unwrap();
    assert!(
        session
            .failures
            .iter()
            .all(|failure| failure.stage == "environment"),
        "{:?}",
        session.failures
    );
}

/// Native-only: runs the capture PowerShell worker against stand-in COM objects.
#[test]
fn capture_preserves_each_existing_automation_security_value_without_writing_it() {
    for security in [1, 2, 3] {
        let setup = format!(
            "{FULL_STANDIN}\n$app | Add-Member -Force -MemberType ScriptProperty -Name AutomationSecurity -Value {{ {security} }} -SecondValue {{ throw 'AutomationSecurity must never be assigned' }}\n"
        );
        let (code, lines, result) = run_main_flow(
            &format!("preserve-security-{security}"),
            &fixture("infra_m1_2/untrusted-link.xlsx"),
            &setup,
        );
        assert_eq!(code, Some(0), "{lines:?}\n{result}");
        assert_eq!(result["fatal"], Value::Null, "{result}");
        assert_eq!(result["automation_security_before"], security);
        assert_eq!(result["automation_security_after"], security);
        let settings = result["environment"]["settings"].as_array().unwrap();
        assert!(settings.contains(&json!(["automation_security", security.to_string()])));
    }
    for script in [
        include_str!("../worker/timing-worker.ps1"),
        include_str!("../../../tools/excel-session/host.ps1"),
    ] {
        assert!(
            member_assignments(script)
                .iter()
                .all(|name| !name.ends_with(".AutomationSecurity"))
        );
    }
}

/// The control of the refusal test above: a stand-in with only idle add-ins passes the isolation
/// and the worker goes on to open the workbook (the stand-in's Workbooks.Open is called, and
/// throws), so the stand-in does show an open when one happens.
#[test]
fn an_excel_without_loaded_add_ins_goes_on_to_open_the_workbook() {
    let setup = "$global:StandinApplication = New-StandinApplication @((New-StandinComAddIn 'Contoso.Idle' $false)) @((New-StandinAddIn 'C:\\AddIns\\Idle.xlam' $false $false), (New-StandinAddIn 'C:\\AddIns\\Installed.xll' $false $true))";
    let (code, lines, result) =
        run_main_flow("no-add-ins", &fixture("infra_m1_2/replay.xlsx"), setup);
    assert_eq!(code, Some(1), "{lines:?}");
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("STANDIN: Workbooks.Open ")
                && line.ends_with("replay.xlsx")),
        "{lines:?}"
    );
    assert!(lines.contains(&"STANDIN: Quit".to_owned()), "{lines:?}");
    assert_eq!(failure_messages(&result, "isolation"), Vec::<String>::new());
    assert!(
        result["fatal"]
            .as_str()
            .unwrap()
            .contains("the stand-in Application opens no workbook"),
        "{}",
        result["fatal"]
    );
}

/// A fuller stand-in Excel, `tests/fixtures/standin-excel.ps1`: an Application with the settings
/// and methods the main flow reads and calls, a workbook whose Sheet1 holds the formula cells of
/// `$global:StandinCells` (by default A1 and B1 of `infra_m1_2/untrusted-link.xlsx`, holding 111
/// and 112, neither spilling) and whose SaveAs writes the package `$global:StandinSaved` (by
/// default the input package) to the output path.
const FULL_STANDIN: &str = include_str!("../tests/fixtures/standin-excel.ps1");

/// The whole main flow, run against [`FULL_STANDIN`]: the worker reports the stand-in, sets the
/// session settings, opens the input, rebuilds, reads the two formula cells and the settings,
/// saves, reads the saved package's caches, reopens the output and reads the cells again, quits,
/// and its result converts to a CaptureRecord version 2 with no failure but of the environment
/// (this host may have no Click-to-Run configuration). This shows the flow's code runs and the
/// result's shape; what Excel itself does is the excel route's to show.
#[test]
fn the_main_flow_runs_to_a_record_against_a_stand_in_excel() {
    let (code, lines, result) = run_main_flow(
        "full-flow",
        &fixture("infra_m1_2/untrusted-link.xlsx"),
        FULL_STANDIN,
    );
    assert_eq!(code, Some(0), "{lines:?}\n{result}");
    assert_eq!(result["fatal"], Value::Null, "{result}");
    let standin: Vec<&str> = lines
        .iter()
        .filter_map(|line| line.strip_prefix("STANDIN: "))
        .filter(|line| !line.starts_with("Workbooks.Open arguments "))
        .collect();
    assert_eq!(
        standin,
        [
            "New-Object -ComObject Excel.Application",
            "Workbooks.Open untrusted-link.xlsx from in",
            "CalculateFullRebuild",
            "SaveAs 51 from untrusted-link.xlsx",
            "Close",
            "Workbooks.Open untrusted-link.xlsx from out",
            "Close",
            "Quit"
        ]
    );
    // Both opens (the input, then the saved package) pass Workbooks.Open these arguments, by the
    // position of Excel's parameters, as the stand-in received them: links never updated,
    // read-only, a password no workbook has (an encrypted one then fails to open instead of
    // prompting), WriteResPassword omitted (Excel 16.0.20430 failed Open of replay.xlsx, which has
    // no write reservation, given one), no repair on load, and not added to the recent-files list.
    let opens: Vec<&str> = lines
        .iter()
        .filter_map(|line| line.strip_prefix("STANDIN: Workbooks.Open arguments "))
        .collect();
    assert_eq!(opens.len(), 2, "{lines:?}");
    for open in opens {
        let (count, list) = open.split_once(": ").unwrap();
        assert_eq!(count, "15", "{open}");
        let arguments: Vec<(&str, &str)> = list
            .split("; ")
            .map(|pair| pair.split_once('=').unwrap())
            .collect();
        assert_eq!(
            arguments,
            [
                ("FileName", "untrusted-link.xlsx (String)"),
                ("UpdateLinks", "0 (Int32)"),
                ("ReadOnly", "True (Boolean)"),
                ("Format", "missing"),
                ("Password", "dnacalc-no-password (String)"),
                ("WriteResPassword", "missing"),
                ("IgnoreReadOnlyRecommended", "True (Boolean)"),
                ("Origin", "missing"),
                ("Delimiter", "missing"),
                ("Editable", "False (Boolean)"),
                ("Notify", "False (Boolean)"),
                ("Converter", "missing"),
                ("AddToMru", "False (Boolean)"),
                ("Local", "False (Boolean)"),
                ("CorruptLoad", "0 (Int32)"),
            ],
            "{open}"
        );
    }
    let record = convert::convert(
        &result,
        Frame {
            case: "k".to_owned(),
            source: "sha256:in".to_owned(),
            tool: "t".to_owned(),
            revision: 0,
            operations: Vec::new(),
            output_package: None,
            failures: Vec::new(),
        },
    )
    .unwrap_or_else(|e| panic!("{e}\n{result}"));
    let session = record.session.as_ref().unwrap();
    let not_environment: Vec<_> = session
        .failures
        .iter()
        .filter(|failure| failure.stage != "environment")
        .collect();
    assert!(not_environment.is_empty(), "{not_environment:?}");
    let numbers = |observations: &[calc_verdict::CellObservation]| -> Vec<(String, Option<f64>)> {
        observations
            .iter()
            .map(|observed| {
                (
                    observed.cell.a1(),
                    match &observed.observation {
                        calc_verdict::Observation::ExcelRaw(value) => value.as_number(),
                        _ => None,
                    },
                )
            })
            .collect()
    };
    let expected = [
        ("A1".to_owned(), Some(111.0)),
        ("B1".to_owned(), Some(112.0)),
    ];
    assert_eq!(numbers(&record.observations), expected);
    assert_eq!(numbers(&session.reopened), expected);
    assert_eq!(session.saved.len(), 2);
    assert_eq!(
        record.formulas[0].formula.as_deref(),
        Some("=[1]Sheet1!$A$1")
    );
    let settings = &record.environment.as_ref().unwrap().settings;
    let setting = |name: &str| {
        settings
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    };
    // The stand-in starts at 1; capture records it without changing the macro policy.
    assert_eq!(setting("automation_security"), Some("1"));
    assert_eq!(setting("display_alerts"), Some("false"));
    assert_eq!(setting("external_links"), Some("link-target.xlsx"));
    assert_eq!(setting("add_ins"), Some(""));
    assert_eq!(setting("com_add_ins_connected"), Some(""));
    assert_eq!(setting("add_ins_installed"), Some(""));
    // The stand-in workbook's ReadOnly, read back; the excel route shows Excel's own.
    assert_eq!(setting("opened_read_only"), Some("true"));
    assert_eq!(setting("calculation"), Some("automatic"));
    // The request's max_change (0.001) as set and read back, an indexed International and
    // LanguageID read, and the target's compatibility from both calculation versions.
    assert_eq!(setting("max_change"), Some("3f50624dd2f1a9fc"));
    assert_eq!(setting("iteration"), Some("false"));
    assert_eq!(setting("decimal_separator"), Some("3"));
    assert_eq!(setting("ui_language_id"), Some("1033"));
    assert_eq!(setting("calculation_version"), Some("191029"));
    // Integer settings Excel gives as doubles (Build, MaxIterations as read back, the numeric
    // International values) are recorded as integers, with no environment failure for them.
    assert_eq!(setting("excel_build_number"), Some("19127"));
    assert_eq!(setting("max_iterations"), Some("100"));
    assert_eq!(setting("country_code"), Some("1"));
    assert_eq!(setting("date_order"), Some("32"));
    let environment: Vec<&str> = session
        .failures
        .iter()
        .filter(|failure| failure.stage == "environment")
        .map(|failure| failure.message.as_str())
        .collect();
    assert!(
        environment
            .iter()
            .all(|message| ["(Build)", "(MaxIterations)", "(International)"]
                .iter()
                .all(|property| !message.contains(property))),
        "{environment:?}"
    );
    assert_eq!(
        record.environment.as_ref().unwrap().target.compatibility(),
        "compat-16/calc-191029"
    );
}

/// The settings the stand-in's getters cannot read (PowerShell's own property read turns each
/// into nothing): MaxChange and Iteration, which the context fingerprint rests on,
/// CalculationVersion, which the target's compatibility names, and IgnoreRemoteRequests, recorded
/// for the record only. Each is recorded as an empty setting with an environment failure that
/// names it, none as a value cast from nothing (a max_change of bits 0, a flag of "", a
/// compatibility of "calc-"), and the conversion refuses the result: first for the empty
/// compatibility, and, with that given back, for the empty iteration. A Build that reads as a
/// double holding no whole number is not an integer either: empty, with its environment failure.
#[test]
fn settings_that_cannot_be_read_are_failures_and_never_defaults() {
    let unreadable = [
        "MaxChange",
        "CalculationVersion",
        "Iteration",
        "IgnoreRemoteRequests",
    ];
    let names: Vec<String> = unreadable.iter().map(|name| format!("'{name}'")).collect();
    let setup = format!(
        "{FULL_STANDIN}\nforeach ($name in @({})) {{\n    $app.PSObject.Properties.Remove($name)\n    $app | Add-Member -MemberType ScriptProperty -Name $name -Value {{ throw 'the stand-in cannot read this setting' }} -SecondValue {{ }}\n}}\n$app.Build = 19127.5\n",
        names.join(", ")
    );
    let (code, lines, result) = run_main_flow(
        "unreadable-settings",
        &fixture("infra_m1_2/untrusted-link.xlsx"),
        &setup,
    );
    assert_eq!(code, Some(0), "{lines:?}\n{result}");
    assert_eq!(result["fatal"], Value::Null, "{result}");
    let settings: Vec<(String, String)> = result["environment"]["settings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| {
            (
                pair[0].as_str().unwrap().to_owned(),
                pair[1].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let setting = |name: &str| {
        settings
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    };
    for name in [
        "max_change",
        "calculation_version",
        "iteration",
        "ignore_remote_requests",
        "excel_build_number",
    ] {
        assert_eq!(setting(name), Some(""), "{name}: {settings:?}");
    }
    // The readable settings around them are still recorded.
    assert_eq!(setting("max_iterations"), Some("100"));
    assert_eq!(setting("use_system_separators"), Some("true"));
    assert_eq!(setting("workbook_calculation_version"), Some("191029"));
    assert!(
        settings.iter().all(|(_, v)| v != "0000000000000000"),
        "{settings:?}"
    );
    assert_eq!(result["environment"]["compatibility"], "", "{result}");
    let environment = failure_messages(&result, "environment");
    for property in unreadable {
        assert!(
            environment
                .iter()
                .any(|message| message.contains(&format!("({property}) cannot be read"))),
            "{property}: {environment:?}"
        );
    }
    assert!(
        environment.iter().any(|message| message.contains(
            "(Build) cannot be read: it reads as 19127.5 (Double), not of the kind integer"
        )),
        "{environment:?}"
    );
    let frame = || Frame {
        case: "k".to_owned(),
        source: "sha256:in".to_owned(),
        tool: "t".to_owned(),
        revision: 0,
        operations: Vec::new(),
        output_package: None,
        failures: Vec::new(),
    };
    let refused = convert::convert(&result, frame()).unwrap_err();
    assert!(refused.contains("empty target part"), "{refused}");
    let mut named = result.clone();
    named["environment"]["compatibility"] = json!("calc-191029");
    let refused = convert::convert(&named, frame()).unwrap_err();
    assert!(
        refused.contains("the setting iteration is \"\""),
        "{refused}"
    );
}

/// The add-in states the record carries name each add-in by its file name: an idle add-in under a
/// user-profile path is recorded as `add-in tool.xll open=false installed=false`, and no part of
/// its directory reaches the result.
#[test]
fn add_in_states_carry_file_names_only() {
    let setup = format!(
        "{FULL_STANDIN}\n$app.AddIns2 = @(New-StandinAddIn 'C:\\Users\\someone\\AppData\\Roaming\\Microsoft\\AddIns\\tool.xll' $false $false)\n"
    );
    let (code, lines, result) = run_main_flow(
        "add-in-names",
        &fixture("infra_m1_2/untrusted-link.xlsx"),
        &setup,
    );
    assert_eq!(code, Some(0), "{lines:?}\n{result}");
    assert_eq!(result["fatal"], Value::Null, "{result}");
    let add_ins = result["environment"]["settings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pair| pair[0] == "add_ins")
        .map(|pair| pair[1].clone());
    assert_eq!(
        add_ins,
        Some(json!("add-in tool.xll open=false installed=false"))
    );
    assert!(!result.to_string().contains("someone"), "{result}");
}

/// A CalculationState that reads as something other than an integer (here the text "0") is
/// refused, never taken for xlDone: the worker's result is fatal and names the read, and no cell
/// is captured.
#[test]
fn a_calculation_state_that_is_not_an_integer_is_fatal() {
    let setup = format!("{FULL_STANDIN}\n$app.CalculationState = '0'\n");
    let (code, lines, result) = run_main_flow(
        "calculation-state",
        &fixture("infra_m1_2/untrusted-link.xlsx"),
        &setup,
    );
    assert_ne!(code, Some(0), "{lines:?}\n{result}");
    let fatal = result["fatal"].as_str().unwrap_or_default();
    assert!(
        fatal.contains("Application.CalculationState reads as 0 (String)"),
        "{result}"
    );
    assert_eq!(result["cells"], json!([]), "{result}");
}

// --- the spill scenarios -------------------------------------------------------------------------

/// The setup of a stand-in Excel for `worker/spill.xlsx`: Sheet1!B1 =SEQUENCE(3,2) (Value2 1, the
/// top-left value of its array) with the spill `b1` (a PowerShell value of `standin-excel.ps1`'s
/// form), D1 =A1+1 (2) and E1 =SUM(A1:A2*2) (6), neither spilling. Its SaveAs writes the fixture
/// `saved` (the input, spill.xlsx, when `None`), and its SpecialCells lists the cells `listed`
/// (row, column) after the formula cells.
fn spill_setup(b1: &str, saved: Option<&str>, listed: &[(u32, u32)]) -> String {
    let saved = saved.map_or_else(
        || "$null".to_owned(),
        |path| quoted(&fixture(path).to_string_lossy()),
    );
    let listed: String = listed
        .iter()
        .map(|(row, column)| format!("[void]$global:StandinListed.Add(@({row}, {column}))\n"))
        .collect();
    format!(
        "$global:StandinCells = @(@(1, 2, '=SEQUENCE(3,2)', 1.0, {b1}), @(1, 4, '=A1+1', 2.0, $null), @(1, 5, '=SUM(A1:A2*2)', 6.0, $null))\n$global:StandinSaved = {saved}\n$global:StandinListed = New-Object System.Collections.ArrayList\n{listed}{FULL_STANDIN}"
    )
}

/// The frame the adapter gives the conversion, for a stand-in run of `worker/spill.xlsx`.
fn spill_frame() -> Frame {
    Frame {
        case: "spill-standin".to_owned(),
        source: format!(
            "sha256:{}",
            crate::sha256_hex(&std::fs::read(fixture("worker/spill.xlsx")).unwrap())
        ),
        tool: crate::capture_tool_revision(None),
        revision: 0,
        operations: Vec::new(),
        output_package: None,
        failures: Vec::new(),
    }
}

/// The worker's result for `worker/spill.xlsx` against the stand-in Excel of [`spill_setup`], which
/// must have asked the guard for its Excel and completed, the lines the stand-in printed, and the
/// conversion of the result.
fn spill_capture(
    name: &str,
    b1: &str,
    saved: Option<&str>,
    listed: &[(u32, u32)],
) -> (Value, Vec<String>, Result<CaptureRecord, String>) {
    let (code, lines, result) = run_main_flow(
        name,
        &fixture("worker/spill.xlsx"),
        &spill_setup(b1, saved, listed),
    );
    assert_eq!(code, Some(0), "{lines:?}\n{result}");
    assert_eq!(result["fatal"], Value::Null, "{result}");
    let standin: Vec<String> = lines
        .iter()
        .filter_map(|line| line.strip_prefix("STANDIN: "))
        .map(str::to_owned)
        .collect();
    assert_eq!(
        standin.first().map(String::as_str),
        Some("New-Object -ComObject Excel.Application"),
        "{lines:?}"
    );
    let record = convert::convert(&result, spill_frame());
    (result, standin, record)
}

fn sheet1(column: u32) -> CellAddress {
    CellAddress::new("Sheet1", 1, column).unwrap()
}

/// The cells of `list`, in A1 form.
fn a1s(list: &[CellObservation]) -> Vec<String> {
    list.iter().map(|observed| observed.cell.a1()).collect()
}

/// The failures of `record` at Sheet1!`a1`, as (stage, message).
fn failures_of(record: &CaptureRecord, a1: &str) -> Vec<(String, String)> {
    record
        .session
        .as_ref()
        .expect("a version 2 record")
        .failures
        .iter()
        .filter(|failure| {
            failure
                .cell
                .as_ref()
                .is_some_and(|cell| cell.sheet() == "Sheet1" && cell.a1() == a1)
        })
        .map(|failure| (failure.stage.clone(), failure.message.clone()))
        .collect()
}

/// The entry of the result's list `list` (`cells`, `reopened`, `saved_formulas`) at Sheet1 row 1
/// and `column`.
fn entry<'a>(result: &'a Value, list: &str, column: u64) -> &'a Value {
    result[list]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["sheet"] == "Sheet1" && entry["row"] == 1 && entry["column"] == column)
        .unwrap_or_else(|| panic!("{list} holds no Sheet1 row 1 column {column}: {result}"))
}

/// Each judged cell (A1) with its verdict's name, and the number of spill failures the report
/// counts.
type Verdicts = (Vec<(String, &'static str)>, u64);

/// The verdict of each of B1, D1 and E1 when a run calculated B1 as `b1`, D1 as 2 and E1 as 6,
/// with an evaluation of each in the evaluation record, and the number of spill failures the
/// report counts; or why calc-verdict refuses to judge the record.
fn verdicts_with(record: &CaptureRecord, b1: CalcValue) -> Result<Verdicts, String> {
    verdicts_of(
        record,
        &[
            (sheet1(2), b1),
            (sheet1(4), CalcValue::number(2.0)),
            (sheet1(5), CalcValue::number(6.0)),
        ],
    )
}

/// [`verdicts_with`] B1 calculated as 1, the value its Value2 shows.
fn verdicts(record: &CaptureRecord) -> Result<Verdicts, String> {
    verdicts_with(record, CalcValue::number(1.0))
}

/// The array of `rows` by `columns` holding 1, 2, 3, ... in row-major order: what the stand-in's
/// spill range shows.
fn counting(rows: usize, columns: usize) -> CalcValue {
    let cells = (1..=rows * columns)
        .map(|n| CalcValue::number(n as f64))
        .collect();
    CalcValue::array(
        calc_verdict::value::CalcArray::new(
            calc_verdict::value::ArrayShape {
                rows,
                cols: columns,
            },
            cells,
        )
        .unwrap(),
    )
}

/// The verdict of each judged cell when a run calculated each cell of `cells` as its number, with
/// an evaluation of each in the evaluation record, and the number of spill failures the report
/// counts; or why calc-verdict refuses to judge the record.
fn verdicts_of(
    record: &CaptureRecord,
    cells: &[(CellAddress, CalcValue)],
) -> Result<Verdicts, String> {
    let session = record.session.as_ref().expect("a version 2 record");
    let results = ResultSet {
        case: record.case.clone(),
        source: record.source.clone(),
        run: RunIdentity {
            revision: session.revision,
            context: session.context.clone(),
        },
        results: cells
            .iter()
            .map(|(cell, value)| CellResult {
                cell: cell.clone(),
                provenance: Provenance::Calculated {
                    tick: 1,
                    revision: session.revision,
                    context: session.context.clone(),
                    external_links: false,
                },
                value: Some(value.clone()),
            })
            .collect(),
    };
    let evaluations = EvaluationRecord {
        case: record.case.clone(),
        source: record.source.clone(),
        evaluations: cells
            .iter()
            .map(|(cell, _)| Evaluation {
                cell: cell.clone(),
                tick: 1,
                revision: session.revision,
                context: session.context.clone(),
            })
            .collect(),
    };
    let report = judge(record, &results, &evaluations, &VerdictPolicy::default())
        .map_err(|e| e.to_string())?;
    let spill_failures = report
        .capture_failures
        .as_ref()
        .and_then(|failures| failures.by_stage.get(STAGE_SPILL).copied())
        .unwrap_or_default();
    Ok((
        report
            .cells
            .iter()
            .map(|cell| (cell.formula.cell.a1(), cell.verdict.name()))
            .collect(),
        spill_failures,
    ))
}

fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

/// Sheet1!B1 spills over B1:C3 after the full rebuild (the stand-in's HasSpill is True, and its
/// SpillingToRange's Address(False, False), Rows.Count, Columns.Count and Value2 are B1:C3, 3, 2
/// and 1 to 6): the worker reads that spill with its values after the rebuild and after
/// reopening, and the saved package's formulas with B1's cell metadata resolved to the
/// dynamic-array metadata; the record keeps B1 in the inventory with the array as its raw
/// observation and its reopened value, and no saved cache (dc-677); a calculated B1 equal to that
/// array is matched, one equal to the value B1's own Value2 shows (1) is mismatched, and D1 and E1
/// (a dynamic-array formula over its own cell, flagged) are matched.
#[test]
fn a_formula_that_spills_is_recorded_as_its_array_never_as_a_scalar() {
    let (result, _, record) = spill_capture("spill", "@('B1:C3', 3, 2)", None, &[]);
    let number = |n: f64| json!({"number": format!("{:016x}", n.to_bits())});
    for list in ["cells", "reopened"] {
        assert_eq!(
            entry(&result, list, 2)["spill"],
            json!({"ref": "B1:C3", "rows": 3, "columns": 2, "values": [number(1.0), number(2.0), number(3.0), number(4.0), number(5.0), number(6.0)]}),
            "{list}"
        );
        assert_eq!(
            entry(&result, list, 2)["value"],
            json!({"number": format!("{:016x}", 1f64.to_bits())}),
            "{list}"
        );
        for column in [4, 5] {
            assert_eq!(entry(&result, list, column)["spill"], Value::Null, "{list}");
        }
    }
    assert_eq!(
        entry(&result, "saved_formulas", 2),
        &json!({"sheet": "Sheet1", "row": 1, "column": 2, "type": "array", "si": null, "ref": "B1:C3", "cm": "1", "dynamic_array_metadata": true})
    );
    assert_eq!(
        entry(&result, "saved_formulas", 4)["dynamic_array_metadata"],
        false
    );
    assert_eq!(
        entry(&result, "saved_formulas", 5)["dynamic_array_metadata"],
        true
    );
    assert_eq!(result["spill_members"], json!([]));

    let record = record.unwrap_or_else(|e| panic!("the spilling capture converts: {e}\n{result}"));
    let session = record.session.as_ref().unwrap();
    let inventory: Vec<(String, FormulaKind, bool)> = record
        .formulas
        .iter()
        .map(|formula| (formula.cell.a1(), formula.kind, formula.dynamic_array))
        .collect();
    assert_eq!(
        inventory,
        [
            ("B1".to_owned(), FormulaKind::Ordinary, false),
            ("D1".to_owned(), FormulaKind::Ordinary, false),
            ("E1".to_owned(), FormulaKind::Ordinary, true)
        ]
    );
    assert_eq!(a1s(&record.observations), ["B1", "D1", "E1"]);
    assert_eq!(
        record.observations[0].observation,
        Observation::ExcelRaw(counting(3, 2))
    );
    assert_eq!(a1s(&session.saved), ["D1", "E1"]);
    assert_eq!(a1s(&session.reopened), ["B1", "D1", "E1"]);
    assert_eq!(
        session.reopened[0].observation,
        Observation::ExcelRaw(counting(3, 2))
    );
    for a1 in ["B1", "D1", "E1"] {
        assert_eq!(failures_of(&record, a1), [], "{a1}");
    }
    assert!(
        session
            .failures
            .iter()
            .all(|failure| failure.stage == "environment"),
        "{:?}",
        session.failures
    );
    let encoded = record.encode().unwrap();
    assert_eq!(CaptureRecord::decode(encoded.as_bytes()).unwrap(), record);
    let (judged, spill_failures) = verdicts_with(&record, counting(3, 2)).unwrap();
    assert_eq!(
        judged,
        [
            ("B1".to_owned(), "matched"),
            ("D1".to_owned(), "matched"),
            ("E1".to_owned(), "matched")
        ]
    );
    assert_eq!(spill_failures, 0);
    let (judged, _) = verdicts(&record).unwrap();
    assert_eq!(judged[0], ("B1".to_owned(), "mismatched"));
    let (judged, _) = verdicts_with(&record, counting(2, 2)).unwrap();
    assert_eq!(judged[0], ("B1".to_owned(), "mismatched"));
    println!(
        "NOTE: B1 spilling over B1:C3 (HasSpill True; SpillingToRange B1:C3, 3 rows, 2 columns, Value2 1 to 6): the worker read the spill and its values after the rebuild and after reopening, and the saved B1 as array B1:C3 with cm 1 resolved to XLDAPR; the record keeps B1 with the 3 by 2 array as its raw observation and reopened value, no saved cache, no failure; judged B1 is matched by that array, mismatched by the scalar 1 and by a 2 by 2 array, and D1 and E1 are matched (E1 flagged dynamic, D1 not)"
    );
}

/// Sheet1!B1, which the input saved as a dynamic-array formula over B1:C3, does not spill after
/// the full rebuild: it is recorded as Excel shows it (Value2 1), flagged as a dynamic-array
/// formula, and judged matched; the saved package (here the input's bytes) that still declares a
/// dynamic-array range B1:C3 is a save failure of B1.
#[test]
fn a_dynamic_array_formula_that_does_not_spill_is_recorded_as_excel_shows_it() {
    let (result, _, record) = spill_capture("no-spill", "$null", None, &[]);
    let record = record.unwrap_or_else(|e| panic!("{e}\n{result}"));
    assert_eq!(
        (
            record.formulas[0].cell.a1(),
            record.formulas[0].dynamic_array
        ),
        ("B1".to_owned(), true)
    );
    assert_eq!(
        record.observations[0],
        CellObservation {
            cell: sheet1(2),
            observation: Observation::ExcelRaw(CalcValue::number(1.0)),
        }
    );
    assert_eq!(
        failures_of(&record, "B1"),
        owned(&[(
            "save",
            "Excel reports no spill after the full rebuild, and the saved package holds the formula as type \"array\" with ref B1:C3 and cell metadata 1 (the dynamic-array metadata XLDAPR)"
        )])
    );
    let (judged, spill_failures) = verdicts(&record).unwrap();
    assert_eq!(judged[0], ("B1".to_owned(), "matched"));
    assert_eq!(spill_failures, 0);
    println!(
        "NOTE: B1 not spilling (input ref B1:C3): recorded as Excel shows it (1), flagged dynamic, matched; the saved package still declaring B1:C3 is the save failure {:?}",
        failures_of(&record, "B1")[0].1
    );
}

/// The saved package is read from the package Excel saved, never from the input: the stand-in
/// saves `worker/spill-saved-cm.xlsx`, whose B1 is held over B1:C3 with cell metadata 2, which
/// names no dynamic-array metadata, while the input's B1 has cell metadata 1, which does. B1 then
/// gets a save failure naming that cell metadata, and the same result with the input's formulas in
/// place of the saved ones gives none. A spill Excel reports over B1:C4 while the saved package
/// declares B1:C3 is a save failure too.
#[test]
fn a_saved_package_that_disagrees_with_the_spill_is_a_save_failure() {
    let (result, standin, record) = spill_capture(
        "saved-cm",
        "@('B1:C3', 3, 2)",
        Some("worker/spill-saved-cm.xlsx"),
        &[],
    );
    assert!(
        standin.contains(&"SaveAs 51 from spill-saved-cm.xlsx".to_owned()),
        "{standin:?}"
    );
    let saved_failure = "Excel reports a spill over Sheet1!B1:C3 (3 rows by 2 columns) after the full rebuild, and the saved package holds the formula as type \"array\" with ref B1:C3 and cell metadata 2, which does not name the dynamic-array metadata XLDAPR";
    let record = record.unwrap_or_else(|e| panic!("{e}\n{result}"));
    assert_eq!(
        failures_of(&record, "B1"),
        owned(&[("save", saved_failure)])
    );
    // What the worker read of B1 in each package: cell metadata 2, naming no dynamic-array
    // metadata, in the saved one, and 1, naming it, in the input.
    assert_eq!(
        (
            &entry(&result, "saved_formulas", 2)["cm"],
            &entry(&result, "saved_formulas", 2)["dynamic_array_metadata"]
        ),
        (&json!("2"), &json!(false))
    );
    assert_eq!(
        (
            &entry(&result, "formulas", 2)["cm"],
            &entry(&result, "formulas", 2)["dynamic_array_metadata"]
        ),
        (&json!("1"), &json!(true))
    );
    // The input's formulas give no save failure: the one above comes from the saved package.
    let mut input = result.clone();
    input["saved_formulas"] = result["formulas"].clone();
    let from_input = convert::convert(&input, spill_frame()).unwrap();
    assert_eq!(failures_of(&from_input, "B1"), []);
    println!(
        "NOTE: with the stand-in saving spill-saved-cm.xlsx (B1 over B1:C3, cm 2, no XLDAPR block) for an input whose B1 has cm 1 (XLDAPR), B1 has the save failure {saved_failure:?}, and none when the input's formulas stand in for the saved ones"
    );

    let (result, _, record) = spill_capture("saved-ref", "@('B1:C4', 4, 2)", None, &[]);
    let record = record.unwrap_or_else(|e| panic!("{e}\n{result}"));
    let failures = failures_of(&record, "B1");
    assert_eq!(
        failures,
        owned(&[(
            "save",
            "Excel reports a spill over Sheet1!B1:C4 (4 rows by 2 columns) after the full rebuild, and the saved package holds the formula as type \"array\" with ref B1:C3 and cell metadata 1 (the dynamic-array metadata XLDAPR)"
        )])
    );
    assert_eq!(a1s(&record.observations), ["B1", "D1", "E1"]);
    assert_eq!(
        verdicts_with(&record, counting(4, 2)).unwrap().0[0],
        ("B1".to_owned(), "matched")
    );
    println!(
        "NOTE: with the stand-in reporting B1 spilling over B1:C4 while the saved package declares B1:C3, B1 has its 4 by 2 array as its observation and the failures {failures:?}"
    );
}

/// A spilling cell the saved package holds no formula for (the stand-in saves
/// `worker/spill-saved-values.xlsx`, whose B1 holds only the value 1) gets the worker's save
/// failure naming the cell, beside its spill failure.
#[test]
fn a_spilling_cell_without_a_saved_formula_is_a_save_failure_naming_it() {
    let (result, _, record) = spill_capture(
        "saved-values",
        "@('B1:C3', 3, 2)",
        Some("worker/spill-saved-values.xlsx"),
        &[],
    );
    let save: Vec<&Value> = result["failures"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|failure| failure["stage"] == "save")
        .collect();
    assert_eq!(
        save,
        [
            &json!({"stage": "save", "sheet": "Sheet1", "row": 1, "column": 2, "message": "the saved package holds no formula for this formula cell"})
        ]
    );
    let record = record.unwrap_or_else(|e| panic!("{e}\n{result}"));
    assert_eq!(
        failures_of(&record, "B1"),
        owned(&[(
            "save",
            "the saved package holds no formula for this formula cell"
        )])
    );
    println!(
        "NOTE: with the stand-in saving spill-saved-values.xlsx (B1 a value, no formula), the worker's save failure names Sheet1!B1: {}",
        save[0]
    );
}

/// A HasSpill or a SpillingToRange that cannot be read is written as unread, after the rebuild and
/// after reopening, never as no spill, and the conversion refuses the capture.
#[test]
fn a_spill_that_cannot_be_read_refuses_the_capture() {
    for (b1, why) in [
        (
            "'unreadable-range'",
            "Range.HasSpill reads as True and Range.SpillingToRange as nothing",
        ),
        (
            "'unreadable-hasspill'",
            "Range.HasSpill reads as nothing, not a boolean",
        ),
    ] {
        let (result, _, record) = spill_capture("spill-unread", b1, None, &[]);
        for list in ["cells", "reopened"] {
            assert_eq!(
                entry(&result, list, 2)["spill"],
                json!({"unread": why}),
                "{b1} {list}"
            );
            assert_eq!(entry(&result, list, 4)["spill"], Value::Null, "{b1} {list}");
        }
        let refused = record.expect_err("a spill that cannot be read refuses the capture");
        assert!(
            refused.starts_with(&format!(
                "Excel's spill state of Sheet1!B1 cannot be read: {why}; "
            )),
            "{b1}: {refused}"
        );
        println!("NOTE: {b1}: the conversion refused the capture: {refused}");
    }
}

/// Cells Excel lists among its formula cells that lie in the spill range of B1 (C1, B2, C2, B3 and
/// C3 of B1:C3) are spill members, never inventory failures: the record holds no inventory failure
/// and calc-verdict judges it. A listed cell outside every spill (B4, below it) is still an
/// inventory failure, and calc-verdict refuses the record.
#[test]
fn cells_excel_lists_in_a_spill_range_are_spill_members_not_inventory_failures() {
    let children = [(1, 3), (2, 2), (2, 3), (3, 2), (3, 3)];
    let members: Vec<Value> = children
        .iter()
        .map(|(row, column)| json!({"sheet": "Sheet1", "row": row, "column": column}))
        .collect();
    let (result, _, record) = spill_capture("listed", "@('B1:C3', 3, 2)", None, &children);
    assert_eq!(result["spill_members"], json!(members));
    assert_eq!(
        failure_messages(&result, STAGE_INVENTORY),
        Vec::<String>::new()
    );
    let record = record.unwrap_or_else(|e| panic!("{e}\n{result}"));
    assert!(
        record
            .session
            .as_ref()
            .unwrap()
            .failures
            .iter()
            .all(|failure| failure.stage != STAGE_INVENTORY),
        "{record:?}"
    );
    assert_eq!(record.formulas.len(), 3);
    let (judged, _) = verdicts_with(&record, counting(3, 2)).unwrap();
    assert_eq!(judged[0], ("B1".to_owned(), "matched"));

    let mut beyond = children.to_vec();
    beyond.push((4, 2));
    let (result, _, record) = spill_capture("listed-beyond", "@('B1:C3', 3, 2)", None, &beyond);
    assert_eq!(result["spill_members"], json!(members));
    let inventory: Vec<&Value> = result["failures"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|failure| failure["stage"] == STAGE_INVENTORY)
        .collect();
    assert_eq!(
        inventory,
        [
            &json!({"stage": "inventory", "sheet": "Sheet1", "row": 4, "column": 2, "message": "Excel reports a formula here that the package does not hold"})
        ]
    );
    let record = record.unwrap_or_else(|e| panic!("{e}\n{result}"));
    let refused = verdicts(&record).unwrap_err();
    assert!(
        refused.contains("records an inventory failure at Sheet1!B4"),
        "{refused}"
    );
    println!(
        "NOTE: SpecialCells listing C1, B2, C2, B3 and C3 of B1's spill B1:C3: spill members, no inventory failure, judged; a listed B4 outside it: the inventory failure {}, and calc-verdict refuses: {refused}",
        inventory[0]["message"]
    );
}

// --- legacy array formulas ----------------------------------------------------------------------

/// The package reader lists every cell of a legacy array formula's range: on
/// `worker/legacy-array.xlsx`, the anchor B1 of B1:B3 (t="array", no cell metadata) and the
/// single-cell C1 as the package holds them, D1, and then B2 and B3, which the package holds only
/// values for, as members naming their anchor B1. On `worker/legacy-array-conflict.xlsx`, whose B2
/// holds a formula of its own inside B1's range, B2 is listed once, as its own formula, and is an
/// inventory failure.
#[test]
fn the_package_reader_lists_each_cell_of_a_legacy_array() {
    let read = read_package(&fixture("worker/legacy-array.xlsx"));
    let member = |row: u32| json!({"sheet": "Sheet1", "row": row, "column": 2, "type": "array_member", "si": null, "ref": null, "cm": null, "dynamic_array_metadata": false, "anchor": "B1"});
    assert_eq!(
        read["formulas"],
        json!([
            {"sheet": "Sheet1", "row": 1, "column": 2, "type": "array", "si": null, "ref": "B1:B3", "cm": null, "dynamic_array_metadata": false},
            {"sheet": "Sheet1", "row": 1, "column": 3, "type": "array", "si": null, "ref": "C1", "cm": null, "dynamic_array_metadata": false},
            {"sheet": "Sheet1", "row": 1, "column": 4, "type": "normal", "si": null, "ref": null, "cm": null, "dynamic_array_metadata": false},
            member(2),
            member(3)
        ])
    );
    assert_eq!(read["failures"], json!([]));
    assert_eq!(read["refusals"], json!([]));

    let read = read_package(&fixture("worker/legacy-array-conflict.xlsx"));
    let listed: Vec<String> = read["formulas"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            format!(
                "R{}C{} {}",
                f["row"],
                f["column"],
                f["type"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            "R1C2 array",
            "R1C3 array",
            "R1C4 normal",
            "R2C2 normal",
            "R3C2 array_member"
        ]
    );
    assert_eq!(
        read["failures"],
        json!([{"stage": "inventory", "sheet": "Sheet1", "row": 2, "column": 2, "message": "the cell lies in the range B1:B3 of the legacy array formula at Sheet1!B1 and holds a formula of its own or lies in the range of another legacy array formula or data table"}])
    );
}

/// The worker's result for `worker/legacy-array.xlsx` against the stand-in Excel, in which every
/// cell of B1's legacy array B1:B3 is a formula cell showing its own element (10, 20, 30), C1
/// shows 6 and D1 2, none spilling; its SaveAs writes the input's bytes, and its SpecialCells lists
/// the five formula cells and then the cells `listed` (row, column). The result's conversion.
fn legacy_capture(name: &str, listed: &[(u32, u32)]) -> (Value, Result<CaptureRecord, String>) {
    let listed: String = listed
        .iter()
        .map(|(row, column)| format!("[void]$global:StandinListed.Add(@({row}, {column}))\n"))
        .collect();
    let setup = format!(
        "$global:StandinCells = @(@(1, 2, '=A1:A3*10', 10.0, $null), @(2, 2, '=A1:A3*10', 20.0, $null), @(3, 2, '=A1:A3*10', 30.0, $null), @(1, 3, '=SUM(A1:A3)', 6.0, $null), @(1, 4, '=A1+1', 2.0, $null))\n$global:StandinSaved = $null\n$global:StandinListed = New-Object System.Collections.ArrayList\n{listed}{FULL_STANDIN}"
    );
    let (code, lines, result) = run_main_flow(name, &fixture("worker/legacy-array.xlsx"), &setup);
    assert_eq!(code, Some(0), "{lines:?}\n{result}");
    assert_eq!(result["fatal"], Value::Null, "{result}");
    let frame = Frame {
        case: "legacy-standin".to_owned(),
        source: format!(
            "sha256:{}",
            crate::sha256_hex(&std::fs::read(fixture("worker/legacy-array.xlsx")).unwrap())
        ),
        tool: crate::capture_tool_revision(None),
        revision: 0,
        operations: Vec::new(),
        output_package: None,
        failures: Vec::new(),
    };
    let record = convert::convert(&result, frame);
    (result, record)
}

/// Each cell of a legacy array formula is a formula cell of the record, as Excel lists it: the
/// worker reads the members B2 and B3 like any inventory cell (their Value2 after the rebuild, the
/// saved package's caches and the Value2 after reopening), Excel's listing of them is no inventory
/// failure, the conversion records B1 and C1 as anchors and B2 and B3 as members of B1, and
/// calc-verdict judges each by its own value: matched when calculated as Excel shows it, B3
/// mismatched when calculated as the anchor's value. A further cell Excel lists outside every
/// range (B4) is still an inventory failure, and calc-verdict refuses the record.
#[test]
fn each_cell_of_a_legacy_array_is_read_recorded_and_judged() {
    let (result, record) = legacy_capture("legacy", &[]);
    assert_eq!(
        failure_messages(&result, STAGE_INVENTORY),
        Vec::<String>::new()
    );
    for (row, value) in [(2u64, 20f64), (3, 30.0)] {
        for list in ["cells", "reopened"] {
            let read = result[list]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["row"] == row && entry["column"] == 2)
                .unwrap_or_else(|| panic!("{list} holds no B{row}: {result}"));
            assert_eq!(
                read["value"],
                json!({"number": format!("{:016x}", value.to_bits())}),
                "{list} B{row}"
            );
            assert_eq!(read["spill"], Value::Null, "{list} B{row}");
        }
    }
    let record = record.unwrap_or_else(|e| panic!("the legacy capture converts: {e}\n{result}"));
    let session = record.session.as_ref().unwrap();
    let member = FormulaKind::LegacyArrayMember {
        anchor_row: 1,
        anchor_column: 2,
    };
    let inventory: Vec<(String, FormulaKind)> = record
        .formulas
        .iter()
        .map(|formula| (formula.cell.a1(), formula.kind))
        .collect();
    assert_eq!(
        inventory,
        [
            (
                "B1".to_owned(),
                FormulaKind::LegacyArrayAnchor {
                    rows: 3,
                    columns: 1
                }
            ),
            (
                "C1".to_owned(),
                FormulaKind::LegacyArrayAnchor {
                    rows: 1,
                    columns: 1
                }
            ),
            ("D1".to_owned(), FormulaKind::Ordinary),
            ("B2".to_owned(), member),
            ("B3".to_owned(), member),
        ]
    );
    let all = ["B1", "C1", "D1", "B2", "B3"];
    assert_eq!(a1s(&record.observations), all);
    assert_eq!(a1s(&session.saved), all);
    assert_eq!(a1s(&session.reopened), all);
    assert!(
        session
            .failures
            .iter()
            .all(|failure| failure.stage == "environment"),
        "{:?}",
        session.failures
    );
    let encoded = record.encode().unwrap();
    assert_eq!(CaptureRecord::decode(encoded.as_bytes()).unwrap(), record);

    let at = |row: u32, column: u32| CellAddress::new("Sheet1", row, column).unwrap();
    let mut calculated: Vec<(CellAddress, CalcValue)> = [
        (at(1, 2), 10.0),
        (at(1, 3), 6.0),
        (at(1, 4), 2.0),
        (at(2, 2), 20.0),
        (at(3, 2), 30.0),
    ]
    .into_iter()
    .map(|(cell, value)| (cell, CalcValue::number(value)))
    .collect();
    let (judged, _) = verdicts_of(&record, &calculated).unwrap();
    assert_eq!(
        judged,
        all.iter()
            .map(|a1| ((*a1).to_owned(), "matched"))
            .collect::<Vec<_>>()
    );
    calculated[4].1 = CalcValue::number(10.0);
    let (judged, _) = verdicts_of(&record, &calculated).unwrap();
    assert_eq!(judged[4], ("B3".to_owned(), "mismatched"));

    let (result, record) = legacy_capture("legacy-beyond", &[(4, 2)]);
    assert_eq!(
        failure_messages(&result, STAGE_INVENTORY),
        ["Excel reports a formula here that the package does not hold"]
    );
    let record = record.unwrap_or_else(|e| panic!("{e}\n{result}"));
    let refused = verdicts_of(&record, &[]).unwrap_err();
    assert!(
        refused.contains("records an inventory failure at Sheet1!B4"),
        "{refused}"
    );
    println!(
        "NOTE: legacy array B1:B3 (=A1:A3*10) and C1: the worker read B2 and B3 as inventory cells (Value2 20 and 30 after the rebuild and after reopening), no inventory failure; recorded as anchors B1 (3 by 1) and C1 (1 by 1) and members B2, B3 of B1; judged each by its own value (B3 calculated as 10 is mismatched); a listed B4 outside the range is an inventory failure and calc-verdict refuses: {refused}"
    );
}

// --- text checks of the script ------------------------------------------------------------------

/// The trimmed line on which each catch with an empty body starts: `catch {}`, `catch { }`, a
/// body of white space or comments over several lines, with or without exception types.
fn empty_catches(script: &str) -> Vec<String> {
    let bytes = script.as_bytes();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = script[from..].find("catch").map(|i| i + from) {
        from = at + 5;
        let word_start =
            at == 0 || !(bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
        let word_end = bytes
            .get(at + 5)
            .is_none_or(|b| !(b.is_ascii_alphanumeric() || *b == b'_'));
        if !word_start || !word_end {
            continue;
        }
        let mut i = at + 5;
        // Exception types: [Type], [Type], ...
        loop {
            while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
                i += 1;
            }
            match (bytes.get(i), script[i..].find(']')) {
                (Some(b'['), Some(end)) => i += end + 1,
                _ => break,
            }
        }
        if bytes.get(i) != Some(&b'{') {
            continue;
        }
        i += 1;
        // White space and comments only, up to the closing brace.
        loop {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if script[i..].starts_with("<#") {
                i = script[i..]
                    .find("#>")
                    .map_or(bytes.len(), |end| i + end + 2);
            } else if bytes.get(i) == Some(&b'#') {
                i = script[i..].find('\n').map_or(bytes.len(), |end| i + end);
            } else {
                break;
            }
        }
        if bytes.get(i) == Some(&b'}') {
            let start = script[..at].rfind('\n').map_or(0, |n| n + 1);
            let end = script[at..].find('\n').map_or(script.len(), |n| at + n);
            found.push(script[start..end].trim().to_owned());
        }
    }
    found
}

/// The lint finds every form of an empty catch, and no catch that does something.
#[test]
fn the_empty_catch_lint_finds_every_form() {
    let sample = "try { a } catch {}\n\
                  try { b } catch { }\n\
                  try { c } catch {\n    \n}\n\
                  try { d } catch [System.IO.IOException], [System.Exception] { }\n\
                  try { e } catch { # nothing to do\n}\n\
                  try { f } catch {<# nothing #>}\n\
                  try { g } catch { Add-Failure 'x' $null 'y' }\n\
                  # every catch below records a failure\n\
                  $catches = 1; try { h } catch{throw}\n";
    assert_eq!(
        empty_catches(sample),
        [
            "try { a } catch {}",
            "try { b } catch { }",
            "try { c } catch {",
            "try { d } catch [System.IO.IOException], [System.Exception] { }",
            "try { e } catch { # nothing to do",
            "try { f } catch {<# nothing #>}",
        ]
    );
}

/// Every catch of the worker records a failure or refuses the capture, except the cleanup of its
/// finally block: an empty catch appears only on the three cleanup lines.
#[test]
fn the_worker_swallows_no_error_outside_its_cleanup() {
    assert_eq!(
        empty_catches(WORKER_SCRIPT),
        [
            "if ($null -ne $workbook) { try { $workbook.Close($false) } catch { } }",
            "try { $excel.Quit() } catch { }",
            "try { [void][System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($excel) } catch { }",
        ]
    );
}

/// The members `var.member = ...` assigns in `script`, as "var.member" (`var` without its `$`),
/// for a plain or a dynamic member name; `==` is not an assignment.
fn member_assignments(script: &str) -> Vec<String> {
    let bytes = script.as_bytes();
    let name = |at: usize| {
        let end = script[at..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
            .map_or(script.len(), |n| at + n);
        (end, &script[at..end])
    };
    let mut found = Vec::new();
    for (at, _) in script.match_indices('$') {
        let (dot, var) = name(at + 1);
        if var.is_empty() || bytes.get(dot) != Some(&b'.') {
            continue;
        }
        let (end, member) = name(dot + 1);
        let mut i = end;
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if !member.is_empty() && bytes.get(i) == Some(&b'=') && bytes.get(i + 1) != Some(&b'=') {
            found.push(format!("{var}.{member}"));
        }
    }
    found
}

/// The worker changes no setting that persists beyond its Excel instance: the only members of
/// Excel's Application it assigns are the session settings and the calculation settings, it
/// assigns no AskToUpdateLinks, IgnoreRemoteRequests, Connect, Installed or FeatureInstall on any
/// object and no member by a computed name, and it writes no registry value.
#[test]
fn the_worker_assigns_only_session_settings() {
    let assigned = member_assignments(WORKER_SCRIPT);
    let mut excel: Vec<&str> = assigned
        .iter()
        .filter_map(|a| a.strip_prefix("excel."))
        .collect();
    excel.sort_unstable();
    // Calculation is assigned in two places: before a manual capture opens the workbook, and
    // after any other opens it (dc-10y).
    excel.dedup();
    assert_eq!(
        excel,
        [
            "Calculation",
            "DisplayAlerts",
            "EnableEvents",
            "Interactive",
            "Iteration",
            "MaxChange",
            "MaxIterations",
            "ScreenUpdating",
            "Visible",
        ]
    );
    for forbidden in [
        "AutomationSecurity",
        "AskToUpdateLinks",
        "IgnoreRemoteRequests",
        "Connect",
        "Installed",
        "FeatureInstall",
    ] {
        let hits: Vec<&String> = assigned
            .iter()
            .filter(|a| a.ends_with(&format!(".{forbidden}")))
            .collect();
        assert!(hits.is_empty(), "{forbidden} is assigned: {hits:?}");
    }
    let computed: Vec<&String> = assigned.iter().filter(|a| a.contains(".$")).collect();
    assert!(
        computed.is_empty(),
        "members assigned by a computed name: {computed:?}"
    );
    for writer in [
        "Set-ItemProperty",
        "New-ItemProperty",
        "Remove-ItemProperty",
        "reg add",
        "reg.exe",
    ] {
        assert!(!WORKER_SCRIPT.contains(writer), "the worker uses {writer}");
    }
    // The check reads assignments as they are written.
    assert_eq!(
        member_assignments(
            "$excel.AskToUpdateLinks = $false\n$a.Installed= $true\n$excel.$name = 1\nif ($x.Connect == 1) {}"
        ),
        ["excel.AskToUpdateLinks", "a.Installed", "excel.$name"]
    );
}

/// The worker creates its Excel through `New-Object -ComObject` and through nothing else, so the
/// main-flow tests' guard on New-Object stands between them and Excel.
#[test]
fn the_worker_creates_com_objects_only_through_new_object() {
    assert_eq!(WORKER_SCRIPT.matches("-ComObject").count(), 1);
    assert!(WORKER_SCRIPT.contains("$excel = New-Object -ComObject Excel.Application"));
    for other in [
        "GetTypeFromProgID",
        "GetTypeFromCLSID",
        "CreateInstance",
        "GetActiveObject",
        "BindToMoniker",
        "CreateObject",
        "Start-Process",
        "Invoke-Item",
    ] {
        assert!(!WORKER_SCRIPT.contains(other), "the worker uses {other}");
    }
}

// --- fixture provenance -------------------------------------------------------------------------

/// The files `tests/fixtures/make-fixtures.ps1` writes, relative to its output directory.
const GENERATED: [&str; 14] = [
    "infra_m1_2/link-target.xlsx",
    "infra_m1_2/replay.xlsx",
    "infra_m1_2/untrusted-link.xlsx",
    "worker/dynamic-array.xlsx",
    "worker/external-content.xlsx",
    "worker/external-functions.xlsx",
    "worker/legacy-array-conflict.xlsx",
    "worker/legacy-array.xlsx",
    "worker/macro-sheet.xlsm",
    "worker/pivot-refresh.xlsx",
    "worker/query-table-refresh.xlsx",
    "worker/spill-saved-cm.xlsx",
    "worker/spill-saved-values.xlsx",
    "worker/spill.xlsx",
];

/// Every generated fixture under `tests/fixtures/` is, byte for byte, what the committed generator
/// writes, and the generator writes nothing else.
#[test]
fn fixtures_are_the_output_of_their_generator() {
    let dir = scratch("generated");
    let out = powershell(&[
        "-File",
        &fixture("make-fixtures.ps1").to_string_lossy(),
        "-OutDir",
        &dir.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "the generator failed: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let mut written = Vec::new();
    let mut pending = vec![dir.clone()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                written.push(
                    path.strip_prefix(&dir)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    written.sort();
    assert_eq!(written, GENERATED);
    for name in GENERATED {
        let generated = std::fs::read(dir.join(name)).unwrap();
        let committed = std::fs::read(fixture(name)).unwrap();
        assert!(
            generated == committed,
            "{name}: the committed fixture is not the generator's output (sha256 {} committed, {} generated)",
            crate::sha256_hex(&committed),
            crate::sha256_hex(&generated)
        );
    }
    // Every Override of each package's [Content_Types].xml names one part of the package (a
    // PartName that is "/" and the part's name) and gives it a non-empty ContentType: a generator
    // that joins the name and the type into one PartName writes a package Excel refuses to open.
    let names: Vec<String> = GENERATED
        .iter()
        .map(|name| quoted(&dir.join(name).to_string_lossy()))
        .collect();
    let listing = powershell(&[
        "-Command",
        &format!(
            "Add-Type -AssemblyName System.IO.Compression.FileSystem; foreach ($path in @({})) {{ $zip = [System.IO.Compression.ZipFile]::OpenRead($path); try {{ $parts = @($zip.Entries | ForEach-Object {{ '/' + $_.FullName }}); $entry = $zip.GetEntry('[Content_Types].xml'); $reader = New-Object System.IO.StreamReader($entry.Open()); $xml = [xml]$reader.ReadToEnd(); $reader.Dispose(); foreach ($node in $xml.Types.ChildNodes) {{ if ($node.LocalName -eq 'Override') {{ [Console]::Out.WriteLine([System.IO.Path]::GetFileName($path) + [char]9 + $node.GetAttribute('PartName') + [char]9 + $node.GetAttribute('ContentType') + [char]9 + ($parts -contains $node.GetAttribute('PartName'))) }} }} }} finally {{ $zip.Dispose() }} }}",
            names.join(", ")
        ),
    ]);
    assert!(
        listing.status.success(),
        "the content types could not be read: {}",
        String::from_utf8_lossy(&listing.stderr)
    );
    let overrides: Vec<Vec<String>> = String::from_utf8_lossy(&listing.stdout)
        .lines()
        .map(|line| line.split('\t').map(str::to_owned).collect())
        .collect();
    assert!(
        overrides.len() >= GENERATED.len(),
        "every package has overrides: {overrides:?}"
    );
    for entry in &overrides {
        assert_eq!(entry.len(), 4, "{entry:?}");
        assert!(
            entry[1].starts_with('/') && !entry[1].contains(' ') && !entry[2].is_empty(),
            "{}: the override {:?} has the content type {:?}",
            entry[0],
            entry[1],
            entry[2]
        );
        assert_eq!(
            entry[3], "True",
            "{}: the override {:?} names no part of the package",
            entry[0], entry[1]
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
