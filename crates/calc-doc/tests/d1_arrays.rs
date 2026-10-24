//! dc-ndi (D1): dynamic-array and legacy array formulas open and save.
//!
//! The fixture `tests/fixtures/cdoc_m1_2/d1.saved.xlsx` is the package Excel 16.0.20430 saved,
//! through `cargo xtask capture`'s adapter, from `d1.xlsx`, which the D1 section of
//! `cdoc_m1_2/make-fixtures.ps1` writes: dynamic arrays that spill (`Calc!A1:B3`, `D1:D5`, `F1:F3`), one
//! blocked by a value (`H1`, #SPILL!) and one with an empty result (`J1`, #CALC!), legacy array
//! formulas (`L1:L3`, `N1`), and a name reference Excel stored as a dynamic array (`W1`).

#![cfg(not(target_arch = "wasm32"))]

mod cdoc_m1_1_support;

use calc_doc::model::value::{GridArea, WorksheetErrorCode};
use calc_doc::model::{ArrayKind, CachedValue, CellInput, OpBatch, WorkbookOp};
use calc_doc::{EditError, prepare_edit};
use cdoc_m1_1_support::d1::*;

const FIXTURE: &str = "../cdoc_m1_2/d1.saved.xlsx";

fn area(sheet: calc_doc::model::value::SheetId, a: &str, b: &str) -> GridArea {
    GridArea::new(at(sheet, a), at(sheet, b)).unwrap()
}

fn cached_number(value: &CachedValue) -> f64 {
    match value {
        CachedValue::Number(number) => number.get(),
        other => panic!("a number, not {other:?}"),
    }
}

#[test]
fn d1_arrays__excel_arrays_load_with_anchor_area_and_kind() {
    let loaded = load(&fixture(FIXTURE));
    let calc = sheet(&loaded, "Calc");
    let model = loaded.model();
    let dynamic = ArrayKind::Dynamic { collapsed: false };
    let mut arrays: Vec<(GridArea, ArrayKind)> = model
        .definitions()
        .arrays()
        .map(|array| (array.area(), array.kind()))
        .collect();
    arrays.sort_by_key(|(area, _)| (area.first().row(), area.first().column()));
    assert_eq!(
        arrays,
        vec![
            (area(calc, "A1", "B3"), dynamic),
            (area(calc, "D1", "D5"), dynamic),
            (area(calc, "F1", "F3"), dynamic),
            (area(calc, "H1", "H1"), dynamic),
            (area(calc, "J1", "J1"), dynamic),
            (area(calc, "L1", "L3"), ArrayKind::Legacy),
            (area(calc, "N1", "N1"), ArrayKind::Legacy),
            (area(calc, "W1", "W1"), dynamic),
        ]
    );
    // The anchor holds the formula text as the file wrote it; the cells it fills hold no input.
    assert_eq!(
        formula_text(model.cell(at(calc, "A1"))).as_deref(),
        Some("_xlfn.SEQUENCE(3,2)")
    );
    assert_eq!(
        formula_text(model.cell(at(calc, "L1"))).as_deref(),
        Some("Data!A1:A3*10")
    );
    for child in ["B1", "A2", "B2", "A3", "B3", "D5", "F3", "L2", "L3"] {
        assert_eq!(model.cell(at(calc, child)), None, "{child} holds no input");
        let array = model
            .array_containing(at(calc, child))
            .expect("in an array");
        assert!(
            model.definitions().not_available(at(calc, child), false),
            "{child} reads as not available, anchored at {:?}",
            array.anchor()
        );
    }
    // A blocked spill's cell (H2) is an input of its own.
    assert!(matches!(
        model.cell(at(calc, "H2")),
        Some(CellInput::Text(_))
    ));
    // The cells' cached values are file caches beside the model.
    let records = loaded.sheet(calc).unwrap();
    let child = |a: &str| {
        let key = at(calc, a);
        cached_number(
            records
                .value_cache(key.row(), key.column())
                .unwrap()
                .value(),
        )
    };
    assert_eq!(
        ["B1", "A2", "B2", "A3", "B3"].map(child),
        [2.0, 3.0, 4.0, 5.0, 6.0]
    );
    assert_eq!(["D2", "D3", "D4", "D5"].map(child), [4.0, 3.0, 2.0, 1.0]);
    assert_eq!(["L2", "L3"].map(child), [20.0, 30.0]);
    let anchor_cache = |a: &str| {
        let key = at(calc, a);
        records
            .formula(key.row(), key.column())
            .and_then(|record| record.cache())
            .map(|cache| cache.value().clone())
    };
    assert_eq!(cached_number(&anchor_cache("A1").unwrap()), 1.0);
    // vm resolves the rich errors behind Excel's #VALUE! fallback.
    assert_eq!(
        anchor_cache("H1"),
        Some(CachedValue::Error(WorksheetErrorCode::Spill))
    );
    assert_eq!(
        anchor_cache("J1"),
        Some(CachedValue::Error(WorksheetErrorCode::Calc))
    );
    assert_eq!(cached_number(&anchor_cache("N1").unwrap()), 55.0);
}

#[test]
fn d1_arrays__untouched_save_is_byte_for_byte() {
    let bytes = fixture(FIXTURE);
    let saved = save_untouched(&bytes);
    assert_eq!(saved, bytes, "every entry copied as stored");
    keep_for_excel("d1_untouched.xlsx", &saved);
    keep_for_excel(
        "d1_repair_control.xlsx",
        &repair_control(&saved, "xl/worksheets/sheet1.xml"),
    );
    // A loaded workbook saved at its own loaded revision writes nothing anew either.
    let loaded = load(&bytes);
    let (same, receipt) = save_model(&loaded, loaded.model());
    assert_eq!(same, bytes);
    assert_eq!(receipt.parts_written, 0);
}

#[test]
fn d1_arrays__edited_save_keeps_arrays_and_metadata() {
    let bytes = fixture(FIXTURE);
    let loaded = load(&bytes);
    let calc = sheet(&loaded, "Calc");
    let model = edited(
        &loaded,
        vec![WorkbookOp::SetCell {
            at: at(calc, "U2"),
            input: number(8.0),
        }],
    );
    let (saved, receipt) = save_model(&loaded, &model);
    keep_for_excel("d1_edited_value.xlsx", &saved);
    // The metadata and rich value parts are copied as stored.
    for name in [
        "xl/metadata.xml",
        "xl/richData/rdrichvalue.xml",
        "xl/richData/rdrichvaluestructure.xml",
        "xl/richData/rdRichValueTypes.xml",
        "xl/styles.xml",
        "xl/theme/theme1.xml",
        "xl/tables/table1.xml",
    ] {
        assert_eq!(part(&saved, name), part(&bytes, name), "{name}");
    }
    let sheet_xml = part_text(&saved, "xl/worksheets/sheet2.xml");
    // Anchors keep cm and their array formula; vm and the cached value go with the cache.
    assert!(
        sheet_xml
            .contains(r#"<c r="A1" cm="1"><f t="array" ref="A1:B3">_xlfn.SEQUENCE(3,2)</f></c>"#),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml.contains(
            r#"<c r="H1" cm="1"><f t="array" aca="1" ref="H1" ca="1">_xlfn.SEQUENCE(2)</f></c>"#
        ),
        "{sheet_xml}"
    );
    assert!(sheet_xml.contains(r#"<c r="L1"><f t="array" ref="L1:L3">Data!A1:A3*10</f></c>"#));
    // The cells the arrays fill are gone with their cached values; H2's own text stays.
    for gone in [r#"r="B1""#, r#"r="A2""#, r#"r="D5""#, r#"r="L3""#] {
        assert!(!sheet_xml.contains(gone), "{gone} in {sheet_xml}");
    }
    assert!(sheet_xml.contains(r#"<c r="H2" t="s"><v>14</v></c>"#));
    // 16 formula caches on Calc and the table's totals on Data, plus the 13 cells the arrays
    // fill beside their anchors.
    assert_eq!(receipt.caches_dropped, 16 + 1 + 13, "{receipt:?}");
    // The reloaded workbook holds the same arrays and the edited value.
    let reloaded = load(&saved);
    let again: Vec<_> = reloaded.model().definitions().arrays().copied().collect();
    let before: Vec<_> = loaded.model().definitions().arrays().copied().collect();
    assert_eq!(again.len(), before.len());
    for (a, b) in again.iter().zip(&before) {
        assert_eq!(
            (
                a.area().first().row(),
                a.area().first().column(),
                a.area().last().row(),
                a.area().last().column(),
                a.kind()
            ),
            (
                b.area().first().row(),
                b.area().first().column(),
                b.area().last().row(),
                b.area().last().column(),
                b.kind()
            )
        );
    }
    let calc_again = sheet(&reloaded, "Calc");
    assert_eq!(
        reloaded.model().cell(at(calc_again, "U2")),
        Some(&number(8.0))
    );
    assert!(
        reloaded
            .sheet(calc_again)
            .unwrap()
            .value_caches()
            .next()
            .is_none()
    );
    // The calculation chain stays: the set of formula cells and arrays did not change.
    assert!(part(&saved, "xl/calcChain.xml").is_some());
    // Every formula is saved without a value. Excel calculates an ordinary formula without one
    // when it opens the package, but not an array formula (tests/fixtures/d1/excel-open.json), so
    // a full calculation is asked for.
    assert!(part_text(&saved, "xl/workbook.xml").contains(r#"fullCalcOnLoad="1""#));
}

#[test]
fn d1_arrays__replacing_an_anchor_removes_its_array() {
    let bytes = fixture(FIXTURE);
    let loaded = load(&bytes);
    let calc = sheet(&loaded, "Calc");
    let model = edited(
        &loaded,
        vec![
            WorkbookOp::SetCell {
                at: at(calc, "A1"),
                input: number(9.0),
            },
            WorkbookOp::Clear {
                area: area(calc, "L1", "L3"),
            },
        ],
    );
    assert!(model.definitions().array(at(calc, "A1")).is_none());
    assert!(model.definitions().array(at(calc, "L1")).is_none());
    assert_eq!(model.definitions().arrays().count(), 6);
    let (saved, _) = save_model(&loaded, &model);
    keep_for_excel("d1_edited_anchor.xlsx", &saved);
    let sheet_xml = part_text(&saved, "xl/worksheets/sheet2.xml");
    assert!(
        sheet_xml.contains(r#"<c r="A1"><v>9</v></c>"#),
        "{sheet_xml}"
    );
    for gone in [r#"r="B1""#, r#"r="A3""#, r#"r="L1""#, r#"r="L2""#] {
        assert!(!sheet_xml.contains(gone), "{gone} in {sheet_xml}");
    }
    // The chain marks array anchors, so it goes when the arrays change.
    assert!(part(&saved, "xl/calcChain.xml").is_none());
    let reloaded = load(&saved);
    let calc = sheet(&reloaded, "Calc");
    assert_eq!(reloaded.model().cell(at(calc, "A1")), Some(&number(9.0)));
    assert_eq!(reloaded.model().cell(at(calc, "B1")), None);
    assert_eq!(reloaded.model().definitions().arrays().count(), 6);
}

#[test]
fn d1_arrays__part_of_an_array_is_refused() {
    let loaded = load(&fixture(FIXTURE));
    let calc = sheet(&loaded, "Calc");
    let model = loaded.model();
    let refused = |operations: Vec<WorkbookOp>| {
        prepare_edit(
            model,
            &OpBatch {
                base: model.revision(),
                operations,
            },
        )
        .map(|_| ())
    };
    // Any cell of a legacy array set, part of a legacy array cleared.
    for (op, anchor) in [
        (
            WorkbookOp::SetCell {
                at: at(calc, "L1"),
                input: number(1.0),
            },
            "L1",
        ),
        (
            WorkbookOp::Clear {
                area: area(calc, "L2", "L3"),
            },
            "L1",
        ),
    ] {
        assert_eq!(
            refused(vec![op]),
            Err(EditError::ArrayPart {
                operation: 0,
                anchor: at(calc, anchor),
            })
        );
    }
    // Clearing cells a dynamic array fills, or a whole legacy array, replacing a dynamic array's
    // anchor or a single-cell legacy array, and a value into a dynamic array's spill area (which
    // blocks the spill: the engine shows #SPILL!, dc-35p), are admitted.
    for op in [
        WorkbookOp::SetCell {
            at: at(calc, "B2"),
            input: number(1.0),
        },
        WorkbookOp::Clear {
            area: area(calc, "B1", "B3"),
        },
        WorkbookOp::Clear {
            area: area(calc, "K1", "L3"),
        },
        WorkbookOp::SetCell {
            at: at(calc, "D1"),
            input: formula("1+1"),
        },
        WorkbookOp::SetCell {
            at: at(calc, "N1"),
            input: number(3.0),
        },
    ] {
        assert_eq!(refused(vec![op]), Ok(()));
    }
}

/// dc-35p (D2): an authored array formula. A legacy array over a free area is saved as Excel
/// saves one (`<f t="array" ref="..">` without cell metadata); one that changes part of a legacy
/// array is refused, and one that meets a dynamic array without replacing it overlaps it.
#[test]
fn d1_arrays__an_authored_array_formula_saves_and_is_checked() {
    let bytes = fixture(FIXTURE);
    let loaded = load(&bytes);
    let calc = sheet(&loaded, "Calc");
    let model = loaded.model();
    let set_array = |area: GridArea, kind: ArrayKind, text: &str| WorkbookOp::SetArray {
        array: calc_doc::model::ArrayFormula::new(area, kind),
        input: formula(text),
    };
    let authored = edited(
        &loaded,
        vec![set_array(
            area(calc, "Y1", "Y3"),
            ArrayKind::Legacy,
            "Data!A1:A3+1",
        )],
    );
    let array = authored
        .definitions()
        .array(at(calc, "Y1"))
        .expect("the authored array");
    assert_eq!(array.area(), area(calc, "Y1", "Y3"));
    let (saved, _) = save_model(&loaded, &authored);
    let sheet_xml = part_text(&saved, "xl/worksheets/sheet2.xml");
    assert!(
        sheet_xml.contains(r#"<c r="Y1"><f t="array" ref="Y1:Y3">Data!A1:A3+1</f></c>"#),
        "{sheet_xml}"
    );
    let refused = |operation: WorkbookOp| {
        prepare_edit(
            model,
            &OpBatch {
                base: model.revision(),
                operations: vec![operation],
            },
        )
        .map(|_| ())
    };
    assert_eq!(
        refused(set_array(area(calc, "L2", "L4"), ArrayKind::Legacy, "1")),
        Err(EditError::ArrayPart {
            operation: 0,
            anchor: at(calc, "L1"),
        })
    );
    assert_eq!(
        refused(set_array(
            area(calc, "B2", "B2"),
            ArrayKind::Dynamic { collapsed: false },
            "1"
        )),
        Err(EditError::Model(
            calc_doc::model::ApplyError::ArrayOverlap {
                operation: 0,
                anchor: at(calc, "A1"),
            }
        ))
    );
    assert_eq!(
        refused(WorkbookOp::SetArray {
            array: calc_doc::model::ArrayFormula::new(area(calc, "Y1", "Y1"), ArrayKind::Legacy),
            input: number(1.0),
        }),
        Err(EditError::Model(calc_doc::model::ApplyError::ArrayInput {
            operation: 0
        }))
    );
}
