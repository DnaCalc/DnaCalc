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
    assert!(matches!(failed.diagnostics[1], dna_onecalc::Diagnostic::Eval(_)));
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
