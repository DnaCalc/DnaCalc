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
    let bytes = vba_project::replace_sources(
        LOCAL,
        &[
            ("Module1", MODULE),
            ("PrivateModule", "Option Private Module\n"),
            ("Sheet1", ""),
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
    .unwrap()
    .unwrap();
    owner.clone().install(&mut session);
    (session, owner)
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
        for (row, column, expected) in [(2, 2, 50.0), (3, 1, 7.0), (5, 1, 9.0)] {
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
