//! L1 (dc-mzk) through the engine's route: formulas that use defined names (workbook and sheet
//! scope, named LAMBDAs), array constants, LET, LAMBDA, the reference operators and OFFSET are
//! bound by begin_recalc through CalcFml, with the snapshot's names, and calculated by step.
//! Every expected value is derived by hand from the literals and the formula texts.

use calc_doc_model::value::{
    CalcValue, CellKey, ExcelText, IdAllocator, NumericMode, SheetId, WorksheetErrorCode,
};
use calc_doc_model::{
    AuthoredRevision, CellInput, DefinedName, Definitions, FiniteNumber, FormulaText, ModelRoot,
    NameScope, SheetEntry, SheetKey, SheetName, SheetVisibility, SkinLayout,
};
use calc_engine::{
    EngineState, FormulaResult, NotModeled, RecalcProgress, Unobserved, WorkBudget, begin_recalc,
    finish, step,
};
use calc_fml::EvalFault;
use std::sync::Arc;

fn sheet_entry(id: SheetId, key: u32, name: &str) -> SheetEntry {
    SheetEntry::new(
        id,
        SheetKey::new(key),
        SheetName::new(ExcelText::from_interop_assignment(name)).unwrap(),
        SheetVisibility::Visible,
    )
}

fn formula(text: &str) -> CellInput {
    CellInput::Formula(FormulaText::new(ExcelText::from_interop_assignment(text)).unwrap())
}

fn number(value: f64) -> CellInput {
    CellInput::Number(FiniteNumber::new(value).unwrap())
}

fn name(name: &str, scope: NameScope, formula: &str) -> DefinedName {
    DefinedName {
        name: name.to_owned(),
        scope,
        formula: formula.to_owned(),
        hidden: false,
        comment: None,
        function: false,
        vb_procedure: false,
        xlm: false,
    }
}

/// Builds a two-sheet workbook (Data, Calc) with `cells` and `names`, recalculates it once and
/// gives the state.
fn calculated(
    cells: Vec<(CellKey, CellInput)>,
    names: Vec<DefinedName>,
    sheets: (SheetId, SheetId),
) -> EngineState {
    let entries = vec![
        sheet_entry(sheets.0, 1, "Data"),
        sheet_entry(sheets.1, 2, "Calc"),
    ];
    let root: Arc<ModelRoot> = ModelRoot::with_definitions(
        AuthoredRevision::new(0),
        entries,
        cells,
        SkinLayout::default(),
        Definitions::new([], names, vec![], []),
    )
    .unwrap();
    let mut state = EngineState::new(&root);
    let mut work = begin_recalc(&mut state, &root, NumericMode::Fast).unwrap();
    assert_eq!(
        step(
            &state,
            &root,
            &mut work,
            WorkBudget::UNLIMITED,
            &mut Unobserved
        ),
        RecalcProgress::Complete
    );
    finish(&mut state, work).unwrap();
    state
}

fn value(state: &EngineState, at: CellKey) -> CalcValue {
    match state.result(at) {
        Some(FormulaResult::Calculated { value, .. }) => value.clone(),
        other => panic!("{at:?}: {other:?}"),
    }
}

#[test]
fn names_arrays_let_lambda_and_offset_calculate_through_the_engine() {
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let (data, calc) = (ids.issue().unwrap(), ids.issue().unwrap());
    let d = |row, column| CellKey::new(data, row, column).unwrap();
    let c = |row, column| CellKey::new(calc, row, column).unwrap();
    let mut cells = vec![
        (d(1, 1), number(10.0)),
        (d(2, 1), number(20.0)),
        (d(3, 1), number(30.0)),
        (d(1, 2), number(0.25)),
    ];
    let formulas = [
        // 10+20+30.
        (c(1, 1), "SUM(Inputs)"),
        // Calc's own Rate, 0.5, shadows the workbook's: 60 * (1 + 0.5).
        (c(2, 1), "SUM(Inputs)*(1+Rate)"),
        (c(3, 1), "Rate"),
        // On Data the workbook's Rate, Data!B1: 0.25.
        (d(1, 3), "Rate*1"),
        // The named LAMBDA: 7^2.
        (c(4, 1), "Square(7)"),
        // An array constant: 1+2+3+4.
        (c(5, 1), "SUM({1,2;3,4})"),
        // LET and a LET-bound LAMBDA, in the file form: (5+1)*2.
        (
            c(6, 1),
            "_xlfn.LET(_xlpm.f,_xlfn.LAMBDA(_xlpm.x,_xlpm.x+1),_xlpm.f(5)*2)",
        ),
        // OFFSET from Data!A1 down two rows: 30.
        (c(7, 1), "OFFSET(Data!A1,2,0)"),
        // The union: 10 + 30.
        (c(8, 1), "SUM((Data!A1,Data!A3))"),
        // The intersection of A1:A3 and A2:B2: 20.
        (c(9, 1), "SUM(Data!A1:A3 Data!A2:B2)"),
        // A name the workbook does not hold: #NAME?.
        (c(10, 1), "NoSuchName"),
        // A legacy formula intersects a range with its own row: Calc!A2 is row 2, Data!A2 = 20.
        (c(2, 3), "Data!A1:A3*1"),
        // SUMPRODUCT lifts: 10*1 + 20*2 + 30*3.
        (c(1, 4), "SUMPRODUCT(Data!A1:A3*{1;2;3})"),
    ];
    cells.extend(formulas.iter().map(|(at, text)| (*at, formula(text))));
    let names = vec![
        name("Inputs", NameScope::Workbook, "Data!$A$1:$A$3"),
        name("Rate", NameScope::Workbook, "Data!$B$1"),
        name("Rate", NameScope::Sheet(calc), "0.5"),
        name(
            "Square",
            NameScope::Workbook,
            "_xlfn.LAMBDA(_xlpm.x,_xlpm.x*_xlpm.x)",
        ),
    ];
    let state = calculated(cells, names, (data, calc));
    let n = CalcValue::number;
    for (at, expected) in [
        (c(1, 1), n(60.0)),
        (c(2, 1), n(90.0)),
        (d(1, 3), n(0.25)),
        (c(3, 1), n(0.5)),
        (c(4, 1), n(49.0)),
        (c(5, 1), n(10.0)),
        (c(6, 1), n(12.0)),
        (c(7, 1), n(30.0)),
        (c(8, 1), n(40.0)),
        (c(9, 1), n(20.0)),
        (c(10, 1), CalcValue::error(WorksheetErrorCode::Name)),
        (c(2, 3), n(20.0)),
        (c(1, 4), n(140.0)),
    ] {
        assert_eq!(value(&state, at), expected, "{at:?}");
    }
}

/// OFFSET into a formula cell the schedule orders later: the engine cannot order a reference the
/// evaluation computed (L2 owns dynamic dependencies), so that formula is refused and every other
/// formula still calculates.
#[test]
fn offset_into_a_later_formula_cell_is_refused_alone() {
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let (data, calc) = (ids.issue().unwrap(), ids.issue().unwrap());
    let c = |row, column| CellKey::new(calc, row, column).unwrap();
    let cells = vec![
        // Calc!A1 reads Calc!B1 and, through OFFSET, Calc!A5, which comes later in the schedule.
        (c(1, 1), formula("OFFSET(B1,4,-1)")),
        (c(2, 1), formula("1+1")),
        (c(5, 1), formula("2*3")),
    ];
    let state = calculated(cells, vec![], (data, calc));
    assert_eq!(value(&state, c(2, 1)), CalcValue::number(2.0));
    assert_eq!(value(&state, c(5, 1)), CalcValue::number(6.0));
    match state.result(c(1, 1)) {
        Some(FormulaResult::NotModeled {
            refusal: NotModeled::Evaluation(EvalFault::NotModelled { owner, .. }),
            ..
        }) => assert_eq!(*owner, "dc-0k9"),
        other => panic!("{other:?}"),
    }
}
