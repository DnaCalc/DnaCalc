//! dc-5sr: Excel binary workbooks (.xlsb) open as the SpreadsheetML package they stand for.
//!
//! The fixture `tests/fixtures/dc_5sr/features.xlsb` and its twin `features.xlsx` are one workbook
//! Excel 16.0.20430 saved twice (`make-fixture.ps1`, which lists what it holds: values, shared,
//! array and dynamic-array formulas, future functions, names, 3-D references, styles, geometry,
//! merged cells, a comment, a validation, a conditional format, a hyperlink, frozen panes and a
//! table with a calculated column and a totals row). calc-doc's model of the .xlsb is its model of
//! the .xlsx, and a save of the opened .xlsb is an .xlsx that opens to the same model.

mod cdoc_m1_1_support;

use calc_doc::model::{CellInput, ModelRoot};
use calc_doc::{LoadedWorkbook, MemorySource, OpenPolicy, PackageRead};
use cdoc_m1_1_support::d1::{load, open, save_untouched};
use std::sync::Arc;

const XLSB: &[u8] = include_bytes!("fixtures/dc_5sr/features.xlsb");
const XLSX: &[u8] = include_bytes!("fixtures/dc_5sr/features.xlsx");

/// The first difference between two loaded workbooks' models, styled cells and geometry.
fn difference(a: &LoadedWorkbook, b: &LoadedWorkbook) -> Option<String> {
    let (x, y): (&ModelRoot, &ModelRoot) = (a.model(), b.model());
    if x.sheets() != y.sheets() {
        return Some(format!("sheets {:?} and {:?}", x.sheets(), y.sheets()));
    }
    for entry in x.sheets() {
        let cells = |root: &ModelRoot| -> Vec<_> {
            root.sheet_cells(entry.id())
                .into_iter()
                .flatten()
                .map(|(at, input)| (at, input.clone()))
                .collect()
        };
        let (p, q) = (cells(x), cells(y));
        if p != q {
            let first = p.iter().zip(&q).find(|(m, n)| m != n);
            return Some(format!(
                "cells of {:?}: {} and {}; first {first:?}",
                entry.id(),
                p.len(),
                q.len()
            ));
        }
    }
    let (d, e) = (x.definitions(), y.definitions());
    for (kind, m, n) in [
        (
            "names",
            format!("{:?}", d.names()),
            format!("{:?}", e.names()),
        ),
        (
            "arrays",
            format!(
                "{:?}",
                d.arrays()
                    .map(|a| (a.anchor(), a.kind()))
                    .collect::<Vec<_>>()
            ),
            format!(
                "{:?}",
                e.arrays()
                    .map(|a| (a.anchor(), a.kind()))
                    .collect::<Vec<_>>()
            ),
        ),
        (
            "tables",
            format!("{:?}", d.tables()),
            format!("{:?}", e.tables()),
        ),
        (
            "hidden rows",
            format!("{:?}", d.hidden_rows_all().collect::<Vec<_>>()),
            format!("{:?}", e.hidden_rows_all().collect::<Vec<_>>()),
        ),
        (
            "unloaded cells",
            format!("{:?}", d.unloaded_cells().collect::<Vec<_>>()),
            format!("{:?}", e.unloaded_cells().collect::<Vec<_>>()),
        ),
        (
            "macro_enabled",
            format!("{:?}", d.macro_enabled()),
            format!("{:?}", e.macro_enabled()),
        ),
    ] {
        if m != n {
            return Some(format!("{kind}: {m} and {n}"));
        }
    }
    if d.cell_formats().ne(e.cell_formats()) {
        return Some("cell formats".to_owned());
    }
    for (s, t) in a.sheets().iter().zip(b.sheets()) {
        let (m, n): (Vec<_>, Vec<_>) = (s.styled_cells().collect(), t.styled_cells().collect());
        if m != n {
            return Some(format!("styled cells of {:?}", s.id()));
        }
        // Each encoding has its own retained XML (including revision IDs and namespaces).
        // Compare every semantic validation field, as before source retention was added.
        let semantic = |rules: &[calc_doc::Validation]| rules.iter().cloned().map(|mut r| { r.source=None; r }).collect::<Vec<_>>();
        if semantic(s.validations()) != semantic(t.validations()) {
            return Some(format!(
                "validations of {:?}: {:?} and {:?}",
                s.id(),
                s.validations(),
                t.validations()
            ));
        }
        if s.conditional_formats() != t.conditional_formats() {
            return Some(format!(
                "conditional formats of {:?}: {:?} and {:?}",
                s.id(),
                s.conditional_formats(),
                t.conditional_formats()
            ));
        }
        if s.geometry() != t.geometry() {
            return Some(format!(
                "geometry of {:?}: {:?} and {:?}",
                s.id(),
                s.geometry(),
                t.geometry()
            ));
        }
    }
    if (a.date_system(), a.calc_mode()) != (b.date_system(), b.calc_mode()) {
        return Some("settings".to_owned());
    }
    None
}

fn formula(loaded: &LoadedWorkbook, sheet: usize, row: u32, column: u32) -> Option<String> {
    let id = loaded.sheets()[sheet].id();
    let at = calc_doc::model::value::CellKey::new(id, row, column).unwrap();
    match loaded.model().cell(at) {
        Some(CellInput::Formula(text)) => Some(text.text().to_string_lossy()),
        _ => None,
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_5sr__an_xlsb_opens_with_the_model_of_its_xlsx_twin() {
    let binary = load(XLSB);
    let twin = load(XLSX);
    assert_eq!(difference(&binary, &twin), None);
    // The formulas are the text Excel writes in SpreadsheetML.
    for (row, column, text) in [
        (1, 3, "SUM(A1:A5)"),
        (2, 3, "A2*2"),
        (5, 3, "A5*2"),
        (1, 4, "A1:A2*10"),
        (1, 5, "_xlfn.SEQUENCE(3)"),
        (1, 6, "_xlfn.XLOOKUP(2.5,A1:A5,A1:A5)"),
        (2, 6, "Rate*2"),
        (3, 6, "'Other Sheet'!A1+1"),
        (4, 6, "SUM(Data:'Other Sheet'!A1)"),
        (7, 6, "_xlfn.LET(_xlpm.x,2,_xlpm.x*3)"),
        (8, 6, "0.000000000015+Local"),
    ] {
        assert_eq!(
            formula(&binary, 0, row, column).as_deref(),
            Some(text),
            "R{row}C{column}"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_5sr__a_saved_xlsb_is_an_xlsx_that_opens_to_the_same_model() {
    let saved = save_untouched(XLSB);
    let document = open(&saved);
    let main = document
        .package()
        .parts()
        .iter()
        .find(|part| part.name() == "xl/workbook.xml")
        .and_then(|part| part.content_type().map(str::to_owned));
    assert_eq!(
        main.as_deref(),
        Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml")
    );
    assert!(
        document
            .package()
            .parts()
            .iter()
            .all(|part| !part.name().ends_with(".bin")),
        "no binary part is left"
    );
    assert_eq!(difference(&load(&saved), &load(XLSB)), None);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_5sr__the_conversion_is_what_open_reads_and_only_for_binary_workbooks() {
    let source =
        |bytes: &[u8]| -> Arc<dyn PackageRead> { Arc::new(MemorySource::new(bytes.to_vec())) };
    let converted = calc_doc::convert_binary_workbook(source(XLSB), &OpenPolicy::default())
        .expect("converted")
        .expect("a binary workbook");
    assert_eq!(difference(&load(&converted), &load(XLSB)), None);
    assert_eq!(
        calc_doc::convert_binary_workbook(source(XLSX), &OpenPolicy::default()).expect("opened"),
        None
    );
}


#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn binary_function_table_matches_the_existing_measurement() {
    let bytes = include_bytes!("fixtures/dc_5sr/functions.json");
    let source: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    let mut expected: Vec<_> = source["functions"].as_object().unwrap().iter().map(|(id, row)| {
        let index: u16 = id.parse().unwrap();
        let name = row[0].as_str().unwrap();
        let arity = row[1].as_i64().map_or(-1, |value| value as i8);
        (index, name, arity)
    }).collect();
    expected.sort_by_key(|row| row.0);
    assert_eq!(calc_catalogue::EXCEL_FUNCTION_IDS, expected.as_slice());
}
