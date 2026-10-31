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
    assert_eq!(result(&mut s, "=Add2(A1,2)"), CalcValue::number(42.0));
    let calculation = s.calculate();
    assert_eq!(calculation.reads.len(), 1);
    assert!(calculation.diagnostics.is_empty());
    assert_eq!(result(&mut s, "=mAtH.aDd2(A1,2)"), CalcValue::number(42.0));
    assert_eq!(
        result(&mut s, "=SUM(Add2(A1,2),1)"),
        CalcValue::number(43.0)
    );

    s.load_vba_module(
        "Math",
        &calc_vba::EXAMPLE_MODULE.replace("a + b", "a + b + 10"),
        1000,
    )
    .unwrap();
    assert_eq!(result(&mut s, "=Add2(A1,2)"), CalcValue::number(52.0));
    assert!(
        s.load_vba_module("Math", "Function Broken(\n", 1000)
            .is_err()
    );
    assert_eq!(result(&mut s, "=Add2(A1,2)"), CalcValue::number(52.0));
    s.clear_user_functions();
    assert_eq!(
        result(&mut s, "=Add2(A1,2)"),
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
        result(&mut s, "=IF(FALSE,Spin(),Add2(A1,2))"),
        CalcValue::number(42.0)
    );
    for (formula, diagnostic) in [
        ("=Spin()", "budget"),
        ("=Fail()", "VBA error 5"),
        ("=Add2(1)", "arguments"),
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
    assert_eq!(result(&mut s, "=Add2(A1,2)"), CalcValue::number(42.0));
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
        result(&mut s, "=Add2(A1,2)"),
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
    assert_eq!(result(&mut s, "=TypedLong(3.5)"), CalcValue::number(4.0));
    assert_eq!(
        result(&mut s, "=TypedLong(NA())"),
        CalcValue::error(WorksheetErrorCode::Value)
    );
    s.set_formula("=IFERROR(RangeSum(Z9:Z10),7)");
    let missing = s.calculate();
    assert!(missing.result.is_none());
    assert!(format!("{:?}", missing.diagnostics).contains("MissingInput"));
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
