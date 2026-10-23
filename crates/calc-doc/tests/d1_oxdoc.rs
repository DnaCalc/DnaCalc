//! dc-ndi (D1): OxDoc's conformance fixtures (OxDoc c103183, crates/oxdoc-conformance/fixtures,
//! copied under tests/fixtures/d1/oxdoc/), each zipped from its parts in memory as OxDoc's
//! conformance harness does, loaded and saved through calc_doc.
//!
//! - w010-rich-error-cell-edit: rich errors behind `vm` (#FIELD!, #CALC!, a rich #VALUE!), as
//!   OxDoc's value_metadata_edit cases resolve them;
//! - w010-styled-cell-edit: styled cells and a dynamic-array anchor (cm) through a value edit, as
//!   OxDoc's styled_cell_edit cases keep them;
//!   this hand-written fixture does not open in Excel 16.0.20430 even as it is (a normal load
//!   fails; its metadata type carries `copyCell` where Excel writes `copy`), so it shows
//!   calc-doc's behaviour, not an Excel round trip;
//! - w010-cell-add: cells added into gaps, new rows and a property-only row;
//! - c1-preservation: a defined name, a table, a style table, a calculation chain and parts no
//!   save touches (VBA, pivot, drawing, custom XML).

#![cfg(not(target_arch = "wasm32"))]

mod cdoc_m1_1_support;

use calc_doc::model::value::{GridArea, WorksheetErrorCode};
use calc_doc::model::{ArrayKind, CachedValue, CellInput, NameScope, WorkbookOp};
use cdoc_m1_1_support::d1::*;

fn package(name: &str) -> Vec<u8> {
    package_from_dir(&fixtures().join("oxdoc").join(name).join("parts"))
}

fn cache(
    loaded: &calc_doc::LoadedWorkbook,
    sheet_name: &str,
    address: &str,
) -> Option<CachedValue> {
    let key = at(sheet(loaded, sheet_name), address);
    loaded
        .sheet(key.sheet())
        .unwrap()
        .formula(key.row(), key.column())
        .and_then(|record| record.cache())
        .map(|cache| cache.value().clone())
}

#[test]
fn d1_oxdoc__rich_errors_resolve_through_value_metadata() {
    let bytes = package("w010-rich-error-cell-edit");
    let loaded = load(&bytes);
    let name =
        String::from_utf16_lossy(loaded.model().sheets()[0].name().text().utf16_code_units());
    let error = |code| Some(CachedValue::Error(code));
    // B1 FIELDVALUE: errorType 12; C1 BYCOL: 13; D1 IMAGE: a rich #VALUE! (2); E1 LAMBDA: 13.
    assert_eq!(
        cache(&loaded, &name, "B1"),
        error(WorksheetErrorCode::Field)
    );
    assert_eq!(cache(&loaded, &name, "C1"), error(WorksheetErrorCode::Calc));
    assert_eq!(
        cache(&loaded, &name, "D1"),
        error(WorksheetErrorCode::Value)
    );
    assert_eq!(cache(&loaded, &name, "E1"), error(WorksheetErrorCode::Calc));
    // F1 is a classic error constant without vm.
    let f1 = at(sheet(&loaded, &name), "F1");
    assert_eq!(
        loaded.model().cell(f1),
        Some(&CellInput::Error(WorksheetErrorCode::Div0))
    );
    assert_eq!(save_untouched(&bytes), bytes);
    // A value edit: the formula cells lose vm with their cached values; E1 keeps its style.
    let model = edited(
        &loaded,
        vec![WorkbookOp::SetCell {
            at: at(sheet(&loaded, &name), "A1"),
            input: number(4.0),
        }],
    );
    let (saved, _) = save_model(&loaded, &model);
    keep_for_excel("d1_oxdoc_rich_error_edit.xlsx", &saved);
    let xml = part_text(&saved, "xl/worksheets/sheet1.xml");
    assert!(xml.contains(r#"<c r="A1"><v>4</v></c>"#), "{xml}");
    assert!(
        xml.contains(r#"<c r="B1"><f>_xlfn.FIELDVALUE(A1,"Price")</f></c>"#),
        "{xml}"
    );
    assert!(
        xml.contains(r#"<c r="E1" s="1"><f>_xlfn.LAMBDA(1)</f></c>"#),
        "{xml}"
    );
    assert!(!xml.contains("vm="), "{xml}");
    assert_eq!(
        part(&saved, "xl/metadata.xml"),
        part(&bytes, "xl/metadata.xml")
    );
}

#[test]
fn d1_oxdoc__styled_cells_and_an_anchor_survive_a_value_edit() {
    let bytes = package("w010-styled-cell-edit");
    keep_for_excel("d1_oxdoc_styled_source.xlsx", &bytes);
    let loaded = load(&bytes);
    let name =
        String::from_utf16_lossy(loaded.model().sheets()[0].name().text().utf16_code_units());
    let id = sheet(&loaded, &name);
    let array = loaded.model().definitions().array(at(id, "E1")).copied();
    assert_eq!(
        array.map(|array| (array.area(), array.kind())),
        Some((
            GridArea::cell(at(id, "E1")),
            ArrayKind::Dynamic { collapsed: false }
        ))
    );
    let records = loaded.sheet(id).unwrap();
    assert_eq!(
        [(1, 1), (1, 2), (1, 3), (1, 4), (1, 5), (1, 7), (2, 1)].map(|(r, c)| records.style(r, c)),
        [1, 2, 1, 3, 1, 2, 1]
    );
    let model = edited(
        &loaded,
        vec![WorkbookOp::SetCell {
            at: at(id, "A1"),
            input: number(13.0),
        }],
    );
    let (saved, _) = save_model(&loaded, &model);
    keep_for_excel("d1_oxdoc_styled_edit.xlsx", &saved);
    let xml = part_text(&saved, "xl/worksheets/sheet1.xml");
    assert!(xml.contains(r#"<c r="A1" s="1"><v>13</v></c>"#), "{xml}");
    assert!(
        xml.contains(r#"<c r="E1" s="1" cm="1"><f t="array" ref="E1">_xlfn.SEQUENCE(1)</f></c>"#),
        "{xml}"
    );
    assert!(xml.contains(r#"<c r="G1" s="2"/>"#), "{xml}");
    for name in ["xl/metadata.xml", "xl/styles.xml", "xl/sharedStrings.xml"] {
        assert_eq!(part(&saved, name), part(&bytes, name), "{name}");
    }
}

#[test]
fn d1_oxdoc__cells_are_added_into_gaps_and_new_rows() {
    let bytes = package("w010-cell-add");
    let loaded = load(&bytes);
    let name =
        String::from_utf16_lossy(loaded.model().sheets()[0].name().text().utf16_code_units());
    let id = sheet(&loaded, &name);
    let model = edited(
        &loaded,
        vec![
            WorkbookOp::SetCell {
                at: at(id, "B1"),
                input: number(5.0),
            },
            WorkbookOp::SetCell {
                at: at(id, "A3"),
                input: formula("A1+B1"),
            },
            WorkbookOp::SetCell {
                at: at(id, "B4"),
                input: number(6.0),
            },
            WorkbookOp::SetCell {
                at: at(id, "C6"),
                input: formula("SEQUENCE(2)"),
            },
        ],
    );
    let (saved, _) = save_model(&loaded, &model);
    keep_for_excel("d1_oxdoc_cell_add.xlsx", &saved);
    let reloaded = load(&saved);
    let id = sheet(&reloaded, &name);
    assert_eq!(reloaded.model().cell(at(id, "B1")), Some(&number(5.0)));
    assert_eq!(
        formula_text(reloaded.model().cell(at(id, "A3"))).as_deref(),
        Some("A1+B1")
    );
    assert_eq!(reloaded.model().cell(at(id, "B4")), Some(&number(6.0)));
    // An authored SEQUENCE is written as Excel writes it: a dynamic array with its prefix, over a
    // metadata part this package did not have.
    assert_eq!(
        formula_text(reloaded.model().cell(at(id, "C6"))).as_deref(),
        Some("_xlfn.SEQUENCE(2)")
    );
    assert_eq!(
        reloaded
            .model()
            .definitions()
            .array(at(id, "C6"))
            .map(|array| array.kind()),
        Some(ArrayKind::Dynamic { collapsed: false })
    );
    let xml = part_text(&saved, "xl/worksheets/sheet1.xml");
    assert!(xml.contains(r#"<dimension ref="A1:C6"/>"#), "{xml}");
    // The formula set changed, so the chain is gone.
    assert!(part(&saved, "xl/calcChain.xml").is_none());
}

#[test]
fn d1_oxdoc__names_tables_styles_and_untouched_parts() {
    let bytes = package("c1-preservation");
    let loaded = load(&bytes);
    let model = loaded.model();
    let names = model.definitions().names();
    assert_eq!(names.len(), 1);
    assert_eq!(
        (
            names[0].name.as_str(),
            names[0].scope,
            names[0].formula.as_str()
        ),
        ("DataRange", NameScope::Workbook, "C1!$A$1:$B$1")
    );
    let id = sheet(&loaded, "C1");
    let tables = model.definitions().tables();
    assert_eq!(tables.len(), 1);
    assert_eq!(
        (
            tables[0].name.as_str(),
            tables[0].display_name.as_str(),
            tables[0].area,
            tables[0].header_rows,
            tables[0].totals_rows
        ),
        (
            "C1Table",
            "C1Table",
            GridArea::new(at(id, "A1"), at(id, "B1")).unwrap(),
            1,
            0
        )
    );
    let styles = loaded.styles();
    assert_eq!(styles.cell_formats.len(), 2);
    assert_eq!(styles.number_format_code(1), Some("#,##0.000"));
    assert_eq!(styles.number_format_code(0), Some("General"));
    assert_eq!(loaded.sheet(id).unwrap().style(1, 1), 1);
    assert_eq!(save_untouched(&bytes), bytes);
    // An edit outside the table keeps every part the edit does not touch byte for byte.
    let model = edited(
        &loaded,
        vec![WorkbookOp::SetCell {
            at: at(id, "E5"),
            input: number(1.0),
        }],
    );
    let (saved, _) = save_model(&loaded, &model);
    for name in [
        "xl/vbaProject.bin",
        "xl/tables/table1.xml",
        "xl/styles.xml",
        "xl/drawings/drawing1.xml",
        "xl/pivotTables/pivotTable1.xml",
        "xl/pivotCache/pivotCacheDefinition1.xml",
        "customXml/item1.xml",
        "xl/calcChain.xml",
    ] {
        assert_eq!(part(&saved, name), part(&bytes, name), "{name}");
    }
    let xml = part_text(&saved, "xl/worksheets/sheet1.xml");
    assert!(
        xml.contains(r#"<conditionalFormatting sqref="A1:B1">"#),
        "{xml}"
    );
    assert!(
        part_text(&saved, "xl/workbook.xml")
            .contains(r#"<definedName name="DataRange">C1!$A$1:$B$1</definedName>"#)
    );
}
