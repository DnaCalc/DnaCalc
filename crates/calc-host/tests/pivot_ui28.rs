//! Pivot refresh against Excel 16.0.20527 (UI28, tests/fixtures/ui28): `pivots-built.xlsx` is the
//! workbook Excel built with 22 pivots (sheets P1..P20, PName, PTable) over the Source sheet;
//! `pivots-refreshed.tsv` is every cell Excel held on those sheets after Source!D3 changed from
//! 107.25 to 1234.5 and each pivot was refreshed. DNA Calc applies the same edit, refreshes each
//! pivot through the host command and is compared cell for cell. A refused pivot counts as a
//! failure here as everywhere; the lists below name what is expected to match and what refuses.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

#[allow(dead_code)]
#[path = "../../calc-doc/tests/cdoc_m1_1_support/mod.rs"]
mod existing_wire_support;

use calc_host::value::{CalcValue, CellKey, CoreRef, SheetId};
use calc_host::{
    HostOptions, HostSession, MemorySource, OpenPolicy, PackageRead, ProjectedValue,
    ProjectionRequest, SavePolicy, SkinIntent, project,
};
use std::collections::BTreeMap;
use std::sync::Arc;

const WORKBOOK: &[u8] = include_bytes!("fixtures/ui28/pivots-built.xlsx");
const EXCEL: &str = include_str!("fixtures/ui28/pivots-refreshed.tsv");
const STRUCTURE: &str = include_str!("fixtures/ui28/pivots-structure.txt");

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
    Boolean(bool),
    Error(String),
}
fn expected() -> BTreeMap<String, BTreeMap<String, Cell>> {
    let mut out: BTreeMap<String, BTreeMap<String, Cell>> = BTreeMap::new();
    for line in EXCEL.lines().filter(|l| !l.is_empty()) {
        let f: Vec<_> = line.splitn(4, '\t').collect();
        let cell = if f[2] == "n" {
            Cell::Number(f[3].parse().unwrap())
        } else {
            Cell::Text(f[3].to_owned())
        };
        out.entry(f[0].to_owned()).or_default().insert(f[1].to_owned(), cell);
    }
    out
}

/// What DNA Calc holds in a pivot sheet after the refresh, read through the host projection.
fn held(book: &HostSession, sheet: SheetId, width: u32, height: u32) -> BTreeMap<String, Cell> {
    let mut cells = Vec::new();
    for r in 1..=height {
        for c in 1..=width {
            cells.push(CellKey::new(sheet, r, c).unwrap());
        }
    }
    let page = project(book.host(), &ProjectionRequest { cells: cells.clone() }).unwrap();
    let mut out = BTreeMap::new();
    for (key, cell) in cells.iter().zip(page.cells()) {
        let value: Option<CalcValue> = match cell.value() {
            ProjectedValue::Literal(v) => Some(v.clone()),
            ProjectedValue::Calculated { value, .. } => Some(value.clone()),
            _ => None,
        };
        let Some(value) = value else { continue };
        let at = address(key.column(), key.row());
        match value.core_ref() {
            CoreRef::Number(n) => {
                out.insert(at, Cell::Number(n));
            }
            CoreRef::Text(t) => {
                out.insert(at, Cell::Text(String::from_utf16_lossy(t.utf16_code_units())));
            }
            CoreRef::Logical(b) => { out.insert(at, Cell::Boolean(b)); }
            CoreRef::Error(e) => { out.insert(at, Cell::Error(e.excel_spelling().into())); }
            _ => {}
        }
    }
    out
}

/// One line per pivot sheet: `matched`, or what differs, or the refusal.
fn outcomes() -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let expected = expected();
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(WORKBOOK.to_vec()));
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
    let receipt = book.dispatch(SkinIntent::EnterCell {
        at: at(source_sheet, "D3"),
        text: "1234.5".into(),
    });
    assert!(receipt.accepted(), "{receipt:?}");
    let mut out = BTreeMap::new();
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
        let mut verdict = None;
        for part in parts {
            let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
                sheet: *sheet,
                revision: book.host().revision().get(),
                part,
            }));
            if !receipt.accepted() {
                verdict = Some(format!("refused: {receipt:?}"));
            }
        }
        let want = &expected[name];
        let verdict = verdict.unwrap_or_else(|| {
            let got = held(&book, *sheet, 60, 60);
            if &got == want {
                "matched".to_owned()
            } else {
                let diff: Vec<_> = want
                    .iter()
                    .filter(|(k, v)| got.get(*k) != Some(*v))
                    .map(|(k, v)| format!("{k}: want {v:?} got {:?}", got.get(k)))
                    .chain(got.keys().filter(|k| !want.contains_key(*k)).map(|k| format!("{k}: unexpected {:?}", got[k])))
                    .take(4)
                    .collect();
                format!("mismatched: {}", diff.join("; "))
            }
        });
        out.insert(name.clone(), verdict);
    }
    // The saved package, reopened: location, item lists, row items and column items against what
    // Excel wrote for the same refreshed pivots.
    assert!(book.dispatch(SkinIntent::Recalculate).accepted());
    let mut saved = Vec::new();
    book.save(&SavePolicy::default(), &mut saved).expect("it saves");
    if let Ok(dir) = std::env::var("DNACALC_UI30_PIVOTS") {
        std::fs::write(format!("{dir}/pivots-refreshed-by-dna.xlsx"), &saved).unwrap();
    }
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(saved));
    let reopened = HostSession::open(source, &OpenPolicy::default(), HostOptions::default())
        .expect("the saved workbook opens");
    let mut structure = BTreeMap::new();
    for line in STRUCTURE.lines().filter(|l| !l.is_empty()) {
        let name = line.split('|').next().unwrap().to_owned();
        let got = reopened
            .host()
            .model()
            .definitions()
            .pivots()
            .tables()
            .find(|t| t.name == name)
            .map(canonical);
        structure.insert(
            name,
            match got {
                Some(got) if got == line => "matched".to_owned(),
                Some(got) => format!("want {line} got {got}"),
                None => "missing".to_owned(),
            },
        );
    }
    (out, structure)
}
fn canonical(t: &calc_doc_model::pivot::PivotTable) -> String {
    let a = |items: &[calc_doc_model::pivot::PivotAxisItem]| {
        items
            .iter()
            .map(|i| {
                format!(
                    "{}/{}/{}:{}",
                    i.kind,
                    i.repeat,
                    i.data_index,
                    i.indexes.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
                )
            })
            .collect::<Vec<_>>()
            .join(";")
    };
    let fields = t
        .fields
        .iter()
        .enumerate()
        .filter(|(_, f)| !f.items.is_empty())
        .map(|(i, f)| {
            format!(
                "{i}={}",
                f.items
                    .iter()
                    .map(|it| {
                        let s = match it.index {
                            Some(x) if it.kind == "data" => x.to_string(),
                            _ => it.kind.clone(),
                        };
                        if it.hidden { format!("{s}!") } else { s }
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    let location = t
        .location
        .iter()
        .filter(|(k, _)| k.as_str() != "ref")
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{}|{}:{}|{location}|{fields}|{}|{}",
        t.name,
        address(t.area.first().column(), t.area.first().row()),
        address(t.area.last().column(), t.area.last().row()),
        a(&t.row_items),
        a(&t.col_items)
    )
}

#[test]
/// Native-only: the report test prints every outcome; the tracked lists below are asserted.
fn ui28__pivot_refresh_against_excel() {
    let (outcomes, structure) = outcomes();
    for (name, verdict) in &structure {
        println!("saved {name}: {}", &verdict[..verdict.len().min(700)]);
    }
    for (name, verdict) in &outcomes {
        println!("{name}: {}", &verdict[..verdict.len().min(300)]);
    }
    let matched: Vec<_> = outcomes
        .iter()
        .filter(|(_, v)| *v == "matched")
        .map(|(n, _)| n.as_str())
        .collect();
    println!("matched {} of {}: {matched:?}", matched.len(), outcomes.len());
    assert_eq!(matched, MUST_MATCH, "{outcomes:#?}");
    let saved: Vec<_> = structure
        .iter()
        .filter(|(_, v)| *v == "matched")
        .map(|(n, _)| n.as_str())
        .collect();
    println!("saved structure matched {} of {}: {saved:?}", saved.len(), structure.len());
    assert_eq!(saved, MUST_MATCH_SAVED, "{structure:#?}");
}
const MUST_MATCH: &[&str] = &["P1", "P10", "P11", "P12", "P13", "P14", "P15", "P16", "P17", "P18", "P19", "P2", "P20", "P3", "P4", "P5", "P6", "P7", "P8", "P9", "PName", "PTable"];
// All 22 pivot definitions saved and reopened equal Excel's.
const MUST_MATCH_SAVED: &[&str] = &["pvt_average", "pvt_count", "pvt_grand_off", "pvt_max", "pvt_min", "pvt_named", "pvt_one_row_sum", "pvt_repeat_off", "pvt_repeat_on", "pvt_report_filter", "pvt_row_col", "pvt_row_col_dates", "pvt_sort_ascending", "pvt_sort_descending", "pvt_sort_manual", "pvt_subtotal_bottom", "pvt_subtotal_top", "pvt_table", "pvt_two_data", "pvt_two_row_compact", "pvt_two_row_outline", "pvt_two_row_tabular"];

const UI30: &[u8] = include_bytes!("fixtures/ui30/pivots-ui30-excel.xlsx");

/// UI30 P8: pivots Excel built over the Source sheet of the UI28 workbook (report filters with
/// chosen items, two data fields, two column fields, three row fields in each layout with the
/// automatic subtotals on and off, a date field grouped by month and year, percent of column,
/// top 10, a values filter and three data number formats). The workbook holds Excel's cells; each
/// pivot is refreshed and must give the same cells. A refused or different pivot is a failure in
/// the report; the list below is what must match.
#[test]
/// Native-only: the report test prints every outcome; the tracked list below is asserted.
fn ui30__pivot_layouts_refresh_to_the_cells_excel_held() {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(UI30.to_vec()));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default())
        .expect("it opens");
    let sheets: Vec<_> = book
        .host()
        .sheets()
        .iter()
        .map(|s| (s.id(), String::from_utf16_lossy(s.name().text().utf16_code_units())))
        .collect();
    let mut outcomes = BTreeMap::new();
    for (sheet, name) in sheets.iter().filter(|s| s.1.starts_with('N')) {
        let tables: Vec<(String, String)> = book
            .host()
            .model()
            .definitions()
            .pivots()
            .sheets
            .get(sheet)
            .map(|p| p.tables.iter().map(|t| (t.part.clone(), t.name.clone())).collect())
            .unwrap_or_default();
        let want = held(&book, *sheet, 60, 80);
        let want_items: Vec<_> = book
            .host()
            .model()
            .definitions()
            .pivots()
            .sheets
            .get(sheet)
            .map(|p| p.tables.iter().map(|t| t.row_items.clone()).collect())
            .unwrap_or_default();
        let want_col_items: Vec<_> = book.host().model().definitions().pivots().sheets.get(sheet)
            .map(|p| p.tables.iter().map(|t| t.col_items.clone()).collect()).unwrap_or_default();
        let mut verdict = None;
        for (part, _) in &tables {
            let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
                sheet: *sheet,
                revision: book.host().revision().get(),
                part: part.clone(),
            }));
            if !receipt.accepted() {
                verdict = Some(format!("refused: {receipt:?}"));
            }
        }
        let verdict = verdict.unwrap_or_else(|| {
            let got = held(&book, *sheet, 60, 80);
            let got_items: Vec<_> = book
                .host()
                .model()
                .definitions()
                .pivots()
                .sheets
                .get(sheet)
                .map(|p| p.tables.iter().map(|t| t.row_items.clone()).collect())
                .unwrap_or_default();
            let got_col_items: Vec<_> = book.host().model().definitions().pivots().sheets.get(sheet)
                .map(|p| p.tables.iter().map(|t| t.col_items.clone()).collect()).unwrap_or_default();
            if got == want && got_items != want_items {
                format!("mismatched: row items want {want_items:?} got {got_items:?}")
            } else if got == want && tables.iter().any(|(_, name)|
                name == "pvt_two_col_fields" || name == "pvt_two_data_col") && got_col_items != want_col_items {
                format!("mismatched: column items want {want_col_items:?} got {got_col_items:?}")
            } else if got == want {
                "matched".to_owned()
            } else {
                let diff: Vec<_> = want
                    .iter()
                    .filter(|(k, v)| got.get(*k) != Some(*v))
                    .map(|(k, v)| format!("{k}: want {v:?} got {:?}", got.get(k)))
                    .chain(got.keys().filter(|k| !want.contains_key(*k)).map(|k| format!("{k}: unexpected")))
                    .take(3)
                    .collect();
                format!("mismatched: {}", diff.join("; "))
            }
        });
        let label = tables.first().map_or(name.clone(), |t| t.1.clone());
        outcomes.insert(label, verdict);
    }
    for (name, verdict) in &outcomes {
        println!("{name}: {}", &verdict[..verdict.len().min(400)]);
    }
    let matched: Vec<_> = outcomes
        .iter()
        .filter(|(_, v)| *v == "matched")
        .map(|(n, _)| n.as_str())
        .collect();
    println!("matched {} of {}: {matched:?}", matched.len(), outcomes.len());
    assert_eq!(outcomes.len(), 16, "{outcomes:#?}");
    assert_eq!(matched, UI30_MUST_MATCH, "{outcomes:#?}");
}
// Data number formats and three row fields (all layouts, cells and row items) refresh. Report
// The 16 captured layouts, including the measured date groups and filter cases, match.
const UI30_MUST_MATCH: &[&str] = &[
    "pvt_date_group_months",
    "pvt_fmt_0_00",
    "pvt_fmt_currency",
    "pvt_fmt_percent",
    "pvt_pct_of_column",
    "pvt_pg_two_filters",
    "pvt_three_rows_compact_suboff",
    "pvt_three_rows_compact_subon",
    "pvt_three_rows_outline_suboff",
    "pvt_three_rows_outline_subon",
    "pvt_three_rows_tabular_suboff",
    "pvt_three_rows_tabular_subon",
    "pvt_top10_products",
    "pvt_two_col_fields",
    "pvt_two_data_col",
    "pvt_value_filter",
];

#[test]
/// Native-only: opens, refreshes, saves and reopens the Excel workbook.
fn dc_8wp__selected_page_items_survive_save_and_refresh() {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(UI30.to_vec()));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let sheet = book.host().sheets().iter().find(|s|
        String::from_utf16_lossy(s.name().text().utf16_code_units()) == "N1").unwrap().id();
    let expected = held(&book, sheet, 60, 80);
    let (part, selections) = {
        let table = book.host().model().definitions().pivots().sheet(sheet).unwrap().tables.first().unwrap();
        (table.part.clone(), table.page_fields.clone())
    };
    assert!(selections.iter().any(|p| p.selected_item.is_some()));
    assert!(book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
        sheet, revision: book.host().revision().get(), part: part.clone(),
    })).accepted());
    assert_eq!(held(&book, sheet, 60, 80), expected);
    let mut bytes = Vec::new();
    book.save(&SavePolicy::default(), &mut bytes).unwrap();
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
    let mut reopened = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let sheet = reopened.host().sheets().iter().find(|s|
        String::from_utf16_lossy(s.name().text().utf16_code_units()) == "N1").unwrap().id();
    let table = reopened.host().model().definitions().pivots().sheet(sheet).unwrap().tables.first().unwrap();
    assert_eq!(table.page_fields, selections);
    let part = table.part.clone();
    assert!(reopened.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
        sheet, revision: reopened.host().revision().get(), part,
    })).accepted());
    assert_eq!(held(&reopened, sheet, 60, 80), expected);
}

#[test]
/// Native-only: refreshes, saves and reopens the Excel workbook.
fn dc_8wp__value_filters_survive_save_and_refresh() {
    for name in ["N12", "N13"] {
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(UI30.to_vec()));
        let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
        let sheet = book.host().sheets().iter().find(|s|
            String::from_utf16_lossy(s.name().text().utf16_code_units()) == name).unwrap().id();
        let expected = held(&book, sheet, 60, 80);
        let (part, filters) = {
            let table = book.host().model().definitions().pivots().sheet(sheet).unwrap().tables.first().unwrap();
            (table.part.clone(), table.filters.clone())
        };
        assert_eq!(filters.len(), 1);
        assert!(book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
            sheet, revision: book.host().revision().get(), part,
        })).accepted());
        assert_eq!(held(&book, sheet, 60, 80), expected);
        let mut bytes = Vec::new();
        book.save(&SavePolicy::default(), &mut bytes).unwrap();
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
        let mut reopened = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
        let sheet = reopened.host().sheets().iter().find(|s|
            String::from_utf16_lossy(s.name().text().utf16_code_units()) == name).unwrap().id();
        let table = reopened.host().model().definitions().pivots().sheet(sheet).unwrap().tables.first().unwrap();
        assert_eq!(table.filters, filters);
        let part = table.part.clone();
        assert!(reopened.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
            sheet, revision: reopened.host().revision().get(), part,
        })).accepted());
        assert_eq!(held(&reopened, sheet, 60, 80), expected);
    }
}

/// M1's saved source was edited by the final missing-items sequence. Restore the original
/// Product column before refreshing the independent captured cases.
#[test]
/// Native-only: opens the Excel workbook and refreshes through the host command.
fn dc_8wp__m1_calculation_stages_against_excel() {
    const M1: &[u8] = include_bytes!("fixtures/pivot/m1-calculation/built.xlsx");
    const CELLS: &str = include_str!("fixtures/pivot/m1-calculation/cells.tsv");
    let mut expected: BTreeMap<String, BTreeMap<String, Cell>> = BTreeMap::new();
    for line in CELLS.lines().filter(|l| !l.is_empty()) {
        let f: Vec<_> = line.splitn(4, '\t').collect();
        let value = match f[2] {
            "n" => Cell::Number(f[3].parse().unwrap()),
            "s" => Cell::Text(f[3].to_owned()),
            "b" => Cell::Boolean(f[3].eq_ignore_ascii_case("true") || f[3] == "1"),
            "e" => Cell::Error(match f[3] {
                "-2146826281" => "#DIV/0!", "-2146826246" => "#N/A",
                "-2146826259" => "#NAME?", "-2146826288" => "#NULL!",
                "-2146826252" => "#NUM!", "-2146826265" => "#REF!",
                "-2146826273" => "#VALUE!", other => panic!("unknown Excel error {other}"),
            }.to_owned()),
            other => panic!("unknown captured cell type {other}"),
        };
        expected.entry(f[0].to_owned()).or_default().insert(f[1].to_owned(), value);
    }
    assert_eq!(expected.len(), 85);
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(M1.to_vec()));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let sheets: Vec<_> = book.host().sheets().iter().map(|s|
        (s.id(), String::from_utf16_lossy(s.name().text().utf16_code_units()))).collect();
    let source_sheet = sheets.iter().find(|(_, name)| name == "Source").unwrap().0;
    let original_source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(WORKBOOK.to_vec()));
    let original = HostSession::open(original_source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let original_sheet = original.host().sheets().iter().find(|s|
        String::from_utf16_lossy(s.name().text().utf16_code_units()) == "Source").unwrap().id();
    let original_cells = held(&original, original_sheet, 5, 37);
    for row in 2..=37 {
        let address = format!("B{row}");
        let Cell::Text(text) = &original_cells[&address] else { panic!("non-text original Product at {address}") };
        assert!(book.dispatch(SkinIntent::EnterCell {
            at: at(source_sheet, &address), text: text.clone().into(),
        }).accepted());
    }
    assert!(book.dispatch(SkinIntent::Recalculate).accepted());
    // Replay the original source, a removal, and the captured zero-retention sequence.
    let (missing_sheet, missing_part) = sheets.iter().find_map(|(sheet, _)| {
        book.host().model().definitions().pivots().sheets.get(sheet)
            .and_then(|p| p.tables.iter().find(|t| t.name == "pvt_missing_items"))
            .map(|t| (*sheet, t.part.clone()))
    }).unwrap();
    let mut missing_verdicts = BTreeMap::new();
    for stage in ["missing-items-built", "missing-items-refresh-default", "missing-items-refresh-none", "missing-items-refresh-added", "missing-items-delta-removed-auto", "missing-items-max-limit"] {
        if stage == "missing-items-refresh-default" {
            for row in 2..=37 {
                let address = format!("B{row}");
                if original_cells.get(&address) == Some(&Cell::Text("Gamma".into())) {
                    assert!(book.dispatch(SkinIntent::EnterCell {
                        at: at(source_sheet, &address), text: "Alpha".into(),
                    }).accepted());
                }
            }
        }
        if stage == "missing-items-refresh-none" {
            let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotForgetMissingItems {
                sheet: missing_sheet, revision: book.host().revision().get(), part: missing_part.clone(),
            }));
            assert!(receipt.accepted(), "{:?}", receipt.error);
        }
        if stage == "missing-items-refresh-added" {
            for (address, text) in [("B5", "Gamma"), ("B9", "Delta")] {
                assert!(book.dispatch(SkinIntent::EnterCell { at: at(source_sheet, address), text: text.into() }).accepted());
            }
        }
        if stage == "missing-items-delta-removed-auto" || stage == "missing-items-max-limit" {
            if stage == "missing-items-delta-removed-auto" {
                assert!(book.dispatch(SkinIntent::EnterCell { at: at(source_sheet, "B9"), text: "Alpha".into() }).accepted());
            }
            let limit = if stage == "missing-items-max-limit" { Some(32500) } else { None };
            let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotSetMissingItemsLimit {
                sheet: missing_sheet, revision: book.host().revision().get(), part: missing_part.clone(), limit,
            }));
            assert!(receipt.accepted(), "{stage}: {:?}", receipt.error);
        }
        let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
            sheet: missing_sheet, revision: book.host().revision().get(), part: missing_part.clone(),
        }));
        let verdict = if !receipt.accepted() { format!("refused: {:?}", receipt.error) }
            else if held(&book, missing_sheet, 60, 120) == expected[stage] { "matched".into() }
            else { "mismatched".into() };
        if verdict == "matched" {
            let pivots = book.host().model().definitions().pivots();
            let table = pivots.sheet(missing_sheet).unwrap().tables.iter().find(|table| table.name == "pvt_missing_items").unwrap();
            let field = table.row_fields[0] as usize;
            let cache_field = &pivots.caches.caches[&table.cache_id].fields[field];
            let missing: Vec<_> = table.fields[field].items.iter().filter(|item| item.missing)
                .map(|item| cache_field.items[item.index.unwrap() as usize].clone()).collect();
            let unused: Vec<_> = cache_field.unused_items.iter().map(|&index| cache_field.items[index as usize].clone()).collect();
            let expected_missing = if stage == "missing-items-built" { vec![calc_doc_model::pivot::PivotItem::Text("Delta".into())] }
                else if stage == "missing-items-refresh-default" { vec![calc_doc_model::pivot::PivotItem::Text("Gamma".into()), calc_doc_model::pivot::PivotItem::Text("Delta".into())] }
                else if stage == "missing-items-delta-removed-auto" || stage == "missing-items-max-limit" { vec![calc_doc_model::pivot::PivotItem::Text("Delta".into())] }
                else { Vec::new() };
            assert_eq!(missing, expected_missing, "{stage} retained field items");
            assert_eq!(unused, expected_missing, "{stage} unused cache items");
        }
        if matches!(stage, "missing-items-refresh-none" | "missing-items-refresh-added" | "missing-items-delta-removed-auto" | "missing-items-max-limit") {
            assert_eq!(verdict, "matched", "{stage}");
            let mut bytes = Vec::new();
            book.save(&SavePolicy::default(), &mut bytes).unwrap();
            let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
            book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
            let pivots = book.host().model().definitions().pivots();
            let table = pivots.sheet(missing_sheet).unwrap().tables.iter().find(|t| t.part == missing_part).unwrap();
            let expected_limit = match stage {
                "missing-items-delta-removed-auto" => None,
                "missing-items-max-limit" => Some("32500"),
                _ => Some("0"),
            };
            assert_eq!(pivots.caches.caches[&table.cache_id].attributes.get("missingItemsLimit").map(String::as_str), expected_limit);
            let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
                sheet: missing_sheet, revision: book.host().revision().get(), part: missing_part.clone(),
            }));
            assert!(receipt.accepted(), "{stage} after reopen: {:?}", receipt.error);
            assert_eq!(held(&book, missing_sheet, 60, 120), expected[stage], "{stage} after reopen");
        }
        missing_verdicts.insert(stage.to_owned(), verdict);
    }
    for row in 2..=37 {
        let address = format!("B{row}");
        let Cell::Text(text) = &original_cells[&address] else { unreachable!() };
        assert!(book.dispatch(SkinIntent::EnterCell {
            at: at(source_sheet, &address), text: text.clone().into(),
        }).accepted());
    }
    let mut matched = Vec::new();
    for (stage, want) in &expected {
        if let Some(verdict) = missing_verdicts.get(stage) {
            if verdict == "matched" { matched.push(stage.clone()); }
            println!("{stage}: {verdict}");
            continue;
        }
        let name = format!("pvt_{}", stage.replace('-', "_"));
        let selected = sheets.iter().find_map(|(sheet, _)| {
            book.host().model().definitions().pivots().sheets.get(sheet)
                .and_then(|s| s.tables.iter().find(|t| t.name == name))
                .map(|t| (*sheet, t.part.clone()))
        });
        let verdict = if let Some((sheet, part)) = selected {
            let saved_row_items = if matches!(stage.as_str(), "blank-line-both" | "blank-line-compact-columns" | "blank-line-outer" | "calc-field-arith" | "calc-field-if" | "calc-field-of-calc" | "calc-field-ratio" | "calc-item-diff" | "calc-item-half-sum" | "calc-item-sum" | "group-day-month" | "group-days-7" | "group-number-clean" | "group-number-units" | "group-text-items" | "nodata-rows" | "nodata-two-row-fields" | "prec-grand-only" | "show-percent-of-parent" | "show-percent-of-parent-column" | "show-percent-of-parent-product" | "show-percent-of-parent-row" | "show-percent-running-total" | "show-rank-ascending" | "show-rank-descending" | "show-two-data-fields" | "sort-by-second-data" | "sort-by-value-asc" | "sort-by-value-columns" | "sort-by-value-desc" | "sort-by-value-inner" | "sort-by-value-ties" | "subtotals-multiple-compact" | "subtotals-multiple-outline-bottom" | "subtotals-multiple-outline-top" | "subtotals-multiple-tabular" | "subtotals-multiple-two-data" | "subtotals-none" | "values-rows-first" | "values-rows-region" | "values-rows-two-fields" | "values-rows-with-columns") {
                Some(book.host().model().definitions().pivots().sheet(sheet).unwrap().tables.iter()
                    .find(|table| table.part == part).unwrap().row_items.clone())
            } else { None };
            let saved_structure = if matches!(stage.as_str(), "blank-line-compact-columns" | "calc-field-arith" | "calc-field-if" | "calc-field-of-calc" | "calc-field-ratio" | "calc-item-diff" | "calc-item-half-sum" | "calc-item-sum" | "group-day-month" | "group-days-7" | "group-number-clean" | "group-number-units" | "group-text-items" | "nodata-rows" | "nodata-two-row-fields" | "prec-grand-only" | "show-percent-of-parent" | "show-percent-of-parent-column" | "show-percent-of-parent-product" | "show-percent-of-parent-row" | "show-percent-running-total" | "show-rank-ascending" | "show-rank-descending" | "show-two-data-fields" | "sort-by-second-data" | "sort-by-value-asc" | "sort-by-value-columns" | "sort-by-value-desc" | "sort-by-value-inner" | "sort-by-value-ties" | "subtotals-multiple-compact" | "subtotals-multiple-outline-bottom" | "subtotals-multiple-outline-top" | "subtotals-multiple-tabular" | "subtotals-multiple-two-data" | "subtotals-none" | "values-rows-first" | "values-rows-region" | "values-rows-two-fields" | "values-rows-with-columns") {
                Some(canonical(book.host().model().definitions().pivots().sheet(sheet).unwrap().tables.iter()
                    .find(|table| table.part == part).unwrap()))
            } else { None };
            let saved_calculated_items = if stage.starts_with("calc-item-") {
                let pivots = book.host().model().definitions().pivots();
                let table = pivots.sheet(sheet).unwrap().tables.iter().find(|table| table.part == part).unwrap();
                Some((table.fields.clone(), pivots.caches.caches[&table.cache_id].calculated_items.clone()))
            } else { None };
            let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
                sheet, revision: book.host().revision().get(), part,
            }));
            if matches!(stage.as_str(), "subtotals-all-eleven" | "agg-two-level-columns" | "prec-all-functions") {
                assert!(!receipt.accepted(), "unmodelled calculations must refuse: {stage}");
            }
            if !receipt.accepted() {
                format!("refused: {:?}", receipt.error)
            } else {
                if let Some((expected_fields, expected_definitions)) = saved_calculated_items {
                    let pivots = book.host().model().definitions().pivots();
                    let table = pivots.sheet(sheet).unwrap().tables.iter().find(|table| table.name == name).unwrap();
                    assert_eq!(table.fields, expected_fields, "{stage} calculated item flags");
                    assert_eq!(pivots.caches.caches[&table.cache_id].calculated_items, expected_definitions, "{stage} calculated item definition");
                }
                if let Some(expected_items) = saved_row_items {
                    let table = book.host().model().definitions().pivots().sheet(sheet).unwrap().tables.iter()
                        .find(|table| table.name == name).unwrap();
                    assert_eq!(table.row_items, expected_items, "{stage} row items");
                    if let Some(expected) = saved_structure { assert_eq!(canonical(table), expected, "{stage} structure"); }
                }
                let width = want.keys().map(|address| address.bytes().take_while(u8::is_ascii_alphabetic)
                    .fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1))).max().unwrap_or(0).max(60);
                let got = held(&book, sheet, width, 120);
                if got == *want {
                    matched.push(stage.clone());
                    "matched".into()
                } else {
                    let diff = want.iter().filter(|(k, v)| got.get(*k) != Some(*v))
                        .map(|(k,v)| format!("{k}: want {v:?} got {:?}", got.get(k)))
                        .chain(got.keys().filter(|k| !want.contains_key(*k)).map(|k| format!("{k}: unexpected {:?}", got[k])))
                        .take(25).collect::<Vec<_>>().join("; ");
                    format!("mismatched: {diff}")
                }
            }
        } else {
            "not replayed: no matching independent pivot".into()
        };
        println!("{stage}: {verdict}");
    }
    println!("M1 matched {} of {}: {matched:?}", matched.len(), expected.len());
    assert_eq!(matched, ["agg-all-amount", "agg-all-units", "agg-text-field", "blank-line-both", "blank-line-compact-columns", "blank-line-outer", "calc-field-arith", "calc-field-if", "calc-field-of-calc", "calc-field-ratio", "calc-item-diff", "calc-item-half-sum", "calc-item-sum", "err-keys", "err-keys-columns", "err-keys-desc", "err-values", "err-values-display", "group-day-month", "group-days-7", "group-month", "group-month-year", "group-number-clean", "group-number-units", "group-quarter", "group-quarter-year", "group-text-items", "group-year", "group-year-columns", "missing-items-built", "missing-items-delta-removed-auto", "missing-items-max-limit", "missing-items-refresh-added", "missing-items-refresh-default", "missing-items-refresh-none", "nodata-rows", "nodata-rows-hidden", "nodata-two-row-fields", "prec-grand-only", "show-count-average", "show-difference-east", "show-difference-next", "show-difference-previous", "show-difference-product-alpha", "show-index", "show-percent-difference-east", "show-percent-difference-previous", "show-percent-of-column", "show-percent-of-parent", "show-percent-of-parent-column", "show-percent-of-parent-product", "show-percent-of-parent-row", "show-percent-of-row", "show-percent-of-total", "show-percent-running-total", "show-percentof-east", "show-percentof-previous", "show-rank-ascending", "show-rank-descending", "show-running-total", "show-running-total-product", "show-two-data-fields", "sort-by-second-data", "sort-by-value-asc", "sort-by-value-columns", "sort-by-value-desc", "sort-by-value-inner", "sort-by-value-ties", "subtotals-multiple-compact", "subtotals-multiple-outline-bottom", "subtotals-multiple-outline-top", "subtotals-multiple-tabular", "subtotals-multiple-two-data", "subtotals-none", "values-columns-after-field", "values-rows-first", "values-rows-region", "values-rows-two-fields", "values-rows-with-columns"]);
    let mut saved = Vec::new();
    book.save(&SavePolicy::default(), &mut saved).unwrap();
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(saved));
    let mut reopened = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    for name in ["pvt_blank_line_both", "pvt_blank_line_compact_columns", "pvt_blank_line_outer", "pvt_calc_field_arith", "pvt_calc_field_if", "pvt_calc_field_of_calc", "pvt_calc_field_ratio", "pvt_calc_item_diff", "pvt_calc_item_half_sum", "pvt_calc_item_sum", "pvt_group_day_month", "pvt_group_days_7", "pvt_group_number_clean", "pvt_group_number_units", "pvt_group_text_items", "pvt_nodata_rows", "pvt_nodata_two_row_fields", "pvt_prec_grand_only", "pvt_show_percent_of_parent", "pvt_show_percent_of_parent_column", "pvt_show_percent_of_parent_product", "pvt_show_percent_of_parent_row", "pvt_show_percent_running_total", "pvt_show_rank_ascending", "pvt_show_rank_descending", "pvt_show_two_data_fields", "pvt_sort_by_second_data", "pvt_sort_by_value_asc", "pvt_sort_by_value_columns", "pvt_sort_by_value_desc", "pvt_sort_by_value_inner", "pvt_sort_by_value_ties", "pvt_subtotals_multiple_compact", "pvt_subtotals_multiple_outline_bottom", "pvt_subtotals_multiple_outline_top", "pvt_subtotals_multiple_tabular", "pvt_subtotals_multiple_two_data", "pvt_subtotals_none", "pvt_values_rows_first", "pvt_values_rows_region", "pvt_values_rows_two_fields", "pvt_values_rows_with_columns"] {
        let original = book.host().model().definitions().pivots().tables().find(|table| table.name == name).unwrap();
        let restored = reopened.host().model().definitions().pivots().tables().find(|table| table.name == name).unwrap();
        assert_eq!(restored.row_items, original.row_items, "{name} after save and reopen");
        if name.starts_with("pvt_calc_item_") {
            assert_eq!(restored.fields, original.fields, "{name} calculated item flags after reopen");
            let old_cache = &book.host().model().definitions().pivots().caches.caches[&original.cache_id];
            let new_cache = &reopened.host().model().definitions().pivots().caches.caches[&restored.cache_id];
            assert_eq!(new_cache.calculated_items, old_cache.calculated_items, "{name} calculated item definition after reopen");
            assert_eq!(new_cache.fields.iter().map(|field| &field.formula_items).collect::<Vec<_>>(),
                old_cache.fields.iter().map(|field| &field.formula_items).collect::<Vec<_>>(), "{name} cache item flags after reopen");
        }
        if matches!(name, "pvt_blank_line_compact_columns" | "pvt_calc_field_arith" | "pvt_calc_field_if" | "pvt_calc_field_of_calc" | "pvt_calc_field_ratio" | "pvt_calc_item_diff" | "pvt_calc_item_half_sum" | "pvt_calc_item_sum" | "pvt_group_day_month" | "pvt_group_days_7" | "pvt_group_number_clean" | "pvt_group_number_units" | "pvt_group_text_items" | "pvt_nodata_rows" | "pvt_nodata_two_row_fields" | "pvt_prec_grand_only" | "pvt_show_percent_of_parent" | "pvt_show_percent_of_parent_column" | "pvt_show_percent_of_parent_product" | "pvt_show_percent_of_parent_row" | "pvt_show_percent_running_total" | "pvt_show_rank_ascending" | "pvt_show_rank_descending" | "pvt_show_two_data_fields" | "pvt_sort_by_second_data" | "pvt_sort_by_value_asc" | "pvt_sort_by_value_columns" | "pvt_sort_by_value_desc" | "pvt_sort_by_value_inner" | "pvt_sort_by_value_ties" | "pvt_subtotals_multiple_compact" | "pvt_subtotals_multiple_outline_bottom" | "pvt_subtotals_multiple_outline_top" | "pvt_subtotals_multiple_tabular" | "pvt_subtotals_multiple_two_data" | "pvt_subtotals_none" | "pvt_values_rows_first" | "pvt_values_rows_region" | "pvt_values_rows_two_fields" | "pvt_values_rows_with_columns") {
            assert_eq!(canonical(restored), canonical(original), "{name} structure after save and reopen");
        }
    }
    {
        let original = book.host().model().definitions().pivots();
        let restored = reopened.host().model().definitions().pivots();
        let old_table = original.tables().find(|table| table.name == "pvt_missing_items").unwrap();
        let new_table = restored.tables().find(|table| table.name == "pvt_missing_items").unwrap();
        let field = old_table.row_fields[0] as usize;
        assert_eq!(new_table.fields[field].items, old_table.fields[field].items, "retained items after reopening");
        assert_eq!(restored.caches.caches[&new_table.cache_id].fields[field].unused_items,
            original.caches.caches[&old_table.cache_id].fields[field].unused_items, "unused items after reopening");
    }
    for (name, stage) in [("pvt_calc_field_arith", "calc-field-arith"), ("pvt_calc_field_if", "calc-field-if"),
        ("pvt_calc_field_of_calc", "calc-field-of-calc"), ("pvt_calc_field_ratio", "calc-field-ratio"),
        ("pvt_calc_item_diff", "calc-item-diff"), ("pvt_calc_item_half_sum", "calc-item-half-sum"),
        ("pvt_calc_item_sum", "calc-item-sum"),
        ("pvt_group_day_month", "group-day-month"), ("pvt_group_days_7", "group-days-7"),
        ("pvt_group_text_items", "group-text-items"),
        ("pvt_group_number_clean", "group-number-clean"), ("pvt_group_number_units", "group-number-units"),
        ("pvt_nodata_rows", "nodata-rows"), ("pvt_nodata_two_row_fields", "nodata-two-row-fields"),
        ("pvt_prec_grand_only", "prec-grand-only"),
        ("pvt_show_percent_of_parent", "show-percent-of-parent"),
        ("pvt_show_percent_of_parent_column", "show-percent-of-parent-column"),
        ("pvt_show_percent_of_parent_product", "show-percent-of-parent-product"),
        ("pvt_show_percent_of_parent_row", "show-percent-of-parent-row"),
        ("pvt_show_percent_running_total", "show-percent-running-total"),
        ("pvt_show_rank_ascending", "show-rank-ascending"), ("pvt_show_rank_descending", "show-rank-descending"),
        ("pvt_show_two_data_fields", "show-two-data-fields"),
        ("pvt_sort_by_second_data", "sort-by-second-data"), ("pvt_sort_by_value_asc", "sort-by-value-asc"),
        ("pvt_sort_by_value_columns", "sort-by-value-columns"), ("pvt_sort_by_value_desc", "sort-by-value-desc"),
        ("pvt_sort_by_value_inner", "sort-by-value-inner"),
        ("pvt_sort_by_value_ties", "sort-by-value-ties"),
        ("pvt_subtotals_multiple_compact", "subtotals-multiple-compact"),
        ("pvt_subtotals_multiple_outline_bottom", "subtotals-multiple-outline-bottom"),
        ("pvt_subtotals_multiple_outline_top", "subtotals-multiple-outline-top"),
        ("pvt_subtotals_multiple_tabular", "subtotals-multiple-tabular"),
        ("pvt_subtotals_multiple_two_data", "subtotals-multiple-two-data")] {
        let (sheet, part) = reopened.host().model().definitions().pivots().sheets.iter().find_map(|(sheet, pivots)|
            pivots.tables.iter().find(|table| table.name == name).map(|table| (*sheet, table.part.clone()))).unwrap();
        assert!(reopened.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
            sheet, revision: reopened.host().revision().get(), part,
        })).accepted());
        assert_eq!(held(&reopened, sheet, 60, 120), expected[stage], "{name} after save and refresh");
    }
}

#[test]
/// Native-only: edits the captured source and refreshes through the host command.
fn dc_8wp__value_sort_ties_refuse_refresh() {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(
        include_bytes!("fixtures/pivot/m1-calculation/built.xlsx").to_vec(),
    ));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let source_sheet = book.host().sheets().iter().find(|sheet|
        String::from_utf16_lossy(sheet.name().text().utf16_code_units()) == "Source").unwrap().id();
    for row in 2..=37 {
        assert!(book.dispatch(SkinIntent::EnterCell {
            at: at(source_sheet, &format!("D{row}")), text: "0".into(),
        }).accepted());
    }
    let (sheet, part) = book.host().model().definitions().pivots().sheets.iter().find_map(|(sheet, pivots)|
        pivots.tables.iter().find(|table| table.name == "pvt_sort_by_value_desc")
            .map(|table| (*sheet, table.part.clone()))).unwrap();
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
        sheet, revision: book.host().revision().get(), part,
    }));
    assert!(!receipt.accepted(), "unmeasured tied values must refuse: {receipt:?}");
    for name in ["pvt_sort_by_value_inner", "pvt_show_two_data_fields", "pvt_show_rank_ascending", "pvt_show_rank_descending", "pvt_show_percent_running_total",
        "pvt_show_percent_of_parent_row", "pvt_show_percent_of_parent_column",
        "pvt_show_percent_of_parent", "pvt_show_percent_of_parent_product"] {
        let (sheet, part) = book.host().model().definitions().pivots().sheets.iter().find_map(|(sheet, pivots)|
            pivots.tables.iter().find(|table| table.name == name).map(|table| (*sheet, table.part.clone()))).unwrap();
        let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
            sheet, revision: book.host().revision().get(), part,
        }));
        assert!(!receipt.accepted(), "unmeasured ties or zero denominators must refuse for {name}: {receipt:?}");
    }
}

#[test]
/// Native-only: edits the captured source and refreshes through the host command.
fn dc_8wp__unmeasured_calculated_item_inputs_refuse_refresh() {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(
        include_bytes!("fixtures/pivot/m1-calculation/built.xlsx").to_vec(),
    ));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let source_sheet = book.host().sheets().iter().find(|sheet|
        String::from_utf16_lossy(sheet.name().text().utf16_code_units()) == "Source").unwrap().id();
    let (sheet, part) = book.host().model().definitions().pivots().sheets.iter().find_map(|(sheet, pivots)|
        pivots.tables.iter().find(|table| table.name == "pvt_calc_item_diff")
            .map(|table| (*sheet, table.part.clone()))).unwrap();
    let (field_sheet, field_part) = book.host().model().definitions().pivots().sheets.iter().find_map(|(sheet, pivots)|
        pivots.tables.iter().find(|table| table.name == "pvt_calc_field_ratio")
            .map(|table| (*sheet, table.part.clone()))).unwrap();
    for (address, value) in [("D2", "0.1"), ("D2", "100"), ("A2", "EastMinusWest")] {
        assert!(book.dispatch(SkinIntent::EnterCell {
            at: at(source_sheet, address), text: value.into(),
        }).accepted());
        if (address, value) == ("D2", "100") { continue; }
        let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
            sheet, revision: book.host().revision().get(), part: part.clone(),
        }));
        assert!(!receipt.accepted(), "an unmeasured calculated-item input must refuse: {address}: {receipt:?}");
        if address == "D2" {
            let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
                sheet: field_sheet, revision: book.host().revision().get(), part: field_part.clone(),
            }));
            assert!(!receipt.accepted(), "an unmeasured calculated-field input must refuse: {receipt:?}");
        }
    }
    assert!(book.dispatch(SkinIntent::EnterCell { at: at(source_sheet, "A2"), text: "North".into() }).accepted());
    let source_cells = held(&book, source_sheet, 5, 37);
    for row in 2..=37 {
        let address = format!("A{row}");
        if source_cells.get(&address) == Some(&Cell::Text("North".into())) {
            assert!(book.dispatch(SkinIntent::EnterCell { at: at(source_sheet, &address), text: "East".into() }).accepted());
        }
    }
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
        sheet, revision: book.host().revision().get(), part,
    }));
    assert!(!receipt.accepted() && format!("{:?}", receipt.error).contains("removed calculated-item source member"),
        "a removed referenced item must refuse: {receipt:?}");
}

#[test]
/// Native-only: edits the captured source and refreshes through the host command.
fn dc_8wp__unmeasured_numeric_group_members_refuse_refresh() {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(
        include_bytes!("fixtures/pivot/m1-calculation/built.xlsx").to_vec(),
    ));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let source_sheet = book.host().sheets().iter().find(|sheet|
        String::from_utf16_lossy(sheet.name().text().utf16_code_units()) == "Source").unwrap().id();
    let (sheet, part) = book.host().model().definitions().pivots().sheets.iter().find_map(|(sheet, pivots)|
        pivots.tables.iter().find(|table| table.name == "pvt_group_number_units")
            .map(|table| (*sheet, table.part.clone()))).unwrap();
    for value in ["2.5", "6"] {
        assert!(book.dispatch(SkinIntent::EnterCell {
            at: at(source_sheet, "E2"), text: value.into(),
        }).accepted());
        let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
            sheet, revision: book.host().revision().get(), part: part.clone(),
        }));
        assert!(!receipt.accepted(), "an unmeasured group member must refuse: {value}: {receipt:?}");
    }
}

#[test]
/// Native-only: edits the captured source and refreshes through the host command.
fn dc_8wp__new_discrete_group_item_refuses_refresh() {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(
        include_bytes!("fixtures/pivot/m1-calculation/built.xlsx").to_vec(),
    ));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let source_sheet = book.host().sheets().iter().find(|sheet|
        String::from_utf16_lossy(sheet.name().text().utf16_code_units()) == "Source").unwrap().id();
    assert!(book.dispatch(SkinIntent::EnterCell {
        at: at(source_sheet, "A2"), text: "Unseen region".into(),
    }).accepted());
    let (sheet, part) = book.host().model().definitions().pivots().sheets.iter().find_map(|(sheet, pivots)|
        pivots.tables.iter().find(|table| table.name == "pvt_group_text_items")
            .map(|table| (*sheet, table.part.clone()))).unwrap();
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
        sheet, revision: book.host().revision().get(), part,
    }));
    assert!(!receipt.accepted(), "a new group member must refuse: {receipt:?}");
}

#[test]
/// Native-only: opens the captured workbook and refreshes through the host command.
fn dc_8wp__unmodelled_show_values_extension_refuses_refresh() {
    use existing_wire_support::{RawEntry, build_zip, parse_zip};
    let bytes = include_bytes!("fixtures/pivot/m1-calculation/built.xlsx");
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    let original = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let pivots = original.host().model().definitions().pivots();
    let part = pivots.tables().find(|table| table.name == "pvt_show_percent_of_parent").unwrap().part.clone();
    let calculated = pivots.tables().find(|table| table.name == "pvt_calc_field_ratio").unwrap();
    let cache_part = pivots.caches.caches[&calculated.cache_id].part.clone();
    let mut changed = false;
    let mut changed_formula = false;
    let entries: Vec<_> = parse_zip(bytes).entries.iter().map(|entry| {
        let payload = entry.payload(bytes);
        if entry.name == part.trim_start_matches('/') {
            let data = if entry.method == 0 { payload.to_vec() }
                else { miniz_oxide::inflate::decompress_to_vec(payload).unwrap() };
            let xml = String::from_utf8(data).unwrap();
            let rewritten = xml.replace("pivotShowAs=\"percentOfParent\"", "pivotShowAs=\"unknownDisplay\"");
            assert_ne!(xml, rewritten);
            changed = true;
            RawEntry::stored(&entry.name, rewritten.as_bytes())
        } else if entry.name == cache_part.trim_start_matches('/') {
            let data = if entry.method == 0 { payload.to_vec() }
                else { miniz_oxide::inflate::decompress_to_vec(payload).unwrap() };
            let xml = String::from_utf8(data).unwrap();
            let rewritten = xml.replace("formula=\"Amount/Units\"", "formula=\"INDIRECT(&quot;A1&quot;)\"");
            assert_ne!(xml, rewritten);
            changed_formula = true;
            RawEntry::stored(&entry.name, rewritten.as_bytes())
        } else if entry.method == 0 { RawEntry::stored(&entry.name, payload) }
        else { RawEntry::deflated(&entry.name, payload, entry.size, entry.crc32) }
    }).collect();
    assert!(changed && changed_formula);
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(build_zip(&entries)));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let (sheet, part) = book.host().model().definitions().pivots().sheets.iter().find_map(|(sheet, pivots)|
        pivots.tables.iter().find(|table| table.name == "pvt_show_percent_of_parent")
            .map(|table| (*sheet, table.part.clone()))).unwrap();
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
        sheet, revision: book.host().revision().get(), part,
    }));
    assert!(!receipt.accepted(), "an unmodelled calculation must refuse: {receipt:?}");
    let (sheet, part) = book.host().model().definitions().pivots().sheets.iter().find_map(|(sheet, pivots)|
        pivots.tables.iter().find(|table| table.name == "pvt_calc_field_ratio")
            .map(|table| (*sheet, table.part.clone()))).unwrap();
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
        sheet, revision: book.host().revision().get(), part,
    }));
    assert!(!receipt.accepted() && format!("{:?}", receipt.error).contains("References in calculated pivot fields"),
        "an unmodelled calculated-field reference must refuse: {receipt:?}");
}

#[test]
/// Native-only: opens, refreshes, saves and reopens the Excel workbook.
fn dc_8wp__grouped_dates_survive_save_and_refresh() {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(UI30.to_vec()));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let sheet = book.host().sheets().iter().find(|s|
        String::from_utf16_lossy(s.name().text().utf16_code_units()) == "N10").unwrap().id();
    let expected = held(&book, sheet, 60, 80);
    let (part, cache_id) = {
        let table = &book.host().model().definitions().pivots().sheet(sheet).unwrap().tables[0];
        (table.part.clone(), table.cache_id)
    };
    let groups = |book: &HostSession| {
        let pivots = book.host().model().definitions().pivots();
        let cache = &pivots.caches.caches[&cache_id];
        cache.fields.iter().map(|field| (field.group.clone(),
            field.group.as_ref().filter(|group| group.range.is_some()).map(|_| field.items.clone())))
            .collect::<Vec<_>>()
    };
    let original_groups = groups(&book);
    assert!(original_groups.iter().filter(|(group, _)| group.as_ref().is_some_and(|g| g.range.is_some())).count() >= 2);
    assert!(book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
        sheet, revision: book.host().revision().get(), part,
    })).accepted());
    assert_eq!(held(&book, sheet, 60, 80), expected);
    let mut bytes = Vec::new();
    book.save(&SavePolicy::default(), &mut bytes).unwrap();
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
    let mut reopened = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    assert_eq!(groups(&reopened), original_groups);
    let sheet = reopened.host().sheets().iter().find(|s|
        String::from_utf16_lossy(s.name().text().utf16_code_units()) == "N10").unwrap().id();
    let part = reopened.host().model().definitions().pivots().sheet(sheet).unwrap().tables[0].part.clone();
    assert!(reopened.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
        sheet, revision: reopened.host().revision().get(), part,
    })).accepted());
    assert_eq!(held(&reopened, sheet, 60, 80), expected);
}

#[test]
/// Native-only: refreshes the captured precision pivot through the host command.
fn dc_8wp_10__pivot_precision_uses_the_same_path_in_both_numeric_modes() {
    use calc_value::{NumericMode, NumericTargetId};
    let target = NumericTargetId::new("16.0.20527.20150", "Current", "Excel", "x86-64").unwrap();
    let mut outputs = Vec::new();
    for numeric in [NumericMode::Fast, NumericMode::Exact(target)] {
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(include_bytes!("fixtures/pivot/m1-calculation/built.xlsx").to_vec()));
        let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions { numeric, ..Default::default() }).unwrap();
        let (sheet, part) = book.host().sheets().iter().find_map(|sheet| {
            book.host().model().definitions().pivots().sheets.get(&sheet.id())
                .and_then(|p| p.tables.iter().find(|t| t.name == "pvt_prec_sum_average"))
                .map(|t| (sheet.id(), t.part.clone()))
        }).unwrap();
        let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
            sheet, revision: book.host().revision().get(), part,
        }));
        assert!(receipt.accepted(), "{receipt:?}");
        let cells = held(&book, sheet, 5, 20);
        for address in ["B4", "C4", "B9", "C9"] {
            let captured = include_str!("fixtures/pivot/m1-calculation/cells.tsv").lines()
                .map(|line| line.splitn(4, '\t').collect::<Vec<_>>())
                .find(|f| f.len() == 4 && f[0] == "prec-sum-average" && f[1] == address).unwrap();
            assert_eq!(cells.get(address), Some(&Cell::Number(captured[3].parse().unwrap())), "{address}");
        }
        outputs.push(cells);
    }
    assert_eq!(outputs[0], outputs[1]);
}

#[test]
/// Native-only: checks refusal through a captured workbook and the host refresh command.
fn dc_8wp_10__relative_percentage_with_zero_base_refuses_without_writing() {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(
        include_bytes!("fixtures/pivot/m1-calculation/built.xlsx").to_vec(),
    ));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let source_sheet = book.host().sheets().iter().find(|s|
        String::from_utf16_lossy(s.name().text().utf16_code_units()) == "Source").unwrap().id();
    let source_cells = held(&book, source_sheet, 5, 37);
    let mut changed = 0;
    for row in 2..=37 {
        if source_cells.get(&format!("A{row}")) == Some(&Cell::Text("East".into())) {
            let receipt = book.dispatch(SkinIntent::EnterCell { at: at(source_sheet, &format!("D{row}")), text: "0".into() });
            assert!(receipt.accepted(), "{receipt:?}");
            changed += 1;
        }
    }
    assert!(changed > 0);
    let (sheet, part) = book.host().model().definitions().pivots().sheets.iter().find_map(|(sheet, pivots)|
        pivots.tables.iter().find(|table| table.name == "pvt_show_percentof_east")
            .map(|table| (*sheet, table.part.clone()))).unwrap();
    let before = held(&book, sheet, 6, 12);
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
        sheet, revision: book.host().revision().get(), part,
    }));
    assert!(!receipt.accepted(), "{receipt:?}");
    assert_eq!(held(&book, sheet, 6, 12), before);
}

#[test]
/// Native-only: replays Excel's saved number-format stages through refresh and XLSX save.
fn dc_8wp_12__data_field_number_formats_match_capture_after_refresh() {
    let bytes = include_bytes!("fixtures/pivot/m2-formatting/built.xlsx");
    let formats = include_str!("fixtures/pivot/m2-formatting/formats.tsv");
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions { locale: calc_host::FormatProfile::for_culture("en-US"), ..Default::default() }).unwrap();
    for stage in ["numfmt-number-2dp", "numfmt-percent", "numfmt-currency", "numfmt-scientific",
        "numfmt-text-prefix", "numfmt-general", "numfmt-date-like", "numfmt-negative-red"] {
        let name = format!("pvt_{}", stage.replace('-', "_"));
        let (sheet, part) = book.host().model().definitions().pivots().sheets.iter().find_map(|(sheet, pivots)|
            pivots.tables.iter().find(|table| table.name == name).map(|table| (*sheet, table.part.clone()))).unwrap();
        let expected: Vec<_> = formats.lines().filter_map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            (fields[0] == stage && fields[2] == "n").then(|| (fields[1], fields[4],
                book.host().number_format(at(sheet, fields[1])).map(str::to_owned)))
        }).collect();
        assert!(!expected.is_empty());
        if stage == "numfmt-percent" {
            let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::FormatCells {
                areas: vec![calc_host::value::GridArea::cell(at(sheet, "B6"))],
                patch: calc_host::FormatPatch { number_format: Some("0.00".into()), ..Default::default() },
            }));
            assert!(receipt.accepted(), "{receipt:?}");
            assert!(book.dispatch(SkinIntent::EnterCell { at: at(sheet, "Z1"), text: "=CELL(\"format\",B6)".into() }).accepted());
            let page = project(book.host(), &ProjectionRequest { cells: vec![at(sheet, "Z1")] }).unwrap();
            assert_eq!(held(&book, sheet, 26, 1).get("Z1"), Some(&Cell::Text("F2".into())), "{:?}", page.cells()[0].value());
        }
        let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
            sheet, revision: book.host().revision().get(), part,
        }));
        assert!(receipt.accepted(), "{stage}: {receipt:?}");
        if stage == "numfmt-percent" {
            assert_eq!(held(&book, sheet, 26, 1).get("Z1"), Some(&Cell::Text("P1".into())));
        }
        let mut display_matches = 0;
        for (address, display_format, saved_format) in &expected {
            let actual = book.host().number_format(at(sheet, address));
            assert_eq!(actual, saved_format.as_deref(), "{stage} {address} cell format");
            if actual == Some(*display_format) { display_matches += 1; }
        }
        println!("{stage}: {} of {} DisplayFormat number codes agree with cell formats", display_matches, expected.len());
        let mut saved = Vec::new(); book.save(&SavePolicy::default(), &mut saved).unwrap();
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(saved));
        book = HostSession::open(source, &OpenPolicy::default(), HostOptions { locale: calc_host::FormatProfile::for_culture("en-US"), ..Default::default() }).unwrap();
        for (address, _, saved_format) in &expected {
            assert_eq!(book.host().number_format(at(sheet, address)), saved_format.as_deref(), "{stage} {address} after reopen");
        }
    }
}

#[test]
/// Native-only: compares lookups with the captured values in the saved M1 layouts.
fn dc_8wp_14__lookup_values_on_rows_uses_the_displayed_measure() {
    let bytes = include_bytes!("fixtures/pivot/m1-calculation/built.xlsx");
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    for stage in ["values_rows_region", "values_rows_first", "values_rows_with_columns"] {
        let name = format!("pvt_{stage}");
        let sheet = book.host().model().definitions().pivots().sheets.iter().find_map(|(sheet, pivots)|
            pivots.tables.iter().any(|table| table.name == name).then_some(*sheet)).unwrap();
        let mut cases = vec![
            ("Z1", "=GETPIVOTDATA(\"Sum Amount\",A3,\"Region\",\"East\")", 2074.5),
            ("Z2", "=GETPIVOTDATA(\"Sum Units\",A3,\"Region\",\"East\")", 26.0),
            ("Z3", "=GETPIVOTDATA(\"Sum Amount\",A3)", 7162.75),
            ("Z4", "=GETPIVOTDATA(\"Sum Units\",A3)", 105.0),
        ];
        if stage == "values_rows_with_columns" {
            cases.push(("Z5", "=GETPIVOTDATA(\"Sum Amount\",A3,\"Region\",\"East\",\"Product\",\"Alpha\")", 691.5));
            cases.push(("Z6", "=GETPIVOTDATA(\"Sum Units\",A3,\"Region\",\"East\",\"Product\",\"Alpha\")", 7.0));
        }
        for (cell, formula, _) in &cases {
            assert!(book.dispatch(SkinIntent::EnterCell { at: at(sheet, cell), text: (*formula).into() }).accepted());
        }
        let got = held(&book, sheet, 26, 6);
        for (cell, _, value) in cases { assert_eq!(got.get(cell), Some(&Cell::Number(value)), "{stage} {cell}"); }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_8wp_12__saved_style_and_area_records_survive_refresh_and_save() {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(
        include_bytes!("fixtures/pivot/m2-formatting/built.xlsx").to_vec(),
    ));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let pivots = book.host().model().definitions().pivots();
    assert_eq!(pivots.tables().count(), 70);
    assert_eq!(pivots.tables().filter(|t| t.style.is_some()).count(), 70);
    assert_eq!(pivots.tables().map(|t| t.formats.len()).sum::<usize>(), 22);
    let style = pivots.tables().find(|t| t.name == "pvt_option_medium9_no_row_headers").unwrap();
    let attrs = &style.style.as_ref().unwrap().attributes;
    assert_eq!(attrs.get("name").map(String::as_str), Some("PivotStyleMedium9"));
    assert_eq!(attrs.get("showRowHeaders").map(String::as_str), Some("0"));
    let table = pivots.tables().find(|t| t.name == "pvt_area_data_field_range").unwrap();
    assert_eq!(table.formats.len(), 3);
    let format = &table.formats[0];
    assert_eq!(format.attributes.get("dxfId").map(String::as_str), Some("21"));
    let area = format.area.as_ref().unwrap();
    assert_eq!(area.references.len(), 2);
    assert_eq!(area.references[0].attributes.get("field").map(String::as_str), Some("4294967294"));
    assert_eq!(area.references[0].items, vec![0]);
    let (sheet, part) = (table.area.sheet(), table.part.clone());
    let expected_style = table.style.clone();
    let expected_formats = table.formats.clone();
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
        sheet, revision: book.host().revision().get(), part,
    }));
    assert!(receipt.accepted(), "{receipt:?}");
    let mut saved = Vec::new();
    book.save(&SavePolicy::default(), &mut saved).unwrap();
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(saved));
    let reopened = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let pivots = reopened.host().model().definitions().pivots();
    let table = pivots.tables().find(|t| t.name == "pvt_area_data_field_range").unwrap();
    assert_eq!(table.style, expected_style);
    assert_eq!(table.formats, expected_formats);
}

// Convert the host's resolved RGB color to the OLE color integer in the capture.
fn captured_color(host: &calc_host::HostState, color: &calc_doc_model::StyleColor) -> Option<String> {
    let rgb = host.resolved_style_color(color)?;
    let rgb = rgb.strip_prefix('#')?;
    if rgb.len() != 6 { return None; }
    let r = u32::from_str_radix(&rgb[0..2], 16).ok()?;
    let g = u32::from_str_radix(&rgb[2..4], 16).ok()?;
    let b = u32::from_str_radix(&rgb[4..6], 16).ok()?;
    Some((r | g << 8 | b << 16).to_string())
}

fn captured_border(host: &calc_host::HostState, edge: Option<&calc_doc_model::BorderEdge>) -> Option<String> {
    let Some(edge) = edge else { return Some("-4142:2:0".into()); };
    let Some(style) = edge.style.as_deref() else { return Some("-4142:2:0".into()); };
    let (line, weight) = match style {
        "thin" => (1, 2), "medium" => (1, -4138), "thick" => (1, 4),
        "double" => (-4119, 4), "dashed" => (-4115, 2), "dotted" => (-4118, 2),
        _ => return None,
    };
    let color = captured_color(host, edge.color.as_ref()?)?;
    Some(format!("{line}:{weight}:{color}"))
}

// Read the same authored style and geometry that the host exposes to its visual consumers.
// Missing or unrecognised records count as failures rather than as Excel defaults.
fn captured_properties(host: &calc_host::HostState, at: CellKey) -> Vec<Option<String>> {
    let styles = host.styles();
    let xf = styles.cell_formats.get(host.cell_style(at) as usize);
    let parent = xf.and_then(|x| x.xf_id).and_then(|i| styles.cell_style_formats.get(i as usize));
    let font = xf.and_then(|x| x.font_id).or_else(|| parent.and_then(|x| x.font_id)).and_then(|i| styles.fonts.get(i as usize));
    let fill = xf.and_then(|x| x.fill_id).or_else(|| parent.and_then(|x| x.fill_id)).and_then(|i| styles.fills.get(i as usize));
    let border = xf.and_then(|x| x.border_id).or_else(|| parent.and_then(|x| x.border_id)).and_then(|i| styles.borders.get(i as usize));
    let fill_color = fill.and_then(|f| match f.pattern_type.as_deref() {
        Some("none") => Some("16777215".into()),
        Some("solid") => captured_color(host, f.fg_color.as_ref()?),
        _ => None,
    });
    let pattern = fill.and_then(|f| match f.pattern_type.as_deref() { Some("none") => Some("-4142".into()), Some("solid") => Some("1".into()), _ => None });
    let edges = border.map(|b| [&b.left, &b.top, &b.bottom, &b.right].map(|e| captured_border(host, e.as_ref())));
    let alignment = host.cell_alignment(at);
    let horizontal = match alignment.and_then(|a| a.horizontal.as_deref()) {
        None | Some("general") => Some("1".into()), Some("left") => Some("-4131".into()), _ => None,
    };
    let indent = Some(alignment.and_then(|a| a.indent.clone()).unwrap_or_else(|| "0".into()));
    let geometry = host.sheet_geometry(at.sheet());
    // This fixture's workbook default is Aptos Narrow 11, whose maximum digit width is 7 pixels.
    let width = geometry.as_ref().map(|g| {
        let pixels = if let Some(w) = g.column(at.column()).and_then(|c| c.width).or(g.default_column_width) {
            (((256.0 * w + (128.0_f64 / 7.0).trunc()) / 256.0) * 7.0).trunc()
        } else { ((g.base_column_width.unwrap_or(8.0) * 7.0 + 5.0) / 8.0).ceil() * 8.0 };
        (((pixels - 5.0) / 7.0 * 100.0 + 0.5).trunc() / 100.0).max(0.0).to_string()
    });
    let height = geometry.as_ref().and_then(|g| g.row(at.row()).and_then(|r| r.height).or(g.default_row_height)).map(|x| x.to_string());
    vec![host.number_format(at).map(str::to_owned), font.and_then(|f| f.name.clone()), font.and_then(|f| f.size.clone()),
        font.map(|f| f.bold.to_string()), font.map(|f| f.italic.to_string()), font.and_then(|f| captured_color(host, f.color.as_ref()?)),
        fill_color, pattern,
        edges.as_ref().and_then(|e| e[0].clone()), edges.as_ref().and_then(|e| e[1].clone()),
        edges.as_ref().and_then(|e| e[2].clone()), edges.as_ref().and_then(|e| e[3].clone()), horizontal, indent, width, height]
}

fn property_equal(property: usize, actual: &str, expected: &str) -> bool {
    if actual == expected || (matches!(property, 3 | 4) && actual.eq_ignore_ascii_case(expected)) { return true; }
    matches!(property, 2 | 5 | 6 | 7 | 12 | 13 | 14 | 15)
        && matches!((actual.parse::<f64>(), expected.parse::<f64>()), (Ok(a), Ok(b)) if (a - b).abs() < 0.001)
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_8wp_12__all_captured_stages_report_number_format_codes() {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(
        include_bytes!("fixtures/pivot/m2-formatting/built.xlsx").to_vec(),
    ));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions {
        locale: calc_host::FormatProfile::for_culture("en-US"), ..Default::default()
    }).unwrap();
    let source_sheet = book.host().sheets().iter().find(|s|
        String::from_utf16_lossy(s.name().text().utf16_code_units()) == "Source").unwrap().id();
    let mut stages: BTreeMap<&str, Vec<Vec<&str>>> = BTreeMap::new();
    for line in include_str!("fixtures/pivot/m2-formatting/formats.tsv").lines() {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 18);
        stages.entry(fields[0]).or_default().push(fields);
    }
    assert_eq!(stages.len(), 76);
    assert_eq!(stages.values().map(Vec::len).sum::<usize>(), 3330);
    let mut matches = 0;
    let mut refused_cells = 0;
    let names = ["number format", "font name", "font size", "bold", "italic", "font color", "fill color", "fill pattern", "left border", "top border", "bottom border", "right border", "alignment", "indent", "column width", "row height"];
    let mut property_matches = [0usize; 16];
    let mut unavailable = [0usize; 16];
    let mut value_matches = 0;
    let mut text_matches = 0;
    let mut value_unavailable = 0;
    let mut text_unavailable = 0;
    for (stage, cells) in &stages {
        let base = stage.strip_suffix("-built").or_else(|| stage.strip_suffix("-refreshed")).unwrap_or(stage);
        let name = format!("pvt_{}", base.replace('-', "_"));
        let (sheet, part) = book.host().model().definitions().pivots().sheets.iter().find_map(|(sheet, pivots)|
            pivots.tables.iter().find(|table| table.name == name).map(|table| (*sheet, table.part.clone())))
            .unwrap_or_else(|| panic!("missing {name}"));
        // The workbook was saved after the refresh scenarios. Restore the captured direct fill
        // before measuring each stage so the refresh begins with the same formatting as Excel.
        if stage.starts_with("refresh-") {
            let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::FormatCells {
                areas: vec![calc_host::value::GridArea::cell(at(sheet, "B5"))],
                patch: calc_host::FormatPatch { fill: Some("#ffff00".into()), ..Default::default() },
            }));
            assert!(receipt.accepted(), "{stage} pre-refresh fill: {receipt:?}");
        }
        if stage.ends_with("-refreshed") {
            for (address, text) in [("B3", "A very long product name that needs a wide column"), ("D3", "123456789.125")] {
                assert!(book.dispatch(SkinIntent::EnterCell { at: at(source_sheet, address), text: text.into() }).accepted());
            }
            let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
                sheet, revision: book.host().revision().get(), part,
            }));
            if !receipt.accepted() {
                refused_cells += cells.len();
                for (address, text) in [("B3", "Beta"), ("D3", "107.25")] {
                    assert!(book.dispatch(SkinIntent::EnterCell { at: at(source_sheet, address), text: text.into() }).accepted());
                }
                continue;
            }
        }
        let mut stage_matches = 0;
        let mut differences = BTreeMap::<(String, String), usize>::new();
        let page = project(book.host(), &ProjectionRequest { cells: cells.iter().map(|f| at(sheet, f[1])).collect() }).unwrap();
        for (fields, cell) in cells.iter().zip(page.cells()) {
            let value = match cell.value() {
                ProjectedValue::Literal(v) | ProjectedValue::Calculated { value: v, .. } => Some(v),
                _ => None,
            };
            if let Some(value) = value {
                if match (fields[2], value.core_ref()) {
                    ("n", CoreRef::Number(n)) => fields[3].parse::<f64>().ok().is_some_and(|v| v == n),
                    ("s", CoreRef::Text(s)) => s.to_string_lossy() == fields[3],
                    _ => false,
                } { value_matches += 1; }
                let text = book.host().options().locale.as_ref().and_then(|profile| {
                    let system = if book.host().declared_date1904() { calc_func::locale::DateSystem::System1904 } else { calc_func::locale::DateSystem::System1900 };
                    calc_func::cell_text(profile, system, value, book.host().number_format(cell.at())?).ok()
                });
                match text { Some(text) if text == fields[5] => text_matches += 1, None => text_unavailable += 1, _ => {} }
            } else { value_unavailable += 1; text_unavailable += 1; }
            let address = fields[1];
            let expected = fields[4];
            let actual = book.host().number_format(at(sheet, address));
            if actual == Some(expected) { stage_matches += 1; }
            else { *differences.entry((expected.to_owned(), actual.unwrap_or("<unavailable>").to_owned())).or_default() += 1; }
            let edges: Vec<_> = fields[13].split('|').collect();
            assert_eq!(edges.len(), 4);
            let expected = [fields[4], fields[6], fields[7], fields[8], fields[9], fields[10], fields[11], fields[12], edges[0], edges[1], edges[2], edges[3], fields[14], fields[15], fields[16], fields[17]];
            for (i, (actual, expected)) in captured_properties(book.host(), at(sheet, address)).iter().zip(expected).enumerate() {
                match actual {
                    Some(actual) if property_equal(i, actual, expected) => property_matches[i] += 1,
                    None => unavailable[i] += 1,
                    _ => {},
                }
            }
        }
        matches += stage_matches;
        if !differences.is_empty() { println!("{stage}: {stage_matches}/{} number formats match; {differences:?}", cells.len()); }
        if stage.ends_with("-refreshed") {
            for (address, text) in [("B3", "Beta"), ("D3", "107.25")] {
                assert!(book.dispatch(SkinIntent::EnterCell { at: at(source_sheet, address), text: text.into() }).accepted());
            }
        }
    }
    let mismatches = 3330 - matches - refused_cells;
    let failed = mismatches + refused_cells;
    println!("Captured number-format codes: {matches}/3330 match ({:.2}%); {failed} fail ({mismatches} mismatches, {refused_cells} refused) across 76 stages", 100.0 * matches as f64 / 3330.0);
    println!("value: {value_matches}/3330 match; {} fail ({value_unavailable} unavailable, {refused_cells} refresh refusals)", 3330 - value_matches);
    println!("display text: {text_matches}/3330 match; {} fail ({text_unavailable} unavailable, {refused_cells} refresh refusals)", 3330 - text_matches);
    for (i, name) in names.iter().enumerate() {
        let fail = 3330 - property_matches[i];
        println!("{name}: {}/3330 match; {fail} fail ({} unavailable, {refused_cells} refresh refusals)", property_matches[i], unavailable[i]);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_8wp_12__refresh_reuses_retained_product_in_captured_layouts() {
    let formats = include_str!("fixtures/pivot/m2-formatting/formats.tsv");
    let stages = ["refresh-preserve-autofit", "refresh-preserve-noautofit",
        "refresh-nopreserve-autofit", "refresh-nopreserve-noautofit",
        "refresh-userwidth-autofit", "refresh-userwidth-noautofit"];
    for stage in stages {
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(
            include_bytes!("fixtures/pivot/m2-formatting/built.xlsx").to_vec(),
        ));
        let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
        let source_sheet = book.host().sheets().iter().find(|s|
            String::from_utf16_lossy(s.name().text().utf16_code_units()) == "Source").unwrap().id();
        for (address, text) in [("B3", "A very long product name that needs a wide column"), ("D3", "123456789.125")] {
            assert!(book.dispatch(SkinIntent::EnterCell { at: at(source_sheet, address), text: text.into() }).accepted());
        }
        let name = format!("pvt_{}", stage.replace('-', "_"));
        let (sheet, part) = book.host().model().definitions().pivots().sheets.iter().find_map(|(sheet, pivots)|
            pivots.tables.iter().find(|table| table.name == name).map(|table| (*sheet, table.part.clone()))).unwrap();
        let mut areas = vec![calc_host::value::GridArea::cell(at(sheet, "B5"))];
        if stage == "refresh-nopreserve-noautofit" {
            areas.push(calc_host::value::GridArea::cell(at(sheet, "Z5")));
        }
        let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::FormatCells {
            areas, patch: calc_host::FormatPatch { fill: Some("#ffff00".into()), ..Default::default() },
        }));
        assert!(receipt.accepted(), "{stage} pre-refresh fill: {receipt:?}");
        if stage == "refresh-nopreserve-autofit" {
            let mut bytes = Vec::new();
            book.save(&SavePolicy::default(), &mut bytes).unwrap();
            let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
            book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
            assert_eq!(captured_properties(book.host(), at(sheet, "B5"))[6].as_deref(), Some("65535"));
        }
        let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
            sheet, revision: book.host().revision().get(), part,
        }));
        assert!(receipt.accepted(), "{stage}: {receipt:?}");
        let captured = formats.lines().map(|line| line.split('\t').collect::<Vec<_>>())
            .find(|fields| fields[0] == format!("{stage}-refreshed") && fields[1] == "B5")
            .unwrap();
        let properties = captured_properties(book.host(), at(sheet, "B5"));
        for (index, expected) in [(6, captured[11]), (7, captured[12])] {
            assert!(properties[index].as_deref().is_some_and(|actual| property_equal(index, actual, expected)),
                "{stage} B5 property {index}: Excel {expected}, host {:?}", properties[index]);
        }
        if stage == "refresh-nopreserve-noautofit" {
            assert_eq!(captured_properties(book.host(), at(sheet, "Z5"))[6].as_deref(), Some("65535"));
            assert!(book.dispatch(SkinIntent::Undo).accepted());
            assert_eq!(captured_properties(book.host(), at(sheet, "B5"))[6].as_deref(), Some("65535"));
            assert!(book.dispatch(SkinIntent::Redo).accepted());
            let mut bytes = Vec::new();
            book.save(&SavePolicy::default(), &mut bytes).unwrap();
            let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
            let reopened = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
            assert_eq!(captured_properties(reopened.host(), at(sheet, "B5"))[6].as_deref(), Some("16777215"));
            assert_eq!(captured_properties(reopened.host(), at(sheet, "Z5"))[6].as_deref(), Some("65535"));
        }
        let actual = held(&book, sheet, 16, 30);
        let mut checked = 0;
        for fields in formats.lines().map(|line| line.split('\t').collect::<Vec<_>>())
            .filter(|fields| fields[0] == format!("{stage}-refreshed")) {
            let expected = match fields[2] {
                "n" => Cell::Number(fields[3].parse().unwrap()),
                "s" => Cell::Text(fields[3].into()),
                kind => panic!("unexpected captured cell kind {kind}"),
            };
            assert_eq!(actual.get(fields[1]), Some(&expected), "{stage} {}", fields[1]);
            checked += 1;
        }
        assert_eq!(checked, 38);
        assert_eq!(actual.len(), checked, "{stage}: unexpected output cells");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_8wp_19__model_pivot_requires_its_own_connection_id() {
    use existing_wire_support::{RawEntry, build_zip, parse_zip};
    let bytes = include_bytes!("fixtures/pivot_cube/model-pivot.xlsx");
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    let original = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let data = original.host().model().definitions().data_model().unwrap();
    assert_eq!(data.model_connection_id, Some(1));
    let pivots = original.host().model().definitions().pivots();
    let table = pivots.tables().find(|table| table.name == "pvt_model").unwrap();
    let cache = &pivots.caches.caches[&table.cache_id];
    assert_eq!(cache.source_connection_id, Some(1));
    let cache_part = cache.part.trim_start_matches('/').to_owned();
    let needle = "<cacheSource type=\"external\" connectionId=\"1\"/>";
    let scenarios = [
        (cache_part.as_str(), needle, "<cacheSource type=\"external\" connectionId=\"2\"/>"),
        (cache_part.as_str(), needle, "<cacheSource type=\"external\"/>"),
        (cache_part.as_str(), needle, "<cacheSource type=\"external\" connectionId=\"999\"/>"),
        (cache_part.as_str(), needle, "<cacheSource type=\"external\" connectionId=\"1\"/><cacheSource type=\"external\" connectionId=\"1\"/>"),
        ("xl/connections.xml", "<connection id=\"2\"", "<connection id=\"1\""),
        ("xl/connections.xml", "name=\"ThisWorkbookDataModel\" description=\"Data Model\" type=\"5\"",
            "name=\"ThisWorkbookDataModel\" description=\"Data Model\" type=\"1\""),
        ("xl/connections.xml", "<x15:connection id=\"\" model=\"1\"/>",
            "<x15:connection id=\"\" model=\"1\"/><x15:connection id=\"\" model=\"1\"/>"),
        ("xl/connections.xml", "<x15:connection id=\"\" model=\"1\"/>",
            "<unexpected><x15:connection id=\"\" model=\"1\"/></unexpected>"),
        ("xl/connections.xml", "rowDrillCount=\"1000\"/><extLst><ext uri=\"{DE250136-89BD-433C-8126-D09CA5730AF9}\"",
            "rowDrillCount=\"1000\"/><extLst><ext uri=\"{00000000-0000-0000-0000-000000000000}\""),
    ];
    for (part, needle, replacement) in scenarios {
        let mut changed = false;
        let entries: Vec<_> = parse_zip(bytes).entries.iter().map(|entry| {
            let payload = entry.payload(bytes);
            if entry.name == part {
                let xml = if entry.method == 0 { payload.to_vec() } else { miniz_oxide::inflate::decompress_to_vec(payload).unwrap() };
                let xml = String::from_utf8(xml).unwrap();
                assert_eq!(xml.matches(needle).count(), 1);
                changed = true;
                RawEntry::stored(&entry.name, xml.replace(needle, replacement).as_bytes())
            } else if entry.method == 0 { RawEntry::stored(&entry.name, payload) }
            else { RawEntry::deflated(&entry.name, payload, entry.size, entry.crc32) }
        }).collect();
        assert!(changed);
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(build_zip(&entries)));
        let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
        let sheet = book.host().sheets().iter().find(|s| String::from_utf16_lossy(s.name().text().utf16_code_units()) == "GpdModel").unwrap().id();
        assert!(book.dispatch(SkinIntent::Rebuild).accepted());
        while book.has_unfinished_calculation() { book.advance_calculation(); }
        let page = project(book.host(), &ProjectionRequest { cells: vec![at(sheet, "B6")] }).unwrap();
        assert!(matches!(page.cells()[0].value(), ProjectedValue::NotModeled { .. }), "{replacement}: {:?}", page.cells()[0].value());
    }
}
