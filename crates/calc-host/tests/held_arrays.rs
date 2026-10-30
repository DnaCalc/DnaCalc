//! dc-h9u: a GROUPBY whose LAMBDA returns an array keeps that array in its result's cell, end to
//! end through calc-host in dynamic-array formulas typed as Excel's Range.Formula2 enters them.
//! Every expectation is Excel 16.0.20527's, read through COM on 2026-10-30 (the catalogue's
//! dch9u_functions rows hold the legacy-formula forms, which the oracle corpus compares): the
//! spilled cell shows #VALUE!, an operator over the result is #VALUE! there, INDEX of the cell
//! shows the group's first value and ROWS of it counts the group.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_host::value::{CellKey, CoreRef, WorksheetErrorCode};
use calc_host::{
    CalcMode, HostOptions, HostSession, MemorySource, OpenPolicy, PackageRead, ProjectedValue,
    ProjectionRequest, SkinIntent, project,
};
use std::sync::Arc;

const G: &str = r#"GROUPBY({"a";"b";"a"},{1;2;3},LAMBDA(x,x),0,0)"#;
const G78: &str = r#"GROUPBY({"a";"b";"a"},{1;2;3},LAMBDA(x,{7,8}),0,0)"#;

fn shown(session: &HostSession, at: CellKey) -> String {
    let projection =
        project(session.host(), &ProjectionRequest { cells: vec![at] }).expect("projected");
    match projection.cells()[0].value() {
        ProjectedValue::Calculated { value, .. } => match value.core_ref() {
            CoreRef::Number(n) => format!("{n}"),
            CoreRef::Text(t) => String::from_utf16_lossy(t.utf16_code_units()),
            CoreRef::Logical(b) => format!("{b}"),
            CoreRef::Error(code) => format!("{code:?}"),
            other => format!("{other:?}"),
        },
        other => format!("{other:?}"),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_h9u__a_groupby_cell_holds_its_lambdas_array() {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(calc_host::blank_package()));
    let mut session = HostSession::open(
        source,
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Automatic),
            ..HostOptions::default()
        },
    )
    .expect("a new workbook opens");
    let sheet = session.host().sheets()[0].id();
    let at = |row: u32, column: u32| CellKey::new(sheet, row, column).expect("a cell");
    // Each entry in column A of its own row, with room to spill two columns and two rows.
    let entries = [
        (1, format!("={G}")),
        (4, format!("=INDEX({G},1,2)")),
        (5, format!("=ROWS(INDEX({G},1,2))")),
        (6, format!("=TYPE(INDEX({G},1,2))")),
        (7, format!("=INDEX({G},1,2)*10")),
        (8, format!("=SUM({G})")),
        (9, format!("=COUNT({G})")),
        (10, format!("=MAX({G})")),
        (11, format!("={G78}*2")),
        (14, format!("=ISNUMBER({G78})")),
        (17, format!("=INDEX({G78},1,2)")),
        (18, format!("=SUM({G78})")),
    ];
    for (row, text) in &entries {
        let receipt = session.dispatch(SkinIntent::EnterCell {
            at: at(*row, 1),
            text: text.clone(),
        });
        assert!(receipt.accepted(), "{text}: {receipt:?}");
    }
    let value = format!("{:?}", WorksheetErrorCode::Value);
    let na = format!("{:?}", WorksheetErrorCode::NA);
    for (row, column, expected) in [
        // =GROUPBY(...): A1:B2 a|#VALUE!|b|#VALUE!
        (1, 1, "a"),
        (1, 2, value.as_str()),
        (2, 1, "b"),
        (2, 2, value.as_str()),
        (4, 1, "1"),
        (5, 1, "2"),
        (6, 1, "1"),
        (7, 1, "10"),
        (8, 1, "6"),
        (9, 1, "3"),
        (10, 1, na.as_str()),
        // ={G78}*2: every cell #VALUE! (the labels are text).
        (11, 1, value.as_str()),
        (11, 2, value.as_str()),
        (12, 2, value.as_str()),
        // =ISNUMBER({G78}): FALSE beside the labels, TRUE beside the held arrays.
        (14, 1, "false"),
        (14, 2, "true"),
        (15, 2, "true"),
        (17, 1, "7"),
        (18, 1, "30"),
    ] {
        assert_eq!(shown(&session, at(row, column)), expected, "row {row}, column {column}");
    }
}
