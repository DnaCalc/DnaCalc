//! dc-xuw: an Excel 4.0 macro sheet is carried through open and save.
//!
//! The fixtures `tests/fixtures/dc_xuw/macro-sheet.xlsm` and `macro-sheet.xlsb` are one workbook
//! Excel 16.0.20430 saved twice (`make-fixture.ps1`): the worksheet Data and the macro sheet
//! Macro1, whose cells A3:A6 hold macro formulas (ARGUMENT, RETURN, the command ALERT) and A2 a
//! constant that Data!A3 reads. The macro sheet opens as a sheet of the model; its formula cells
//! are unloaded (not calculated) with what Excel cached for them; a save that leaves it alone
//! keeps its part's bytes, and one that would change it is refused.

mod cdoc_m1_1_support;

use calc_doc::model::{CellInput, UnloadedCell, WorkbookOp};
use calc_doc::{SavePolicy, SaveRefusal, prepare_save};
use cdoc_m1_1_support::d1::{at, edited, load, number, part, save_model, save_untouched, sheet};

const XLSM: &[u8] = include_bytes!("fixtures/dc_xuw/macro-sheet.xlsm");
const XLSB: &[u8] = include_bytes!("fixtures/dc_xuw/macro-sheet.xlsb");
const MACRO_PART: &str = "xl/macrosheets/sheet1.xml";

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_xuw__a_macro_sheet_opens_with_its_formula_cells_unloaded() {
    let loaded = load(XLSM);
    let model = loaded.model();
    assert_eq!(model.sheets().len(), 2);
    let macro_sheet = sheet(&loaded, "Macro1");
    let data = sheet(&loaded, "Data");
    let flags: Vec<bool> = loaded
        .sheets()
        .iter()
        .map(|sheet| sheet.is_macro_sheet())
        .collect();
    assert_eq!(flags, vec![false, true]);
    // The macro formulas are not read: no input, unloaded, with the value Excel cached.
    for cell in ["A3", "A4", "A5", "A6"] {
        let key = at(macro_sheet, cell);
        assert_eq!(model.cell(key), None, "{cell} holds no input");
        assert_eq!(
            model.definitions().unloaded(key),
            Some(&UnloadedCell::MacroSheet),
            "{cell}"
        );
    }
    let cached = loaded.sheets()[1]
        .value_cache(4, 1)
        .map(|cache| format!("{:?}", cache.value()));
    assert!(cached.is_some(), "A4 keeps what Excel cached for it");
    // Its constants are inputs, which the worksheet's formulas read.
    assert_eq!(model.cell(at(macro_sheet, "A2")), Some(&number(21.0)));
    assert!(matches!(
        model.cell(at(data, "A3")),
        Some(CellInput::Formula(_))
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_xuw__a_save_keeps_the_macro_sheet_and_refuses_to_change_it() {
    // Untouched, the package is saved as it was read.
    assert_eq!(save_untouched(XLSM), XLSM);
    let loaded = load(XLSM);
    let data = sheet(&loaded, "Data");
    let macro_sheet = sheet(&loaded, "Macro1");
    // An edit of the worksheet saves; the macro sheet's part keeps its bytes.
    let model = edited(
        &loaded,
        vec![WorkbookOp::SetCell {
            at: at(data, "B1"),
            input: number(5.0),
        }],
    );
    let (saved, _) = save_model(&loaded, &model);
    assert_eq!(part(&saved, MACRO_PART), part(XLSM, MACRO_PART));
    let again = load(&saved);
    assert_eq!(
        again.model().cell(at(sheet(&again, "Data"), "B1")),
        Some(&number(5.0))
    );
    // An edit of the macro sheet is refused by the save, before a byte is written.
    let model = edited(
        &loaded,
        vec![WorkbookOp::SetCell {
            at: at(macro_sheet, "B1"),
            input: number(5.0),
        }],
    );
    match prepare_save(&loaded.snapshot(&model), &SavePolicy::default()) {
        Err(SaveRefusal::MacroSheetChanged { part }) => assert_eq!(part, MACRO_PART),
        other => panic!("a macro sheet edit was not refused: {:?}", other.err()),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_xuw__a_binary_workbook_with_a_macro_sheet_opens_as_its_twin() {
    let (binary, twin) = (load(XLSB), load(XLSM));
    assert_eq!(binary.model().sheets(), twin.model().sheets());
    for entry in twin.model().sheets() {
        let cells = |loaded: &calc_doc::LoadedWorkbook| -> Vec<_> {
            loaded
                .model()
                .sheet_cells(entry.id())
                .into_iter()
                .flatten()
                .map(|(at, input)| (at, input.clone()))
                .collect()
        };
        assert_eq!(cells(&binary), cells(&twin));
    }
    let unloaded = |loaded: &calc_doc::LoadedWorkbook| -> Vec<_> {
        loaded
            .model()
            .definitions()
            .unloaded_cells()
            .map(|(at, why)| (at, why.clone()))
            .collect()
    };
    assert_eq!(unloaded(&binary), unloaded(&twin));
    assert_eq!(
        format!("{:?}", binary.model().definitions().names()),
        format!("{:?}", twin.model().definitions().names())
    );
    assert!(binary.model().definitions().macro_enabled());
    // Saved, the binary workbook is a macro-enabled SpreadsheetML package with the macro sheet.
    let saved = save_untouched(XLSB);
    let text =
        String::from_utf8(part(&saved, MACRO_PART).expect("the macro sheet part")).expect("UTF-8");
    assert!(text.contains("ALERT(\"hello\",2)</f>"), "{text}");
}
