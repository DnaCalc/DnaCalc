//! Unit tests of the worker script, Windows only, as the worker is. None starts Excel.
//!
//! - The package reader and the pre-open refusals: the embedded worker script is dot-sourced into
//!   Windows PowerShell, which defines its functions and starts nothing, and `Read-Package` and
//!   `Get-PackageRefusals` are run on the fixture packages under `tests/fixtures/`.
//! - The main flow: the script is run as the adapter runs it (`-Request`, `-Result`), from a
//!   harness that first shadows `New-Object` with a guard. The guard passes every .NET object
//!   through and creates no COM object: `New-Object -ComObject` returns the test's stand-in
//!   Application when the test gives one, and throws otherwise, so a regression that let a
//!   refused package reach Excel's start fails the test instead of starting Excel. The script
//!   creates its Excel through `New-Object -ComObject` only, which a test here checks.
//! - Text checks of the script: the empty catches, and the settings it assigns.
//! - The fixtures are the output of their committed generator, `tests/fixtures/make-fixtures.ps1`.

use crate::convert::{self, Frame};
use crate::{WORKER_SCRIPT, windows_powershell};
use calc_verdict::FormulaKind;
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

/// A macro sheet, whose relationship type is in Microsoft's namespace, is recorded as an inventory
/// failure naming it, and the worksheet beside it is still read: the package reader does not end
/// the capture on it.
#[test]
fn a_macro_sheet_is_an_inventory_failure_and_not_fatal() {
    let read = read_package(&fixture("worker/macro-sheet.xlsm"));
    assert_eq!(
        read["sheets"],
        json!([{"name": "Sheet1", "visibility": "visible"}])
    );
    assert_eq!(
        read["formulas"],
        json!([{"sheet": "Sheet1", "row": 1, "column": 1, "type": "normal", "si": null, "ref": null, "cm": null}])
    );
    assert_eq!(
        read["failures"],
        json!([{
            "stage": "inventory",
            "message": "sheet Macro1 is a xlMacrosheet (relationship type http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet), whose cells are not captured"
        }])
    );
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
            {"name": "Refreshing", "type": "1", "refresh_on_load": true},
            {"name": "Quiet", "type": "1", "refresh_on_load": false}
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
        json!([{"name": "WebQuery", "type": "4", "refresh_on_load": false}])
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
/// saved as Excel with dynamic arrays saves a single-cell formula (t="array", cm="1", ref B1), is
/// an ordinary formula flagged as dynamic; C1 is an ordinary formula, not flagged.
#[test]
fn a_package_with_a_single_cell_dynamic_array_formula_converts() {
    let read = read_package(&fixture("worker/dynamic-array.xlsx"));
    assert_eq!(
        read["formulas"],
        json!([
            {"sheet": "Sheet1", "row": 1, "column": 2, "type": "array", "si": null, "ref": "B1", "cm": "1"},
            {"sheet": "Sheet1", "row": 1, "column": 3, "type": "normal", "si": null, "ref": null, "cm": null}
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
            {"sheet": "Sheet1", "row": 1, "column": 2, "formula": units("=SUM(A1:A2*2)"), "value": {"number": format!("{:016x}", 6f64.to_bits())}},
            {"sheet": "Sheet1", "row": 1, "column": 3, "formula": units("=A1+1"), "value": {"number": format!("{:016x}", 2f64.to_bits())}}
        ],
        "saved": [], "reopened": [], "failures": []
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

/// The harness every main-flow test runs: it shadows `New-Object` with the guard (module
/// documentation), defines the stand-in builders, dot-sources the file `$Setup`, then runs the
/// worker as the adapter
/// runs it, and exits with the worker's exit code. Lines the guard and the stand-ins print start
/// with `STANDIN:`.
const HARNESS: &str = r#"param([string]$Worker, [string]$Request, [string]$Result, [string]$Setup)
$global:StandinApplication = $null
function global:Write-Standin([string]$Text) { [Console]::Out.WriteLine('STANDIN: ' + $Text); [Console]::Out.Flush() }
function global:New-Object {
    param(
        [Parameter(Position = 0)][string]$TypeName,
        [Parameter(Position = 1)][object[]]$ArgumentList,
        [string]$ComObject,
        [hashtable]$Property,
        [switch]$Strict
    )
    if ($PSBoundParameters.ContainsKey('ComObject')) {
        Write-Standin "New-Object -ComObject $ComObject"
        if ($null -eq $global:StandinApplication) { throw "the test's guard creates no COM object ($ComObject)" }
        return $global:StandinApplication
    }
    $made = Microsoft.PowerShell.Utility\New-Object @PSBoundParameters
    return , $made
}
# A stand-in Excel Application: a hidden window of this process stands for Excel's, so its PID is
# what the worker reports; Workbooks.Open and Quit print what they were called with.
# Each setting named in $Unreadable is one whose read fails (a getter that throws, which
# PowerShell reads as nothing) and whose write is ignored.
function global:New-StandinApplication($ComAddIns, $AddIns, [string[]]$Unreadable = @()) {
    Add-Type -AssemblyName System.Windows.Forms
    $global:StandinForm = Microsoft.PowerShell.Utility\New-Object System.Windows.Forms.Form
    $workbooks = [pscustomobject]@{}
    $workbooks | Add-Member -MemberType ScriptMethod -Name Open -Value { Write-Standin ('Workbooks.Open ' + $args[0]); throw 'the stand-in Application opens no workbook' }
    $application = [pscustomobject]@{
        Hwnd = $global:StandinForm.Handle.ToInt64()
        Visible = $true; ScreenUpdating = $true
        COMAddIns = @($ComAddIns); AddIns2 = @($AddIns); RegisteredFunctions = $null
        Workbooks = $workbooks
    }
    $settings = [ordered]@{ DisplayAlerts = $true; EnableEvents = $true; Interactive = $true; AutomationSecurity = 1 }
    foreach ($name in $settings.Keys) {
        if ($Unreadable -contains $name) {
            $application | Add-Member -MemberType ScriptProperty -Name $name -Value { throw 'the stand-in cannot read this setting' } -SecondValue { }
        } else {
            $application | Add-Member -MemberType NoteProperty -Name $name -Value $settings[$name]
        }
    }
    $application | Add-Member -MemberType ScriptMethod -Name Quit -Value { Write-Standin 'Quit' }
    return $application
}
function global:New-StandinComAddIn([string]$ProgId, $Connect) {
    $addIn = [pscustomobject]@{ ProgId = $ProgId }
    if ($null -eq $Connect) {
        $addIn | Add-Member -MemberType ScriptProperty -Name Connect -Value { throw 'the stand-in cannot read Connect' }
    } else {
        $addIn | Add-Member -MemberType NoteProperty -Name Connect -Value $Connect
    }
    return $addIn
}
function global:New-StandinAddIn([string]$FullName, [bool]$IsOpen, [bool]$Installed) {
    return [pscustomobject]@{ FullName = $FullName; IsOpen = $IsOpen; Installed = $Installed }
}
. $Setup
& $Worker -Request $Request -Result $Result
exit $LASTEXITCODE
"#;

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

/// A package the worker refuses never reaches Excel's start: run as the adapter runs it, the
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

/// An Excel with add-ins loaded is refused before the workbook is opened, nothing switched off:
/// a stand-in Application reporting a connected COM add-in, a COM add-in whose Connect cannot be
/// read, a real COM object (a closed ADODB.Recordset, which has neither ProgId nor Connect) in the
/// COM add-in list, an add-in open in the instance, and a DisplayAlerts that cannot be read back
/// makes the worker report the stand-in's PID, refuse the capture with each named (and neither the
/// idle add-ins nor one installed in the profile but not open), never call Workbooks.Open, and
/// quit the stand-in. A read that fails, through
/// PowerShell's property read or through COM, never passes for a state that is off.
#[test]
fn an_excel_with_loaded_add_ins_is_refused_before_the_workbook_opens() {
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
    assert!(lines.contains(&"STANDIN: Quit".to_owned()), "{lines:?}");
    let fatal = result["fatal"].as_str().unwrap();
    assert!(
        fatal.starts_with("refused before the workbook was opened: this Excel instance is not isolated (a capture needs an Excel profile with no loaded add-ins, and nothing was switched off)"),
        "{fatal}"
    );
    let isolation = failure_messages(&result, "isolation");
    assert_eq!(isolation.len(), 5, "{isolation:?}");
    assert_eq!(
        isolation[0],
        "DisplayAlerts reads back as nothing after it was set to False; the workbook was not opened"
    );
    assert_eq!(
        isolation[1],
        "COM add-in Contoso.Loaded is connected; the workbook was not opened"
    );
    assert_eq!(
        isolation[2],
        "the state of COM add-in Contoso.Broken cannot be read: Connect reads as nothing, not a boolean; the workbook was not opened"
    );
    assert!(
        isolation[3].starts_with("the state of COM add-in 4 cannot be read: ")
            && isolation[3].contains("Unknown name"),
        "{}",
        isolation[3]
    );
    assert_eq!(
        isolation[4],
        "add-in C:\\AddIns\\Tool.xlam is open; the workbook was not opened"
    );
    assert!(!fatal.contains("Idle"), "{fatal}");
    assert!(!fatal.contains("Installed.xll"), "{fatal}");
    assert_eq!(result["cells"], json!([]));
}

/// The control of the test above: a stand-in with only idle add-ins passes the isolation and the
/// worker goes on to open the workbook (the stand-in's Workbooks.Open is called, and throws), so
/// the stand-in does show an open when one happens.
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

/// A fuller stand-in Excel for `infra_m1_2/untrusted-link.xlsx`: an Application with the settings
/// and methods the main flow reads and calls, a workbook whose Sheet1!A1 and B1 hold 111 and 112
/// and whose SaveAs copies the input package to the output path. The collections are a small C#
/// class, so they can be both enumerated and indexed as Excel's are.
const FULL_STANDIN: &str = r#"
Add-Type -TypeDefinition @'
using System.Collections;
using System.Collections.Generic;
public class StandinItems : IEnumerable {
    private readonly List<object> order = new List<object>();
    private readonly Dictionary<string, object> named = new Dictionary<string, object>();
    public void Add(string key, object item) { order.Add(item); named[key] = item; }
    public object Item(object key) { if (key is int) { return order[(int)key - 1]; } return named[(string)key]; }
    public object Item(object row, object column) { return named[row + "," + column]; }
    public IEnumerator GetEnumerator() { return order.GetEnumerator(); }
}
'@
$global:StandinApplication = New-StandinApplication @() @()
$app = $global:StandinApplication
foreach ($pair in @(@('Calculation', -4105), @('Iteration', $false), @('MaxIterations', 100), @('MaxChange', 0.001), @('CalculationState', 0), @('Version', '16.0'), @('Build', 19127), @('OperatingSystem', 'Windows (64-bit) NT 10.00'), @('UseSystemSeparators', $true), @('CalculationVersion', 191029), @('AskToUpdateLinks', $true), @('IgnoreRemoteRequests', $false))) {
    $app | Add-Member -MemberType NoteProperty -Name $pair[0] -Value $pair[1]
}
$app | Add-Member -MemberType ScriptMethod -Name CalculateFullRebuild -Value { Write-Standin 'CalculateFullRebuild' }
$app | Add-Member -MemberType ScriptMethod -Name International -Value { param($index) return [string]$index }
$languages = [pscustomobject]@{}
$languages | Add-Member -MemberType ScriptMethod -Name LanguageID -Value { param($kind) return 1033 }
$app | Add-Member -MemberType NoteProperty -Name LanguageSettings -Value $languages
$cells = New-Object StandinItems
$formulaCells = @()
foreach ($spec in @(@(1, 1, '=[1]Sheet1!$A$1', 111.0), @(1, 2, '=A1+1', 112.0))) {
    $cell = [pscustomobject]@{ Row = $spec[0]; Column = $spec[1]; HasFormula = $true; Formula2 = $spec[2]; Value2 = $spec[3] }
    $cells.Add(([string]$spec[0] + ',' + [string]$spec[1]), $cell)
    $formulaCells += $cell
}
$used = [pscustomobject]@{ HasFormula = $true }
$used | Add-Member -MemberType NoteProperty -Name FormulaCells -Value $formulaCells
$used | Add-Member -MemberType ScriptMethod -Name SpecialCells -Value { param($type) return [pscustomobject]@{ Cells = $this.FormulaCells } }
$sheet = [pscustomobject]@{ Name = 'Sheet1'; Cells = $cells; UsedRange = $used }
$sheets = New-Object StandinItems
$sheets.Add('Sheet1', $sheet)
$book = [pscustomobject]@{ Worksheets = $sheets; Connections = @(); CompatibilityVersion = 16; CalculationVersion = 191029; Date1904 = $false; PrecisionAsDisplayed = $false; HasVBProject = $false }
$book | Add-Member -MemberType ScriptMethod -Name LinkSources -Value { param($kind) if ($kind -eq 1) { return , @('C:\nowhere\link-target.xlsx') } return $null }
$book | Add-Member -MemberType ScriptMethod -Name SaveAs -Value { param($path, $format) Write-Standin "SaveAs $format"; Copy-Item -LiteralPath $global:StandinInput -Destination $path }
$book | Add-Member -MemberType ScriptMethod -Name Close -Value { Write-Standin 'Close' }
$global:StandinBook = $book
$global:StandinInput = $null
$app.Workbooks | Add-Member -MemberType ScriptMethod -Name Open -Value { Write-Standin ('Workbooks.Open ' + [System.IO.Path]::GetFileName($args[0]) + ' from ' + [System.IO.Path]::GetFileName([System.IO.Path]::GetDirectoryName($args[0]))); if ($null -eq $global:StandinInput) { $global:StandinInput = $args[0] }; return $global:StandinBook } -Force
"#;

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
        .collect();
    assert_eq!(
        standin,
        [
            "New-Object -ComObject Excel.Application",
            "Workbooks.Open untrusted-link.xlsx from in",
            "CalculateFullRebuild",
            "SaveAs 51",
            "Close",
            "Workbooks.Open untrusted-link.xlsx from out",
            "Close",
            "Quit"
        ]
    );
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
    assert_eq!(setting("automation_security"), Some("3"));
    assert_eq!(setting("display_alerts"), Some("false"));
    assert_eq!(setting("external_links"), Some("link-target.xlsx"));
    assert_eq!(setting("add_ins"), Some(""));
    assert_eq!(setting("calculation"), Some("automatic"));
    // The request's max_change (0.001) as set and read back, an indexed International and
    // LanguageID read, and the target's compatibility from both calculation versions.
    assert_eq!(setting("max_change"), Some("3f50624dd2f1a9fc"));
    assert_eq!(setting("iteration"), Some("false"));
    assert_eq!(setting("decimal_separator"), Some("3"));
    assert_eq!(setting("ui_language_id"), Some("1033"));
    assert_eq!(setting("calculation_version"), Some("191029"));
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
/// compatibility, and, with that given back, for the empty iteration.
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
        "{FULL_STANDIN}\nforeach ($name in @({})) {{\n    $app.PSObject.Properties.Remove($name)\n    $app | Add-Member -MemberType ScriptProperty -Name $name -Value {{ throw 'the stand-in cannot read this setting' }} -SecondValue {{ }}\n}}\n",
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
    assert_eq!(
        excel,
        [
            "AutomationSecurity",
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
const GENERATED: [&str; 9] = [
    "infra_m1_2/link-target.xlsx",
    "infra_m1_2/replay.xlsx",
    "infra_m1_2/untrusted-link.xlsx",
    "worker/dynamic-array.xlsx",
    "worker/external-content.xlsx",
    "worker/external-functions.xlsx",
    "worker/macro-sheet.xlsm",
    "worker/pivot-refresh.xlsx",
    "worker/query-table-refresh.xlsx",
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
    let _ = std::fs::remove_dir_all(&dir);
}
