//! Workbook VBA Subs and Functions through the command route (dc-vba-command-es70w): one
//! isolated invocation, one outcome, the policy's denials recorded even when VBA catches them.
use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy};
use calc_host_vba::{Outcome, ProcedureArg, ProcedureResult};
use std::rc::Rc;
use std::sync::Arc;
#[path = "../../calc-doc/tests/cdoc_m1_1_support/mod.rs"]
#[allow(dead_code)]
mod support;
#[path = "../../calc-doc/tests/cdoc_m1_1_support/vba_project.rs"]
mod vba_project;

const LOCAL: &[u8] = include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/local.xlsm");

const MODULE: &str = "\
Public Sub Fill()
 Application.Range(\"A1\").Value2 = 5
 Application.Range(\"B1\").Value2 = \"x\"
End Sub
Public Sub DefaultLet()
 Range(\"B2\") = 50
 Cells(3, 1) = 7
 Range(\"A4\").Offset(1, 0) = 9
 Sheet1.Cells(6, 1) = 11
 Sheet1.Range(\"C1\") = 12
End Sub
Public Function Add2(ByVal a As Double, ByVal b As Double) As Double
 Add2 = a + b
End Function
Public Sub Boom()
 Dim x As Long
 x = 0
 x = 1 \\ x
End Sub
Public Sub CaughtDenial()
 On Error Resume Next
 Dim o As Object
 Set o = CreateObject(\"WinHttp.WinHttpRequest.5.1\")
 Application.Range(\"A1\").Value2 = Err.Number
End Sub
Public Function WithDefault(Optional ByVal x As Double = 7) As Double
 WithDefault = x * 2
End Function
Public Sub ViaRun()
 Application.Range(\"A2\").Value2 = Application.Run(\"Add2\", 20, 22)
End Sub
Public Sub Store(ByVal cell As String, ByVal v As Variant)
 Application.Range(cell).Value2 = v
End Sub
Public Function Greet(ByVal who As String) As String
 Greet = \"hello \" & who
End Function
Public Sub ViaRunSub()
 Application.Run \"Store\", \"A4\", 7.5
 Application.Range(\"A5\").Value2 = Application.Run(\"Greet\", \"bob\")
End Sub
Public Sub ViaRunCaught()
 On Error Resume Next
 Application.Run \"Boom\"
 Application.Range(\"A6\").Value2 = Err.Number
End Sub
Public Sub ViaRunUncaught()
 Application.Run \"Boom\"
 Application.Range(\"A7\").Value2 = 1
End Sub
Public Sub RunForever()
 Application.Run \"RunForever\"
End Sub
Public Sub RunMissing()
 On Error Resume Next
 Application.Run \"NoSuchMacro\"
 Application.Range(\"A3\").Value2 = Err.Number
End Sub
Public Function Describe(ByVal n As Long, ByVal t As String, ByVal b As Boolean, ByVal v As Variant) As String
 Describe = CStr(n) & \"|\" & t & \"|\" & CStr(b) & \"|\" & CStr(v)
End Function
Public Function WhenIs(ByVal d As Date, ByVal c As Currency, ByVal s As Single, ByVal i As Integer, ByVal y As Byte) As Double
 WhenIs = CDbl(d) + CDbl(c) + CDbl(s) + i + y
End Function
Public Sub TakesRange(ByVal r As Range)
End Sub
Public Sub TakesRest(ParamArray rest() As Variant)
End Sub
Public Sub ExportChart()
 Dim frame As Object
 Application.Range(\"A1\").Value2 = \"Region\"
 Application.Range(\"B1\").Value2 = \"Q1\"
 Application.Range(\"A2\").Value2 = \"North\"
 Application.Range(\"B2\").Value2 = 10
 Application.Range(\"A3\").Value2 = \"South\"
 Application.Range(\"B3\").Value2 = 30
 Set frame = Application.ActiveSheet.ChartObjects.Add(10, 10, 300, 200)
 frame.Chart.SetSourceData Application.Range(\"A1:B3\")
 frame.Chart.Export \"out.png\", \"PNG\"
End Sub
Public Function EnumVariable() As Double
 Dim mode As XlCalculation
 mode = Application.Calculation
 EnumVariable = mode
End Function
Public Function PumpEvents() As Double
 DoEvents
 PumpEvents = 7
End Function
Public Sub NeedsWindow()
 Application.ActiveWindow.NewWindow
End Sub
";

fn opened(bytes: &[u8]) -> HostSession {
    HostSession::open(
        Arc::new(MemorySource::new(bytes.to_vec()).named("cmd.xlsm")),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap()
}

fn project(
    policy: calc_macro_security::VbaPolicy,
    backend: calc_vba::Backend,
) -> (HostSession, Rc<calc_host_vba::WorkbookEvents>) {
    project_with(MODULE, policy, backend)
}

fn project_with(
    module: &str,
    policy: calc_macro_security::VbaPolicy,
    backend: calc_vba::Backend,
) -> (HostSession, Rc<calc_host_vba::WorkbookEvents>) {
    try_project_with(module, policy, backend).unwrap()
}

fn try_project_with(
    module: &str,
    policy: calc_macro_security::VbaPolicy,
    backend: calc_vba::Backend,
) -> Result<(HostSession, Rc<calc_host_vba::WorkbookEvents>), String> {
    try_project_sources(module, "", policy, backend)
}

fn try_project_sources(
    module: &str,
    sheet: &str,
    policy: calc_macro_security::VbaPolicy,
    backend: calc_vba::Backend,
) -> Result<(HostSession, Rc<calc_host_vba::WorkbookEvents>), String> {
    let bytes = vba_project::replace_sources(
        LOCAL,
        &[
            ("Module1", module),
            ("PrivateModule", "Option Private Module\n"),
            ("Sheet1", sheet),
            ("ThisWorkbook", ""),
        ],
    );
    let mut session = opened(&bytes);
    let grant = calc_host_vba::ProjectGrant::trust_all("test", policy);
    let owner = calc_host_vba::prepare_file_project_with_events(
        &grant,
        session.host(),
        "cmd.xlsm",
        1_000_000,
        Some(backend),
        false,
    )
    .map_err(|e| format!("{e:?}"))?
    .ok_or("no project")?;
    owner.clone().install(&mut session);
    Ok((session, owner))
}

fn backends() -> Vec<calc_vba::Backend> {
    let mut backends = vec![calc_vba::Backend::Interpreter];
    #[cfg(not(target_arch = "wasm32"))]
    backends.push(calc_vba::Backend::Jit);
    backends
}

fn cell(session: &HostSession, row: u32, column: u32) -> Option<calc_host::CellInput> {
    let sheet = session.host().sheets()[0].id();
    session
        .host()
        .model()
        .cell(calc_value::CellKey::new(sheet, row, column).unwrap())
        .cloned()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_sub_mutates_the_workbook_and_a_function_returns() {
    for backend in backends() {
        let (mut session, owner) = project(calc_macro_security::VbaPolicy::Guarded, backend);
        let names: Vec<String> = owner.procedures().iter().map(|p| p.name.clone()).collect();
        assert!(names.contains(&"Fill".to_owned()) && names.contains(&"Add2".to_owned()), "{names:?}");
        let fill = owner.run_procedure(&mut session, "Module1.Fill", &[]);
        assert_eq!(fill.outcome, Outcome::Success(ProcedureResult::Empty), "{fill:?}");
        assert!(fill.denials.is_empty());
        assert!(matches!(cell(&session, 1, 1), Some(calc_host::CellInput::Number(n)) if n.get() == 5.0));
        let add = owner.run_procedure(
            &mut session,
            "add2",
            &[ProcedureArg::Number(40.0), ProcedureArg::Number(2.0)],
        );
        assert_eq!(add.outcome, Outcome::Success(ProcedureResult::Number(42.0)), "{add:?}");
        assert_eq!(add.entry, "Module1.Add2");
        assert_eq!(owner.invocations().len(), 2);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_runtime_error_and_a_missing_procedure_are_outcomes() {
    for backend in backends() {
        let (mut session, owner) = project(calc_macro_security::VbaPolicy::Guarded, backend);
        let boom = owner.run_procedure(&mut session, "Module1.Boom", &[]);
        assert!(matches!(boom.outcome, Outcome::RuntimeError { number: 11, .. }), "{boom:?}");
        let none = owner.run_procedure(&mut session, "Module1.Nope", &[]);
        assert!(matches!(none.outcome, Outcome::NotCallable(_)), "{none:?}");
        let args = owner.run_procedure(&mut session, "Module1.Add2", &[ProcedureArg::Number(1.0)]);
        assert!(matches!(args.outcome, Outcome::NotCallable(_)), "{args:?}");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_denial_vba_catches_is_still_recorded_and_an_unmodelled_member_is_a_typed_refusal() {
    for backend in backends() {
        let (mut session, owner) = project(calc_macro_security::VbaPolicy::Guarded, backend);
        let denied = owner.run_procedure(&mut session, "Module1.CaughtDenial", &[]);
        assert_eq!(denied.outcome, Outcome::Success(ProcedureResult::Empty), "{denied:?}");
        assert!(
            denied.denials.iter().any(|d| d.capability.contains("ComActivate") && d.layer == "vba-host"),
            "{denied:?}"
        );
        assert!(matches!(cell(&session, 1, 1), Some(calc_host::CellInput::Number(n)) if n.get() == 429.0));
        let needs = owner.run_procedure(&mut session, "Module1.NeedsWindow", &[]);
        match &needs.outcome {
            Outcome::Refused { member, owner, .. } => {
                assert!(!member.is_empty() && owner.starts_with("dc-"), "{needs:?}");
            }
            other => panic!("expected a typed refusal: {other:?}"),
        }
        // Isolated: the next call's record holds nothing of the earlier denial.
        let fill = owner.run_procedure(&mut session, "Module1.Fill", &[]);
        assert!(fill.denials.is_empty(), "{fill:?}");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn unavailable_excel_member_stays_visible_when_vba_catches_it() {
    let source = r#"
Public Function Probe() As Long
    On Error Resume Next
    Application.RegisterXLL "pa1-nonexistent.xll"
    Probe = Err.Number
End Function

Public Function Independent() As Boolean
    Independent = True
End Function
"#;
    for backend in backends() {
        let (mut session, owner) = project_with(source, calc_macro_security::VbaPolicy::Full, backend);
        let run = owner.run_procedure(&mut session, "Probe", &[]);
        #[cfg(not(windows))]
        assert!(matches!(
            run.outcome,
            Outcome::NotAvailableHere {
                capability: calc_host::portability::Need::Xll,
                route: Some("DNA Calc on Windows"),
                ..
            }
        ), "{run:?}");
        #[cfg(windows)]
        assert!(!matches!(run.outcome, Outcome::NotAvailableHere { .. }), "{run:?}");
        let independent = owner.run_procedure(&mut session, "Independent", &[]);
        assert_eq!(independent.outcome, Outcome::Success(ProcedureResult::Bool(true)));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn application_run_runs_another_procedure_nested_and_raises_excels_error_for_a_missing_macro() {
    for backend in backends() {
        let (mut session, owner) = project(calc_macro_security::VbaPolicy::Guarded, backend);
        let via = owner.run_procedure(&mut session, "Module1.ViaRun", &[]);
        assert_eq!(via.outcome, Outcome::Success(ProcedureResult::Empty), "{via:?}");
        assert!(matches!(cell(&session, 2, 1), Some(calc_host::CellInput::Number(n)) if n.get() == 42.0));
        // A Sub with arguments, a String result, and a nested run-time error, on each backend.
        let sub = owner.run_procedure(&mut session, "Module1.ViaRunSub", &[]);
        assert_eq!(sub.outcome, Outcome::Success(ProcedureResult::Empty), "{sub:?}");
        assert!(matches!(cell(&session, 4, 1), Some(calc_host::CellInput::Number(n)) if n.get() == 7.5));
        assert!(matches!(cell(&session, 5, 1), Some(calc_host::CellInput::Text(t)) if t.text().to_string_lossy() == "hello bob"));
        let caught = owner.run_procedure(&mut session, "Module1.ViaRunCaught", &[]);
        assert_eq!(caught.outcome, Outcome::Success(ProcedureResult::Empty), "{caught:?}");
        assert!(matches!(cell(&session, 6, 1), Some(calc_host::CellInput::Number(n)) if n.get() == 11.0));
        let uncaught = owner.run_procedure(&mut session, "Module1.ViaRunUncaught", &[]);
        assert!(
            matches!(&uncaught.outcome, Outcome::RuntimeError { number: 11, .. }),
            "{uncaught:?}"
        );
        assert!(!matches!(cell(&session, 7, 1), Some(calc_host::CellInput::Number(_))), "{backend:?} {uncaught:?}");
        let missing = owner.run_procedure(&mut session, "Module1.RunMissing", &[]);
        assert_eq!(missing.outcome, Outcome::Success(ProcedureResult::Empty), "{missing:?}");
        assert!(matches!(cell(&session, 3, 1), Some(calc_host::CellInput::Number(n)) if n.get() == 1004.0));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dna_view_select_uses_the_live_workbook_selection_without_a_history_step() {
    let module = r#"
Public Sub ReviewCells()
    Application.Run "DnaCalc.View.Select", Application.ActiveSheet.Name & "!B2:C3"
    If Application.ActiveCell.Row <> 2 Then Err.Raise 5
    Application.Run "dnacalc.select", "D4:E5"
    If Application.ActiveCell.Column <> 4 Then Err.Raise 5
End Sub
Public Sub InvalidAddress()
    Application.Run "DnaCalc.View.Select", "'Missing sheet'!A1"
End Sub
Public Sub InvalidArgument()
    Application.Run "DnaCalc.View.Select", True
End Sub
"#;
    for backend in backends() {
        let (mut session, owner) = project_with(module, calc_macro_security::VbaPolicy::Guarded, backend);
        let before = session.skin_history();
        let revision = session.host().revision();
        let run = owner.run_procedure(&mut session, "ReviewCells", &[]);
        assert_eq!(run.outcome, Outcome::Success(ProcedureResult::Empty), "{run:?}");
        assert!(run.denials.is_empty(), "{run:?}");
        for (procedure, expected) in [("InvalidAddress", 1004), ("InvalidArgument", 13)] {
            let run = owner.run_procedure(&mut session, procedure, &[]);
            assert!(matches!(run.outcome, Outcome::RuntimeError { number, .. } if number == expected), "{run:?}");
        }
        let selected = session.selection();
        let at = selected.active.unwrap();
        assert_eq!((at.row(), at.column()), (4, 4));
        assert_eq!(selected.areas.len(), 1);
        let last = selected.areas[0].last();
        assert_eq!((last.row(), last.column()), (5, 5));
        assert_eq!(session.skin_history(), before);
        assert_eq!(session.host().revision(), revision);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn an_omitted_optional_argument_takes_its_declared_default() {
    for backend in backends() {
        let (mut session, owner) = project(calc_macro_security::VbaPolicy::Guarded, backend);
        let none = owner.run_procedure(&mut session, "Module1.WithDefault", &[]);
        assert_eq!(none.outcome, Outcome::Success(ProcedureResult::Number(14.0)), "{none:?}");
        let given = owner.run_procedure(&mut session, "Module1.WithDefault", &[ProcedureArg::Number(2.0)]);
        assert_eq!(given.outcome, Outcome::Success(ProcedureResult::Number(4.0)), "{given:?}");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn required_scalar_parameters_get_a_documented_fixture() {
    for backend in backends() {
        let (mut session, owner) = project(calc_macro_security::VbaPolicy::Guarded, backend);
        let entries = owner.procedures();
        let entry = |name: &str| entries.iter().find(|p| p.name == name).unwrap();
        let fixture = entry("Describe").argument_fixture().expect("four scalar parameters");
        assert_eq!(
            fixture,
            vec![
                ProcedureArg::Number(1.0),
                ProcedureArg::Text("1".into()),
                ProcedureArg::Bool(true),
                ProcedureArg::Number(1.0)
            ]
        );
        let run = owner.run_procedure(&mut session, "Module1.Describe", &fixture);
        assert_eq!(run.outcome, Outcome::Success(ProcedureResult::Text("1|1|True|1".into())), "{run:?}");
        let when = entry("WhenIs").argument_fixture().expect("scalar parameters");
        let run = owner.run_procedure(&mut session, "Module1.WhenIs", &when);
        assert_eq!(run.outcome, Outcome::Success(ProcedureResult::Number(5.0)), "{run:?}");
        assert_eq!(entry("WithDefault").argument_fixture(), Some(vec![]));
        assert_eq!(entry("TakesRange").argument_fixture(), None);
        assert_eq!(entry("TakesRest").argument_fixture(), None);
    }
}

/// A Sub that runs itself through Application.Run ends: a nested run shares the call's step budget,
/// so recursion cannot outlast it (dc-oxh8w.34; Excel itself stops at its stack limit).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn application_run_recursion_ends_within_the_calls_step_budget() {
    for backend in backends() {
        let (mut session, owner) = project(calc_macro_security::VbaPolicy::Guarded, backend);
        let run = owner.run_procedure(&mut session, "Module1.RunForever", &[]);
        assert!(matches!(&run.outcome, Outcome::RuntimeError { number: 28, .. }), "{backend:?} {run:?}");
    }
}

/// A macro's Chart.Export reaches the host's file output: the project's host keeps the bytes
/// and the shell takes them (dc-oxh8w.37).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_macro_chart_export_writes_through_the_host_file_output() {
    fn stub(
        _: &calc_host::chart::ChartView,
        width: u32,
        height: u32,
        format: calc_host::chart::ExportFormat,
    ) -> Result<Vec<u8>, String> {
        Ok(format!("{format:?} {width}x{height}").into_bytes())
    }
    calc_host::chart::set_chart_exporter(stub);
    for backend in backends() {
        let (mut session, owner) = project(calc_macro_security::VbaPolicy::Full, backend);
        assert!(owner.take_output_files().is_empty());
        let run = owner.run_procedure(&mut session, "Module1.ExportChart", &[]);
        assert_eq!(run.outcome, Outcome::Success(ProcedureResult::Empty), "{run:?}");
        assert_eq!(owner.take_output_files(), [("out.png".to_owned(), b"Png 400x267".to_vec())]);
        assert!(owner.take_output_files().is_empty(), "taking the files empties the list");
    }
}

/// A variable declared `As XlCalculation` holds a Long (Excel's enumerations are Long), so
/// assigning `Application.Calculation` to it works; `DoEvents` runs and returns.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn an_excel_enum_typed_variable_holds_a_long_and_do_events_returns() {
    for backend in backends() {
        let (mut session, owner) = project(calc_macro_security::VbaPolicy::Guarded, backend);
        let mode = owner.run_procedure(&mut session, "Module1.EnumVariable", &[]);
        assert_eq!(mode.outcome, Outcome::Success(ProcedureResult::Number(-4105.0)), "{mode:?}");
        let pump = owner.run_procedure(&mut session, "Module1.PumpEvents", &[]);
        assert_eq!(pump.outcome, Outcome::Success(ProcedureResult::Number(7.0)), "{pump:?}");
    }
}

/// dc-cmruz: Let of the default member of a global `Range`/`Cells` call or of a method result
/// (`Range("A4").Offset(1, 0) = 9`) writes the cell, as in Excel.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn default_member_let_of_a_range_call_writes_the_cell() {
    for backend in backends() {
        let (mut session, owner) = project(calc_macro_security::VbaPolicy::Guarded, backend);
        let run = owner.run_procedure(&mut session, "Module1.DefaultLet", &[]);
        assert_eq!(run.outcome, Outcome::Success(ProcedureResult::Empty), "{backend:?} {run:?}");
        for (row, column, expected) in [(2, 2, 50.0), (3, 1, 7.0), (5, 1, 9.0), (6, 1, 11.0), (1, 3, 12.0)] {
            assert!(
                matches!(cell(&session, row, column), Some(calc_host::CellInput::Number(n)) if n.get() == expected),
                "{backend:?} ({row},{column}): {:?}",
                cell(&session, row, column)
            );
        }
    }
}

/// dc-cmruz: Excel enumeration members the corpus projects name resolve to the documented value
/// in a module with Option Explicit (an undefined name there is a compile error, not Empty).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn documented_excel_enumeration_members_compile_under_option_explicit() {
    let names: &[(&str, f64)] = &[
        ("xlTopToBottom", 1.0),
        ("xlLeftToRight", 2.0),
        ("xlDownThenOver", 1.0),
        ("xlOverThenDown", 2.0),
        ("xlDiagonalDown", 5.0),
        ("xlDiagonalUp", 6.0),
        ("xlUnderlineStyleNone", -4142.0),
        ("xlUnderlineStyleSingle", 2.0),
        ("xlThemeColorDark1", 1.0),
        ("xlThemeColorAccent1", 5.0),
        ("xlThemeColorAccent6", 10.0),
        ("xlCellTypeAllValidation", -4174.0),
        ("xlCellTypeSameValidation", -4175.0),
        ("xlCellTypeAllFormatConditions", -4172.0),
        ("xlLinkTypeExcelLinks", 1.0),
        ("xlSortNormal", 0.0),
        ("xlValidAlertStop", 1.0),
        ("xlValidateList", 3.0),
        ("xlBetween", 1.0),
        ("xlEqual", 3.0),
        ("xlTextQualifierDoubleQuote", 1.0),
        ("xlSkipColumn", 9.0),
        ("xlInsertDeleteCells", 1.0),
        ("xlOverwriteCells", 0.0),
        ("xlWindows", 2.0),
        ("xlDialogSaveAs", 5.0),
        ("xlPrintNoComments", -4142.0),
        ("xlSum", -4157.0),
        ("xlDataField", 4.0),
        ("xlCSVUTF8", 62.0),
        ("xlTypePDF", 0.0),
    ];
    let mut source = String::from("Option Explicit\n");
    for (i, (name, _)) in names.iter().enumerate() {
        source += &format!("Public Function C{i}() As Double\n C{i} = {name}\nEnd Function\n");
    }
    for backend in backends() {
        let bytes = vba_project::replace_sources(
            LOCAL,
            &[("Module1", &source), ("PrivateModule", "Option Private Module\n"), ("Sheet1", ""), ("ThisWorkbook", "")],
        );
        let session = opened(&bytes);
        let grant = calc_host_vba::ProjectGrant::trust_all("test", calc_macro_security::VbaPolicy::Guarded);
        let prepared = calc_host_vba::prepare_file_project_with_events(&grant, session.host(), "cmd.xlsm", 1_000_000, Some(backend), false);
        let owner = match prepared {
            Ok(Some(owner)) => owner,
            other => panic!("{backend:?}: {:?}", other.err()),
        };
        let mut session = session;
        owner.clone().install(&mut session);
        for (i, (name, expected)) in names.iter().enumerate() {
            let run = owner.run_procedure(&mut session, &format!("Module1.C{i}"), &[]);
            assert_eq!(run.outcome, Outcome::Success(ProcedureResult::Number(*expected)), "{backend:?} {name}");
        }
    }
}

const APP_FUNCTIONS: &str = "\
Public Sub Funcs()
 Range(\"C1\").Value2 = 4
 Range(\"C2\").Value2 = \"x\"
 Range(\"A1\").Value2 = Application.Match(5, Array(3, 5, 7), 0)
 Range(\"A2\").Value2 = Application.CountA(Range(\"C1:C3\"))
 Range(\"A3\").Value2 = Application.CountIf(Range(\"C1:C3\"), \">1\")
 Range(\"A4\").Value2 = Application.Transpose(Array(8, 9))(2, 1)
 Range(\"A5\").Value2 = IIf(IsError(Application.Match(9, Array(3, 5), 0)), 1, 0)
End Sub
";

/// dc-711ye: Excel's Application answers the worksheet-function names (Application.Match,
/// CountA, CountIf, Transpose) that its type library does not declare; they compile and calculate.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_711ye__application_answers_worksheet_function_names() {
    for backend in backends() {
        let (mut session, owner) = project_with(APP_FUNCTIONS, calc_macro_security::VbaPolicy::Guarded, backend);
        let run = owner.run_procedure(&mut session, "Module1.Funcs", &[]);
        assert_eq!(run.outcome, Outcome::Success(ProcedureResult::Empty), "{backend:?} {run:?}");
        for (row, expected) in [(1, 2.0), (2, 2.0), (3, 1.0), (4, 9.0), (5, 1.0)] {
            assert!(
                matches!(cell(&session, row, 1), Some(calc_host::CellInput::Number(n)) if n.get() == expected),
                "{backend:?} A{row}: {:?}",
                cell(&session, row, 1)
            );
        }
    }
}

const EXCEL_ANSWERS: &str = "\
Public Sub Probe()
 Dim x As Variant
 Dim a() As Long
 x = Application.Match(9, Array(3, 5), 0)
 Range(\"A1\").Value2 = TypeName(x) & \"|\" & CStr(x)
 On Error Resume Next
 Range(\"A2\").Value2 = CStr(Application.WorksheetFunction.Match(9, Array(3, 5), 0))
 Range(\"A3\").Value2 = CStr(Err.Number) & \"|\" & Err.Description
 On Error GoTo 0
 ReDim a(1 To 2) As Long
 a(1) = 7
 ReDim Preserve a(1 To 3) As Long
 Range(\"A4\").Value2 = CStr(a(1)) & \"|\" & CStr(UBound(a))
End Sub
";

/// dc-711ye (ww22, Excel 16.0.20625): Application.Match that misses returns Error 2042; WorksheetFunction.Match that
/// misses raises 1004 "Unable to get the Match property of the WorksheetFunction class"; `ReDim a(..) As Long` on an
/// existing array compiles and keeps the values (Preserve).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_711ye__excel_answers_for_a_missed_match_and_redim_as() {
    for backend in backends() {
        let (mut session, owner) = project_with(EXCEL_ANSWERS, calc_macro_security::VbaPolicy::Guarded, backend);
        let run = owner.run_procedure(&mut session, "Module1.Probe", &[]);
        assert_eq!(run.outcome, Outcome::Success(ProcedureResult::Empty), "{backend:?} {run:?}");
        let text = |row| match cell(&session, row, 1) {
            Some(calc_host::CellInput::Text(t)) => t.text().to_string_lossy(),
            other => format!("{other:?}"),
        };
        assert_eq!(text(1), "Error|Error 2042", "{backend:?}");
        assert_eq!(text(3), "1004|Unable to get the Match property of the WorksheetFunction class", "{backend:?}");
        assert_eq!(text(4), "7|3", "{backend:?}");
    }
}

/// dc-711ye (ww22): `Dim Name As Integer` / `Dim Error As Integer` style locals compile and run (Excel returned 42); `Me`
/// in a standard module is a compile error ("Invalid use of Me keyword"), not a run-time failure.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_711ye__name_local_compiles_and_me_in_a_standard_module_does_not() {
    for backend in backends() {
        let module = "Public Sub Probe()\n Dim Name As Integer\n Name = 42\n Range(\"A1\").Value2 = Name\nEnd Sub\n";
        let (mut session, owner) = project_with(module, calc_macro_security::VbaPolicy::Guarded, backend);
        let run = owner.run_procedure(&mut session, "Module1.Probe", &[]);
        assert_eq!(run.outcome, Outcome::Success(ProcedureResult::Empty), "{backend:?} {run:?}");
        assert!(matches!(cell(&session, 1, 1), Some(calc_host::CellInput::Number(n)) if n.get() == 42.0), "{backend:?}");
        let me = "Public Sub Probe()\n Dim x As Object\n Set x = Me\nEnd Sub\n";
        let policy = calc_macro_security::VbaPolicy::Guarded;
        match try_project_sources(me, "", policy, backend) {
            Err(e) => assert!(e.to_string().contains("Me"), "{backend:?}: {e}"),
            Ok(_) => panic!("{backend:?}: Me in a standard module compiled"),
        }
    }
}

/// (name, in a worksheet module, statement).
const HOST_MEMBERS: [(&str, bool, &str); 3] = [
    ("Cb", false, "Application.CommandBars(\"Cell\").Enabled = True"),
    ("Prn", true, "PrintOut"),
    ("Shp", true, "Shapes(\"x\").Delete"),
];

/// Dialogs and Toolbars are not in the descriptor yet (RUN-ON-WINDOWS regeneration).
const HOST_MEMBERS_PENDING_DESCRIPTOR: [(&str, bool, &str); 2] = [
    ("Dlg", false, "Application.Dialogs(5).Show"),
    ("Tb", false, "Application.Toolbars(\"Standard\").Visible = True"),
];

fn host_members_compile_and_refuse_when_run(cases: &[(&str, bool, &str)]) {
    let mut failures = Vec::new();
    for backend in backends() {
        for (name, in_sheet, body) in cases {
            let proc = format!("Public Sub {name}()\n {body}\nEnd Sub\n");
            let ok = "Public Sub Ok()\n Range(\"A1\").Value2 = 1\nEnd Sub\n";
            let (module, sheet, entry) =
                if *in_sheet { (ok.to_string(), proc, format!("Sheet1.{name}")) } else { (format!("{ok}{proc}"), String::new(), format!("Module1.{name}")) };
            let policy = calc_macro_security::VbaPolicy::Guarded;
            let (mut session, owner) = match try_project_sources(&module, &sheet, policy, backend) {
                Ok(p) => p,
                Err(e) => {
                    failures.push(format!("{backend:?} {name}: {e}"));
                    continue;
                }
            };
            let run = owner.run_procedure(&mut session, "Module1.Ok", &[]);
            assert_eq!(run.outcome, Outcome::Success(ProcedureResult::Empty), "{backend:?} {name} {run:?}");
            let run = owner.run_procedure(&mut session, &entry, &[]);
            if matches!(run.outcome, Outcome::Success(_)) {
                failures.push(format!("{backend:?} {name}: ran without an error: {run:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// dc-711ye cause 2: a project that mentions CommandBars, or PrintOut or Shapes bare in a worksheet
/// module (the sheet's own members), compiles; running one that acts on them fails with a run-time
/// error, not a compile refusal.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_711ye__unmodelled_host_members_compile_and_refuse_when_run() {
    host_members_compile_and_refuse_when_run(&HOST_MEMBERS);
}

/// Native-only: waits for the Windows descriptor regeneration (Dialogs, Toolbars; RUN-ON-WINDOWS.txt, dc-711ye).
#[test]
#[ignore = "Dialogs and Toolbars are not in the descriptor until it is regenerated on Windows"]
fn dc_711ye__dialogs_and_toolbars_compile_and_refuse_when_run() {
    host_members_compile_and_refuse_when_run(&HOST_MEMBERS_PENDING_DESCRIPTOR);
}

/// dc-711ye (ww24, Excel 16.0.20625): `Shapes("nosuch").Delete` in a worksheet module raises -2147024809 "The item with
/// the specified name wasn't found."
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_711ye__shapes_of_a_missing_name_raises_the_item_not_found_error() {
    for backend in backends() {
        let ok = "Public Sub Go()\n Sheet1.Shp\nEnd Sub\n";
        let sheet = "Public Sub Shp()\n Shapes(\"nosuch\").Delete\nEnd Sub\n";
        let policy = calc_macro_security::VbaPolicy::Guarded;
        let (mut session, owner) = try_project_sources(ok, sheet, policy, backend).expect("compiles");
        let run = owner.run_procedure(&mut session, "Module1.Go", &[]);
        match &run.outcome {
            Outcome::RuntimeError { number, description, .. } => {
                assert_eq!(*number, -2147024809, "{backend:?} {run:?}");
                assert_eq!(description, "The item with the specified name wasn't found.", "{backend:?}");
            }
            other => panic!("{backend:?}: {other:?}"),
        }
    }
}
