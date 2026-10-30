//! dc-b1l: OFFSET and INDIRECT lifted over an array give an array of references, end to end
//! through calc-host in dynamic-array formulas typed as Excel's Range.Formula2 enters them. Every
//! expectation is Excel 16.0.20527's, read through COM on 2026-10-30 over 1..6 in A1:A6, 10..60 in
//! B1:B6 and "x" in C1 (the catalogue's dcb1l_language rows hold legacy-formula forms, which the
//! oracle corpus compares): SUM, AVERAGE and MAX read each reference's first cell, COUNT counts
//! none and COUNTA each, SUBTOTAL, SUMIF, COUNTIF, AREAS, ROW, INDEX, N and T run once per
//! reference, and the array itself, an operator over it and TEXT of it are #VALUE!.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_host::value::{CellKey, CoreRef, WorksheetErrorCode};
use calc_host::{
    CalcMode, HostOptions, HostSession, MemorySource, OpenPolicy, PackageRead, ProjectedValue,
    ProjectionRequest, SkinIntent, project,
};
use std::sync::Arc;

fn shown(session: &HostSession, at: CellKey) -> String {
    let projection =
        project(session.host(), &ProjectionRequest { cells: vec![at] }).expect("projected");
    match projection.cells()[0].value() {
        ProjectedValue::Calculated { value, .. } => match value.core_ref() {
            CoreRef::Number(n) => format!("{n}"),
            CoreRef::Text(t) => format!("{:?}", String::from_utf16_lossy(t.utf16_code_units())),
            CoreRef::Logical(b) => format!("{b}"),
            CoreRef::Error(code) => format!("{code:?}"),
            other => format!("{other:?}"),
        },
        ProjectedValue::NotModeled { .. } => "not modelled".to_owned(),
        other => format!("{other:?}"),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_b1l__offset_and_indirect_over_an_array_give_references() {
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
    let mut enter = |row: u32, column: u32, text: &str| {
        let receipt = session.dispatch(SkinIntent::EnterCell {
            at: at(row, column),
            text: text.to_owned(),
        });
        assert!(receipt.accepted(), "{text}: {receipt:?}");
    };
    for row in 1..=6 {
        enter(row, 1, &row.to_string());
        enter(row, 2, &(10 * row).to_string());
    }
    enter(1, 3, "x");
    // Each formula in column E, four rows apart so that a spill of up to three rows has room.
    let formulas = [
        "=SUM(OFFSET(A1,{0,1,2},0))",
        "=SUM(OFFSET(A1,{0;1;2},0,2))",
        "=SUBTOTAL(9,OFFSET(A1,{0;1;2},0,2))",
        "=SUM(SUBTOTAL(9,OFFSET(A1,{0;1;2},0,2)))",
        "=N(OFFSET(A1,{0;1;2},0))",
        "=SUMIF(OFFSET(A1,{0;1},0,3),\">2\")",
        "=SUM(INDIRECT({\"A1\",\"A2\",\"B1\"}))",
        "=ROWS(OFFSET(A1,{0;1;2},0))",
        "=OFFSET(A1,{0;1;2},0)",
        "=COUNT(OFFSET(A1,{0,1,2},0))",
        "=COUNTA(OFFSET(A1,{0,1,2},0))",
        "=MAX(OFFSET(A1,{0,1,2},0,2))",
        "=AVERAGE(OFFSET(A1,{0,1,2},0))",
        "=ISREF(OFFSET(A1,{0;1},0))",
        "=AREAS(OFFSET(A1,{0;1},0))",
        "=ROW(OFFSET(A1,{0;1},0))",
        "=SUM(OFFSET(A1,{0,1,2},0)*1)",
        "=TEXT(OFFSET(A1,{0;1},0),\"0.0\")",
        "=INDEX(OFFSET(A1,{0;1},0,2),1,1)",
        "=T(OFFSET(C1,{0;1},0))",
        "=SUM(INDIRECT({\"A1:A2\",\"B1:B2\"}))",
    ];
    for (i, formula) in formulas.iter().enumerate() {
        enter(4 * i as u32 + 1, 5, formula);
    }
    let value = format!("{:?}", WorksheetErrorCode::Value);
    let expected: [&[&str]; 21] = [
        &["6"],
        &["6"],
        &["3", "5", "7"],
        &["15"],
        &["1", "2", "3"],
        &["3", "7"],
        &["13"],
        &["3"],
        &[&value, &value, &value],
        &["0"],
        &["3"],
        &["3"],
        &["2"],
        &["false"],
        &["1", "1"],
        &["1", "2"],
        &[&value],
        &[&value, &value],
        &["1", "2"],
        &["\"x\"", "\"\""],
        &["11"],
    ];
    for (i, values) in expected.iter().enumerate() {
        for (offset, value) in values.iter().enumerate() {
            let row = 4 * i as u32 + 1 + offset as u32;
            assert_eq!(
                shown(&session, at(row, 5)),
                *value,
                "{} (E{row})",
                formulas[i]
            );
        }
    }
}
