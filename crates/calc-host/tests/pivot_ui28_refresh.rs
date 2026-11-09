//! Pivot refresh scenarios against Excel 16.0.20527 (UI28, tests/fixtures/ui28/scenarios):
//! each `<name>.xlsx` is the workbook Excel saved before the change, `<name>.edits` the Source
//! cells that differ in the workbook Excel saved after refreshing, and `<name>.expected` every
//! cell Excel held on the pivot sheets then. DNA Calc applies the same edits, refreshes every
//! pivot on the other sheets and is compared cell for cell. A refusal counts as a failure.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_host::value::{CellKey, CoreRef, SheetId};
use calc_host::{
    HostOptions, HostSession, MemorySource, OpenPolicy, PackageRead, ProjectedValue,
    ProjectionRequest, SavePolicy, SkinIntent, project,
};
use std::collections::BTreeMap;
use std::sync::Arc;

macro_rules! scenario {
    ($name:literal) => {
        (
            $name,
            include_bytes!(concat!("fixtures/ui28/scenarios/", $name, ".xlsx")).as_slice(),
            include_str!(concat!("fixtures/ui28/scenarios/", $name, ".edits")),
            include_str!(concat!("fixtures/ui28/scenarios/", $name, ".expected")),
        )
    };
}
const SCENARIOS: &[(&str, &[u8], &str, &str)] = &[
    scenario!("add-category"),
    scenario!("add-row"),
    scenario!("blank-and-error"),
    scenario!("format-shrink"),
    scenario!("grow-shift"),
    scenario!("manual-pending"),
    scenario!("overlap-nonblank"),
    scenario!("remove-category"),
    scenario!("source-sort"),
];

fn at(sheet: SheetId, address: &str) -> CellKey {
    let split = address.find(|c: char| c.is_ascii_digit()).unwrap();
    let (letters, digits) = address.split_at(split);
    let column = letters
        .bytes()
        .fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1));
    CellKey::new(sheet, digits.parse().unwrap(), column).unwrap()
}
fn address(column: u32, row: u32) -> String {
    let mut letters = String::new();
    let mut n = column;
    while n > 0 {
        letters.insert(0, char::from(b'A' + ((n - 1) % 26) as u8));
        n = (n - 1) / 26;
    }
    format!("{letters}{row}")
}
#[derive(Debug, PartialEq)]
enum Cell {
    Number(f64),
    Text(String),
    Error(String),
}
fn run(bytes: &[u8], edits: &str, expected: &str) -> String {
    run_session(bytes, edits, expected).0
}
fn run_session(bytes: &[u8], edits: &str, expected: &str) -> (String, Option<HostSession>) {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default())
        .expect("it opens");
    assert!(book.dispatch(SkinIntent::Recalculate).accepted());
    let sheets: Vec<_> = book
        .host()
        .sheets()
        .iter()
        .map(|s| (s.id(), String::from_utf16_lossy(s.name().text().utf16_code_units())))
        .collect();
    let source_sheet = sheets.iter().find(|s| s.1 == "Source").unwrap().0;
    for line in edits.lines().filter(|l| !l.is_empty()) {
        let f: Vec<_> = line.splitn(3, '\t').collect();
        let text = if f[1] == "clear" { "" } else { f[2] };
        let receipt = book.dispatch(SkinIntent::EnterCell {
            at: at(source_sheet, f[0]),
            text: text.into(),
        });
        if !receipt.accepted() {
            return (format!("edit {line} refused: {receipt:?}"), None);
        }
    }
    let mut want: BTreeMap<String, BTreeMap<String, Cell>> = BTreeMap::new();
    for line in expected.lines().filter(|l| !l.is_empty()) {
        let f: Vec<_> = line.splitn(4, '\t').collect();
        let cell = match f[2] {
            "n" => Cell::Number(f[3].parse().unwrap()),
            "e" => Cell::Error(f[3].to_owned()),
            _ => Cell::Text(f[3].to_owned()),
        };
        want.entry(f[0].to_owned()).or_default().insert(f[1].to_owned(), cell);
    }
    for (sheet, name) in &sheets {
        if name == "Source" {
            continue;
        }
        let parts: Vec<String> = book
            .host()
            .model()
            .definitions()
            .pivots()
            .sheets
            .get(sheet)
            .map(|p| p.tables.iter().map(|t| t.part.clone()).collect())
            .unwrap_or_default();
        for part in parts {
            let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
                sheet: *sheet,
                revision: book.host().revision().get(),
                part,
            }));
            if !receipt.accepted() {
                return (format!("{name}: refused: {:?}", receipt.error), None);
            }
        }
    }
    let mut differences = Vec::new();
    for (sheet, name) in &sheets {
        if name == "Source" {
            continue;
        }
        let mut cells = Vec::new();
        for r in 1..=120 {
            for c in 1..=12 {
                cells.push(CellKey::new(*sheet, r, c).unwrap());
            }
        }
        let page = project(book.host(), &ProjectionRequest { cells: cells.clone() }).unwrap();
        let mut got = BTreeMap::new();
        for (key, cell) in cells.iter().zip(page.cells()) {
            let value = match cell.value() {
                ProjectedValue::Literal(v) => v.clone(),
                ProjectedValue::Calculated { value, .. } => value.clone(),
                _ => continue,
            };
            let at = address(key.column(), key.row());
            match value.core_ref() {
                CoreRef::Number(n) => {
                    got.insert(at, Cell::Number(n));
                }
                CoreRef::Text(t) => {
                    got.insert(at, Cell::Text(String::from_utf16_lossy(t.utf16_code_units())));
                }
                CoreRef::Error(e) => {
                    got.insert(at, Cell::Error(e.excel_spelling().into()));
                }
                _ => {}
            }
        }
        let none = BTreeMap::new();
        let want = want.get(name).unwrap_or(&none);
        for (k, v) in want {
            if got.get(k) != Some(v) {
                differences.push(format!("{name}!{k}: want {v:?} got {:?}", got.get(k)));
            }
        }
        for (k, v) in &got {
            if !want.contains_key(k) {
                differences.push(format!("{name}!{k}: unexpected {v:?}"));
            }
        }
    }
    let verdict = if differences.is_empty() {
        "matched".into()
    } else {
        format!("{} differences: {}", differences.len(), differences.iter().take(4).cloned().collect::<Vec<_>>().join("; "))
    };
    (verdict, Some(book))
}

fn part_text(package: &[u8], name: &str) -> String {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(package.to_vec()));
    let document = calc_doc::open(source, &OpenPolicy::default()).expect("the package opens");
    String::from_utf8(document.package().read_part(name).expect("the part")).unwrap()
}
fn formats_element(xml: &str) -> &str {
    let start = xml.find("<formats").expect("a formats element");
    &xml[start..xml.find("</formats>").unwrap() + "</formats>".len()]
}

#[test]
/// Native-only: opens, refreshes and saves a workbook.
fn ui28__refresh_keeps_the_pivot_formats_element_as_excel_does() {
    // format-shrink: Excel kept the formats element byte for byte after a refresh that removed
    // rows. DNA Calc keeps it too; it does not restyle the refreshed output cells.
    let (_, bytes, edits, expected) = SCENARIOS.iter().find(|s| s.0 == "format-shrink").unwrap();
    let (verdict, book) = run_session(bytes, edits, expected);
    assert_eq!(verdict, "matched");
    let mut book = book.unwrap();
    assert!(book.dispatch(SkinIntent::Recalculate).accepted());
    let mut saved = Vec::new();
    book.save(&SavePolicy::default(), &mut saved).expect("it saves");
    let part = "xl/pivotTables/pivotTable1.xml";
    assert_eq!(
        formats_element(&part_text(&saved, part)),
        formats_element(&part_text(bytes, part))
    );
}

#[test]
/// Native-only: the report prints every scenario; the matching ones and the others are asserted.
fn ui28__pivot_refresh_scenarios_against_excel() {
    let (mut matched, mut other) = (Vec::new(), Vec::new());
    for (name, bytes, edits, expected) in SCENARIOS {
        let verdict = run(bytes, edits, expected);
        println!("{name}: {}", &verdict[..verdict.len().min(500)]);
        if verdict == "matched" {
            matched.push(*name);
        } else {
            other.push((*name, verdict.split(':').next().unwrap().to_owned()));
        }
    }
    println!("matched {} of {}: {matched:?}", matched.len(), SCENARIOS.len());
    assert_eq!(
        matched,
        [
            "add-category",
            "blank-and-error",
            "format-shrink",
            "grow-shift",
            "manual-pending",
            "remove-category",
            "source-sort"
        ]
    );
    // Not matched, and why:
    // - add-row: Excel moves a range source that spans a table to the grown table (A1:E37 became
    //   A1:E38); DNA Calc does not grow a table when a cell is entered below it.
    // - overlap-nonblank: Excel collapses the pivot to its corner cell and so does DNA Calc now
    //   (dc-8wp.16, tests/pivot_m5.rs); the one difference is the corner's error: this capture
    //   holds #VALUE! (read from the saved file), the live Value2 in the m5 capture is code 2045,
    //   #SPILL!, which DNA Calc shows.
    assert_eq!(
        other,
        [
            ("add-row", "11 differences".to_owned()),
            ("overlap-nonblank", "1 differences".to_owned()),
        ]
    );
}

#[test]
/// Native-only: verifies structural edits and an XLSX save of a workbook with pivots.
fn dc_8wp_16__unrelated_sheet_axes_leave_pivot_sources_and_outputs_intact() {
    let bytes = include_bytes!("fixtures/pivot_getpivotdata/oracle.xlsx");
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let sheet = |book: &HostSession, name: &str| book.host().sheets().iter()
        .find(|s| s.name().text().to_string_lossy() == name).unwrap().id();
    let source = sheet(&book, "Source");
    let output = sheet(&book, "PvOne");
    let unrelated = sheet(&book, "GPD");
    let axis = |sheet| calc_host::AxisEdit::new(sheet, calc_host::Axis::Rows, calc_host::AxisChange::Insert, 200, 1).unwrap();
    let before = book.host().revision();
    // dc-8wp.16 phase two: an edit after every pivot output and source is placed (nothing moves); one inside
    // a pivot output is refused with Excel's text and leaves the workbook as it was.
    let inside = calc_host::AxisEdit::new(output, calc_host::Axis::Rows, calc_host::AxisChange::Insert, 5, 1).unwrap();
    let refused = book.dispatch(SkinIntent::EditAxis(inside));
    assert!(!refused.accepted());
    assert!(format!("{refused:?}").contains("will affect a PivotTable"), "{refused:?}");
    assert_eq!(book.host().revision(), before);
    assert!(book.dispatch(SkinIntent::EditAxis(axis(source))).accepted());
    assert!(book.dispatch(SkinIntent::EditAxis(axis(output))).accepted());
    assert!(book.dispatch(SkinIntent::EditAxis(axis(unrelated))).accepted());
    let rename = |sheet, text: &str| SkinIntent::Edit { edits: vec![calc_host::CellEdit::Sheet(
        calc_doc_model::SheetEdit::Rename { sheet, name: calc_doc_model::SheetName::new(
            calc_host::value::ExcelText::from_interop_assignment(text)).unwrap() })] };
    assert!(!book.dispatch(rename(source, "Source2")).accepted());
    assert!(!book.dispatch(rename(output, "Pivot2")).accepted());
    let renamed = book.dispatch(rename(unrelated, "Queries"));
    assert!(!renamed.accepted(), "the workbook has other retained table references: {renamed:?}");
    let mut saved = Vec::new();
    book.save(&SavePolicy::default(), &mut saved).unwrap();
    let reopened: Arc<dyn PackageRead> = Arc::new(MemorySource::new(saved));
    let mut book = HostSession::open(reopened, &OpenPolicy::default(), HostOptions::default()).unwrap();
    assert!(book.dispatch(SkinIntent::Recalculate).accepted());
    let result = project(book.host(), &ProjectionRequest { cells: vec![at(unrelated, "B43")] }).unwrap();
    assert!(matches!(result.cells()[0].value(), ProjectedValue::Calculated { value, .. }
        if matches!(value.core_ref(), CoreRef::Number(n) if n == 691.5)));
}

#[test]
/// Native-only: drives layout edits using a saved workbook and its retained cache records.
fn dc_8wp_15__layout_uses_saved_records_and_survives_reopen() {
    let bytes = include_bytes!("fixtures/pivot_getpivotdata/oracle.xlsx");
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let sheet = |book: &HostSession, name: &str| book.host().sheets().iter()
        .find(|s| s.name().text().to_string_lossy() == name).unwrap().id();
    let source = sheet(&book, "Source");
    let pivot = sheet(&book, "PvRowCol");
    let result_sheet = sheet(&book, "GPD");
    let part = book.host().model().definitions().pivots().sheets[&pivot].tables[0].part.clone();
    assert!(book.dispatch(SkinIntent::EnterCell { at: at(source, "D2"), text: "999999".into() }).accepted());
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotSetLayout {
        sheet: pivot, revision: book.host().revision().get(), part: part.clone(), rows: vec![1], columns: vec![0],
    }));
    assert!(receipt.accepted(), "{receipt:?}");
    let state = book.host().model().definitions().pivots();
    let table = state.sheet(pivot).unwrap().tables.iter().find(|t| t.part == part).unwrap();
    assert_eq!(table.row_fields, vec![1]);
    assert_eq!(table.column_fields, vec![0]);
    assert!(matches!(&state.caches.caches[&table.cache_id].snapshot.as_ref().unwrap().origin,
        calc_doc_model::pivot::PivotSnapshotOrigin::FileRecords { .. }));
    let verify = |book: &HostSession| {
        let result = project(book.host(), &ProjectionRequest { cells: vec![at(result_sheet, "B43")] }).unwrap();
        assert!(matches!(result.cells()[0].value(), ProjectedValue::Calculated { value, .. }
            if matches!(value.core_ref(), CoreRef::Number(n) if n == 691.5)), "{:?}", result.cells()[0].value());
    };
    verify(&book);
    let revision = book.host().revision();
    let rejected = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotSetLayout {
        sheet: pivot, revision: revision.get(), part: part.clone(), rows: vec![1], columns: vec![1],
    }));
    assert!(!rejected.accepted());
    assert_eq!(book.host().revision(), revision);
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(book.host().model().definitions().pivots().sheet(pivot).unwrap().tables[0].row_fields, vec![0]);
    assert!(book.dispatch(SkinIntent::Redo).accepted());
    verify(&book);
    let mut saved = Vec::new();
    book.save(&SavePolicy::default(), &mut saved).unwrap();
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(saved));
    let mut reopened = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    assert!(reopened.dispatch(SkinIntent::Recalculate).accepted());
    let table = &reopened.host().model().definitions().pivots().sheets[&pivot].tables[0];
    assert_eq!((&table.row_fields, &table.column_fields), (&vec![1], &vec![0]));
    verify(&reopened);
}

#[test]
/// Native-only: creates a two-axis pivot from workbook cells and writes the XLSX package.
fn dc_8wp_15__create_two_axes_through_the_same_walk() {
    let bytes = include_bytes!("fixtures/pivot_getpivotdata/oracle.xlsx");
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let sheet = |book: &HostSession, name: &str| book.host().sheets().iter()
        .find(|s| s.name().text().to_string_lossy() == name).unwrap().id();
    let source = sheet(&book, "Source");
    let destination = sheet(&book, "GPD");
    let range = calc_host::value::GridArea::new(at(source, "A1"), at(source, "E36")).unwrap();
    let revision = book.host().revision().get();
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotCreateLayout {
        revision, source: range, destination: "GPD!H150".into(), rows: vec![0], columns: vec![1], values: vec![(3,"sum".into()), (4,"sum".into())],
    }));
    assert!(receipt.accepted(), "{receipt:?}");
    let state = book.host().model().definitions().pivots();
    let table = state.sheet(destination).unwrap().tables.iter().find(|t| t.area.first() == at(destination, "H150")).unwrap();
    assert_eq!((&table.row_fields, &table.column_fields), (&vec![0], &vec![1, -2]));
    let part = table.part.clone();
    assert!(book.dispatch(SkinIntent::EnterCell { at: at(destination,"H140"),
        text: "=GETPIVOTDATA(\"Sum of Amount\",H150,\"Region\",\"East\",\"Product\",\"Alpha\")".into() }).accepted());
    let observed = |book: &HostSession| {
        let result = project(book.host(), &ProjectionRequest { cells: vec![at(destination,"H140")] }).unwrap();
        match result.cells()[0].value() {
            ProjectedValue::Calculated { value, .. } => Some(value.clone()), _ => None,
        }
    };
    let before = observed(&book);
    assert!(matches!(before.as_ref().map(|value| value.core_ref()), Some(CoreRef::Number(n)) if n == 691.5), "{before:?}");
    assert!(book.dispatch(SkinIntent::EnterCell { at: at(destination,"H141"),
        text: "=GETPIVOTDATA(\"Sum of Units\",H150,\"Region\",\"East\",\"Product\",\"Alpha\")".into() }).accepted());
    let units = |book: &HostSession| {
        let result = project(book.host(), &ProjectionRequest { cells: vec![at(destination,"H141")] }).unwrap();
        match result.cells()[0].value() { ProjectedValue::Calculated { value, .. } => Some(value.clone()), _ => None }
    };
    assert!(matches!(units(&book).as_ref().map(|value| value.core_ref()), Some(CoreRef::Number(n)) if n == 7.0));
    let moved = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotSetLayout {
        sheet: destination, revision: book.host().revision().get(), part: part.clone(), rows: vec![0, -2], columns: vec![1],
    }));
    assert!(moved.accepted(), "{moved:?}");
    assert_eq!(observed(&book), before);
    assert!(matches!(units(&book).as_ref().map(|value| value.core_ref()), Some(CoreRef::Number(n)) if n == 7.0));
    let mut saved = Vec::new(); book.save(&SavePolicy::default(), &mut saved).unwrap();
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(saved));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    assert!(book.dispatch(SkinIntent::Recalculate).accepted());
    assert_eq!(observed(&book), before);
    assert!(matches!(units(&book).as_ref().map(|value| value.core_ref()), Some(CoreRef::Number(n)) if n == 7.0));
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
        sheet: destination, revision: book.host().revision().get(), part,
    }));
    assert!(receipt.accepted(), "{receipt:?}");
    assert_eq!(observed(&book), before);
}

#[test]
/// Native-only: adds a column axis to a saved row-only pivot and reopens the authored part.
fn dc_8wp_15__add_a_column_axis_to_a_saved_pivot() {
    let bytes = include_bytes!("fixtures/pivot_getpivotdata/oracle.xlsx");
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let pivot = book.host().sheets().iter().find(|s| s.name().text().to_string_lossy() == "PvOne").unwrap().id();
    let part = book.host().model().definitions().pivots().sheets[&pivot].tables[0].part.clone();
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotSetLayout {
        sheet: pivot, revision: book.host().revision().get(), part: part.clone(), rows: vec![0], columns: vec![1],
    }));
    assert!(receipt.accepted(), "{receipt:?}");
    assert!(book.dispatch(SkinIntent::EnterCell { at: at(pivot, "Z1"),
        text: "=GETPIVOTDATA(\"Sum of Amount\",A3,\"Region\",\"East\",\"Product\",\"Alpha\")".into() }).accepted());
    let verify = |book: &HostSession| {
        let page = project(book.host(), &ProjectionRequest { cells: vec![at(pivot, "Z1")] }).unwrap();
        assert!(matches!(page.cells()[0].value(), ProjectedValue::Calculated { value, .. }
            if matches!(value.core_ref(), CoreRef::Number(n) if n == 691.5)), "{:?}", page.cells()[0].value());
    };
    verify(&book);
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotSetLayout {
        sheet: pivot, revision: book.host().revision().get(), part: part.clone(), rows: vec![0], columns: vec![],
    }));
    assert!(receipt.accepted(), "{receipt:?}");
    let page = project(book.host(), &ProjectionRequest { cells: vec![at(pivot, "Z1")] }).unwrap();
    assert!(matches!(page.cells()[0].value(), ProjectedValue::Calculated { value, .. }
        if matches!(value.core_ref(), CoreRef::Error(calc_host::value::WorksheetErrorCode::Ref))));
    let mut row_only = Vec::new(); book.save(&SavePolicy::default(), &mut row_only).unwrap();
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(row_only));
    book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    assert!(book.host().model().definitions().pivots().sheet(pivot).unwrap().tables[0].column_fields.is_empty());
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotSetLayout {
        sheet: pivot, revision: book.host().revision().get(), part, rows: vec![0], columns: vec![1],
    }));
    assert!(receipt.accepted(), "{receipt:?}");
    verify(&book);
    let mut saved = Vec::new(); book.save(&SavePolicy::default(), &mut saved).unwrap();
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(saved));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    assert!(book.dispatch(SkinIntent::Recalculate).accepted());
    assert_eq!(book.host().model().definitions().pivots().sheet(pivot).unwrap().tables[0].column_fields, vec![1]);
    verify(&book);
}

/// Every number, text and error DNA Calc holds on the sheets named, as `sheet<TAB>address<TAB>kind<TAB>value`.
fn dump_cells(book: &HostSession, names: &[&str]) -> Vec<String> {
    let mut lines = Vec::new();
    for sheet in book.host().sheets().iter() {
        let name = sheet.name().text().to_string_lossy().to_string();
        if !names.contains(&name.as_str()) {
            continue;
        }
        let mut cells = Vec::new();
        for r in 1..=80 {
            for c in 1..=14 {
                cells.push(CellKey::new(sheet.id(), r, c).unwrap());
            }
        }
        let page = project(book.host(), &ProjectionRequest { cells: cells.clone() }).unwrap();
        for (key, cell) in cells.iter().zip(page.cells()) {
            let value = match cell.value() {
                ProjectedValue::Literal(v) => v.clone(),
                ProjectedValue::Calculated { value, .. } => value.clone(),
                _ => continue,
            };
            let at = address(key.column(), key.row());
            match value.core_ref() {
                CoreRef::Number(n) => lines.push(format!("{name}\t{at}\tn\t{n}")),
                CoreRef::Text(t) => lines.push(format!("{name}\t{at}\ts\t{}", String::from_utf16_lossy(t.utf16_code_units()))),
                CoreRef::Error(e) => lines.push(format!("{name}\t{at}\te\t{}", e.excel_spelling())),
                _ => {}
            }
        }
    }
    lines
}

#[test]
/// Native-only: saves the workbooks DNA Calc creates and edits pivots in; when DNACALC_PIVOT_SAVES
/// names a folder it keeps them there with the values DNA Calc holds (for the Excel replay in
/// tools/excel-session/pivot-job.ps1 kind `open`), and always checks each save reopens to the same cells.
fn dc_8wp_15__saves_for_the_excel_replay_reopen_to_the_same_cells() {
    type Saved = Vec<(&'static str, Vec<u8>, Vec<String>)>;
    let bytes = include_bytes!("fixtures/pivot_getpivotdata/oracle.xlsx");
    let open = |bytes: Vec<u8>| {
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
        HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap()
    };
    let sheet = |book: &HostSession, name: &str| book.host().sheets().iter()
        .find(|s| s.name().text().to_string_lossy() == name).unwrap().id();
    let part = |book: &HostSession, sheet| book.host().model().definitions().pivots().sheets[&sheet].tables[0].part.clone();
    let mut saves: Saved = Vec::new();
    let save = |name: &'static str, book: &mut HostSession, sheets: &[&str], saves: &mut Saved| {
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        let mut out = Vec::new();
        book.save(&SavePolicy::default(), &mut out).unwrap();
        saves.push((name, out, dump_cells(book, sheets)));
    };
    // Untouched round trip of the Excel-made oracle (what DNA Calc writes without any pivot edit).
    let mut book = open(bytes.to_vec());
    save("p7-roundtrip", &mut book, &["PvOne", "PvRowCol", "GPD"], &mut saves);
    // P7: a layout set from saved records after a source edit, with the axes swapped.
    let mut book = open(bytes.to_vec());
    let (source, pivot) = (sheet(&book, "Source"), sheet(&book, "PvRowCol"));
    let p = part(&book, pivot);
    assert!(book.dispatch(SkinIntent::EnterCell { at: at(source, "D2"), text: "999999".into() }).accepted());
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotSetLayout {
        sheet: pivot, revision: book.host().revision().get(), part: p, rows: vec![1], columns: vec![0],
    }));
    assert!(receipt.accepted(), "{receipt:?}");
    save("p7-layout-swapped-after-source-edit", &mut book, &["PvRowCol", "GPD"], &mut saves);
    // P7: add a column axis to a row-only pivot, and remove it again.
    let mut book = open(bytes.to_vec());
    let one = sheet(&book, "PvOne");
    let p = part(&book, one);
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotSetLayout {
        sheet: one, revision: book.host().revision().get(), part: p.clone(), rows: vec![0], columns: vec![1],
    }));
    assert!(receipt.accepted(), "{receipt:?}");
    save("p7-column-axis-added", &mut book, &["PvOne"], &mut saves);
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotSetLayout {
        sheet: one, revision: book.host().revision().get(), part: p, rows: vec![0], columns: vec![],
    }));
    assert!(receipt.accepted(), "{receipt:?}");
    save("p7-column-axis-removed", &mut book, &["PvOne"], &mut saves);
    // P7: creation with two axes and two values, and with one row field and one value.
    let creations: [(&'static str, Vec<u32>, Vec<u32>, Vec<(u32, String)>); 2] = [
        ("p7-created-two-axes-two-values", vec![0], vec![1], vec![(3, "sum".into()), (4, "sum".into())]),
        ("p7-created-one-row-one-value", vec![0], vec![], vec![(3, "sum".into())]),
    ];
    for (name, rows, columns, values) in creations {
        let mut book = open(bytes.to_vec());
        let source = sheet(&book, "Source");
        let range = calc_host::value::GridArea::new(at(source, "A1"), at(source, "E36")).unwrap();
        let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotCreateLayout {
            revision: book.host().revision().get(), source: range, destination: "GPD!H150".into(),
            rows, columns, values,
        }));
        assert!(receipt.accepted(), "{receipt:?}");
        save(name, &mut book, &["GPD"], &mut saves);
    }
    // P8 phase one: an axis edit on an unrelated sheet leaves the pivots intact.
    let mut book = open(bytes.to_vec());
    let unrelated = sheet(&book, "GPD");
    let edit = calc_host::AxisEdit::new(unrelated, calc_host::Axis::Rows, calc_host::AxisChange::Insert, 200, 1).unwrap();
    assert!(book.dispatch(SkinIntent::EditAxis(edit)).accepted());
    save("p8-unrelated-row-inserted", &mut book, &["PvOne", "PvRowCol", "GPD"], &mut saves);
    // Refresh of two pivots after a source edit (the DNA-refreshed save).
    let mut book = open(bytes.to_vec());
    let source = sheet(&book, "Source");
    assert!(book.dispatch(SkinIntent::EnterCell { at: at(source, "D2"), text: "123".into() }).accepted());
    for name in ["PvOne", "PvRowCol"] {
        let s = sheet(&book, name);
        let p = part(&book, s);
        let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
            sheet: s, revision: book.host().revision().get(), part: p,
        }));
        assert!(receipt.accepted(), "{receipt:?}");
    }
    save("refresh-after-source-edit", &mut book, &["PvOne", "PvRowCol"], &mut saves);

    let folder = std::env::var_os("DNACALC_PIVOT_SAVES").map(std::path::PathBuf::from);
    if let Some(folder) = &folder {
        std::fs::create_dir_all(folder).unwrap();
    }
    // Excel refuses a package whose shared-formula `ref` spans cells carrying other formulas; the
    // oracle's equal GETPIVOTDATA formulas in B2 and B20 must be written individually.
    for (name, package, _) in &saves {
        if *name != "p7-roundtrip" {
            assert!(!part_text(package, "xl/worksheets/sheet17.xml").contains("t=\"shared\""), "{name}");
        }
    }
    for (name, package, cells) in &saves {
        let mut reopened = open(package.clone());
        assert!(reopened.dispatch(SkinIntent::Recalculate).accepted());
        let names: Vec<&str> = cells.iter().filter_map(|l| l.split('\t').next()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        assert_eq!(&dump_cells(&reopened, &names), cells, "{name} reopens to the cells it was saved with");
        if let Some(folder) = &folder {
            std::fs::write(folder.join(format!("{name}.xlsx")), package).unwrap();
            std::fs::write(folder.join(format!("{name}.dna.tsv")), cells.join("\n") + "\n").unwrap();
        }
    }
}
