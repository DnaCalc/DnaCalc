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
    // - overlap-nonblank: Excel with alerts off collapses the pivot to its corner cell; DNA Calc
    //   refuses before changing anything.
    assert_eq!(
        other,
        [
            ("add-row", "11 differences".to_owned()),
            ("overlap-nonblank", "P".to_owned()),
        ]
    );
}
