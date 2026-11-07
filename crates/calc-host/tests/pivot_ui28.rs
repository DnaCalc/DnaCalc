//! Pivot refresh against Excel 16.0.20527 (UI28, tests/fixtures/ui28): `pivots-built.xlsx` is the
//! workbook Excel built with 22 pivots (sheets P1..P20, PName, PTable) over the Source sheet;
//! `pivots-refreshed.tsv` is every cell Excel held on those sheets after Source!D3 changed from
//! 107.25 to 1234.5 and each pivot was refreshed. DNA Calc applies the same edit, refreshes each
//! pivot through the host command and is compared cell for cell. A refused pivot counts as a
//! failure here as everywhere; the lists below name what is expected to match and what refuses.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

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
