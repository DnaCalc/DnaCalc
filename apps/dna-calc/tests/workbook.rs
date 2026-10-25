//! dc-rif (D4): DNA Calc's workbook and controller over calc-host, through the route the skins run:
//! a new workbook and an Excel-saved one are opened, edited by cell entries, recalculated,
//! subscribed to, undone and redone, counted by provenance and saved.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_host::value::{CellKey, GridArea, SheetId};
use calc_host::{DeltaMirror, Interest, ProjectedValue, SkinIntent};
use dna_calc::cells::{self, Provenance};
use dna_calc::controller::{Controller, EditOrigin};
use dna_calc::workbook::{Workbook, host_options};
use leptos::prelude::*;

/// calc-doc's D1 fixture, which Excel saved: dynamic arrays, legacy arrays, names and a table.
fn fixture() -> Vec<u8> {
    include_bytes!("../../../crates/calc-doc/tests/fixtures/cdoc_m1_2/d1.saved.xlsx").to_vec()
}

fn at(sheet: SheetId, address: &str) -> CellKey {
    let (row, column) = cells::parse_address(address).expect("an address");
    CellKey::new(sheet, row, column).unwrap()
}

fn sheet(book: &Workbook, name: &str) -> SheetId {
    book.sheets()
        .into_iter()
        .find(|tab| tab.name == name)
        .expect("the sheet")
        .id
}

fn shown(book: &Workbook, at: CellKey) -> cells::CellShown {
    cells::shown(
        &book.cell(at).expect("projected"),
        book.profile(),
        book.host().number_format(at),
    )
}

fn apply(book: &mut Workbook, mirror: &mut DeltaMirror) {
    for delta in book.take_deltas() {
        mirror.apply(&delta).expect("the deltas follow one another");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_rif__a_new_workbook_is_edited_through_the_session_and_its_subscription() {
    assert!(host_options().locale.is_some(), "the en-US profile");
    let mut book = Workbook::new_blank();
    assert_eq!(book.name(), "Book1.xlsx");
    let sheet1 = sheet(&book, "Sheet1");
    let area = GridArea::new(at(sheet1, "A1"), at(sheet1, "H20")).unwrap();
    book.subscribe(Interest::Area(area)).expect("subscribed");
    let mut mirror = DeltaMirror::default();
    apply(&mut book, &mut mirror);
    assert_eq!(mirror.cells().count(), 0);

    for (address, text) in [
        ("A1", "2"),
        ("A2", "3"),
        ("A3", "=A1*A2+1"),
        ("B1", "=WEBSERVICE(\"http://example.com\")"),
        ("C1", "={1;2;3}"),
    ] {
        let receipt = book.enter(at(sheet1, address), text);
        assert!(receipt.accepted(), "{address}: {:?}", receipt.error);
    }
    assert!(book.changed());
    apply(&mut book, &mut mirror);
    let profile = book.profile().cloned();
    let seen = |mirror: &DeltaMirror, address: &str| {
        cells::shown(
            mirror.cell(at(sheet1, address)).expect("in the mirror"),
            profile.as_ref(),
            None,
        )
    };
    let a3 = seen(&mirror, "A3");
    assert_eq!(
        (a3.text.as_str(), a3.provenance, a3.formula),
        ("7", Provenance::Calculated, true)
    );
    // A web function is refused (no web access is modelled), and shows as refused.
    let b1 = seen(&mirror, "B1");
    assert_eq!(
        (b1.text.as_str(), b1.provenance),
        ("refused", Provenance::Refused)
    );
    // The spill reaches the mirror.
    assert_eq!(seen(&mirror, "C3").text, "3");
    assert_eq!(seen(&mirror, "A1").provenance, Provenance::Literal);

    // A refused entry changes nothing.
    let receipt = book.enter(at(sheet1, "D1"), "=");
    assert!(!receipt.accepted());

    // Undo and redo.
    book.enter(at(sheet1, "A1"), "5");
    apply(&mut book, &mut mirror);
    assert_eq!(seen(&mirror, "A3").text, "16");
    book.dispatch(SkinIntent::Undo);
    apply(&mut book, &mut mirror);
    assert_eq!(seen(&mirror, "A3").text, "7");
    book.dispatch(SkinIntent::Redo);
    apply(&mut book, &mut mirror);
    assert_eq!(seen(&mirror, "A3").text, "16");

    // Manual calculation leaves a changed formula stale until it is asked for.
    book.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
    book.enter(at(sheet1, "A2"), "10");
    apply(&mut book, &mut mirror);
    assert_eq!(seen(&mirror, "A3").provenance, Provenance::Stale);
    book.dispatch(SkinIntent::Recalculate);
    apply(&mut book, &mut mirror);
    assert_eq!(seen(&mirror, "A3").text, "51");

    // The census counts every formula, the refused one as refused.
    let census = book.census();
    assert_eq!(census.formulas(), 3);
    assert_eq!(
        (census.calculated(), census.refused(), census.stale()),
        (2, 1, 0)
    );
    assert_eq!(census.sheets[0].used.as_deref(), Some("Sheet1!A1:C3"));
    assert_eq!(census.refusals.values().sum::<usize>(), 1);

    // Saved and reopened, the formulas calculate again.
    let (bytes, _) = book.save().expect("saved");
    assert!(!book.changed());
    let reopened = Workbook::open("Book1.xlsx", bytes).expect("reopens");
    let sheet1 = sheet(&reopened, "Sheet1");
    let a3 = shown(&reopened, at(sheet1, "A3"));
    assert_eq!(
        (a3.text.as_str(), a3.provenance),
        ("51", Provenance::Calculated)
    );
    assert_eq!(
        cells::input_text(reopened.cell(at(sheet1, "A3")).unwrap().input()),
        "=A1*A2+1"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_rif__an_excel_workbook_opens_calculated_and_counts_its_formulas() {
    let mut book = Workbook::open("d1.saved.xlsx", fixture()).expect("it opens");
    let calc = sheet(&book, "Calc");
    // Opened in automatic mode, the workbook is recalculated: values are the engine's.
    let u3 = shown(&book, at(calc, "U3"));
    assert_eq!(
        (u3.text.as_str(), u3.provenance),
        ("21", Provenance::Calculated)
    );
    assert_eq!(shown(&book, at(calc, "B2")).text, "4");
    let census = book.census();
    // The package's 17 formula elements (16 on Calc, 1 on Data), and the two members of the legacy
    // array L1:L3 beside its anchor, which Excel counts as formula cells; the cells the dynamic
    // arrays spill into are not formula cells.
    assert_eq!(census.formulas(), 19);
    // Every formula cell is calculated or refused; nothing is left as the file stored it.
    assert_eq!(census.file_cached(), 0);
    assert_eq!(census.stale(), 0);
    assert_eq!(census.calculated() + census.refused(), census.formulas());
    assert!(book.used_area(calc).is_some());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_rif__the_controller_edits_the_active_cell_and_follows_the_selection() {
    let owner = Owner::new();
    owner.with(|| {
        let controller = Controller::new();
        let sheet1 = controller.sheet.get_untracked().expect("a sheet");
        controller.select(2, 1);
        controller.begin_edit(EditOrigin::Cell, Some("4".to_owned()));
        assert!(controller.commit_edit());
        assert_eq!(controller.editing.get_untracked(), None);
        controller.select(3, 1);
        controller.edit_text.set("=A2*2".to_owned());
        controller.editing.set(Some(EditOrigin::Bar));
        assert!(controller.commit_edit());
        // The application's selection subscription holds the active cell.
        let a3 = controller
            .selection
            .with_untracked(|mirror| mirror.cell(at(sheet1, "A3")).cloned())
            .expect("the selection holds A3");
        assert!(matches!(a3.value(), ProjectedValue::Calculated { .. }));
        assert_eq!(cells::shown(&a3, None, None).text, "8");
        // A refused entry keeps the edit open and says why.
        controller.begin_edit(EditOrigin::Cell, Some("=".to_owned()));
        assert!(!controller.commit_edit());
        assert_eq!(controller.editing.get_untracked(), Some(EditOrigin::Cell));
        assert!(controller.status.get_untracked().refused);
        controller.cancel_edit();
        // Undo restores the selection of the edit it takes back.
        controller.select(1, 5);
        controller.undo();
        assert_eq!(controller.active.get_untracked(), (3, 1));
        // Saving names the workbook and its size.
        let (name, bytes) = controller.save().expect("saved");
        assert_eq!(name, "Book1.xlsx");
        assert!(
            controller
                .status
                .get_untracked()
                .text
                .contains(&format!("{} bytes", bytes.len()))
        );
        // Opening replaces the workbook: a new generation at the first sheet's A1.
        let generation = controller.generation.get_untracked();
        controller.open("d1.saved.xlsx", fixture()).expect("opens");
        assert_eq!(controller.generation.get_untracked(), generation + 1);
        assert_eq!(controller.active.get_untracked(), (1, 1));
        assert!(controller.sheets().iter().any(|tab| tab.name == "Calc"));
        // An undo of an edit on another sheet shows that sheet at the restored cell, whose input
        // the selection holds, so the formula bar shows it.
        let sheets = controller.sheets();
        let data = sheets.iter().find(|tab| tab.name == "Data").unwrap().id;
        let calc = sheets.iter().find(|tab| tab.name == "Calc").unwrap().id;
        controller.show_sheet(data);
        let before = controller
            .active_cell()
            .map(|cell| cells::input_text(cell.input()));
        assert!(before.is_some(), "Data!A1 holds an input");
        controller.begin_edit(EditOrigin::Cell, Some("5".to_owned()));
        assert!(controller.commit_edit());
        controller.show_sheet(calc);
        controller.undo();
        assert_eq!(controller.sheet.get_untracked(), Some(data));
        assert_eq!(controller.active.get_untracked(), (1, 1));
        assert_eq!(
            controller
                .active_cell()
                .map(|cell| cells::input_text(cell.input())),
            before
        );
        // A package that is not one is refused, and the workbook stays.
        assert!(
            controller
                .open("junk.xlsx", b"not a package".to_vec())
                .is_err()
        );
        assert!(controller.status.get_untracked().refused);
        assert!(controller.sheets().iter().any(|tab| tab.name == "Calc"));
    });
}

/// Native-only: it writes the saved workbook to the folder `DNACALC_RIF_SAVE_DIR` names, for Excel
/// to open (calc-host's `tests/fixtures/d2/measure-saved-open.ps1`); without the variable it only
/// saves.
#[test]
fn dc_rif__a_new_workbook_saves_for_excel() {
    let mut book = Workbook::new_blank();
    let sheet1 = sheet(&book, "Sheet1");
    for (address, text) in [
        ("A1", "2"),
        ("A2", "3"),
        ("A3", "=A1*A2+1"),
        ("B1", "rate"),
        ("C1", "={1;2;3}"),
        ("D1", "TRUE"),
    ] {
        assert!(
            book.enter(at(sheet1, address), text).accepted(),
            "{address}"
        );
    }
    let (bytes, _) = book.save().expect("saved");
    if let Ok(dir) = std::env::var("DNACALC_RIF_SAVE_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join("dc_rif_book1.xlsx"), &bytes).unwrap();
    }
}

/// Native-only: it writes the saved workbooks to the folder `DNACALC_RIF_SAVE_DIR` names, for
/// Excel to open (calc-host's `tests/fixtures/d2/measure-saved-open.ps1`): a new workbook edited
/// by a fill down and a copy and paste, and calc-host's `dc_09v/formats.xlsx` with formatted
/// cells edited, whose formats a save keeps (dc-09v). Without the variable it only saves.
#[test]
fn dc_09v__edited_workbooks_save_for_excel() {
    let mut book = Workbook::new_blank();
    let sheet1 = sheet(&book, "Sheet1");
    for (address, text) in [("A1", "1"), ("A2", "2"), ("A3", "3"), ("B1", "=A1*10")] {
        assert!(
            book.enter(at(sheet1, address), text).accepted(),
            "{address}"
        );
    }
    let area = |s: SheetId, a: &str, b: &str| GridArea::new(at(s, a), at(s, b)).unwrap();
    let fill = book.dispatch(SkinIntent::Copy {
        from: area(sheet1, "B1", "B1"),
        to: area(sheet1, "B2", "B3"),
    });
    assert!(fill.accepted(), "{:?}", fill.error);
    let paste = book.dispatch(SkinIntent::Copy {
        from: area(sheet1, "A1", "B3"),
        to: area(sheet1, "D5", "D5"),
    });
    assert!(paste.accepted(), "{:?}", paste.error);
    assert_eq!(shown(&book, at(sheet1, "E7")).text, "30");
    let (copied, _) = book.save().expect("saved");

    let mut formats = Workbook::open(
        "formats.xlsx",
        include_bytes!("../../../crates/calc-host/tests/fixtures/dc_09v/formats.xlsx").to_vec(),
    )
    .expect("it opens");
    let sheet = sheet(&formats, "Formats");
    assert!(formats.enter(at(sheet, "A1"), "9876.5").accepted());
    assert!(formats.enter(at(sheet, "A2"), "0.125").accepted());
    assert_eq!(shown(&formats, at(sheet, "A1")).text, "9,876.50");
    assert_eq!(shown(&formats, at(sheet, "A2")).text, "13%");
    let (edited, _) = formats.save().expect("saved");

    if let Ok(dir) = std::env::var("DNACALC_RIF_SAVE_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        let dir = std::path::Path::new(&dir);
        std::fs::write(dir.join("dc_09v_copied.xlsx"), &copied).unwrap();
        std::fs::write(dir.join("dc_09v_formats_edited.xlsx"), &edited).unwrap();
    }
}

/// Native-only: it reads the workbook the environment variable `DNACALC_OPEN_TIMING` names from
/// the file system and prints how long DNA Calc's open took, read and calculated apart (dc-09v
/// compares it with the browser's); without the variable it does nothing.
#[test]
fn dc_09v__open_timing() {
    let Ok(path) = std::env::var("DNACALC_OPEN_TIMING") else {
        return;
    };
    let bytes = std::fs::read(&path).expect("the workbook");
    let started = dna_calc::controller::now_ms();
    let book = Workbook::open("timed.xlsx", bytes).expect("it opens");
    let total = dna_calc::controller::now_ms() - started;
    let calculated = book.calculated_on_open().unwrap_or(0.0);
    println!(
        "DC09V native open: {total:.0} ms: read {:.0} ms, calculated {calculated:.0} ms",
        total - calculated
    );
}

/// dc-09v (D4b): cells show their number formats through calc-func's one renderer. The expected
/// texts are those Excel 16.0.20430 showed for calc-host's `tests/fixtures/dc_09v/formats.xlsx`
/// (`excel-shown.json`, Range.Text), except A8, whose AM/PM designator Excel took from this
/// machine's locale (`pm`); DNA Calc shows numbers under en-US, whose designator is `PM`.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_09v__cells_show_their_number_formats() {
    let book = Workbook::open(
        "formats.xlsx",
        include_bytes!("../../../crates/calc-host/tests/fixtures/dc_09v/formats.xlsx").to_vec(),
    )
    .expect("it opens");
    let formats = sheet(&book, "Formats");
    for (address, text) in [
        ("A1", "1,234.50"),
        ("A2", "25%"),
        ("A3", "2024-01-31"),
        ("A4", "(5.00)"),
        ("A5", "text"),
        ("A6", "1234,567"),
        ("A7", "2469.0"),
        ("A8", "12:00 PM"),
        ("A9", "3.142"),
        ("A10", "1.50E+06"),
        ("A11", "Qty 12"),
        ("A12", "0.333333333"),
    ] {
        let cell = shown(&book, at(formats, address));
        assert_eq!(cell.text, text, "{address}");
        let expected = if address == "A12" {
            cells::FormatShown::General
        } else {
            cells::FormatShown::Applied
        };
        assert_eq!(cell.format, expected, "{address}");
    }
}
