//! A workbook with a chart sheet and a dialog sheet opens, calculates its worksheets and saves.
//!
//! `tests/fixtures/preserved_sheets/chart-and-dialog-sheets.xlsx` is our own small package
//! (`make-fixture.py`): the worksheet Data, the chart sheet Chart1 (a column chart over
//! Data!A1:B1), the dialog sheet Dialog1 and the worksheet After. Both kinds open as empty
//! sheets of the model, so sheet positions, `SHEETS()` and sheet-scoped names count them; a save
//! that leaves them alone keeps their parts byte for byte, and one that would change or remove
//! one is refused.

#![allow(non_snake_case)]

mod cdoc_m1_1_support;

use calc_doc::model::value::{CellKey, ExcelText, IdAllocator, SheetId};
use calc_doc::model::{
    CellInput, FormulaRewriter, FormulaText, ModelRoot, NameScope, OpBatch, SheetEdit, SheetName,
    StructuralChange, WorkbookOp,
};
use calc_doc::{SavePolicy, SaveRefusal, prepare_save, prepare_sheet_edit};
use std::sync::Arc;
use cdoc_m1_1_support::d1::{at, edited, load, number, part, save_model, save_untouched, sheet};

const XLSX: &[u8] = include_bytes!("fixtures/preserved_sheets/chart-and-dialog-sheets.xlsx");
const CHART_PART: &str = "xl/chartsheets/sheet1.xml";
const DIALOG_PART: &str = "xl/dialogsheets/sheet1.xml";

/// Leaves every formula as it is: these edits change no reference the test reads.
struct Unchanged;

impl FormulaRewriter for Unchanged {
    fn relocate(&self, _: &ModelRoot, f: &FormulaText, _: CellKey, _: CellKey) -> Option<FormulaText> {
        Some(f.clone())
    }
    fn formula(&self, _: &ModelRoot, _: &FormulaText, _: CellKey, _: &StructuralChange) -> Option<FormulaText> {
        None
    }
    fn name(&self, _: &ModelRoot, _: &str, _: NameScope, _: &StructuralChange) -> Option<String> {
        None
    }
}

/// The revision a sheet edit makes of the loaded one.
fn sheet_edited(loaded: &calc_doc::LoadedWorkbook, edit: SheetEdit) -> Arc<ModelRoot> {
    let model = loaded.model();
    let batch = OpBatch {
        base: model.revision(),
        operations: vec![WorkbookOp::EditSheet(edit)],
    };
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    prepare_sheet_edit(model, &batch, &Unchanged, &mut || ids.issue().expect("an ID"))
        .expect("the sheet edit is admitted")
        .into_model()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn preserved_sheets__a_workbook_with_chart_and_dialog_sheets_opens() {
    let loaded = load(XLSX);
    let model = loaded.model();
    let names: Vec<String> = model
        .sheets()
        .iter()
        .map(|entry| String::from_utf16_lossy(entry.name().text().utf16_code_units()))
        .collect();
    assert_eq!(names, ["Data", "Chart1", "Dialog1", "After"]);
    let kinds: Vec<_> = loaded.sheets().iter().map(|s| s.preserved_kind()).collect();
    assert_eq!(
        kinds,
        [None, Some("chart sheet"), Some("dialog sheet"), None]
    );
    // The worksheets load as ever.
    let (data, after) = (sheet(&loaded, "Data"), sheet(&loaded, "After"));
    assert_eq!(model.cell(at(data, "A1")), Some(&number(2.0)));
    assert!(matches!(model.cell(at(data, "B2")), Some(CellInput::Formula(_))));
    assert!(matches!(model.cell(at(after, "B1")), Some(CellInput::Formula(_))));
    // The sheet-scoped name on After keeps the position it has with the two sheets before it.
    let rate = model
        .definitions()
        .names()
        .iter()
        .find(|name| name.name == "Rate")
        .expect("the name Rate");
    assert_eq!(rate.scope, NameScope::Sheet(after));
    // The chart sheet and the dialog sheet hold no cell.
    for name in ["Chart1", "Dialog1"] {
        let id = sheet(&loaded, name);
        assert_eq!(model.sheet_cells(id).into_iter().flatten().count(), 0, "{name}");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn preserved_sheets__a_save_keeps_both_parts_byte_for_byte() {
    assert_eq!(save_untouched(XLSX), XLSX);
    let loaded = load(XLSX);
    let after = sheet(&loaded, "After");
    let model = edited(
        &loaded,
        vec![WorkbookOp::SetCell {
            at: at(after, "A1"),
            input: number(9.0),
        }],
    );
    let (saved, _) = save_model(&loaded, &model);
    for name in [
        CHART_PART,
        DIALOG_PART,
        "xl/drawings/drawing1.xml",
        "xl/charts/chart1.xml",
        "xl/chartsheets/_rels/sheet1.xml.rels",
    ] {
        assert_eq!(part(&saved, name), part(XLSX, name), "{name}");
    }
    let again = load(&saved);
    assert_eq!(again.model().sheets().len(), 4);
    assert_eq!(
        again.model().cell(at(sheet(&again, "After"), "A1")),
        Some(&number(9.0))
    );
    assert!(again.sheets()[1].preserved_kind().is_some());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn preserved_sheets__a_save_refuses_to_change_or_remove_them() {
    let loaded = load(XLSX);
    let refused = |model: Arc<ModelRoot>, expected: (&str, &str)| {
        match prepare_save(&loaded.snapshot(&model), &SavePolicy::default()) {
            Err(SaveRefusal::PreservedSheetChanged { part, kind }) => {
                assert_eq!((part.as_str(), kind), expected)
            }
            other => panic!("not refused: {:?}", other.err()),
        }
    };
    let (chart, dialog) = (sheet(&loaded, "Chart1"), sheet(&loaded, "Dialog1"));
    refused(
        edited(
            &loaded,
            vec![WorkbookOp::SetCell {
                at: at(chart, "B1"),
                input: number(5.0),
            }],
        ),
        (CHART_PART, "chart sheet"),
    );
    refused(
        edited(
            &loaded,
            vec![WorkbookOp::SetCell {
                at: at(dialog, "A1"),
                input: number(5.0),
            }],
        ),
        (DIALOG_PART, "dialog sheet"),
    );
    refused(
        sheet_edited(&loaded, SheetEdit::Remove { sheet: chart }),
        (CHART_PART, "chart sheet"),
    );
    // Renaming a sheet leaves their parts alone and is saved.
    let after = sheet(&loaded, "After");
    let name = SheetName::new(ExcelText::from_utf16_code_units(
        "Later".encode_utf16().collect(),
    ))
    .expect("a name");
    let model = sheet_edited(&loaded, SheetEdit::Rename { sheet: after, name });
    let (saved, _) = save_model(&loaded, &model);
    assert_eq!(part(&saved, CHART_PART), part(XLSX, CHART_PART));
    assert_eq!(load(&saved).model().sheets().len(), 4);
}
