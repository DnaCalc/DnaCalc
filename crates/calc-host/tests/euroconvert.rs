//! EUROCONVERT end to end: a workbook Excel saved with the Euro Currency Tools add-in loaded,
//! opened, recalculated and its cells projected. Every formula is the file form
//! `[1]!EUROCONVERT(...)` over an `xlLibrary` link to EUROTOOL.XLAM, and every expected value is
//! the one Excel 16.0.20527 cached (`fixtures/euroconvert/measured.xlsx`: 2726 amounts over the
//! seventeen units and EUR, with half cases and overflows). `literals.tsv` holds Excel's readings
//! of literal argument forms, each compiled here in the same library-qualified form.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

#[allow(dead_code)]
mod chost_m1_1_support;

use calc_host::value::{CalcValue, CellKey, CoreRef, WorksheetErrorCode};
use calc_host::{
    CalcMode, HostOptions, HostProgress, HostState, MemorySource, OpenPolicy, PackageRead,
    ProjectedValue, ProjectionRequest, SavePolicy, Unobserved, WorkBudget, calculate_full, project,
    save, step,
};
use chost_m1_1_support::stored_zip;
use std::sync::Arc;

const MEASURED: &[u8] = include_bytes!("fixtures/euroconvert/measured.xlsx");
const LITERALS: &str = include_str!("fixtures/euroconvert/literals.tsv");

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

fn shown(value: &CalcValue) -> String {
    match value.core_ref() {
        CoreRef::Number(n) => format!("{:016x}", n.to_bits()),
        CoreRef::Error(code) => format!("{code:?}"),
        other => format!("{other:?}"),
    }
}

/// The workbook's formula cells of `column`, read as the file cached them.
fn cached(host: &HostState, column: u32, rows: u32) -> Vec<(CellKey, String)> {
    let sheet = host.sheets()[0].id();
    let cells: Vec<CellKey> = (1..=rows)
        .map(|row| CellKey::new(sheet, row, column).expect("in the grid"))
        .collect();
    let projection = project(
        host,
        &ProjectionRequest {
            cells: cells.clone(),
        },
    )
    .expect("projected");
    cells
        .into_iter()
        .zip(projection.cells())
        .filter_map(|(cell, projected)| match projected.value() {
            ProjectedValue::FileCached(file) => Some((cell, shown(&file.value().to_value()))),
            _ => None,
        })
        .collect()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_fetch_f0tp__excels_saved_workbook_recalculates_to_its_own_values() {
    let mut host = open(MEASURED.to_vec());
    let mut expected = cached(&host, 2, 2726);
    expected.extend(cached(&host, 4, 200));
    assert!(expected.len() >= 2726, "{}", expected.len());
    calculate_full(&mut host);
    settle(&mut host);
    let cells: Vec<CellKey> = expected.iter().map(|(cell, _)| *cell).collect();
    let projection = project(&host, &ProjectionRequest { cells }).expect("projected");
    let mut wrong = Vec::new();
    for ((cell, excel), projected) in expected.iter().zip(projection.cells()) {
        match projected.value() {
            ProjectedValue::Calculated { value, .. } => {
                let ours = shown(value);
                if &ours != excel {
                    wrong.push(format!("{cell:?}: Excel {excel}, ours {ours}"));
                }
            }
            other => wrong.push(format!("{cell:?}: {other:?}")),
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} differ: {:?}",
        wrong.len(),
        expected.len(),
        &wrong[..wrong.len().min(20)]
    );
}

/// A save keeps the library-qualified file form, the link and the calculated values, and the
/// saved package recalculates to the same bits.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_fetch_f0tp__save_and_reopen_keep_the_add_in_call() {
    let mut host = open(MEASURED.to_vec());
    let expected = cached(&host, 2, 2726);
    calculate_full(&mut host);
    settle(&mut host);
    let mut saved = Vec::new();
    save(&host, &SavePolicy::default(), &mut saved).expect("saved");
    let mut reopened = open(saved);
    assert_eq!(cached(&reopened, 2, 2726), expected);
    calculate_full(&mut reopened);
    settle(&mut reopened);
    let cells: Vec<CellKey> = expected.iter().map(|(cell, _)| *cell).collect();
    let projection = project(&reopened, &ProjectionRequest { cells }).expect("projected");
    for ((_, excel), projected) in expected.iter().zip(projection.cells()) {
        match projected.value() {
            ProjectedValue::Calculated { value, .. } => assert_eq!(&shown(value), excel),
            other => panic!("{other:?}"),
        }
    }
}

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const LIBRARY: &str =
    "http://schemas.microsoft.com/office/2006/relationships/xlExternalLinkPath/xlLibrary";

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

/// A workbook of `formulas` in column A, with the literal inputs of column E and F that the
/// measured formulas read, and the library link Excel writes for the add-in.
fn library_workbook(formulas: &[&str]) -> Vec<u8> {
    let mut rows = String::new();
    for (index, formula) in formulas.iter().enumerate() {
        let row = index + 1;
        let extra = match row {
            1 => r#"<c r="E1"><v>1</v></c><c r="F1"><v>7</v></c>"#,
            2 => r#"<c r="E2"><v>2</v></c><c r="F2" t="b"><v>1</v></c>"#,
            3 => r#"<c r="E3"><v>3</v></c>"#,
            4 => r#"<c r="F4" t="inlineStr"><is><t>text</t></is></c>"#,
            5 => r#"<c r="F5" t="inlineStr"><is><t>DEM</t></is></c>"#,
            6 => r#"<c r="F6"><v>3</v></c>"#,
            _ => "",
        };
        rows.push_str(&format!(
            r#"<row r="{row}"><c r="A{row}"><f>{}</f></c>{extra}</row>"#,
            escape(formula)
        ));
    }
    let sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="{MAIN}"><sheetData>{rows}</sheetData></worksheet>"#
    );
    let types = r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/externalLinks/externalLink1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml"/></Types>"#;
    let root = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{R}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
    );
    let workbook = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="{MAIN}" xmlns:r="{R}"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets><externalReferences><externalReference r:id="rId2"/></externalReferences></workbook>"#
    );
    let workbook_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{R}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{R}/externalLink" Target="externalLinks/externalLink1.xml"/></Relationships>"#
    );
    let link = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><externalLink xmlns="{MAIN}" xmlns:r="{R}"><externalBook r:id="rId1"/></externalLink>"#
    );
    let link_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{LIBRARY}" Target="EUROTOOL.XLAM" TargetMode="External"/></Relationships>"#
    );
    stored_zip(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", workbook_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/externalLinks/externalLink1.xml", link.as_bytes()),
        (
            "xl/externalLinks/_rels/externalLink1.xml.rels",
            link_rels.as_bytes(),
        ),
    ])
}

fn excel_reading(expected: &str) -> String {
    match expected {
        "#VALUE!" => format!("{:?}", WorksheetErrorCode::Value),
        "#NUM!" => format!("{:?}", WorksheetErrorCode::Num),
        number => format!(
            "{:016x}",
            number.parse::<f64>().expect("a measured number").to_bits()
        ),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_fetch_f0tp__excels_readings_of_argument_forms() {
    let rows: Vec<(&str, &str)> = LITERALS
        .lines()
        .map(|line| line.split_once('\t').expect("formula and reading"))
        .collect();
    let formulas: Vec<&str> = rows.iter().map(|(formula, _)| *formula).collect();
    let mut host = open(library_workbook(&formulas));
    calculate_full(&mut host);
    settle(&mut host);
    let sheet = host.sheets()[0].id();
    let cells: Vec<CellKey> = (1..=rows.len() as u32)
        .map(|row| CellKey::new(sheet, row, 1).expect("in the grid"))
        .collect();
    let projection = project(&host, &ProjectionRequest { cells }).expect("projected");
    let mut wrong = Vec::new();
    for ((formula, expected), projected) in rows.iter().zip(projection.cells()) {
        let ours = match projected.value() {
            ProjectedValue::Calculated { value, .. } => shown(value),
            other => format!("{other:?}"),
        };
        if ours != excel_reading(expected) {
            wrong.push(format!("{formula}: Excel {expected}, ours {ours}"));
        }
    }
    assert!(wrong.is_empty(), "{} wrong: {wrong:#?}", wrong.len());
}

/// A bare call is Excel's #NAME? with the add-in unloaded, its default state, whether or not a
/// library link is present, and the entry's catalogue spelling reaches nothing either.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_fetch_f0tp__a_bare_call_stays_a_name_error() {
    let mut host = open(library_workbook(&[
        r#"EUROCONVERT(1,"DEM","EUR")"#,
        r#"_EUROTOOL.EUROCONVERT(1,"DEM","EUR")"#,
    ]));
    calculate_full(&mut host);
    settle(&mut host);
    let sheet = host.sheets()[0].id();
    let cells: Vec<CellKey> = (1..=2)
        .map(|row| CellKey::new(sheet, row, 1).expect("in the grid"))
        .collect();
    let projection = project(&host, &ProjectionRequest { cells }).expect("projected");
    for projected in projection.cells() {
        match projected.value() {
            ProjectedValue::Calculated { value, .. } => {
                assert_eq!(shown(value), format!("{:?}", WorksheetErrorCode::Name))
            }
            other => panic!("{other:?}"),
        }
    }
}
