//! The reader does what Excel does with three defects of real files (bead: Reader tolerance
//! measured against Excel). The authored workbooks of `tests/fixtures/reader_tolerance`
//! (`make-fixtures.py`) each have a control that differs from them in the defect alone. Excel's
//! answers, measured in our own session on the private desktop and recorded in
//! `tests/fixtures/reader_tolerance/excel-answers.md`:
//!
//! * `r:id=""` on a sheet (Excel writes it for a VBA module): opens silently; the sheet is empty and
//!   keeps its state.
//! * an `r:id` that names no relationship: the repair prompt; the sheet is kept, empty.
//! * two sheets with one `sheetId`: the repair prompt; both sheets are kept, the later one gets
//!   a new id.
//! * `<f ca="1"/>` inside a multi-cell array's area (Excel writes it): opens silently, the cell is a
//!   member of the array.
//! * a formula with text inside a multi-cell array's area: the repair prompt; the formula is
//!   dropped and the cell shows the array's result.

#![allow(non_snake_case)]

mod cdoc_m1_1_support;

use calc_doc::model::value::{IdAllocator, SheetId};
use calc_doc::model::{SheetVisibility, WorkbookOp};
use calc_doc::{LoadError, LoadRepair, LoadedWorkbook, SavePolicy, SaveRefusal, prepare_save};
use cdoc_m1_1_support::d1::{at, edited, formula_text, load, number, open, part, save_model, save_untouched, sheet};

const DANGLING_CONTROL: &[u8] = include_bytes!("fixtures/reader_tolerance/dangling-rid-control.xlsx");
const DANGLING: &[u8] = include_bytes!("fixtures/reader_tolerance/dangling-rid.xlsx");
const EMPTY_CONTROL: &[u8] = include_bytes!("fixtures/reader_tolerance/empty-rid-control.xlsx");
const EMPTY_VERYHIDDEN: &[u8] = include_bytes!("fixtures/reader_tolerance/empty-rid-veryhidden.xlsx");
const EMPTY_VISIBLE: &[u8] = include_bytes!("fixtures/reader_tolerance/empty-rid-visible.xlsx");
const ARRAY_CONTROL: &[u8] = include_bytes!("fixtures/reader_tolerance/array-inner-formula-control.xlsx");
const ARRAY_EMPTY_F: &[u8] = include_bytes!("fixtures/reader_tolerance/array-inner-empty-f.xlsx");
const ARRAY_FORMULA: &[u8] = include_bytes!("fixtures/reader_tolerance/array-inner-formula.xlsx");
const ARRAY_STALE: &[u8] = include_bytes!("fixtures/reader_tolerance/array-inner-formula-stale.xlsx");
const DUPLICATE: &[u8] = include_bytes!("fixtures/reader_tolerance/duplicate-sheetid.xlsx");
const DUPLICATE_CONTROL: &[u8] = include_bytes!("fixtures/reader_tolerance/duplicate-sheetid-control.xlsx");
const DUPLICATE_GAP: &[u8] = include_bytes!("fixtures/reader_tolerance/duplicate-sheetid-gap.xlsx");
const DUPLICATE_LOWER: &[u8] = include_bytes!("fixtures/reader_tolerance/duplicate-sheetid-lower.xlsx");
const DUPLICATE_TRIPLE: &[u8] = include_bytes!("fixtures/reader_tolerance/duplicate-sheetid-triple.xlsx");
const DUPLICATE_TWO_PAIRS: &[u8] = include_bytes!("fixtures/reader_tolerance/duplicate-sheetid-two-pairs.xlsx");

fn try_load(bytes: &[u8]) -> Result<LoadedWorkbook, LoadError> {
    let document = open(bytes);
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    calc_doc::load(&document, &mut ids)
}

fn names(loaded: &LoadedWorkbook) -> Vec<String> {
    loaded
        .model()
        .sheets()
        .iter()
        .map(|entry| String::from_utf16_lossy(entry.name().text().utf16_code_units()))
        .collect()
}

fn cells(loaded: &LoadedWorkbook, name: &str) -> usize {
    let id = sheet(loaded, name);
    loaded.model().sheet_cells(id).into_iter().flatten().count()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn reader_tolerance__the_controls_open_without_a_repair() {
    for bytes in [DANGLING_CONTROL, EMPTY_CONTROL, ARRAY_CONTROL, DUPLICATE_CONTROL] {
        assert!(load(bytes).repairs().is_empty());
    }
    assert_eq!(cells(&load(DANGLING_CONTROL), "Two"), 2);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn reader_tolerance__an_empty_rid_is_an_empty_sheet_that_keeps_its_state() {
    for (bytes, visibility) in [
        (EMPTY_VERYHIDDEN, SheetVisibility::VeryHidden),
        (EMPTY_VISIBLE, SheetVisibility::Visible),
    ] {
        let loaded = load(bytes);
        // Excel opens it without a prompt, so nothing is repaired.
        assert!(loaded.repairs().is_empty());
        assert_eq!(names(&loaded), ["One", "Two"]);
        assert_eq!(loaded.model().sheets()[1].visibility(), visibility);
        assert_eq!(loaded.sheets()[1].preserved_kind(), Some("sheet without a part"));
        assert_eq!(cells(&loaded, "One"), 2);
        assert_eq!(cells(&loaded, "Two"), 0);
        assert_eq!(
            loaded.model().cell(at(sheet(&loaded, "One"), "A1")),
            Some(&number(2.0))
        );
        // A save that leaves the workbook alone writes the package as it was.
        assert_eq!(save_untouched(bytes), bytes);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn reader_tolerance__an_empty_rid_sheet_is_not_edited_but_the_others_are() {
    let loaded = load(EMPTY_VERYHIDDEN);
    let (one, two) = (sheet(&loaded, "One"), sheet(&loaded, "Two"));
    let model = edited(
        &loaded,
        vec![WorkbookOp::SetCell {
            at: at(one, "A1"),
            input: number(9.0),
        }],
    );
    let (saved, _) = save_model(&loaded, &model);
    let again = load(&saved);
    assert_eq!(again.model().cell(at(sheet(&again, "One"), "A1")), Some(&number(9.0)));
    assert_eq!(names(&again), ["One", "Two"]);
    assert_eq!(again.model().sheets()[1].visibility(), SheetVisibility::VeryHidden);

    let model = edited(
        &loaded,
        vec![WorkbookOp::SetCell {
            at: at(two, "A1"),
            input: number(1.0),
        }],
    );
    match prepare_save(&loaded.snapshot(&model), &SavePolicy::default()) {
        Err(SaveRefusal::PreservedSheetChanged { kind, .. }) => assert_eq!(kind, "sheet without a part"),
        other => panic!("not refused: {:?}", other.err()),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn reader_tolerance__a_dangling_rid_is_repaired_to_an_empty_sheet() {
    let loaded = load(DANGLING);
    assert_eq!(names(&loaded), ["One", "Two"]);
    assert_eq!(
        loaded.repairs(),
        [LoadRepair::SheetRelationshipMissing {
            sheet: sheet(&loaded, "Two"),
            id: "rId9".to_owned(),
        }]
    );
    assert_eq!(cells(&loaded, "One"), 2);
    assert_eq!(cells(&loaded, "Two"), 0);
    assert_eq!(loaded.model().sheets()[1].visibility(), SheetVisibility::Visible);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn reader_tolerance__an_empty_f_in_an_array_area_is_a_member_and_no_repair() {
    let (control, loaded) = (load(ARRAY_CONTROL), load(ARRAY_EMPTY_F));
    assert!(loaded.repairs().is_empty());
    let (c, l) = (sheet(&control, "Arr"), sheet(&loaded, "Arr"));
    for address in ["A1", "A2", "A3", "B1", "B2", "B3"] {
        assert_eq!(
            control.model().cell(at(c, address)),
            loaded.model().cell(at(l, address)),
            "{address}"
        );
    }
    assert_eq!(
        formula_text(loaded.model().cell(at(l, "A1"))).as_deref(),
        formula_text(control.model().cell(at(c, "A1"))).as_deref()
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn reader_tolerance__a_formula_in_an_array_area_is_dropped_as_excel_repairs_it() {
    let (control, loaded) = (load(ARRAY_CONTROL), load(ARRAY_FORMULA));
    let (c, l) = (sheet(&control, "Arr"), sheet(&loaded, "Arr"));
    assert_eq!(
        loaded.repairs(),
        [LoadRepair::FormulaInsideArray {
            sheet: l,
            row: 2,
            column: 1,
        }]
    );
    // The cell reads as the control's does: a member of the array, with no formula of its own.
    for address in ["A1", "A2", "A3", "B1", "B2", "B3"] {
        assert_eq!(
            control.model().cell(at(c, address)),
            loaded.model().cell(at(l, address)),
            "{address}"
        );
    }
    assert!(formula_text(loaded.model().cell(at(l, "A2"))).is_none());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn reader_tolerance__two_sheets_with_one_sheet_id_are_repaired_with_a_new_id() {
    let loaded = load(DUPLICATE);
    assert_eq!(names(&loaded), ["One", "Two"]);
    let keys: Vec<u32> = loaded.model().sheets().iter().map(|s| s.key().get()).collect();
    assert_eq!(keys, [1, 2]);
    assert_eq!(cells(&loaded, "Two"), 2);
    assert_eq!(
        loaded.repairs(),
        [LoadRepair::DuplicateSheetId {
            sheet: sheet(&loaded, "Two"),
            id: 1,
            now: 2,
        }]
    );
}

/// The sheet ids and repairs a workbook of several repeated ids loads with. Excel's own save of
/// the repaired workbooks (excel-answers) wrote exactly these ids.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn reader_tolerance__repeated_sheet_ids_get_the_ids_excel_gives_them() {
    // (workbook, the ids Excel saved the sheets with, the sheets whose id was repaired)
    let cases: [(&[u8], &[u32], &[&str]); 4] = [
        (DUPLICATE_GAP, &[1, 109, 110], &["Three"]),
        (DUPLICATE_LOWER, &[1, 6, 5], &["Two"]),
        (DUPLICATE_TRIPLE, &[1, 2, 3], &["Two", "Three"]),
        (DUPLICATE_TWO_PAIRS, &[2, 4, 3, 5], &["Two", "Four"]),
    ];
    for (bytes, keys, repaired) in cases {
        let loaded = load(bytes);
        let got: Vec<u32> = loaded.model().sheets().iter().map(|s| s.key().get()).collect();
        assert_eq!(got, keys);
        let repaired: Vec<SheetId> = repaired.iter().map(|name| sheet(&loaded, name)).collect();
        let reported: Vec<SheetId> = loaded
            .repairs()
            .iter()
            .map(|repair| match repair {
                LoadRepair::DuplicateSheetId { sheet, .. } => *sheet,
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(reported, repaired);
        // Every sheet keeps its cells.
        for entry in loaded.model().sheets() {
            let name = String::from_utf16_lossy(entry.name().text().utf16_code_units());
            assert!(cells(&loaded, &name) >= 2, "{name}");
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn reader_tolerance__a_repaired_workbook_saves_untouched_and_takes_an_edit() {
    // The untouched save copies the package as it was, repairs unwritten. What Excel's ordinary
    // Save writes for such a workbook is experiment E7(a) of pro-10 answer 16, not yet run; this
    // assertion changes once it is (dc-1nbp.13).
    for bytes in [DUPLICATE, DANGLING, ARRAY_FORMULA] {
        assert_eq!(save_untouched(bytes), bytes);
    }
    let loaded = load(DUPLICATE);
    let model = edited(
        &loaded,
        vec![WorkbookOp::SetCell {
            at: at(sheet(&loaded, "Two"), "A1"),
            input: number(7.0),
        }],
    );
    let (saved, _) = save_model(&loaded, &model);
    let again = load(&saved);
    assert_eq!(again.model().cell(at(sheet(&again, "Two"), "A1")), Some(&number(7.0)));
    assert_eq!(names(&again), ["One", "Two"]);
}

/// The `<sheet>` tags of the workbook part of `package`, as Excel's saved copy of each fixture
/// records them (`excel-answers/sheets-of-the-workbooks-excel-saved-after-opening.json`).
fn sheet_tags(package: &[u8]) -> Vec<String> {
    let workbook = String::from_utf8(part(package, "xl/workbook.xml").expect("workbook part")).unwrap();
    workbook
        .split("<sheet ")
        .skip(1)
        .map(|tag| tag.split_once("/>").expect("a sheet tag").0)
        .map(|tag| {
            let attribute = |name: &str| {
                let at = tag.find(&format!("{name}=\"")).unwrap_or_else(|| panic!("{name} in {tag}"));
                let rest = &tag[at + name.len() + 2..];
                rest[..rest.find('"').unwrap()].to_owned()
            };
            format!("{}:{}", attribute("name"), attribute("sheetId"))
        })
        .collect()
}

/// dc-1nbp.13: an edited save writes Excel's repairs. The saved package loads with no repair
/// and the same model, and its sheets carry the ids Excel's own save gave them.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn reader_tolerance__an_edited_save_writes_excels_repairs() {
    let answers: serde_json::Value = serde_json::from_slice(include_bytes!(
        "fixtures/reader_tolerance/excel-answers/sheets-of-the-workbooks-excel-saved-after-opening.json"
    ))
    .unwrap();
    let cases: [(&str, &[u8], &str); 8] = [
        ("duplicate-sheetid.xlsx", DUPLICATE, "A1"),
        ("duplicate-sheetid-gap.xlsx", DUPLICATE_GAP, "A1"),
        ("duplicate-sheetid-lower.xlsx", DUPLICATE_LOWER, "A1"),
        ("duplicate-sheetid-triple.xlsx", DUPLICATE_TRIPLE, "A1"),
        ("duplicate-sheetid-two-pairs.xlsx", DUPLICATE_TWO_PAIRS, "A1"),
        ("dangling-rid.xlsx", DANGLING, "A1"),
        ("array-inner-formula.xlsx", ARRAY_FORMULA, "B1"),
        ("array-inner-formula-stale.xlsx", ARRAY_STALE, "B1"),
    ];
    for (file, bytes, cell) in cases {
        let loaded = load(bytes);
        assert!(!loaded.repairs().is_empty(), "{file}");
        let first = loaded.model().sheets()[0].id();
        let model = edited(
            &loaded,
            vec![WorkbookOp::SetCell {
                at: at(first, cell),
                input: number(41.0),
            }],
        );
        let (saved, _) = save_model(&loaded, &model);
        let again = load(&saved);
        assert!(again.repairs().is_empty(), "{file}: {:?}", again.repairs());
        assert_eq!(names(&again), names(&loaded), "{file}");
        for entry in loaded.model().sheets() {
            let name = String::from_utf16_lossy(entry.name().text().utf16_code_units());
            assert_eq!(cells(&again, &name), cells(&loaded, &name), "{file} {name}");
        }
        let expected: Vec<String> = answers[file]
            .as_array()
            .unwrap()
            .iter()
            .map(|tag| {
                let tag = tag.as_str().unwrap();
                let name = tag.split("name=\"").nth(1).unwrap().split('"').next().unwrap();
                let id = tag.split("sheetId=\"").nth(1).unwrap().split('"').next().unwrap();
                format!("{name}:{id}")
            })
            .collect();
        assert_eq!(sheet_tags(&saved), expected, "{file}");
        // A sheet whose `r:id` named nothing is written with a part of its own.
        if file == "dangling-rid.xlsx" {
            assert_eq!(again.sheets()[1].preserved_kind(), None);
            assert!(part(&saved, again.sheets()[1].part().name()).is_some());
        }
    }
}
