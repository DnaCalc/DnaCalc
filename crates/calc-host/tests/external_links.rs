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

#[path = "../../calc-engine/tests/fixtures/seven_cell_async.rs"]
mod async_provider;
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
    package_with_sheet(SHEET)
}

fn package_with_sheet(sheet: &str) -> Vec<u8> {
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
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_vba_cached_external_observations_preserve_qualification_and_observer_stop() {
    let worksheet = SHEET.replace(
        "</row>",
        "<c r=\"H1\"><f>Peek(A1)</f></c><c r=\"I1\"><f>H1+1</f></c></row>",
    );
    let source = "Public Function Peek(ByVal r As Range)\n Peek=r.Value2\nEnd Function\n";
    let open = || {
        let mut host = HostState::open(
            Arc::new(MemorySource::new(package_with_sheet(&worksheet))),
            &OpenPolicy::default(),
            HostOptions {
                calc_mode: Some(CalcMode::Automatic),
                ..HostOptions::default()
            },
        )
        .unwrap();
        host.set_user_functions(
            calc_vba::load_module_with_backend(
                "External",
                source,
                100_000,
                calc_vba::Backend::Interpreter,
            )
            .unwrap(),
        );
        host
    };
    let mut host = open();
    loop {
        match step(&mut host, WorkBudget::UNLIMITED, &mut Unobserved) {
            HostProgress::Calculating { .. } | HostProgress::Published(_) => {}
            _ => break,
        }
    }
    let sheet = host.sheets()[0].id();
    for (column, expected) in [(8, 111.0), (9, 112.0)] {
        let at = CellKey::new(sheet, 1, column).unwrap();
        let p = project(&host, &ProjectionRequest { cells: vec![at] }).unwrap();
        let (value, calculation) = match p.cells()[0].value() {
            ProjectedValue::Calculated { value, calculation } => (value, calculation),
            ProjectedValue::Stale(held) => {
                (held.last_value().unwrap(), held.last_calculation().unwrap())
            }
            other => panic!("{other:?}"),
        };
        assert_eq!(value.as_number(), Some(expected));
        assert!(calculation.reads_external_links());
        let stamp = calculation.stamp().unwrap();
        assert!(stamp.reads.external_cache);
        assert!(
            stamp
                .reads
                .observations
                .iter()
                .any(|read| read.external_cache)
        );
    }
    struct StopCached;
    impl calc_engine::ReadObserver for StopCached {
        fn observe(
            &mut self,
            formula: CellKey,
            _: calc_host::value::GridArea,
            _: u64,
            _: calc_host::CellRun<'_>,
        ) -> std::ops::ControlFlow<()> {
            if formula.column() == 8 {
                std::ops::ControlFlow::Break(())
            } else {
                std::ops::ControlFlow::Continue(())
            }
        }
    }
    let mut stopped = open();
    let failed = loop {
        match step(&mut stopped, WorkBudget::UNLIMITED, &mut StopCached) {
            HostProgress::Calculating { .. } | HostProgress::Published(_) => {}
            other => break other,
        }
    };
    assert!(
        matches!(failed,HostProgress::Failed(calc_host::RecalcFailed::Failed(calc_engine::RecalcFailure::ReadFailed {formula,..})) if formula.column()==8),
        "{failed:?}"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_vba_cached_read_of_a_refused_formula_cannot_become_zero() {
    let worksheet = SHEET
        .replace("[1]Sheet1!$A$1", "NOW()")
        .replace("</row>", "<c r=\"H1\"><f>Peek(A1)</f></c></row>");
    let mut host = HostState::open(
        Arc::new(MemorySource::new(package_with_sheet(&worksheet))),
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Automatic),
            ..HostOptions::default()
        },
    )
    .unwrap();
    host.set_user_functions(calc_vba::load_module_with_backend("Refused","Public Function Peek(ByVal r As Range)\n On Error Resume Next\n Peek=r.Value2\nEnd Function\n",100_000,calc_vba::Backend::Interpreter).unwrap());
    loop {
        match step(&mut host, WorkBudget::UNLIMITED, &mut Unobserved) {
            HostProgress::Calculating { .. } | HostProgress::Published(_) => {}
            _ => break,
        }
    }
    let at = CellKey::new(host.sheets()[0].id(), 1, 8).unwrap();
    let p = project(&host, &ProjectionRequest { cells: vec![at] }).unwrap();
    assert!(
        matches!(p.cells()[0].value(), ProjectedValue::NotModeled { .. }),
        "{:?}",
        p.cells()[0].value()
    );
}

fn source_sheet(cells: &str) -> HostState {
    let sheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1">{cells}</row></sheetData></worksheet>"#
    );
    HostState::open(
        Arc::new(MemorySource::new(package_with_sheet(&sheet))),
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Manual),
            ..HostOptions::default()
        },
    )
    .unwrap()
}

fn drain_domain(domain: &mut calc_host::CalculationDomain, budget: WorkBudget) {
    for _ in 0..10000 {
        match domain.advance(budget) {
            calc_host::DomainProgress::Complete => return,
            calc_host::DomainProgress::Advanced {
                progress: HostProgress::Cancelled,
                ..
            } => panic!("cancelled"),
            calc_host::DomainProgress::Advanced { .. } => {}
            other => panic!("{other:?}"),
        }
    }
    panic!("domain did not complete");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_ylr__fallback_spill_children_cannot_export_calculated_source_values() {
    let mut host = source_sheet(
        r#"<c r="A1"><f t="array" ref="A1:A3">[1]Sheet1!A1:A3</f></c><c r="B1"><f>A2+1</f></c><c r="C1"><f>SUM(A2:A3)</f></c>"#,
    );
    calc_host::calculate(&mut host);
    while matches!(
        step(&mut host, WorkBudget::UNLIMITED, &mut Unobserved),
        HostProgress::Calculating { .. } | HostProgress::Published(_)
    ) {}
    let sheet = host.sheets()[0].id();
    for (column, expected) in [(2, "6 (external link)"), (3, "5 (external link)")] {
        let at = CellKey::new(sheet, 1, column).unwrap();
        assert_eq!(shown_value(&host, at), expected);
        assert!(
            host.linked_snapshot()
                .calculation("Sheet1", 1, column)
                .is_none()
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_ylr__fallback_spill_shape_keeps_its_source_qualification() {
    let mut session = calc_host::HostSession::open(
        Arc::new(MemorySource::new(
            include_bytes!("fixtures/dc_ylr/oracle.xlsx").to_vec(),
        )),
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Manual),
            ..HostOptions::default()
        },
    )
    .unwrap();
    let sheet = session
        .host()
        .sheets()
        .iter()
        .find(|s| s.name().text().to_string_lossy() == "Main")
        .unwrap()
        .id();
    let at = CellKey::new(sheet, 80, 10).unwrap();
    assert!(
        session
            .dispatch(calc_host::SkinIntent::EnterCell {
                at,
                text: "=ROWS(C1#)".into()
            })
            .accepted()
    );
    assert!(
        session
            .dispatch(calc_host::SkinIntent::Recalculate)
            .accepted()
    );
    while matches!(
        session.advance_calculation(),
        HostProgress::Calculating { .. } | HostProgress::Published(_)
    ) {}
    assert_eq!(shown_value(session.host(), at), "3 (external link)");
    assert!(
        session
            .host()
            .linked_snapshot()
            .calculation("Main", 80, 10)
            .is_none()
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_ylr__available_source_calculates_its_formulas_in_its_own_context() {
    for budget in [WorkBudget::evaluations(1), WorkBudget::UNLIMITED] {
        let mut domain = calc_host::CalculationDomain::new();
        let target = domain.insert(source_sheet(r#"<c r="A1"><f>[1]Sheet1!A1</f></c><c r="B1"><f>A1+1</f></c><c r="C1"><f>SUM([1]Sheet1!A1:A2)</f></c><c r="D1"><f>[2]Sheet1!A1</f></c><c r="E1"><f>[1]Sheet1!B9</f></c>"#)).unwrap();
        let source = domain.insert(source_sheet(r#"<c r="A1"><f>B1*10</f><v>999</v></c><c r="B1"><v>7</v></c></row><row r="2"><c r="A2"><v>3</v></c></row><row r="9"><c r="B9"><v>9</v></c>"#)).unwrap();
        domain.connect(target, 1, source).unwrap();
        domain.request_calculation(&[target], false).unwrap();
        drain_domain(&mut domain, budget);
        let host = domain.host(target).unwrap();
        let sheet = host.sheets()[0].id();
        for (column, expected) in [(1, "70"), (2, "71"), (3, "73"), (5, "9")] {
            let at = CellKey::new(sheet, 1, column).unwrap();
            assert_eq!(shown_value(host, at), expected);
            let page = project(host, &ProjectionRequest { cells: vec![at] }).unwrap();
            let ProjectedValue::Calculated { calculation, .. } = page.cells()[0].value() else {
                panic!("not calculated");
            };
            let stamp = calculation.stamp().unwrap();
            assert!(!stamp.reads.external_cache);
            let pin = stamp.reads.linked_inputs.as_ref().unwrap();
            assert_eq!(
                pin.sources[0].workbook(),
                domain.host(source).unwrap().linked_snapshot().workbook()
            );
        }
        assert_eq!(
            shown_value(host, CellKey::new(sheet, 1, 4).unwrap()),
            "Ref (external link)"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_ylr__unavailable_source_results_keep_the_labelled_fallback() {
    let mut target = source_sheet(r#"<c r="A1"><f>[1]Sheet1!A1</f></c><c r="B1"><f>A1+1</f></c>"#);
    let mut source = source_sheet(r#"<c r="A1"><f>NOW()</f><v>111</v></c>"#);
    // A file cache with the matching number is never sufficient.
    target
        .set_linked_workbook(1, Some(source.linked_snapshot()))
        .unwrap();
    calc_host::calculate(&mut target);
    while matches!(
        step(&mut target, WorkBudget::UNLIMITED, &mut Unobserved),
        HostProgress::Calculating { .. } | HostProgress::Published(_)
    ) {}
    let sheet = target.sheets()[0].id();
    assert_eq!(
        shown_value(&target, CellKey::new(sheet, 1, 1).unwrap()),
        "111 (external link)"
    );
    calc_host::calculate(&mut source);
    while matches!(
        step(&mut source, WorkBudget::UNLIMITED, &mut Unobserved),
        HostProgress::Calculating { .. } | HostProgress::Published(_)
    ) {}
    target
        .set_linked_workbook(1, Some(source.linked_snapshot()))
        .unwrap();
    calc_host::calculate(&mut target);
    while matches!(
        step(&mut target, WorkBudget::UNLIMITED, &mut Unobserved),
        HostProgress::Calculating { .. } | HostProgress::Published(_)
    ) {}
    assert_eq!(
        shown_value(&target, CellKey::new(sheet, 1, 2).unwrap()),
        "112 (external link)"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_ylr__fallback_qualification_survives_in_cell_suspension() {
    // A runtime kernel must follow the read; a constant expression is folded away.
    let mut target = source_sheet(
        r#"<c r="A1"><f>[1]Sheet1!A1+SUM(_xlfn.SEQUENCE(8192))</f></c><c r="B1"><f>A1+1</f></c>"#,
    );
    let source = source_sheet(r#"<c r="A1"><f>NOW()</f><v>111</v></c>"#);
    target
        .set_linked_workbook(1, Some(source.linked_snapshot()))
        .unwrap();
    calc_host::calculate(&mut target);
    let mut slices = 0;
    loop {
        match step(&mut target, WorkBudget::evaluations(1), &mut Unobserved) {
            HostProgress::Calculating { .. } | HostProgress::Published(_) => slices += 1,
            other => {
                assert!(matches!(
                    other,
                    HostProgress::Manual | HostProgress::Current
                ));
                break;
            }
        }
    }
    assert!(
        slices > 3,
        "the external read must precede later continuation slices"
    );
    let sheet = target.sheets()[0].id();
    assert_eq!(
        shown_value(&target, CellKey::new(sheet, 1, 1).unwrap()),
        "33558639 (external link)"
    );
    assert_eq!(
        shown_value(&target, CellKey::new(sheet, 1, 2).unwrap()),
        "33558640 (external link)"
    );
    assert!(
        target
            .linked_snapshot()
            .calculation("Sheet1", 1, 1)
            .is_none()
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_ylr__transitive_workbooks_refresh_and_disconnect_without_reusing_old_proofs() {
    use calc_host::{CellEdit, CellInput, FiniteNumber, HostCommand};
    let mut domain = calc_host::CalculationDomain::new();
    let target = domain
        .insert(source_sheet(r#"<c r="A1"><f>[1]Sheet1!A1+1</f></c>"#))
        .unwrap();
    let middle = domain
        .insert(source_sheet(r#"<c r="A1"><f>[1]Sheet1!A1*2</f></c>"#))
        .unwrap();
    let source = domain
        .insert(source_sheet(
            r#"<c r="A1"><f>B1+1</f></c><c r="B1"><v>4</v></c>"#,
        ))
        .unwrap();
    domain.connect(target, 1, middle).unwrap();
    domain.connect(middle, 1, source).unwrap();
    domain.request_calculation(&[target], true).unwrap();
    drain_domain(&mut domain, WorkBudget::evaluations(1));
    let at = CellKey::new(domain.host(target).unwrap().sheets()[0].id(), 1, 1).unwrap();
    assert_eq!(shown_value(domain.host(target).unwrap(), at), "11");
    let host = domain.host_mut(source).unwrap();
    let input = CellKey::new(host.sheets()[0].id(), 1, 2).unwrap();
    calc_host::dispatch(
        host,
        HostCommand {
            base: host.revision(),
            edits: vec![CellEdit::Set {
                at: input,
                input: CellInput::Number(FiniteNumber::new(7.0).unwrap()),
            }],
        },
    )
    .unwrap();
    assert!(!matches!(
        project(
            domain.host(target).unwrap(),
            &ProjectionRequest { cells: vec![at] }
        )
        .unwrap()
        .cells()[0]
            .value(),
        ProjectedValue::Calculated { .. }
    ));
    let save = domain
        .begin_save(
            target,
            &calc_host::SavePolicy::default(),
            calc_host::ExcelSaveContext {
                operation: calc_host::SaveOperation::Save,
                calculate_before_save: true,
                active_sheet: at.sheet(),
                disabled_sheets: &[],
                before_save: None,
            },
        )
        .unwrap();
    drain_domain(&mut domain, WorkBudget::evaluations(1));
    assert_eq!(shown_value(domain.host(target).unwrap(), at), "17");
    let image = domain.take_save(save).unwrap();
    let mut bytes = Vec::new();
    image.write(&mut bytes).unwrap();
    let reopened = HostState::open(
        Arc::new(MemorySource::new(bytes)),
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Manual),
            ..Default::default()
        },
    )
    .unwrap();
    let page = project(
        &reopened,
        &ProjectionRequest {
            cells: vec![CellKey::new(reopened.sheets()[0].id(), 1, 1).unwrap()],
        },
    )
    .unwrap();
    assert!(
        matches!(page.cells()[0].value(),ProjectedValue::FileCached(cache) if cache.value().to_value().as_number()==Some(17.0))
    );
    domain.request_rebuild(&[target]).unwrap();
    drain_domain(&mut domain, WorkBudget::evaluations(1));
    assert_eq!(shown_value(domain.host(target).unwrap(), at), "17");
    domain.disconnect(target, 1).unwrap();
    domain.request_calculation(&[target], false).unwrap();
    drain_domain(&mut domain, WorkBudget::UNLIMITED);
    assert_eq!(
        shown_value(domain.host(target).unwrap(), at),
        "112 (external link)"
    );
    domain.connect(target, 1, middle).unwrap();
    domain.connect(middle, 1, target).unwrap();
    assert!(matches!(
        domain.request_calculation(&[target], false),
        Err(calc_host::DomainError::LinkedCycle { owner: "dc-ylr" })
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_p06_save_false_does_not_invalidate_unchanged_linked_consumers() {
    let mut domain = calc_host::CalculationDomain::new();
    let source = domain
        .insert(source_sheet(r#"<c r="A1"><f>3+4</f></c>"#))
        .unwrap();
    let target = domain
        .insert(source_sheet(r#"<c r="A1"><f>[1]Sheet1!A1+1</f></c>"#))
        .unwrap();
    domain.connect(target, 1, source).unwrap();
    domain.request_calculation(&[target], false).unwrap();
    drain_domain(&mut domain, WorkBudget::UNLIMITED);
    let at = CellKey::new(domain.host(target).unwrap().sheets()[0].id(), 1, 1).unwrap();
    let before = project(
        domain.host(target).unwrap(),
        &ProjectionRequest { cells: vec![at] },
    )
    .unwrap();
    assert!(
        matches!(before.cells()[0].value(),ProjectedValue::Calculated {value,..} if value.as_number()==Some(8.0))
    );
    let s = domain.host(source).unwrap().sheets()[0].id();
    let save = domain
        .begin_save(
            source,
            &calc_host::SavePolicy::default(),
            calc_host::ExcelSaveContext {
                operation: calc_host::SaveOperation::Save,
                calculate_before_save: false,
                active_sheet: s,
                disabled_sheets: &[],
                before_save: None,
            },
        )
        .unwrap();
    domain.take_save(save).unwrap();
    let after = project(
        domain.host(target).unwrap(),
        &ProjectionRequest { cells: vec![at] },
    )
    .unwrap();
    assert_eq!(
        before, after,
        "an unchanged Save(false) cannot remove its peer's admitted linked cut"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_p06_domain_save_refuses_a_changed_cut_without_reentering_a_live_async_prefix() {
    use std::{cell::RefCell, rc::Rc};
    let mut source = source_sheet(r#"<c r="A1"><f>NOW()</f></c>"#);
    source.set_clock(Some(1.0));
    calc_host::calculate(&mut source);
    while matches!(
        step(&mut source, WorkBudget::UNLIMITED, &mut Unobserved),
        HostProgress::Calculating { .. } | HostProgress::Published(_)
    ) {}
    let target = source_sheet(r#"<c r="A1"><f>AsyncEcho(1)+[1]Sheet1!A1</f></c>"#);
    let mut domain = calc_host::CalculationDomain::new();
    let source = domain.insert(source).unwrap();
    let target = domain.insert(target).unwrap();
    domain.connect(target, 1, source).unwrap();
    let pin = domain.host(source).unwrap().linked_snapshot();
    let counts = Rc::new(RefCell::new(async_provider::Counts::default()));
    let host = domain.host_mut(target).unwrap();
    let mut functions = calc_fml::CallableTable::new();
    functions
        .register("AsyncEcho", Rc::new(async_provider::Echo(counts.clone())))
        .unwrap();
    host.set_user_functions(functions);
    host.set_linked_workbook(1, Some(pin)).unwrap();
    calc_host::calculate(host);
    for _ in 0..1000 {
        step(host, WorkBudget::evaluations(1), &mut Unobserved);
        if !host.pending_async().is_empty() {
            break;
        }
    }
    let token = host.pending_async()[0].clone();
    let s = host.sheets()[0].id();
    assert_eq!(counts.borrow().prefix, 1);
    let save = domain
        .begin_save(
            target,
            &calc_host::SavePolicy::default(),
            calc_host::ExcelSaveContext {
                operation: calc_host::SaveOperation::Save,
                calculate_before_save: true,
                active_sheet: s,
                disabled_sheets: &[],
                before_save: None,
            },
        )
        .unwrap();
    let mut failed = false;
    for _ in 0..1000 {
        match domain.advance(WorkBudget::evaluations(1)) {
            calc_host::DomainProgress::Failed { .. } => {
                failed = true;
                break;
            }
            calc_host::DomainProgress::Complete | calc_host::DomainProgress::Waiting { .. } => {
                break;
            }
            _ => {}
        }
    }
    assert!(
        failed,
        "a changed producing source command cannot retarget the parked invocation"
    );
    assert!(domain.take_save(save).is_err());
    assert!(matches!(
        domain
            .complete_async(target, &token, calc_host::value::CalcValue::number(40.0))
            .unwrap(),
        calc_host::AsyncDisposition::Rejected(_)
    ));
    assert_eq!((counts.borrow().prefix, counts.borrow().suffix), (1, 0));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_6__name_diagnostics_never_promote_external_cache_values() {
    use calc_host::value::NameScope;
    use calc_host::{CellEdit, HostCommand, dispatch};
    let mut host = recalculated(&package());
    let sheet = host.sheets()[0].id();
    let anchor = CellKey::new(sheet, 4, 5).unwrap();
    let names = [
        ("CacheScalar", "SUM([1]Sheet1!A1)"),
        ("CacheTransitive", "$A$1+1"),
        ("CacheReference", "[1]Sheet1!A1"),
    ];
    let base = host.revision();
    dispatch(
        &mut host,
        HostCommand {
            base,
            edits: names
                .iter()
                .map(|(name, formula)| CellEdit::SetName {
                    name: (*name).into(),
                    scope: NameScope::Workbook,
                    formula: Some((*formula).into()),
                })
                .collect(),
        },
    )
    .unwrap();
    for (name, _) in names {
        let node = host
            .model()
            .definitions()
            .name_table()
            .lookup(NameScope::Workbook, name)
            .unwrap();
        let diagnostic = host
            .evaluate_name_with_evidence(node.id(), node.binding_version(), anchor)
            .unwrap();
        if name == "CacheReference" {
            assert!(
                diagnostic
                    .evaluation
                    .outcome
                    .unwrap()
                    .as_reference()
                    .is_some()
            );
            assert!(matches!(
                diagnostic.evaluation.invocations[0].result,
                calc_fml::NameResultKind::Reference { .. }
            ));
        } else {
            assert!(
                diagnostic.evaluation.outcome.is_err(),
                "{name}: cache cannot become an unqualified diagnostic value"
            );
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_6__validation_audits_refuse_cached_subjects_and_criteria() {
    use calc_host::ValidationStatus;
    let sheet_xml = SHEET.replace("</row>", r#"<c r="H1"><v>111</v></c><c r="I1"><v>111</v></c><c r="J1"><v>111</v></c></row>"#)
        .replace("</worksheet>", r#"<dataValidations count="3"><dataValidation type="whole" operator="between" allowBlank="0" sqref="A1 B1 H1"><formula1>0</formula1><formula2>200</formula2></dataValidation><dataValidation type="custom" allowBlank="0" sqref="I1"><formula1>$A$1&gt;0</formula1></dataValidation><dataValidation type="list" allowBlank="0" sqref="J1"><formula1>[1]Sheet1!A1</formula1></dataValidation></dataValidations></worksheet>"#);
    let host = recalculated(&package_with_sheet(&sheet_xml));
    let sheet = host.sheets()[0].id();
    for column in [1, 2, 9, 10] {
        assert!(
            matches!(
                host.validation_status(CellKey::new(sheet, 1, column).unwrap()),
                Some(ValidationStatus::Unavailable { .. })
            ),
            "column {column}"
        );
    }
    assert_eq!(
        host.validation_status(CellKey::new(sheet, 1, 8).unwrap()),
        Some(ValidationStatus::Valid)
    );
    let scan = host.scan_validation(sheet);
    assert_eq!((scan.checked, scan.valid, scan.unavailable), (5, 1, 4));
    assert!(scan.invalid.is_empty());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_6__conditional_colour_filter_refuses_external_cache_decisions_atomically() {
    use calc_host::value::GridArea;
    use calc_host::{ConditionalChange, FilterChange, HostSession, SkinIntent, WorkbookCommand};
    let sheet_xml = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1"><f>[1]Sheet1!A1</f></c><c r="H1" t="inlineStr"><is><t>Key</t></is></c></row><row r="2"><c r="H2"><v>1</v></c></row><row r="3"><c r="H3"><v>2</v></c></row></sheetData></worksheet>"#
    );
    let mut book = HostSession::open(
        Arc::new(MemorySource::new(package_with_sheet(&sheet_xml))),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    assert!(book.dispatch(SkinIntent::Recalculate).accepted());
    let sheet = book.host().sheets()[0].id();
    let condition = WorkbookCommand::Conditional {
        sheet,
        revision: book.host().revision().get(),
        index: None,
        change: ConditionalChange::Condition {
            kind: "expression".into(),
            applies: "H2:H3".into(),
            color: "#ff0000".into(),
            formulas: vec!["$A$1>0".into()],
            operator: String::new(),
            rank: 10,
            percent: false,
            stop: false,
        },
    };
    assert!(book.dispatch(SkinIntent::Command(condition)).accepted());
    let first = CellKey::new(sheet, 1, 8).unwrap();
    let last = CellKey::new(sheet, 3, 8).unwrap();
    let enable = WorkbookCommand::AutoFilter {
        sheet,
        revision: book.host().revision().get(),
        change: FilterChange::Enable {
            area: GridArea::new(first, last).unwrap(),
        },
    };
    assert!(book.dispatch(SkinIntent::Command(enable)).accepted());
    assert!(
        book.host()
            .conditional_formats(sheet)
            .look(book.host(), last)
            .is_some(),
        "historical painting keeps its explicit display policy"
    );
    let revision = book.host().revision();
    let history = book.history_depth();
    let styles = book.host().styles().clone();
    let filter = book
        .host()
        .model()
        .definitions()
        .auto_filter(sheet)
        .cloned();
    let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::AutoFilter {
        sheet,
        revision: revision.get(),
        change: FilterChange::Color {
            column: 8,
            cell_color: true,
            color: "#ff0000".into(),
        },
    }));
    assert!(!receipt.accepted(), "{receipt:?}");
    assert_eq!(book.host().revision(), revision);
    assert_eq!(book.history_depth(), history);
    assert_eq!(book.host().styles(), &styles);
    assert_eq!(
        book.host().model().definitions().auto_filter(sheet),
        filter.as_ref()
    );
}
