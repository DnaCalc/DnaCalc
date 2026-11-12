//! dc-1nbp.12: an edit the save would refuse is refused when it is made (the save's own refusal,
//! in `CommandError::LayoutSave`), with the revision and page unchanged; an unrelated cell edit
//! of the same book still commits and saves.

#![allow(non_snake_case)]

use calc_host::value::{CellKey, GridArea, SheetId};
use calc_host::{
    Axis, AxisChange, AxisEdit, CellEdit, CellInput, CommandError, FiniteNumber, HostOptions,
    HostSession, IntentError, MemorySource, OpenPolicy, PackageRead, SavePolicy, SaveRefusal,
    SkinIntent,
};
use std::sync::Arc;

const MACRO: &[u8] = include_bytes!("../../calc-doc/tests/fixtures/dc_xuw/macro-sheet.xlsm");
const PRESERVED: &[u8] =
    include_bytes!("../../calc-doc/tests/fixtures/preserved_sheets/chart-and-dialog-sheets.xlsx");
const PARTLESS: &[u8] =
    include_bytes!("../../calc-doc/tests/fixtures/reader_tolerance/dangling-rid.xlsx");

fn session(bytes: &[u8]) -> HostSession {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).expect("it opens")
}

fn sheet(book: &HostSession, name: &str) -> SheetId {
    book.host()
        .sheets()
        .iter()
        .find(|sheet| sheet.name().text().to_string_lossy() == name)
        .expect("the sheet")
        .id()
}

fn number(value: f64) -> CellInput {
    CellInput::Number(FiniteNumber::new(value).unwrap())
}

fn set(sheet: SheetId, row: u32, column: u32, value: f64) -> SkinIntent {
    SkinIntent::Edit {
        edits: vec![CellEdit::Set {
            at: CellKey::new(sheet, row, column).unwrap(),
            input: number(value),
        }],
    }
}

/// The intent is refused with the layout-save refusal `expected`, and nothing changed.
fn assert_refused(book: &mut HostSession, intent: SkinIntent, expected: &SaveRefusal) {
    let revision = book.host().revision();
    let receipt = book.dispatch(intent);
    match receipt.error {
        Some(IntentError::Command(CommandError::LayoutSave(refusal))) => {
            assert_eq!(&refusal, expected)
        }
        other => panic!("not refused at edit time: {other:?}"),
    }
    assert_eq!(book.host().revision(), revision);
}

fn assert_unrelated_edit_saves(book: &mut HostSession, data: SheetId) {
    assert!(book.dispatch(set(data, 1, 2, 5.0)).accepted());
    let mut bytes = Vec::new();
    book.save(&SavePolicy::default(), &mut bytes).expect("the save");
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
    let again = HostSession::open(source, &OpenPolicy::default(), HostOptions::default())
        .expect("it reopens");
    let at = CellKey::new(sheet(&again, "Data"), 1, 2).unwrap();
    assert_eq!(again.host().model().cell(at), Some(&number(5.0)));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_1nbp_12__a_macro_sheet_refuses_axis_and_cell_edits_at_edit_time() {
    let mut book = session(MACRO);
    let (data, macro_sheet) = (sheet(&book, "Data"), sheet(&book, "Macro1"));
    let part = "xl/macrosheets/sheet1.xml".to_owned();
    let expected = SaveRefusal::MacroSheetChanged { part };
    assert_refused(&mut book, set(macro_sheet, 1, 2, 5.0), &expected);
    let insert = AxisEdit::new(data, Axis::Rows, AxisChange::Insert, 1, 1).unwrap();
    assert_refused(&mut book, SkinIntent::EditAxis(insert), &expected);
    let clear = SkinIntent::Edit {
        edits: vec![CellEdit::Clear {
            area: GridArea::cell(CellKey::new(macro_sheet, 1, 2).unwrap()),
        }],
    };
    assert_refused(&mut book, clear, &expected);
    assert_unrelated_edit_saves(&mut book, data);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_1nbp_12__a_chart_or_dialog_sheet_refuses_axis_and_cell_edits_at_edit_time() {
    let mut book = session(PRESERVED);
    let data = sheet(&book, "Data");
    for (name, kind) in [("Chart1", "chart sheet"), ("Dialog1", "dialog sheet")] {
        let id = sheet(&book, name);
        let part = book.host().loaded().sheet(id).unwrap().part().name().to_owned();
        let expected = SaveRefusal::PreservedSheetChanged { part, kind };
        assert_refused(&mut book, set(id, 1, 1, 5.0), &expected);
    }
    // An axis edit of any sheet is refused with the first preserved sheet's part.
    let chart_part = {
        let id = sheet(&book, "Chart1");
        book.host().loaded().sheet(id).unwrap().part().name().to_owned()
    };
    let expected = SaveRefusal::PreservedSheetChanged { part: chart_part, kind: "chart sheet" };
    let insert = AxisEdit::new(data, Axis::Rows, AxisChange::Insert, 1, 1).unwrap();
    assert_refused(&mut book, SkinIntent::EditAxis(insert), &expected);
    assert_unrelated_edit_saves(&mut book, data);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_1nbp_12__a_sheet_without_a_part_refuses_axis_and_cell_edits_at_edit_time() {
    let mut book = session(PARTLESS);
    let one = sheet(&book, "One");
    let two = sheet(&book, "Two");
    let part = book.host().loaded().sheet(two).unwrap().part().name().to_owned();
    let expected = SaveRefusal::PreservedSheetChanged { part, kind: "sheet without a part" };
    assert_refused(&mut book, set(two, 1, 1, 5.0), &expected);
    let insert = AxisEdit::new(one, Axis::Rows, AxisChange::Insert, 1, 1).unwrap();
    assert_refused(&mut book, SkinIntent::EditAxis(insert), &expected);
    assert_unrelated_edit_saves_on(&mut book, one);
}

fn assert_unrelated_edit_saves_on(book: &mut HostSession, one: SheetId) {
    assert!(book.dispatch(set(one, 1, 1, 9.0)).accepted());
    book.save(&SavePolicy::default(), &mut Vec::new()).expect("the save");
}
