//! dc-ylr: a reference to another workbook, as a file's formula writes it (`[1]Sheet1!A1`), reads
//! the value the workbook's externalLink part caches for that cell, end to end through calc-host:
//! the package opened, recalculated and its cells projected. The rules are those Excel 16.0.20527
//! gave the local corpus's link workbooks (dc-ifc's captures, 2026-10-29): `=[1]Sheet1!A1` is the
//! cached 77, and #REF! where the link's `sheetData` is marked `refreshError` and caches nothing.
//! A result read from a link's cache, directly or through another formula, carries the
//! external-link mark. A cell the cache of a sheet does not hold is blank (Excel 16.0.20527,
//! 2026-10-30: `=[1]Sheet1!C1` of an uncached cell is 0); a book no link caches is refused.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

#[allow(dead_code)]
mod chost_m1_1_support;

use calc_host::value::{CellKey, CoreRef, WorksheetErrorCode};
use calc_host::{
    CalcMode, HostOptions, HostProgress, HostState, MemorySource, OpenPolicy, PackageRead,
    ProjectedValue, ProjectionRequest, Unobserved, WorkBudget, project, step,
};
use chost_m1_1_support::stored_zip;
use std::sync::Arc;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// Sheet1: A1 `[1]Sheet1!$A$1` (cached 111), B1 `A1+1`, C1 `SUM([1]Sheet1!A1:A2)` (111 and 5),
/// D1 `[2]Sheet1!A1` (a link whose cache was not refreshed), E1 `[1]Sheet1!B9` (a cell the cache
/// does not hold: blank), F1 `'[1]Other'!A1` (text "x" cached on the first link's second sheet), G1
/// `[3]Sheet1!A1` (no third link).
const SHEET: &str = r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><f>[1]Sheet1!$A$1</f></c><c r="B1"><f>A1+1</f></c><c r="C1"><f>SUM([1]Sheet1!A1:A2)</f></c><c r="D1"><f>[2]Sheet1!A1</f></c><c r="E1"><f>[1]Sheet1!B9</f></c><c r="F1"><f>'[1]Other'!A1</f></c><c r="G1"><f>[3]Sheet1!A1</f></c></row></sheetData></worksheet>"#;

const LINK1: &str = r#"<?xml version="1.0" encoding="UTF-8"?><externalLink xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><externalBook r:id="rId1"><sheetNames><sheetName val="Sheet1"/><sheetName val="Other"/></sheetNames><sheetDataSet><sheetData sheetId="0"><row r="1"><cell r="A1"><v>111</v></cell></row><row r="2"><cell r="A2"><v>5</v></cell></row></sheetData><sheetData sheetId="1"><row r="1"><cell r="A1" t="str"><v>x</v></cell></row></sheetData></sheetDataSet></externalBook></externalLink>"#;

const LINK2: &str = r#"<?xml version="1.0" encoding="UTF-8"?><externalLink xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><externalBook r:id="rId1"><sheetNames><sheetName val="Sheet1"/></sheetNames><sheetDataSet><sheetData sheetId="0" refreshError="1"/></sheetDataSet></externalBook></externalLink>"#;

fn package() -> Vec<u8> {
    let types = r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/externalLinks/externalLink1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml"/><Override PartName="/xl/externalLinks/externalLink2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml"/></Types>"#;
    let root = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{R}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
    );
    let workbook = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="{MAIN}" xmlns:r="{R}"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets><externalReferences><externalReference r:id="rId2"/><externalReference r:id="rId3"/></externalReferences></workbook>"#
    );
    let workbook_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{R}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{R}/externalLink" Target="externalLinks/externalLink1.xml"/><Relationship Id="rId3" Type="{R}/externalLink" Target="externalLinks/externalLink2.xml"/></Relationships>"#
    );
    let link1_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{R}/externalLinkPath" Target="Book2.xlsx" TargetMode="External"/></Relationships>"#
    );
    let link2_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="http://schemas.microsoft.com/office/2006/relationships/xlExternalLinkPath/xlPathMissing" Target="MissingBook.xlsx" TargetMode="External"/></Relationships>"#
    );
    stored_zip(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", workbook_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", SHEET.as_bytes()),
        ("xl/externalLinks/externalLink1.xml", LINK1.as_bytes()),
        (
            "xl/externalLinks/_rels/externalLink1.xml.rels",
            link1_rels.as_bytes(),
        ),
        ("xl/externalLinks/externalLink2.xml", LINK2.as_bytes()),
        (
            "xl/externalLinks/_rels/externalLink2.xml.rels",
            link2_rels.as_bytes(),
        ),
    ])
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_ylr__external_references_read_the_links_cached_values() {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(package()));
    let mut host = HostState::open(
        source,
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Automatic),
            ..HostOptions::default()
        },
    )
    .expect("the package opens");
    loop {
        match step(&mut host, WorkBudget::UNLIMITED, &mut Unobserved) {
            HostProgress::Calculating { .. } => {}
            HostProgress::Published(delta) => {
                let receipt = delta.completion().unwrap();
                assert_eq!(receipt.coverage.calculated, 6);
                assert_eq!(receipt.coverage.uncertified, 6);
                assert_eq!(receipt.outcome, calc_host::CompletionOutcome::Incomplete);
                break;
            }
            _ => break,
        }
    }
    let sheet = host.sheets()[0].id();
    let cells: Vec<CellKey> = (1..=7)
        .map(|column| CellKey::new(sheet, 1, column).expect("row 1"))
        .collect();
    let projection = project(&host, &ProjectionRequest { cells }).expect("projected");
    let shown: Vec<String> = projection
        .cells()
        .iter()
        .map(|cell| match cell.value() {
            ProjectedValue::Calculated { value, calculation } => {
                assert!(
                    calculation.stamp().unwrap().reads.external_cache,
                    "direct and transitive cached external reads must retain their qualification"
                );
                let mark = if calculation.reads_external_links() {
                    " (external link)"
                } else {
                    ""
                };
                match value.core_ref() {
                    CoreRef::Number(n) => format!("{n}{mark}"),
                    CoreRef::Text(t) => {
                        format!("{:?}{mark}", String::from_utf16_lossy(t.utf16_code_units()))
                    }
                    CoreRef::Error(code) => format!("{code:?}{mark}"),
                    other => format!("{other:?}{mark}"),
                }
            }
            ProjectedValue::NotModeled { .. } => "not modelled".to_owned(),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        shown,
        [
            "111 (external link)".to_owned(),
            "112 (external link)".to_owned(),
            "116 (external link)".to_owned(),
            format!("{:?} (external link)", WorksheetErrorCode::Ref),
            "0 (external link)".to_owned(),
            "\"x\" (external link)".to_owned(),
            "not modelled".to_owned(),
        ]
    );
}

/// The value `cell` projects, as text with the external-link mark (as the first case shows it).
fn shown_value(host: &HostState, cell: CellKey) -> String {
    let projection = project(host, &ProjectionRequest { cells: vec![cell] }).expect("projected");
    match projection.cells()[0].value() {
        ProjectedValue::Calculated { value, calculation } => {
            let mark = if calculation.reads_external_links() {
                " (external link)"
            } else {
                ""
            };
            match value.core_ref() {
                CoreRef::Number(n) => format!("{n}{mark}"),
                CoreRef::Text(t) => {
                    format!("{:?}{mark}", String::from_utf16_lossy(t.utf16_code_units()))
                }
                CoreRef::Logical(b) => format!("{b}{mark}"),
                CoreRef::Error(code) => format!("{code:?}{mark}"),
                other => format!("{other:?}{mark}"),
            }
        }
        ProjectedValue::NotModeled { .. } => "not modelled".to_owned(),
        other => format!("{other:?}"),
    }
}

fn recalculated(bytes: &[u8]) -> HostState {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    let mut host = HostState::open(
        source,
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Automatic),
            ..HostOptions::default()
        },
    )
    .expect("the package opens");
    loop {
        match step(&mut host, WorkBudget::UNLIMITED, &mut Unobserved) {
            HostProgress::Calculating { .. } => {}
            _ => break,
        }
    }
    host
}

/// The dc-ylr oracle (tests/fixtures/dc_ylr, made by Excel 16.0.20527 with
/// make-dc-ylr-oracle.ps1) as Excel saved it, .xlsx and .xlsb: an external reference is a
/// reference to the link's cached sheet, in names too, and the binary twin reads the same link.
/// The values are Excel's, captured with the other book absent (captures/dc_ylr), for a sample
/// of column A's 71 formulas; `cargo xtask compare` matches all 74 formula cells.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_ylr__the_excel_oracle_and_its_binary_twin_read_the_link_as_references() {
    let xlsx = recalculated(include_bytes!("fixtures/dc_ylr/oracle.xlsx"));
    let xlsb = recalculated(include_bytes!("fixtures/dc_ylr/oracle.xlsb"));
    let main = |host: &HostState| {
        host.sheets()
            .iter()
            .find(|sheet| {
                String::from_utf16_lossy(sheet.name().text().utf16_code_units()) == "Main"
            })
            .expect("sheet Main")
            .id()
    };
    let cells = |host: &HostState| -> Vec<String> {
        let sheet = main(host);
        (1..=71)
            .map(|row| shown_value(host, CellKey::new(sheet, row, 1).expect("a cell")))
            .collect()
    };
    let (from_xlsx, from_xlsb) = (cells(&xlsx), cells(&xlsb));
    assert_eq!(from_xlsx, from_xlsb, "the binary twin reads the same link");
    let at = |row: usize| from_xlsx[row - 1].as_str();
    let link = " (external link)";
    for (row, expected) in [
        (1, "1".to_owned()),
        (5, "0".to_owned()),
        (12, "true".to_owned()),
        (18, "2".to_owned()),
        (19, "4".to_owned()),
        (20, format!("{:?}", WorksheetErrorCode::Value)),
        (25, format!("{:?}", WorksheetErrorCode::Value)),
        (27, format!("{:?}", WorksheetErrorCode::NA)),
        (29, format!("{:?}", WorksheetErrorCode::NA)),
        (31, "true".to_owned()),
        (32, "3".to_owned()),
        (61, "6".to_owned()),
        (62, "1048576".to_owned()),
        (64, "2".to_owned()),
        (66, "3".to_owned()),
        (68, "true".to_owned()),
        (70, "100".to_owned()),
    ] {
        assert_eq!(at(row), format!("{expected}{link}"), "Main!A{row}");
    }
    assert_eq!(at(71), "2", "=1+1 reads no link");
}

/// A typed formula naming the other book by its file name, with or without its folder, reads the
/// link to that file and is held by the link's number, as Excel stores it (Excel 16.0.20527,
/// 2026-10-30, the other book absent: `=[src.xlsx]Sheet1!A2` 2, `=SUM(...!A1:A3)` 6,
/// `'<folder>\[src.xlsx]Sheet1'!A3` 3, `'[src.xlsx]My Sheet'!A1` 10; the saved file holds
/// `[1]Sheet1!A2`). A book no link targets is refused.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_ylr__a_typed_formula_names_the_book_by_its_file_name() {
    use calc_host::{CellInput, HostSession, SkinIntent};
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(
        include_bytes!("fixtures/dc_ylr/oracle.xlsx").to_vec(),
    ));
    let mut session = HostSession::open(
        source,
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Automatic),
            ..HostOptions::default()
        },
    )
    .expect("it opens");
    let sheet = session
        .host()
        .sheets()
        .iter()
        .find(|sheet| String::from_utf16_lossy(sheet.name().text().utf16_code_units()) == "Main")
        .expect("sheet Main")
        .id();
    let entries = [
        (
            "=[ylr-source.xlsx]Sheet1!A2",
            "2 (external link)",
            "[1]Sheet1!A2",
        ),
        (
            "=SUM([YLR-SOURCE.XLSX]Sheet1!A1:A3)",
            "6 (external link)",
            "SUM([1]Sheet1!A1:A3)",
        ),
        (
            r"='C:\Data\[ylr-source.xlsx]Sheet1'!A3",
            "3 (external link)",
            "[1]Sheet1!A3",
        ),
        (
            "='[ylr-source.xlsx]My Sheet'!A1",
            "10 (external link)",
            "'[1]My Sheet'!A1",
        ),
        (
            "=[other.xlsx]Sheet1!A1",
            "not modelled",
            "[other.xlsx]Sheet1!A1",
        ),
    ];
    for (row, (text, _, _)) in entries.iter().enumerate() {
        let at = CellKey::new(sheet, row as u32 + 1, 2).expect("a cell");
        let receipt = session.dispatch(SkinIntent::EnterCell {
            at,
            text: (*text).to_owned(),
        });
        assert!(receipt.accepted(), "{text}: {receipt:?}");
    }
    for (row, (text, value, held)) in entries.iter().enumerate() {
        let at = CellKey::new(sheet, row as u32 + 1, 2).expect("a cell");
        assert_eq!(shown_value(session.host(), at), *value, "{text}");
        let projection =
            project(session.host(), &ProjectionRequest { cells: vec![at] }).expect("projected");
        let Some(CellInput::Formula(formula)) = projection.cells()[0].input() else {
            panic!("{text} holds no formula");
        };
        assert_eq!(
            String::from_utf16_lossy(formula.text().utf16_code_units()),
            *held,
            "{text}"
        );
    }
}
