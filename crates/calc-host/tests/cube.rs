//! The CUBE functions end to end: a workbook Excel 16.0.20527 saved with a data model over a
//! worksheet table, opened, recalculated and its cells compared with what Excel cached.
//! `fixtures/cube/measured.xlsx` is that workbook with paths and names removed; its model part
//! is rebuilt to hold the measures alone, since the rows come from the worksheet table.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_host::value::{CellKey, CoreRef};
use calc_host::{
    CalcMode, HostOptions, HostProgress, HostState, MemorySource, OpenPolicy, PackageRead,
    ProjectedValue, ProjectionRequest, SavePolicy, Unobserved, WorkBudget, calculate_full, project,
    save, step,
};
use std::sync::Arc;

const MEASURED: &[u8] = include_bytes!("fixtures/cube/measured.xlsx");
/// Formula rows of column A.
const ROWS: u32 = 112;

fn open(bytes: Vec<u8>) -> HostState {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
    HostState::open(
        source,
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Manual),
            ..HostOptions::default()
        },
    )
    .expect("the package opens")
}

fn settle(host: &mut HostState) {
    loop {
        match step(host, WorkBudget::UNLIMITED, &mut Unobserved) {
            HostProgress::Calculating { .. } => {}
            HostProgress::Failed(why) => panic!("recalculation failed: {why}"),
            _ => break,
        }
    }
}

fn shown(value: &calc_host::value::CalcValue) -> String {
    match value.core_ref() {
        CoreRef::Number(n) => format!("{n:?}"),
        CoreRef::Text(text) => format!("{:?}", text.to_string_lossy()),
        CoreRef::Error(code) => format!("{code:?}"),
        CoreRef::Logical(b) => format!("{b}"),
        other => format!("{other:?}"),
    }
}

fn column_a(host: &HostState) -> Vec<CellKey> {
    let sheet = host.sheets()[0].id();
    (1..=ROWS)
        .map(|row| CellKey::new(sheet, row, 1).expect("in the grid"))
        .collect()
}

fn cached(host: &HostState) -> Vec<Option<String>> {
    let cells = column_a(host);
    let projection = project(host, &ProjectionRequest { cells }).expect("projected");
    projection
        .cells()
        .iter()
        .map(|cell| match cell.value() {
            ProjectedValue::FileCached(file) => Some(shown(&file.value().to_value())),
            _ => None,
        })
        .collect()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_8wp__excels_saved_cube_workbook_recalculates_to_its_own_values() {
    let mut host = open(MEASURED.to_vec());
    let excel = cached(&host);
    assert!(excel.iter().flatten().count() >= 100, "the file's cached values show");
    calculate_full(&mut host);
    settle(&mut host);
    let cells = column_a(&host);
    let projection = project(&host, &ProjectionRequest { cells }).expect("projected");
    let (mut matched, mut wrong) = (0, Vec::new());
    for (index, (cell, excel)) in projection.cells().iter().zip(&excel).enumerate() {
        let row = index as u32 + 1;
        let Some(excel) = excel else { continue };
        match cell.value() {
            ProjectedValue::Calculated { value, .. } => {
                let ours = shown(value);
                if &ours == excel {
                    matched += 1;
                } else {
                    wrong.push(format!("A{row}: Excel {excel}, ours {ours}"));
                }
            }
            other => wrong.push(format!("A{row}: {other:?}")),
        }
    }
    assert!(wrong.is_empty(), "{} differ: {wrong:#?}", wrong.len());
    assert_eq!(matched, excel.iter().flatten().count());
}

/// A save keeps the formulas, the cached values and the model part, and reopens to them.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_8wp__a_cube_workbook_saves_and_reopens() {
    let mut host = open(MEASURED.to_vec());
    let excel = cached(&host);
    calculate_full(&mut host);
    settle(&mut host);
    let mut saved = Vec::new();
    save(&host, &SavePolicy::default(), &mut saved).expect("saved");
    let reopened = open(saved);
    assert_eq!(column_a(&reopened).len(), excel.len());
    let again = cached(&reopened);
    for (index, (first, second)) in excel.iter().zip(&again).enumerate() {
        let row = index as u32 + 1;
        assert_eq!(first, second, "row {row}");
    }
}
