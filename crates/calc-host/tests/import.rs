//! IMPORTCSV and IMPORTTEXT through a host's file-read route, end to end: formulas in a package
//! and a fake route that serves files from memory (no test touches the file system). Every
//! expected result is Excel 16.0.20527's (see `calc_func::import` and
//! `crates/calc-func/tests/fixtures/import`).

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

#[allow(dead_code)]
mod chost_m1_1_support;

use calc_func::ImportRoute;
use calc_host::value::{CalcValue, CellKey, CoreRef, ExcelText, WorksheetErrorCode};
use calc_host::{
    CalcMode, HostOptions, HostProgress, HostState, MemorySource, OpenPolicy, PackageRead,
    ProjectedValue, ProjectionRequest, Unobserved, WorkBudget, calculate_full, project, step,
};
use chost_m1_1_support::stored_zip;
use std::rc::Rc;
use std::sync::Arc;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// The files of Excel's readings, by the file name after `C:\data\`.
#[derive(Debug)]
struct Files;

impl ImportRoute for Files {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        match path.strip_prefix("C:\\data\\")? {
            "basic.csv" => Some(b"a,b,c\n1,2,3\n4,5,6\n".to_vec()),
            "semi.txt" => Some(b"a;b;c\n1;2;3\n".to_vec()),
            "empty.csv" => Some(Vec::new()),
            _ => None,
        }
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

/// Sheet1 with `formulas` in column A, each as the file stores it.
fn workbook(formulas: &[&str]) -> Vec<u8> {
    let mut rows = String::new();
    for (index, formula) in formulas.iter().enumerate() {
        let row = index + 1;
        rows.push_str(&format!(
            r#"<row r="{row}"><c r="A{row}"><f>{}</f></c></row>"#,
            escape(formula)
        ));
    }
    let sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="{MAIN}"><sheetData>{rows}</sheetData></worksheet>"#
    );
    let types = r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#;
    let root = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{R}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
    );
    let book = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="{MAIN}" xmlns:r="{R}"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let book_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{R}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    stored_zip(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", book.as_bytes()),
        ("xl/_rels/workbook.xml.rels", book_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
    ])
}

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

fn values(host: &HostState, rows: usize) -> Vec<CalcValue> {
    let sheet = host.sheets()[0].id();
    let cells: Vec<CellKey> = (1..=rows as u32)
        .map(|row| CellKey::new(sheet, row, 1).expect("in the grid"))
        .collect();
    let projection = project(host, &ProjectionRequest { cells }).expect("projected");
    projection
        .cells()
        .iter()
        .map(|cell| match cell.value() {
            ProjectedValue::Calculated { value, .. } => value.clone(),
            other => panic!("not calculated: {other:?}"),
        })
        .collect()
}

fn results(formulas: &[&str], route: Option<Rc<dyn ImportRoute>>) -> Vec<CalcValue> {
    let mut host = open(workbook(formulas));
    if let Some(route) = route {
        host.set_import_route(route);
    }
    calculate_full(&mut host);
    for _ in 0..100_000 {
        match step(&mut host, WorkBudget::UNLIMITED, &mut Unobserved) {
            HostProgress::Failed(why) => panic!("{why}"),
            HostProgress::Calculating { .. }
            | HostProgress::Waiting { .. }
            | HostProgress::Published(_) => {}
            _ => break,
        }
    }
    values(&host, formulas.len())
}

fn text(value: &str) -> CalcValue {
    CalcValue::text(ExcelText::from_utf16_code_units(
        value.encode_utf16().collect(),
    ))
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_z8c1__imports_read_through_the_hosts_route() {
    let route: Rc<dyn ImportRoute> = Rc::new(Files);
    let formulas = [
        r#"ROWS(_xlfn.IMPORTCSV("C:\data\basic.csv"))"#,
        r#"COLUMNS(_xlfn.IMPORTCSV("C:\data\basic.csv"))"#,
        r#"INDEX(_xlfn.IMPORTCSV("C:\data\basic.csv"),3,2)"#,
        r#"INDEX(_xlfn.IMPORTCSV("C:\data\basic.csv"),1,3)"#,
        r#"INDEX(_xlfn.IMPORTTEXT("C:\data\semi.txt",";"),2,3)"#,
        r#"ROWS(_xlfn.IMPORTCSV("C:\data\basic.csv",1,1))"#,
        r#"_xlfn.IMPORTCSV("C:\data\missing.csv")"#,
        r#"_xlfn.IMPORTCSV("C:\data\empty.csv")"#,
        r#"_xlfn.IMPORTCSV("relative.csv")"#,
    ];
    let got = results(&formulas, Some(route));
    let want = [
        CalcValue::number(3.0),
        CalcValue::number(3.0),
        CalcValue::number(5.0),
        text("c"),
        CalcValue::number(3.0),
        CalcValue::number(1.0),
        CalcValue::error(WorksheetErrorCode::Value),
        CalcValue::error(WorksheetErrorCode::Calc),
        CalcValue::error(WorksheetErrorCode::Value),
    ];
    for ((formula, got), want) in formulas.iter().zip(&got).zip(&want) {
        assert_eq!(shown(got), shown(want), "{formula}");
    }
}

/// A host with no route gives what Excel gives when the file cannot be read.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_z8c1__without_a_route_the_file_cannot_be_read() {
    let got = results(&[r#"_xlfn.IMPORTCSV("C:\data\basic.csv")"#], None);
    assert_eq!(
        shown(&got[0]),
        shown(&CalcValue::error(WorksheetErrorCode::Value))
    );
}

fn shown(value: &CalcValue) -> String {
    match value.core_ref() {
        CoreRef::Number(number) => format!("n{number}"),
        CoreRef::Text(text) => format!("s{:?}", text.to_string_lossy()),
        CoreRef::Error(code) => format!("e{}", code.excel_spelling()),
        other => format!("{other:?}"),
    }
}
