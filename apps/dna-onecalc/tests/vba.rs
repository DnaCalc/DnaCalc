//! dc-snz.3: use the application's controller, also compiled and run in the browsers.
use calc_func::value::{CalcValue, WorksheetErrorCode};
use dna_onecalc::{InputValue, Session};

fn session() -> Session {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_formula_cell("H1").unwrap();
    session
        .set_input("A1", InputValue::number(40.0).unwrap())
        .unwrap();
    session
}

fn result(session: &mut Session, formula: &str) -> CalcValue {
    session.set_formula(formula);
    let calculation = session.calculate();
    calculation
        .result
        .unwrap_or_else(|| panic!("{formula}: {:?}", calculation.diagnostics))
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn vba_add2_reads_the_grid_and_reload_uses_new_code() {
    let mut s = session();
    s.load_vba_module("Math", calc_vba::EXAMPLE_MODULE, 1000)
        .unwrap();
    // ADD2 is a grid address; qualifying the procedure selects the module.
    assert_eq!(result(&mut s, "=Add2(A1,2)"), CalcValue::error(WorksheetErrorCode::Ref));
    assert_eq!(result(&mut s, "=Math.Add2(A1,2)"), CalcValue::number(42.0));
    let calculation = s.calculate();
    assert_eq!(calculation.reads.len(), 1);
    assert!(calculation.diagnostics.is_empty());
    assert_eq!(result(&mut s, "=mAtH.aDd2(A1,2)"), CalcValue::number(42.0));
    assert_eq!(
        result(&mut s, "=SUM(Math.Add2(A1,2),1)"),
        CalcValue::number(43.0)
    );

    s.load_vba_module(
        "Math",
        &calc_vba::EXAMPLE_MODULE.replace("a + b", "a + b + 10"),
        1000,
    )
    .unwrap();
    assert_eq!(result(&mut s, "=Math.Add2(A1,2)"), CalcValue::number(52.0));
    assert!(
        s.load_vba_module("Math", "Function Broken(\n", 1000)
            .is_err()
    );
    assert_eq!(result(&mut s, "=Math.Add2(A1,2)"), CalcValue::number(52.0));
    s.clear_user_functions();
    assert_eq!(
        result(&mut s, "=Math.Add2(A1,2)"),
        CalcValue::error(WorksheetErrorCode::Name)
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn vba_errors_and_runaway_code_return_worksheet_errors() {
    let mut s = session();
    let source = format!(
        "{}\nPublic Function Spin()\n On Error Resume Next\n Do\n Loop\nEnd Function\nPublic Function Fail()\n Err.Raise 5\nEnd Function\nPublic Function SheetError()\n SheetError = CVErr(2042)\nEnd Function\n",
        calc_vba::EXAMPLE_MODULE
    );
    s.load_vba_module("Math", &source, 1000).unwrap();
    assert_eq!(
        result(&mut s, "=IF(FALSE,Spin(),Math.Add2(A1,2))"),
        CalcValue::number(42.0)
    );
    for (formula, diagnostic) in [
        ("=Spin()", "budget"),
        ("=Fail()", "VBA error 5"),
        ("=Math.Add2(1)", "arguments"),
    ] {
        assert_eq!(
            result(&mut s, formula),
            CalcValue::error(WorksheetErrorCode::Value)
        );
        assert!(format!("{:?}", s.calculate().diagnostics).contains(diagnostic));
    }
    assert_eq!(
        result(&mut s, "=SheetError()"),
        CalcValue::error(WorksheetErrorCode::NA)
    );
    assert!(s.calculate().diagnostics.is_empty());
    assert_eq!(result(&mut s, "=IFERROR(Spin(),7)"), CalcValue::number(7.0));
    // The budget starts again for the next call, even after an exhausted call.
    assert_eq!(result(&mut s, "=Math.Add2(A1,2)"), CalcValue::number(42.0));
    // A later evaluation fault does not erase an earlier UDF failure's diagnostic.
    s.set_formula("=LET(t,Fail(),Z9)");
    let failed = s.calculate();
    assert!(failed.result.is_none());
    assert_eq!(failed.diagnostics.len(), 2);
    assert!(format!("{:?}", failed.diagnostics[0]).contains("VBA error 5"));
    assert!(matches!(
        failed.diagnostics[1],
        dna_onecalc::Diagnostic::Eval(_)
    ));
    s.load_vba_module("Math", calc_vba::EXAMPLE_MODULE, 0)
        .unwrap();
    assert_eq!(
        result(&mut s, "=Math.Add2(A1,2)"),
        CalcValue::error(WorksheetErrorCode::Value)
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn vba_preserves_values_rectangles_and_private_byref_inputs() {
    let mut s = session();
    s.load_vba_module("Values", "Public Function Echo(a)\n Echo = a\nEnd Function\nPublic Function Mutate(a)\n a = 99\n Mutate = a\nEnd Function\n", 1000).unwrap();
    s.set_input("B1", InputValue::text("λ\0🙂").unwrap())
        .unwrap();
    s.set_input("A2", InputValue::logical(true)).unwrap();
    s.set_input("B2", InputValue::error(WorksheetErrorCode::NA))
        .unwrap();
    let value = result(&mut s, "=Echo(A1:B2)");
    let array = value.as_array().expect("a rectangular result");
    assert_eq!((array.shape().rows, array.shape().cols), (2, 2));
    for (row, col, address) in [(0, 0, "A1"), (0, 1, "B1"), (1, 0, "A2"), (1, 1, "B2")] {
        assert_eq!(
            array.get(row, col).unwrap(),
            s.input(s.cell(address).unwrap()).unwrap().value()
        );
    }
    s.set_input("C1", InputValue::blank()).unwrap();
    assert_eq!(result(&mut s, "=Echo(C1)"), CalcValue::number(0.0));
    assert_eq!(result(&mut s, "=Mutate(A1)"), CalcValue::number(99.0));
    assert_eq!(
        s.input(s.cell("A1").unwrap()).unwrap().value(),
        &CalcValue::number(40.0)
    );
    assert_eq!(
        result(&mut s, "=Echo(NA())"),
        CalcValue::error(WorksheetErrorCode::NA)
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn vba_optional_paramarray_and_project_storage_use_the_controller_session() {
    let mut s = session();
    let source = "Private n As Long\nPublic Function Counter() As Long\n n = n + 1\n Counter = n\nEnd Function\nPublic Function Peek() As Long\n Peek = n\nEnd Function\nPublic Function Defaults(Optional x As Long = 7, Optional y As Long = 2) As Long\n Defaults = x + y\nEnd Function\nPublic Function MissingArg(Optional x) As Boolean\n MissingArg = IsMissing(x)\nEnd Function\nPublic Function PackSize(ParamArray x() As Variant) As Long\n PackSize = UBound(x) - LBound(x) + 1\nEnd Function\n";
    s.load_vba_module("State", source, 1000).unwrap();
    assert_eq!(result(&mut s, "=Counter()"), CalcValue::number(1.0));
    assert_eq!(result(&mut s, "=Counter()"), CalcValue::number(2.0));
    assert_eq!(result(&mut s, "=Peek()"), CalcValue::number(2.0));
    assert_eq!(result(&mut s, "=Defaults()"), CalcValue::number(9.0));
    assert_eq!(
        result(&mut s, "=Defaults(,3)"),
        CalcValue::number(10.0),
        "{:?}",
        s.calculate().diagnostics
    );
    assert_eq!(result(&mut s, "=MissingArg()"), CalcValue::logical(true));
    assert_eq!(result(&mut s, "=MissingArg(0)"), CalcValue::logical(false));
    assert_eq!(result(&mut s, "=PackSize()"), CalcValue::number(0.0));
    assert_eq!(result(&mut s, "=PackSize(1,2,3)"), CalcValue::number(3.0));
    s.load_vba_module("State", source, 1000).unwrap();
    assert_eq!(result(&mut s, "=Counter()"), CalcValue::number(1.0));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn vba_ranges_caller_and_worksheet_callbacks_use_live_controller_services() {
    let mut s = session();
    s.set_input("A2", InputValue::number(2.0).unwrap()).unwrap();
    let source = r#"Private saved As Object
Public Function RangeCount(r As Range)
 RangeCount = r.Count
End Function
Public Function CallerAddress()
 CallerAddress = Application.Caller.Address
End Function
Public Function RangeSum(r As Range)
 RangeSum = Application.WorksheetFunction.Sum(r)
End Function
Public Function SaveRange(r As Range)
 Set saved = r
 SaveRange = 1
End Function
Public Function ReadSaved()
 ReadSaved = saved.Cells(1, 1).Value2
End Function
Public Function CatchWorksheetError()
 On Error GoTo Failed
 CatchWorksheetError = Application.WorksheetFunction.Match("missing", Array(1, 2), 0)
 Exit Function
Failed:
 CatchWorksheetError = Err.Number
End Function
Public Function ArgType(a)
 ArgType = TypeName(a)
End Function
Public Function ArgVarType(a)
 ArgVarType = VarType(a)
End Function
Public Function CatchVarType(a)
 On Error Resume Next
 CatchVarType = VarType(a)
 CatchVarType = 7
End Function
Public Function TypedLong(x As Long)
 TypedLong = x
End Function
"#;
    // Verify admission of this exact host-object source: native results must exercise JIT,
    // while the same portable cases exercise the interpreter in each browser.
    let registrations = calc_vba::load_module("Grid", source, 1000).unwrap();
    let selected = if cfg!(target_arch = "wasm32") {
        "backend: Interpreter"
    } else {
        "backend: Jit"
    };
    assert!(format!("{registrations:?}").contains(selected));
    s.load_vba_module("Grid", source, 1000).unwrap();
    assert_eq!(
        result(&mut s, "=RangeCount(Z9:Z10)"),
        CalcValue::number(2.0)
    );
    assert!(s.calculate().reads.is_empty());
    assert_eq!(result(&mut s, "=RangeSum(A1:A2)"), CalcValue::number(42.0));
    let reads = s.calculate().reads;
    assert_eq!(reads.len(), 1);
    assert_eq!(
        reads[0].area,
        calc_func::value::GridArea::new(s.cell("A1").unwrap(), s.cell("A2").unwrap()).unwrap()
    );
    assert_eq!(
        result(&mut s, "=CallerAddress()"),
        CalcValue::text(calc_func::value::ExcelText::from_utf16_code_units(
            "$H$1".encode_utf16().collect()
        ))
    );
    s.set_formula_cell("J10").unwrap();
    assert_eq!(
        result(&mut s, "=CallerAddress()"),
        CalcValue::text(calc_func::value::ExcelText::from_utf16_code_units(
            "$J$10".encode_utf16().collect()
        ))
    );
    assert_eq!(result(&mut s, "=SaveRange(A1)"), CalcValue::number(1.0));
    s.set_input("A1", InputValue::number(41.0).unwrap())
        .unwrap();
    assert_eq!(result(&mut s, "=ReadSaved()"), CalcValue::number(41.0));
    assert_eq!(s.calculate().reads.len(), 1);
    assert_eq!(
        result(&mut s, "=CatchWorksheetError()"),
        CalcValue::number(1004.0)
    );
    assert_eq!(
        result(&mut s, "=ArgType(A1)"),
        CalcValue::text(calc_func::value::ExcelText::from_utf16_code_units(
            "Range".encode_utf16().collect()
        ))
    );
    assert_eq!(result(&mut s, "=ArgVarType(A1)"), CalcValue::number(5.0));
    assert_eq!(
        result(&mut s, "=ArgVarType(A1:A2)"),
        CalcValue::number(8204.0)
    );
    s.set_input("A3", InputValue::blank()).unwrap();
    assert_eq!(result(&mut s, "=ArgVarType(A3)"), CalcValue::number(0.0));
    s.set_input("A1", InputValue::text("typed").unwrap())
        .unwrap();
    assert_eq!(result(&mut s, "=ArgVarType(A1)"), CalcValue::number(8.0));
    s.set_input("A1", InputValue::number(41.0).unwrap())
        .unwrap();
    assert_eq!(result(&mut s, "=TypedLong(3.5)"), CalcValue::number(4.0));
    assert_eq!(
        result(&mut s, "=TypedLong(NA())"),
        CalcValue::error(WorksheetErrorCode::Value)
    );
    for formula in [
        "=IFERROR(RangeSum(Z9:Z10),7)",
        "=IFERROR(CatchVarType(Z9),7)",
    ] {
        s.set_formula(formula);
        let missing = s.calculate();
        assert!(missing.result.is_none());
        assert!(format!("{:?}", missing.diagnostics).contains("MissingInput"));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn vba_warm_code_reloads_fresh_state_and_cache_clear_keeps_live_calls_valid() {
    calc_vba::clear_code_cache();
    let before = calc_vba::code_cache_stats();
    let mut s = session();
    let source = "Private n As Long\nPublic Function WarmCounter() As Long\n n = n + 1\n WarmCounter = n\nEnd Function\n";
    s.load_vba_module("Warm", source, 1000).unwrap();
    assert_eq!(result(&mut s, "=WarmCounter()"), CalcValue::number(1.0));
    s.load_vba_module("Warm", source, 1000).unwrap();
    assert_eq!(result(&mut s, "=WarmCounter()"), CalcValue::number(1.0));
    let warm = calc_vba::code_cache_stats();
    assert_eq!(warm.hits, before.hits + 1);
    #[cfg(not(target_arch = "wasm32"))]
    assert_eq!(warm.compilations, before.compilations + 1);
    calc_vba::clear_code_cache();
    assert_eq!(result(&mut s, "=WarmCounter()"), CalcValue::number(2.0));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn vba_callbacks_use_measured_methods_without_formula_lifting_or_swallowed_refusals() {
    let mut s = session();
    s.set_input("A2", InputValue::number(2.0).unwrap()).unwrap();
    let source = r#"Public Function Summaries(r As Range)
 Summaries = Application.WorksheetFunction.Min(r) + Application.WorksheetFunction.Max(r) + Application.WorksheetFunction.Average(r)
End Function
Public Function CatchOffset(r As Object)
 On Error Resume Next
 CatchOffset = r.Offset(1, 0).Value2
 CatchOffset = 7
End Function
Public Function FindValue(x)
 FindValue = Application.WorksheetFunction.Match(x, Array(2, 40), 0)
End Function
Public Function UnsupportedCallback()
 On Error Resume Next
 UnsupportedCallback = Application.WorksheetFunction.Abs(-1)
 UnsupportedCallback = 7
End Function
"#;
    // Verify admission of this exact host-object source: native results must exercise JIT,
    // while the same portable cases exercise the interpreter in each browser.
    let registrations = calc_vba::load_module("Callbacks", source, 1000).unwrap();
    let selected = if cfg!(target_arch = "wasm32") {
        "backend: Interpreter"
    } else {
        "backend: Jit"
    };
    assert!(format!("{registrations:?}").contains(selected));
    s.load_vba_module("Callbacks", source, 1000).unwrap();
    assert_eq!(result(&mut s, "=Summaries(A1:A2)"), CalcValue::number(63.0));
    assert_eq!(result(&mut s, "=FindValue(A1)"), CalcValue::number(2.0));
    for formula in [
        "=IFERROR(UnsupportedCallback(),9)",
        "=IFERROR(Summaries((A1,A2)),9)",
        "=IFERROR(CatchOffset(A1),9)",
        "=IFERROR(FindValue(A1:A2),9)",
        "=IFERROR(FindValue({2,40}),9)",
    ] {
        s.set_formula(formula);
        let outcome = s.calculate();
        assert!(outcome.result.is_none(), "{formula}: {outcome:?}");
        assert!(
            format!("{:?}", outcome.diagnostics).contains("NotModelled")
                || format!("{:?}", outcome.diagnostics).contains("AreaAsValue")
        );
    }
}

/// dc-0f1k.4: owner rule, early-bound COM through imported type libraries first, never late
/// binding as a start. `CreateObject` with a ProgID is a typed refusal that no `On Error` hides and
/// no project reference turns on (late-bound calls on COM objects take the same refusal in the
/// runtime; only a project that references a type library can hold such an object).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn late_bound_com_is_a_typed_refusal() {
    let mut s = session();
    let source = r#"Public Function MakeDictionary()
 On Error Resume Next
 Dim d As Object
 Set d = CreateObject("Scripting.Dictionary")
 MakeDictionary = 7
End Function
"#;
    s.load_vba_module("Late", source, 1000).unwrap();
    s.set_formula("=IFERROR(MakeDictionary(),9)");
    let outcome = s.calculate();
    let diagnostics = format!("{:?}", outcome.diagnostics);
    assert!(outcome.result.is_none(), "{outcome:?}");
    assert!(diagnostics.contains("NotModelled"), "{diagnostics}");
    assert!(diagnostics.contains("CreateObject"), "{diagnostics}");
}

/// dc-0f1k.4: Excel's default for macro files. An .xlsm with the Mark of the Web (zone 3 or 4)
/// never runs its project here and cannot be enabled; any other opens disabled and can be
/// enabled for that file. Either way the project part is held and a save writes it back.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_macro_file_opens_disabled_and_the_mark_of_the_web_blocks_it() {
    use dna_onecalc::port::{LocalPort, MacrosView, Reply, Request, SessionPort};
    for (zone, expected) in [
        (Some(3), MacrosView::Blocked),
        (Some(4), MacrosView::Blocked),
        (Some(2), MacrosView::Disabled),
        (Some(0), MacrosView::Disabled),
        (None, MacrosView::Disabled),
    ] {
        let mut port = LocalPort::new(Session::new("Sheet1").unwrap());
        let Reply::Opened(opened) = port.send(Request::Open {
            name: "DnaExamples.xlsm".into(),
            bytes: EXAMPLES_XLSM.to_vec(),
            zone,
        }) else {
            panic!("opened")
        };
        let view = opened.project.expect("the file's project");
        assert_eq!(view.macros, expected, "{zone:?}");
        assert!(view.functions.is_empty() && !view.notes.is_empty(), "{view:?}");
        assert_eq!(port.session().project().map(|p| p.is_running()), Some(false));
        let reply = port.send(Request::EnableMacros);
        if expected == MacrosView::Blocked {
            assert!(matches!(&reply, Reply::Refused { message } if message.contains("Mark of the Web")), "{reply:?}");
            assert_eq!(port.session().project().map(|p| p.macros), Some(dna_onecalc::MacroState::Blocked(zone.unwrap())));
        } else {
            #[cfg(windows)]
            assert!(matches!(reply, Reply::Opened(_)), "{zone:?}: {reply:?}");
            #[cfg(not(windows))]
            {
                assert!(matches!(&reply, Reply::Refused { message } if message.contains("Scripting") && message.contains("cannot be resolved on this platform") && message.contains("dc-0f1k.4")), "{zone:?}: {reply:?}");
                assert_eq!(port.session().project().map(|p| p.macros), Some(dna_onecalc::MacroState::Disabled));
                assert_eq!(port.session().project().map(|p| p.is_running()), Some(false));
            }
        }
        // The project part is the file's either way: a save writes it back.
        let Reply::File { bytes, .. } = port.send(Request::Save) else {
            panic!("saved")
        };
        let saved = calc_doc::read_session(bytes, dna_onecalc::file::NAMESPACE).unwrap();
        let original = calc_doc::read_vba_file(EXAMPLES_XLSM.to_vec()).unwrap().unwrap();
        assert_eq!(saved.vba.map(|v| v.project), Some(original.project));
    }
}

// dc-0f1k.4: an .xlsm session's own VBA project, through the port the page uses. The project is
// DNA Calc's example add-in project as Excel saved it; its cases hold Excel's answers
// (crates/calc-xll-reference/fixture/vba, measured in an owned Excel session).
const EXAMPLES_XLSM: &[u8] =
    include_bytes!("../../../crates/calc-xll-reference/fixture/vba/DnaExamples.xlsm");
#[cfg(windows)]
const EXAMPLE_CASES: &str =
    include_str!("../../../crates/calc-xll-reference/fixture/vba/x9-vba-cases.json");

fn opened_examples() -> (dna_onecalc::port::LocalPort, dna_onecalc::port::Opened) {
    use dna_onecalc::port::{LocalPort, Reply, Request, SessionPort};
    let mut port = LocalPort::new(Session::new("Sheet1").unwrap());
    let reply = port.send(Request::Open {
        name: "DnaExamples.xlsm".into(),
        bytes: EXAMPLES_XLSM.to_vec(),
        zone: None,
    });
    let Reply::Opened(opened) = reply else {
        panic!("{reply:?}")
    };
    (port, *opened)
}

/// What the page shows for `formula` in H1: its value text, or why there is none.
#[cfg(windows)]
fn shown(port: &mut dna_onecalc::port::LocalPort, formula: &str) -> String {
    use dna_onecalc::port::{Reply, Request, SessionPort};
    port.send(Request::SetFormulaCell { address: "H1".into() });
    port.send(Request::SetFormula { text: formula.into() });
    match port.send(Request::Calculate) {
        Reply::Calculated(view) => view
            .value
            .unwrap_or_else(|| format!("not calculated: {}", view.diagnostics.join(" | "))),
        other => panic!("{formula}: {other:?}"),
    }
}

/// Excel's answer for a case in the typed form the session's view gives (`number 42`,
/// `text "a"`, `logical TRUE`, `error #N/A`).
#[cfg(windows)]
fn excel_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Number(n) => format!("number {}", n.as_f64().unwrap()),
        serde_json::Value::String(s) => format!("text \"{}\"", s.replace('"', "\"\"")),
        serde_json::Value::Bool(b) => if *b { "logical TRUE" } else { "logical FALSE" }.into(),
        serde_json::Value::Object(o) => format!("error {}", o["error"].as_str().unwrap()),
        other => panic!("{other}"),
    }
}

/// Native-only: the project references the Microsoft Scripting Runtime, which only a Windows
/// build reads from the registry and runs through COM.
#[cfg(windows)]
#[test]
fn an_xlsm_session_compiles_its_project_and_answers_its_cases_as_excel_did() {
    use dna_onecalc::port::{MacrosView, Reply, Request, SessionPort};
    let (mut port, opened) = opened_examples();
    // Excel's default: a macro file opens with macros disabled, nothing compiled or run.
    let held = opened.project.expect("the file's project");
    assert_eq!(held.macros, MacrosView::Disabled);
    assert!(held.functions.is_empty(), "{held:?}");
    assert_ne!(shown(&mut port, "=DnaAdd(40,2)"), "number 42");
    let Reply::Opened(enabled) = port.send(Request::EnableMacros) else {
        panic!("enabling macros answers the file as opened")
    };
    let project = enabled.project.expect("the file's project");
    assert_eq!(project.macros, MacrosView::Enabled);
    assert!(project.functions.iter().any(|f| f.eq_ignore_ascii_case("DnaDistinct")), "{project:?}");
    assert!(project.notes.is_empty(), "{project:?}");
    let cases: Vec<serde_json::Value> = serde_json::from_str(EXAMPLE_CASES).unwrap();
    let mut differences = Vec::new();
    for case in &cases {
        let formula = case["formula"].as_str().unwrap();
        // Excel answered with the add-in open; here the project is the .xlsm session's own, and
        // ThisWorkbook.Name is the name of the file opened.
        let excel = if formula == "=DnaBook()" {
            "text \"DnaExamples.xlsm\"".to_owned()
        } else {
            excel_text(&case["value"])
        };
        let ours = shown(&mut port, formula);
        if ours != excel {
            differences.push(format!("{formula}: OneCalc {ours:?}, Excel {excel:?}"));
        }
    }
    // ActiveSheet and Selection are the user's sheet and selection (dc-0f1k.12): every case of
    // the example project, the former typed refusals included, answers as Excel did.
    assert!(differences.is_empty(), "{differences:#?}");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn an_xlsm_session_saves_its_project_part_unchanged_and_an_edit_changes_it() {
    use dna_onecalc::port::{Reply, Request, SessionPort};
    let (mut port, opened) = opened_examples();
    // Disabled macros still save the project part unchanged; editing needs them enabled.
    let held = opened.project.expect("the file's project");
    assert_eq!(held.macros, dna_onecalc::port::MacrosView::Disabled);
    #[cfg(not(windows))]
    {
        let reply = port.send(Request::EnableMacros);
        assert!(matches!(&reply, Reply::Refused { message } if message.contains("Scripting") && message.contains("cannot be resolved on this platform") && message.contains("dc-0f1k.4")), "{reply:?}");
        assert_eq!(port.session().project().map(|p| p.macros), Some(dna_onecalc::MacroState::Disabled));
    }
    let original = calc_doc::read_vba_file(EXAMPLES_XLSM.to_vec()).unwrap().unwrap();
    port.send(Request::SetFormula { text: "=1+1".into() });
    let Reply::File { name, bytes } = port.send(Request::Save) else {
        panic!("saved")
    };
    assert_eq!(name, "DnaExamples.xlsm");
    let saved = calc_doc::read_session(bytes, dna_onecalc::file::NAMESPACE).unwrap();
    assert_eq!(saved.vba.as_ref().map(|v| &v.project), Some(&original.project));
    assert_eq!(saved.vba.as_ref().and_then(|v| v.sheet_code_name.as_deref()), Some("Sheet1"));
    #[cfg(windows)]
    {
        let Reply::Opened(enabled_view) = port.send(Request::EnableMacros) else {
            panic!("enabled")
        };
        assert!(enabled_view.project.is_some_and(|p| !p.functions.is_empty()));
        let Reply::File { name, bytes } = port.send(Request::Save) else { panic!("saved after enabling") };
        assert_eq!(name, "DnaExamples.xlsm");
        let saved = calc_doc::read_session(bytes, dna_onecalc::file::NAMESPACE).unwrap();
        assert_eq!(saved.vba.as_ref().map(|v| &v.project), Some(&original.project));
        assert_eq!(saved.vba.as_ref().and_then(|v| v.sheet_code_name.as_deref()), Some("Sheet1"));
        // Where the project compiles (the Windows build), editing a module through the VBA panel
        // changes the project and the part a save writes.
        let reply = port.send(Request::LoadVba {
            name: "Extra".into(),
            source: "Public Function DnaTwice(ByVal x As Double) As Double\r\n    DnaTwice = 2 * x\r\nEnd Function\r\n".into(),
        });
        assert!(matches!(&reply, Reply::Functions { names } if names.iter().any(|n| n.eq_ignore_ascii_case("DnaTwice"))), "{reply:?}");
        assert_eq!(shown(&mut port, "=DnaTwice(21)"), "number 42");
        assert_eq!(shown(&mut port, "=DnaAdd(40,2)"), "number 42");
        let Reply::File { bytes, .. } = port.send(Request::Save) else {
            panic!("saved")
        };
        let edited = calc_doc::read_session(bytes, dna_onecalc::file::NAMESPACE).unwrap();
        let blob = edited.vba.unwrap().project;
        assert_ne!(blob, original.project);
        let project = calc_vba::read_office_project(&blob).unwrap();
        assert!(project.modules().iter().any(|m| m.name == "Extra" && m.source.contains("DnaTwice = 2 * x")));
        assert_eq!(project.modules().len(), 5);
    }
}

/// Native-only: the project references the Microsoft Scripting Runtime, so activation reaches
/// platform COM. `New Scripting.Dictionary` (early bound, an imported class) works; an explicit
/// `CreateObject` of another class of that library is late binding and stays a typed refusal (owner rule).
#[cfg(windows)]
#[test]
fn with_a_com_reference_new_works_and_createobject_is_refused() {
    use dna_onecalc::port::{Reply, Request, SessionPort};
    let (mut port, _) = opened_examples();
    port.send(Request::EnableMacros);
    let reply = port.send(Request::LoadVba {
        name: "Late".into(),
        source: "Public Function EarlyCount() As Long\r\n    Dim d As New Scripting.Dictionary\r\n    d.Add \"a\", 1\r\n    EarlyCount = d.Count\r\nEnd Function\r\nPublic Function LateCount()\r\n    Dim d As Object\r\n    Set d = CreateObject(\"Scripting.FileSystemObject\")\r\n    LateCount = 1\r\nEnd Function\r\n".into(),
    });
    assert!(matches!(&reply, Reply::Functions { .. }), "{reply:?}");
    assert_eq!(shown(&mut port, "=EarlyCount()"), "number 1");
    let late = shown(&mut port, "=LateCount()");
    assert!(late.starts_with("not calculated") && late.contains("CreateObject"), "{late}");
}

/// dc-0f1k.4: inside a worksheet function a cell write is Excel's trappable error 1004 (measured).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_cell_write_in_a_worksheet_function_raises_1004() {
    let mut s = session();
    s.load_vba_module(
        "Writes",
        "Public Function TryWrite(r As Range)\n On Error Resume Next\n r.Value = 1\n TryWrite = Err.Number\nEnd Function\n",
        1000,
    )
    .unwrap();
    assert_eq!(result(&mut s, "=Writes.TryWrite(A1)"), CalcValue::number(1004.0));
}

/// dc-0f1k.4: after an edit, a module's reported source offset is the one the written blob has.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn an_edited_project_reports_the_source_offsets_it_writes() {
    let part = calc_doc::read_vba_file(EXAMPLES_XLSM.to_vec()).unwrap().unwrap();
    let read = calc_vba::read_office_project(&part.project).unwrap();
    assert!(read.modules().iter().any(|m| m.source_offset > 0), "the fixture keeps its compiled cache");
    let kind = read.modules()[0].kind;
    let name = read.modules()[0].name.clone();
    let edited = read.with_module(&name, kind, "Public Function Edited()\r\nEnd Function\r\n").unwrap();
    let written = calc_vba::read_office_project(&edited.to_blob().unwrap()).unwrap();
    let reported: Vec<_> = edited.modules().iter().map(|m| (&m.name, m.source_offset)).collect();
    let on_disk: Vec<_> = written.modules().iter().map(|m| (&m.name, m.source_offset)).collect();
    assert_eq!(reported, on_disk);
}

/// Native-only (the project compiles on Windows): `Workbook_BeforeSave` runs before anything is
/// written, and `Cancel` stops the save.
#[cfg(windows)]
#[test]
fn workbook_before_save_runs_first_and_cancel_stops_the_save() {
    use dna_onecalc::port::{Reply, Request, SessionPort};
    let (mut port, _) = opened_examples();
    port.send(Request::EnableMacros);
    let load = |port: &mut dna_onecalc::port::LocalPort, cancel: &str| {
        port.send(Request::LoadVba {
            name: "ThisWorkbook".into(),
            source: format!("Private Sub Workbook_BeforeSave(ByVal SaveAsUI As Boolean, Cancel As Boolean)\r\n    Cancel = {cancel}\r\nEnd Sub\r\n"),
        })
    };
    let reply = load(&mut port, "True");
    assert!(matches!(reply, Reply::Functions { .. }), "{reply:?}");
    let reply = port.send(Request::Save);
    assert!(matches!(&reply, Reply::Refused { message } if message.contains("Cancel")), "{reply:?}");
    let reply = load(&mut port, "False");
    assert!(matches!(reply, Reply::Functions { .. }), "{reply:?}");
    assert!(matches!(port.send(Request::Save), Reply::File { .. }));
}

/// dc-0f1k.12: a worksheet function sees the user's active sheet and selection
/// (Excel: not its caller's; fixture/vba/active-objects-measured.json). OneCalc's user has one
/// sheet and has selected the cell the formula is entered in, which is Excel's measured scenario
/// "selection-is-the-caller"; the others (a selection elsewhere) are not a state OneCalc can be in.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn active_sheet_and_selection_are_the_users_as_excel_measured() {
    let measured: serde_json::Value =
        serde_json::from_str(include_str!("../../../crates/calc-xll-reference/fixture/vba/active-objects-measured.json")).unwrap();
    let scenario = measured["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["scenario"] == "selection-is-the-caller")
        .unwrap();
    let answers = scenario["answers"].as_object().unwrap();
    let mut s = session();
    s.set_input("B2", InputValue::number(11.0).unwrap()).unwrap();
    s.load_vba_module(
        "Probe",
        "Public Function ActName() As String\n ActName = ActiveSheet.Name\nEnd Function\n\
         Public Function ActType() As String\n ActType = TypeName(ActiveSheet)\nEnd Function\n\
         Public Function SelType() As String\n SelType = TypeName(Selection)\nEnd Function\n\
         Public Function SelAddr() As String\n SelAddr = Selection.Address\nEnd Function\n\
         Public Function ActRead() As Variant\n ActRead = ActiveSheet.Range(\"B2\").Value2\nEnd Function\n\
         Public Function CallerAddr() As String\n CallerAddr = Application.Caller.Address\nEnd Function\n\
         Public Function TryActiveWrite() As Long\n On Error Resume Next\n ActiveSheet.Range(\"Z99\").Value = 1\n TryActiveWrite = Err.Number\nEnd Function\n",
        1000,
    )
    .unwrap();
    let mut differences = Vec::new();
    for name in ["ActName", "ActType", "SelType", "SelAddr", "ActRead", "CallerAddr", "TryActiveWrite"] {
        s.set_formula(&format!("=Probe.{name}()"));
        let ours = s.calculate();
        let excel = &answers[&format!("={name}()")];
        let ours_text = match ours.result.as_ref() {
            Some(value) => format!("{value:?}"),
            None => format!("not calculated {:?}", ours.diagnostics),
        };
        let expected = match excel {
            serde_json::Value::String(text) if name == "ActName" => text.replace("First", "Sheet1"),
            other => other.to_string(),
        };
        let matched = match (&ours.result, excel) {
            (Some(value), serde_json::Value::String(text)) => {
                let want = if name == "ActName" { "Sheet1" } else { text.as_str() };
                value == &CalcValue::text(calc_func::value::ExcelText::from_interop_assignment(want))
            }
            (Some(value), serde_json::Value::Number(n)) => value == &CalcValue::number(n.as_f64().unwrap()),
            _ => false,
        };
        if !matched {
            differences.push(format!("{name}: OneCalc {ours_text}, Excel {expected}"));
        }
    }
    assert!(differences.is_empty(), "{differences:#?}");
}
