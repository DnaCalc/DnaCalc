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

const ORACLE: &[u8] = include_bytes!("fixtures/pivot_cube/oracle.xlsx");

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_8wp_18__cube_oracle_recalculates_all_captured_cells() {
    let mut host = open(ORACLE.to_vec());
    let sheet = host.sheets()[0].id();
    let cells: Vec<_> = (1..=134).map(|row| CellKey::new(sheet, row, 1).unwrap()).collect();
    let initial = project(&host, &ProjectionRequest { cells: cells.clone() }).unwrap();
    let expected: Vec<_> = initial.cells().iter().map(|cell| match cell.value() {
        ProjectedValue::FileCached(file) => Some(shown(&file.value().to_value())),
        _ => None,
    }).collect();
    assert_eq!(expected.iter().flatten().count(), 131);
    calculate_full(&mut host);
    settle(&mut host);
    let result = project(&host, &ProjectionRequest { cells }).unwrap();
    let differences: Vec<_> = result.cells().iter().zip(expected.iter()).enumerate().filter_map(|(i, (cell, expected))| {
        let expected = expected.as_ref()?;
        match cell.value() {
            ProjectedValue::Calculated { value, .. } if shown(value) == *expected => None,
            other => Some(format!("A{}: Excel {expected}, DNA {other:?}", i + 1)),
        }
    }).collect();
    assert!(differences.is_empty(), "{} differences: {differences:#?}", differences.len());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_8wp_18__model_changes_only_on_refresh_all() {
    use calc_host::{HostSession, SkinIntent, WorkbookCommand};
    let mut host = open(ORACLE.to_vec());
    calculate_full(&mut host);
    settle(&mut host);
    let sheet = host.sheets()[0].id();
    let total = CellKey::new(sheet, 71, 1).unwrap();
    let amount = CellKey::new(sheet, 2, 22).unwrap();
    let mut session = HostSession::new(host);
    let result = |session: &HostSession| {
        let p = project(session.host(), &ProjectionRequest { cells: vec![total] }).unwrap();
        match p.cells()[0].value() {
            ProjectedValue::Calculated { value, .. } => shown(value),
            other => panic!("not calculated: {other:?}"),
        }
    };
    assert_eq!(result(&session), "42.3");
    assert!(session.dispatch(SkinIntent::EnterCell { at: amount, text: "100".into() }).accepted());
    let mut saved_before_refresh = Vec::new();
    assert!(save(session.host(), &SavePolicy::default(), &mut saved_before_refresh).is_err());
    assert!(saved_before_refresh.is_empty());
    assert!(session.dispatch(SkinIntent::Rebuild).accepted());
    while session.has_unfinished_calculation() { session.advance_calculation(); }
    assert_eq!(result(&session), "42.3");
    let revision = session.host().revision().get();
    let connection = session.dispatch(SkinIntent::Command(WorkbookCommand::RefreshConnection { revision, name: "ThisWorkbookDataModel".into() }));
    assert!(connection.accepted());
    assert!(connection.committed.is_none());
    assert_eq!(result(&session), "42.3");
    let revision = session.host().revision().get();
    let receipt = session.dispatch(SkinIntent::Command(WorkbookCommand::RefreshAll { revision }));
    assert!(receipt.accepted());
    assert!(receipt.committed.as_ref().unwrap().pending().contains(&total), "model refresh must mark CUBE cells pending");
    assert!(session.dispatch(SkinIntent::Recalculate).accepted());
    while session.has_unfinished_calculation() { session.advance_calculation(); }
    assert_eq!(result(&session), "132.3");
    let mut saved = Vec::new();
    assert!(save(session.host(), &SavePolicy::default(), &mut saved).is_err(), "changed model bytes cannot be saved yet");
    assert!(saved.is_empty());
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    assert!(session.dispatch(SkinIntent::Recalculate).accepted());
    while session.has_unfinished_calculation() { session.advance_calculation(); }
    assert_eq!(result(&session), "42.3");
    assert!(session.dispatch(SkinIntent::Redo).accepted());
    assert!(session.dispatch(SkinIntent::Recalculate).accepted());
    while session.has_unfinished_calculation() { session.advance_calculation(); }
    assert_eq!(result(&session), "132.3");

    for (column, text) in [(20, "South"), (21, "D"), (22, "3")] {
        let at = CellKey::new(sheet, 8, column).unwrap();
        assert!(session.dispatch(SkinIntent::EnterCell { at, text: text.into() }).accepted());
    }
    let mut tables = session.host().model().definitions().tables().to_vec();
    let table = tables.iter_mut().find(|t| t.name == "Table1").unwrap();
    table.area = calc_host::value::GridArea::new(table.area.first(), CellKey::new(sheet, 8, 22).unwrap()).unwrap();
    if let Some(filter) = &mut table.filter { filter.area = table.area; }
    let receipt = session.dispatch(SkinIntent::Edit { edits: vec![calc_host::CellEdit::Tables(tables)] });
    assert!(receipt.accepted(), "{receipt:?}");
    assert!(session.dispatch(SkinIntent::Rebuild).accepted());
    while session.has_unfinished_calculation() { session.advance_calculation(); }
    assert_eq!(result(&session), "132.3");
    let revision = session.host().revision().get();
    assert!(session.dispatch(SkinIntent::Command(WorkbookCommand::RefreshAll { revision })).accepted());
    assert!(session.dispatch(SkinIntent::Recalculate).accepted());
    while session.has_unfinished_calculation() { session.advance_calculation(); }
    assert_eq!(result(&session), "135.3");
    let count_cells = [58, 89].map(|row| CellKey::new(sheet, row, 1).unwrap()).to_vec();
    let counts = project(session.host(), &ProjectionRequest { cells: count_cells }).unwrap();
    let values: Vec<_> = counts.cells().iter().map(|cell| match cell.value() {
        ProjectedValue::Calculated { value, .. } => shown(value),
        other => panic!("not calculated: {other:?}"),
    }).collect();
    assert_eq!(values, ["5.0", "7.0"]);
    assert!(session.dispatch(SkinIntent::Edit { edits: vec![calc_host::CellEdit::Tables(Vec::new())] }).accepted());
    let revision = session.host().revision().get();
    assert!(session.dispatch(SkinIntent::Command(WorkbookCommand::RefreshAll { revision })).accepted());
    assert!(session.dispatch(SkinIntent::Recalculate).accepted());
    while session.has_unfinished_calculation() { session.advance_calculation(); }
    assert_eq!(result(&session), "135.3");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_8wp_18__malformed_model_snapshot_is_refused() {
    let mut host = open(ORACLE.to_vec());
    let mut data = (**host.model().definitions().data_model().unwrap()).clone();
    data.snapshot.as_mut().unwrap().tables[0].rows[0].clear();
    let command = calc_host::HostCommand { base: host.revision(), edits: vec![calc_host::CellEdit::DataModel(Arc::new(data))] };
    assert!(calc_host::dispatch(&mut host, command).is_err());
    let mut forged = (**host.model().definitions().data_model().unwrap()).clone();
    forged.snapshot.as_mut().unwrap().tables[0].rows[0][2] = Some(calc_host::CachedValue::Number(calc_host::FiniteNumber::new(999.0).unwrap()));
    let command = calc_host::HostCommand { base: host.revision(), edits: vec![calc_host::CellEdit::DataModel(Arc::new(forged))] };
    assert!(calc_host::dispatch(&mut host, command).is_err());
}

/// The M4 saved model-pivot workbook: only newly calculated results count as matches.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_8wp_19__model_pivot_and_mdx_capture_report() {
    use calc_host::{HostSession, SkinIntent, WorkbookCommand};
    let mut host = open(include_bytes!("fixtures/pivot_cube/model-pivot.xlsx").to_vec());
    calculate_full(&mut host);
    settle(&mut host);
    let stages = include_str!("fixtures/pivot_cube/stages.tsv");
    let sheet = |host: &HostState, name: &str| host.sheets().iter().find(|s|
        String::from_utf16_lossy(s.name().text().utf16_code_units()) == name).unwrap().id();
    let gpd = sheet(&host, "GpdModel");
    let rows: Vec<_> = stages.lines().map(|l| l.split('\t').collect::<Vec<_>>())
        .filter(|fields| fields[0] == "gpd-model").collect();
    assert_eq!(rows.len(), 11);
    let keys = rows.iter().map(|r| CellKey::new(gpd, r[1][1..].parse().unwrap(), 2).unwrap()).collect();
    let result = project(&host, &ProjectionRequest { cells: keys }).unwrap();
    let mut counts = [0usize; 4]; // match, mismatch, refused, missing
    for (row, cell) in rows.iter().zip(result.cells()) {
        if row[2] == "e" {
            assert!(matches!(cell.value(), ProjectedValue::Calculated { value, .. }
                if matches!(value.core_ref(), CoreRef::Error(e) if e.excel_spelling() == row[4])),
                "{}: Excel {}, host {:?}", row[1], row[4], cell.value());
        }
        match cell.value() {
            ProjectedValue::Calculated { value, .. } => {
                let matched = match (row[2], value.core_ref()) {
                    ("n", CoreRef::Number(n)) => row[3].parse::<f64>().ok() == Some(n),
                    ("e", CoreRef::Error(e)) => e.excel_spelling() == row[4],
                    _ => false,
                };
                counts[usize::from(!matched)] += 1;
            }
            ProjectedValue::NotModeled { .. } => counts[2] += 1,
            _ => counts[3] += 1,
        }
    }
    println!("M4 MDX: {}/11 match; {} fail ({} mismatched, {} refused, {} missing)", counts[0], 11-counts[0], counts[1], counts[2], counts[3]);
    let pivot_sheet = sheet(&host, "ModelPivot");
    let part = host.model().definitions().pivots().sheet(pivot_sheet).unwrap().tables.iter()
        .find(|t| t.name == "pvt_model").unwrap().part.clone();
    let mut book = HostSession::new(host);
    // A source edit does not change the retained model until Refresh All. The missing-member
    // result must therefore not read the just-edited worksheet cell.
    let source_sheet = sheet(book.host(), "Sheet1");
    assert!(book.dispatch(SkinIntent::EnterCell { at: CellKey::new(source_sheet, 2, 20).unwrap(), text: "Nowhere".into() }).accepted());
    assert!(book.dispatch(SkinIntent::Rebuild).accepted());
    while book.has_unfinished_calculation() { book.advance_calculation(); }
    let missing = CellKey::new(gpd, 5, 2).unwrap();
    let page = project(book.host(), &ProjectionRequest { cells: vec![missing] }).unwrap();
    assert!(matches!(page.cells()[0].value(), ProjectedValue::Calculated { value, .. }
        if matches!(value.core_ref(), CoreRef::Error(e) if e.excel_spelling() == "#REF!")));
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    let revision = book.host().revision().get();
    let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::PivotRefresh { sheet: pivot_sheet, revision, part }));
    let rows: Vec<_> = stages.lines().map(|l| l.split('\t').collect::<Vec<_>>())
        .filter(|fields| fields[0] == "model-pivot").collect();
    assert_eq!(rows.len(), 28);
    assert!(receipt.accepted(), "M4 model pivot refresh: {receipt:?}");
    let keys = rows.iter().map(|r| {
        let address = r[1].as_bytes();
        CellKey::new(pivot_sheet, r[1][1..].parse().unwrap(), u32::from(address[0] - b'A' + 1)).unwrap()
    }).collect();
    let page = project(book.host(), &ProjectionRequest { cells: keys }).unwrap();
    let mut matches = 0;
    for (row, cell) in rows.iter().zip(page.cells()) {
        if let ProjectedValue::Calculated { value, .. } = cell.value() {
            if match (row[2], value.core_ref()) {
                ("n", CoreRef::Number(n)) => row[3].parse::<f64>().ok() == Some(n),
                ("s", CoreRef::Text(t)) => t.to_string_lossy() == row[3],
                _ => false,
            } { matches += 1; }
        }
    }
    println!("M4 model pivot: {matches}/28 match; {} fail", 28-matches);
    assert_eq!(matches, 28);
    assert!(book.dispatch(SkinIntent::Rebuild).accepted());
    while book.has_unfinished_calculation() { book.advance_calculation(); }
    let rows: Vec<_> = stages.lines().map(|l| l.split('\t').collect::<Vec<_>>())
        .filter(|fields| fields[0] == "gpd-model").collect();
    let keys = rows.iter().map(|r| CellKey::new(gpd, r[1][1..].parse().unwrap(), 2).unwrap()).collect();
    let page = project(book.host(), &ProjectionRequest { cells: keys }).unwrap();
    for (row, cell) in rows.iter().zip(page.cells()) {
        let ProjectedValue::Calculated { value, .. } = cell.value() else { panic!("{} was not calculated: {:?}", row[1], cell.value()); };
        assert!(match (row[2], value.core_ref()) {
            ("n", CoreRef::Number(n)) => row[3].parse::<f64>().ok() == Some(n),
            ("e", CoreRef::Error(e)) => e.excel_spelling() == row[4],
            _ => false,
        }, "{} Excel {} host {:?}", row[1], row[4], value);
    }
    println!("M4 MDX after refresh: 11/11 match; 0 fail");
    let mut saved = Vec::new();
    book.save(&SavePolicy::default(), &mut saved).unwrap();
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(saved));
    let mut reopened = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let sheet = reopened.host().sheets().iter().find(|s| String::from_utf16_lossy(s.name().text().utf16_code_units()) == "ModelPivot").unwrap().id();
    let part = reopened.host().model().definitions().pivots().sheet(sheet).unwrap().tables.iter()
        .find(|table| table.name == "pvt_model").unwrap().part.clone();
    let receipt = reopened.dispatch(SkinIntent::Command(WorkbookCommand::PivotRefresh { sheet, revision: reopened.host().revision().get(), part }));
    assert!(receipt.accepted(), "reopened model pivot: {receipt:?}");
    let page = project(reopened.host(), &ProjectionRequest { cells: vec![CellKey::new(sheet, 9, 6).unwrap()] }).unwrap();
    assert!(matches!(page.cells()[0].value(), ProjectedValue::Calculated { value, .. } if value.as_number() == Some(135.3)));
}


/// A model pivot reads its retained snapshot, publishes through the pivot operation and refuses
/// forged snapshot provenance. Exact arithmetic is refused until it has an implementation.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_8wp_19__model_pivot_keeps_snapshot_and_rejects_forgery() {
    use calc_host::{CellEdit, HostSession, SkinIntent, WorkbookCommand};
    use calc_host::value::{NumericMode, NumericTargetId};
    use calc_doc_model::{CachedValue, FiniteNumber};
    use calc_doc_model::pivot::PivotSnapshotOrigin;
    let target = NumericTargetId::new("16.0.20527.20150", "Current", "Excel", "x86-64").unwrap();
    for numeric in [NumericMode::Fast, NumericMode::Exact(target.clone())] {
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(include_bytes!("fixtures/pivot_cube/model-pivot.xlsx").to_vec()));
        let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions { numeric: numeric.clone(), calc_mode: Some(CalcMode::Manual), ..Default::default() }).unwrap();
        let sheet = |book: &HostSession, name: &str| book.host().sheets().iter().find(|s| String::from_utf16_lossy(s.name().text().utf16_code_units()) == name).unwrap().id();
        let source_sheet = sheet(&book, "Sheet1");
        for (col, value) in [(20, "Nowhere"), (22, "999")] {
            assert!(book.dispatch(SkinIntent::EnterCell { at: CellKey::new(source_sheet, 2, col).unwrap(), text: value.into() }).accepted());
        }
        let pivot_sheet = sheet(&book, "ModelPivot");
        let (part, cache_id) = {
            let table = book.host().model().definitions().pivots().sheet(pivot_sheet).unwrap().tables.iter().find(|table| table.name == "pvt_model").unwrap();
            (table.part.clone(), table.cache_id)
        };
        if numeric != NumericMode::Fast {
            let revision = book.host().revision();
            let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::PivotRefresh { sheet: pivot_sheet, revision: revision.get(), part }));
            assert!(!receipt.accepted(), "Exact arithmetic must not be silently calculated as Fast: {receipt:?}");
            assert!(format!("{receipt:?}").contains("dc-8wp.19"), "{receipt:?}");
            assert_eq!(book.host().revision(), revision);
            let total = CellKey::new(pivot_sheet, 9, 6).unwrap();
            let page = project(book.host(), &ProjectionRequest { cells: vec![total] }).unwrap();
            assert!(!matches!(page.cells()[0].value(), ProjectedValue::Calculated { .. }));
            let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::RefreshAll { revision: revision.get() }));
            assert!(!receipt.accepted(), "Exact Refresh All must not publish Fast results: {receipt:?}");
            assert_eq!(book.host().revision(), revision);
            continue;
        }
        let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::PivotRefresh { sheet: pivot_sheet, revision: book.host().revision().get(), part }));
        assert!(receipt.accepted(), "{receipt:?}");
        let total = CellKey::new(pivot_sheet, 9, 6).unwrap();
        let page = project(book.host(), &ProjectionRequest { cells: vec![total] }).unwrap();
        assert!(matches!(page.cells()[0].value(), ProjectedValue::Calculated { value, calculation }
            if value.as_number() == Some(135.3) && calculation.mode() == &numeric && calculation.revision().get() == 0));
        assert!(book.dispatch(SkinIntent::Rebuild).accepted());
        while book.has_unfinished_calculation() { book.advance_calculation(); }
        let gpd = CellKey::new(sheet(&book, "GpdModel"), 1, 2).unwrap();
        let page = project(book.host(), &ProjectionRequest { cells: vec![gpd] }).unwrap();
        assert!(matches!(page.cells()[0].value(), ProjectedValue::Calculated { value, .. } if value.as_number() == Some(135.3)), "{numeric:?}: {:?}", page.cells()[0].value());
        assert!(book.save(&SavePolicy::default(), &mut Vec::new()).is_err(), "changed source rows cannot be saved as a changed model");
        for case in 0..4 {
            let mut state = (**book.host().model().definitions().pivots()).clone();
            let cache = Arc::make_mut(state.caches.caches.get_mut(&cache_id).unwrap());
            let snapshot = Arc::make_mut(cache.snapshot.as_mut().unwrap());
            match case {
                0 => snapshot.source_revision += 1,
                1 => snapshot.numeric = if numeric == NumericMode::Fast { NumericMode::Exact(target.clone()) } else { NumericMode::Fast },
                2 => snapshot.records.push(Vec::new()),
                _ => {
                    let PivotSnapshotOrigin::DataModel { model } = &mut snapshot.origin else { panic!("model origin expected"); };
                    let model = Arc::make_mut(model);
                    model.snapshot.as_mut().unwrap().tables[0].rows[0][2] = Some(CachedValue::Number(FiniteNumber::new(999.0).unwrap()));
                }
            }
            let revision = book.host().revision();
            let receipt = book.dispatch(SkinIntent::Edit { edits: vec![CellEdit::Pivots(Arc::new(state))] });
            assert!(!receipt.accepted(), "forged case {case}: {receipt:?}");
            assert_eq!(book.host().revision(), revision);
            let page = project(book.host(), &ProjectionRequest { cells: vec![total] }).unwrap();
            assert!(matches!(page.cells()[0].value(), ProjectedValue::Calculated { value, .. } if value.as_number() == Some(135.3)));
        }
        let revision = book.host().revision();
        let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::RefreshAll { revision: revision.get() }));
        assert!(!receipt.accepted(), "a changed axis cannot be silently reused: {receipt:?}");
        assert_eq!(book.host().revision(), revision);
        let page = project(book.host(), &ProjectionRequest { cells: vec![total] }).unwrap();
        assert!(matches!(page.cells()[0].value(), ProjectedValue::Calculated { value, .. } if value.as_number() == Some(135.3)));

        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(include_bytes!("fixtures/pivot_cube/model-pivot.xlsx").to_vec()));
        let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions { numeric: numeric.clone(), calc_mode: Some(CalcMode::Manual), ..Default::default() }).unwrap();
        let pivot_sheet = sheet(&book, "ModelPivot");
        let part = book.host().model().definitions().pivots().sheet(pivot_sheet).unwrap().tables.iter().find(|table| table.name == "pvt_model").unwrap().part.clone();
        assert!(book.dispatch(SkinIntent::Command(WorkbookCommand::PivotRefresh { sheet: pivot_sheet, revision: book.host().revision().get(), part })).accepted());
        let source_sheet = sheet(&book, "Sheet1");
        assert!(book.dispatch(SkinIntent::EnterCell { at: CellKey::new(source_sheet, 2, 22).unwrap(), text: "999".into() }).accepted());
        let total = CellKey::new(pivot_sheet, 9, 6).unwrap();
        let page = project(book.host(), &ProjectionRequest { cells: vec![total] }).unwrap();
        assert!(matches!(page.cells()[0].value(), ProjectedValue::Calculated { value, .. } if value.as_number() == Some(135.3)));
        let revision = book.host().revision().get();
        let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::RefreshAll { revision }));
        assert!(receipt.accepted(), "{numeric:?}: {receipt:?}");
        let page = project(book.host(), &ProjectionRequest { cells: vec![total] }).unwrap();
        assert!(matches!(page.cells()[0].value(), ProjectedValue::Calculated { value, calculation } if value.as_number() == Some(1034.3) && calculation.revision().get() == revision && calculation.mode() == &numeric), "{numeric:?}: {:?}", page.cells()[0].value());
        assert!(book.dispatch(SkinIntent::Rebuild).accepted());
        while book.has_unfinished_calculation() { book.advance_calculation(); }
        let gpd = CellKey::new(sheet(&book, "GpdModel"), 1, 2).unwrap();
        let page = project(book.host(), &ProjectionRequest { cells: vec![gpd] }).unwrap();
        assert!(matches!(page.cells()[0].value(), ProjectedValue::Calculated { value, .. } if value.as_number() == Some(1034.3)), "{numeric:?}: {:?}", page.cells()[0].value());
        let numeric_argument = CellKey::new(sheet(&book, "GpdModel"), 20, 2).unwrap();
        assert!(book.dispatch(SkinIntent::EnterCell { at: numeric_argument, text: "=GETPIVOTDATA(1,ModelPivot!$A$3)".into() }).accepted());
        assert!(book.dispatch(SkinIntent::Rebuild).accepted());
        while book.has_unfinished_calculation() { book.advance_calculation(); }
        let page = project(book.host(), &ProjectionRequest { cells: vec![numeric_argument] }).unwrap();
        assert!(matches!(page.cells()[0].value(), ProjectedValue::Calculated { value, .. } if matches!(value.core_ref(), CoreRef::Error(e) if e.excel_spelling() == "#REF!")), "{:?}", page.cells()[0].value());
    }
}

/// Automatic calculation follows the published model-pivot snapshot through refresh and history.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_8wp_19__automatic_model_pivot_refresh_and_history_recalculate_mdx() {
    use calc_host::{HostSession, SkinIntent, WorkbookCommand};
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(include_bytes!("fixtures/pivot_cube/model-pivot.xlsx").to_vec()));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions { calc_mode: Some(CalcMode::Automatic), ..Default::default() }).unwrap();
    while book.has_unfinished_calculation() { book.advance_calculation(); }
    let sheet = |book: &HostSession, name: &str| book.host().sheets().iter().find(|s| String::from_utf16_lossy(s.name().text().utf16_code_units()) == name).unwrap().id();
    let pivot_sheet = sheet(&book, "ModelPivot");
    let part = book.host().model().definitions().pivots().sheet(pivot_sheet).unwrap().tables.iter().find(|table| table.name == "pvt_model").unwrap().part.clone();
    let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::PivotRefresh { sheet: pivot_sheet, revision: book.host().revision().get(), part }));
    assert!(receipt.accepted(), "{receipt:?}");
    while book.has_unfinished_calculation() { book.advance_calculation(); }
    let total = CellKey::new(pivot_sheet, 9, 6).unwrap();
    let gpd = CellKey::new(sheet(&book, "GpdModel"), 1, 2).unwrap();
    let check = |book: &HostSession, expected| {
        let page = project(book.host(), &ProjectionRequest { cells: vec![total, gpd] }).unwrap();
        for cell in page.cells() {
            assert!(matches!(cell.value(), ProjectedValue::Calculated { value, .. } if value.as_number() == Some(expected)), "{:?}: {:?}", cell.at(), cell.value());
        }
    };
    check(&book, 135.3);
    let source_sheet = sheet(&book, "Sheet1");
    assert!(book.dispatch(SkinIntent::EnterCell { at: CellKey::new(source_sheet, 2, 22).unwrap(), text: "999".into() }).accepted());
    while book.has_unfinished_calculation() { book.advance_calculation(); }
    check(&book, 135.3);
    let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::RefreshAll { revision: book.host().revision().get() }));
    assert!(receipt.accepted(), "{receipt:?}");
    while book.has_unfinished_calculation() { book.advance_calculation(); }
    check(&book, 1034.3);
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    while book.has_unfinished_calculation() { book.advance_calculation(); }
    check(&book, 135.3);
    assert!(book.dispatch(SkinIntent::Redo).accepted());
    while book.has_unfinished_calculation() { book.advance_calculation(); }
    check(&book, 1034.3);
}
