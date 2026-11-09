//! Pivot structure scenarios against Excel 16.0.20527 (tests/fixtures/pivot/m5-structure, bead dc-8wp.16
//! phase two): whole rows and columns inserted or deleted around a pivot output and around its
//! worksheet-range or table source, then a refresh. `scenarios.tsv` holds what Excel did (the edit's
//! outcome, the pivot's range and source after the edit and after the refresh, the table's range);
//! `cells.tsv` every cell of `Out` and `Src` at stage 2 (after the refresh). DNA Calc starts from the
//! workbook Excel saved before the edits and is compared on the edit's outcome, the pivot's range and
//! source, and every cell of the pivot after a refresh. A refusal where Excel accepted counts as a failure.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_host::value::{CellKey, CoreRef, GridArea, SheetId};
use calc_host::{
    Axis, AxisChange, AxisEdit, HostOptions, HostSession, MemorySource, OpenPolicy, PackageRead,
    ProjectedValue, ProjectionRequest, SavePolicy, SkinIntent, project,
};
use std::collections::BTreeMap;
use std::sync::Arc;

const SCENARIOS: &str = include_str!("fixtures/pivot/m5-structure/scenarios.tsv");
const CELLS: &str = include_str!("fixtures/pivot/m5-structure/cells.tsv");
const BASE_RANGE: &[u8] = include_bytes!("fixtures/pivot/m5-structure/base-range.xlsx");
const BASE_TABLE: &[u8] = include_bytes!("fixtures/pivot/m5-structure/base-table.xlsx");

struct Excel {
    act: String,
    pivots: [String; 5],
    tables: [String; 5],
}

fn excel(id: &str) -> Excel {
    let header: Vec<&str> = SCENARIOS.lines().next().unwrap().split('\t').collect();
    let line = SCENARIOS
        .lines()
        .find(|l| l.split('\t').next() == Some(id))
        .unwrap_or_else(|| panic!("no scenario {id}"));
    let f: Vec<&str> = line.split('\t').collect();
    let at = |name: &str| f[header.iter().position(|h| *h == name).unwrap()].to_owned();
    Excel {
        act: at("act"),
        pivots: ["pivots0", "pivots1", "pivots2", "pivots3", "pivots4"].map(|n| at(n)),
        tables: ["tables0", "tables1", "tables2", "tables3", "tables4"].map(|n| at(n)),
    }
}

fn letters(mut column: u32) -> String {
    let mut s = String::new();
    while column > 0 {
        s.insert(0, char::from(b'A' + ((column - 1) % 26) as u8));
        column = (column - 1) / 26;
    }
    s
}
fn a1(area: GridArea) -> String {
    let (f, l) = (area.first(), area.last());
    if f == l {
        return format!("{}{}", letters(f.column()), f.row());
    }
    format!("{}{}:{}{}", letters(f.column()), f.row(), letters(l.column()), l.row())
}
fn at(sheet: SheetId, address: &str) -> CellKey {
    let split = address.find(|c: char| c.is_ascii_digit()).unwrap();
    let (l, d) = address.split_at(split);
    CellKey::new(sheet, d.parse().unwrap(), l.bytes().fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1))).unwrap()
}
fn open(bytes: &[u8]) -> HostSession {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    assert!(book.dispatch(SkinIntent::Recalculate).accepted());
    book
}
fn sheet(book: &HostSession, name: &str) -> SheetId {
    book.host().sheets().iter().find(|s| s.name().text().to_string_lossy() == name).unwrap().id()
}
/// `m5@C4:H9@Src!R2C2:R14C5` as DNA Calc holds it: the output area and the cache source in Excel's spelling.
fn pivot_now(book: &HostSession) -> String {
    let state = book.host().model().definitions().pivots();
    let Some(table) = state.tables().next() else { return "-".into() };
    let cache = &state.caches.caches[&table.cache_id];
    let source = match (&cache.source_name, &cache.source_sheet, &cache.source_range) {
        (Some(name), _, _) => name.clone(),
        (None, Some(sheet), Some(range)) => {
            let (a, b) = range.split_once(':').unwrap_or((range, range));
            let rc = |cell: &str| {
                let split = cell.find(|c: char| c.is_ascii_digit()).unwrap();
                let col = cell[..split].bytes().fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1));
                format!("R{}C{}", &cell[split..], col)
            };
            format!("{sheet}!{}:{}", rc(a), rc(b))
        }
        _ => "?".into(),
    };
    format!("{}@{}@{}", table.name.trim_start_matches("pvt_"), a1(table.area), source)
}
fn refresh(book: &mut HostSession) -> Result<(), String> {
    let (sheet, part) = {
        let state = book.host().model().definitions().pivots();
        let table = state.tables().next().unwrap();
        (table.area.sheet(), table.part.clone())
    };
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
        sheet,
        revision: book.host().revision().get(),
        part,
    }));
    if receipt.accepted() { Ok(()) } else { Err(format!("{:?}", receipt.error)) }
}
/// The non-empty cells of `Out` inside `area`, as Excel's capture spells them.
fn dna_cells(book: &HostSession, area: GridArea) -> BTreeMap<String, String> {
    let out = sheet(book, "Out");
    let mut keys = Vec::new();
    for r in area.first().row()..=area.last().row() {
        for c in area.first().column()..=area.last().column() {
            keys.push(CellKey::new(out, r, c).unwrap());
        }
    }
    let page = project(book.host(), &ProjectionRequest { cells: keys.clone() }).unwrap();
    let mut cells = BTreeMap::new();
    for (key, cell) in keys.iter().zip(page.cells()) {
        let value = match cell.value() {
            ProjectedValue::Literal(v) => v.clone(),
            ProjectedValue::Calculated { value, .. } => value.clone(),
            _ => continue,
        };
        let text = match value.core_ref() {
            CoreRef::Number(n) => format!("n\t{n}"),
            CoreRef::Text(t) => format!("s\t{}", String::from_utf16_lossy(t.utf16_code_units())),
            CoreRef::Error(e) => format!("e\t{}", e.excel_spelling()),
            _ => continue,
        };
        cells.insert(format!("{}{}", letters(key.column()), key.row()), text);
    }
    cells
}
fn excel_cells(id: &str, area: GridArea) -> BTreeMap<String, String> {
    excel_cells_at(id, "2", area)
}
fn excel_cells_at(id: &str, stage: &str, area: GridArea) -> BTreeMap<String, String> {
    let (top, bottom, left, right) = (area.first().row(), area.last().row(), area.first().column(), area.last().column());
    let mut cells = BTreeMap::new();
    for line in CELLS.lines().skip(1) {
        let f: Vec<&str> = line.splitn(6, '\t').collect();
        if f[0] != id || f[1] != stage || f[2] != "Out" {
            continue;
        }
        let split = f[3].find(|c: char| c.is_ascii_digit()).unwrap();
        let col = f[3][..split].bytes().fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1));
        let row: u32 = f[3][split..].parse().unwrap();
        if (top..=bottom).contains(&row) && (left..=right).contains(&col) {
            let value = if f[4] == "n" {
                // Excel's repr of a float, compared as a number below.
                format!("n\t{}", f[5].parse::<f64>().unwrap())
            } else if f[4] == "e" {
                // The capture holds Excel's raw error code (HRESULT 0x800A0000 + the CVErr number).
                let code = f[5].parse::<i64>().unwrap() + (1 << 32) - 0x800A_0000;
                let name = match code {
                    2000 => "#NULL!", 2007 => "#DIV/0!", 2015 => "#VALUE!", 2023 => "#REF!", 2029 => "#NAME?",
                    2036 => "#NUM!", 2042 => "#N/A", 2045 => "#SPILL!", other => panic!("error code {other}"),
                };
                format!("e\t{name}")
            } else {
                format!("{}\t{}", f[4], f[5])
            };
            cells.insert(f[3].to_owned(), value);
        }
    }
    cells
}
fn area_on(pivot: &str, sheet: SheetId) -> GridArea {
    let range = pivot.split('@').nth(1).unwrap();
    let (a, b) = range.split_once(':').unwrap_or((range, range));
    GridArea::new(at(sheet, a), at(sheet, b)).unwrap()
}

/// (scenario, sheet, axis, change, first, count): the edit each scenario's Excel script made.
const EDITS: &[(&str, &str, char, char, u32, u32)] = &[
    ("out-ins-row-above", "Out", 'r', 'i', 2, 1),
    ("out-ins-row-first", "Out", 'r', 'i', 4, 1),
    ("out-ins-row-inside", "Out", 'r', 'i', 7, 1),
    ("out-ins-row-last", "Out", 'r', 'i', 9, 1),
    ("out-ins-row-after", "Out", 'r', 'i', 10, 1),
    ("out-ins-rows-three-inside", "Out", 'r', 'i', 6, 3),
    ("out-del-row-above", "Out", 'r', 'd', 1, 1),
    ("out-del-row-first", "Out", 'r', 'd', 4, 1),
    ("out-del-row-header2", "Out", 'r', 'd', 5, 1),
    ("out-del-row-inside", "Out", 'r', 'd', 7, 1),
    ("out-del-row-last", "Out", 'r', 'd', 9, 1),
    ("out-del-rows-across-top", "Out", 'r', 'd', 2, 4),
    ("out-del-rows-across-bottom", "Out", 'r', 'd', 8, 4),
    ("out-ins-col-before", "Out", 'c', 'i', 1, 1),
    ("out-ins-col-first", "Out", 'c', 'i', 3, 1),
    ("out-ins-col-inside", "Out", 'c', 'i', 5, 1),
    ("out-ins-col-last", "Out", 'c', 'i', 8, 1),
    ("out-ins-col-after", "Out", 'c', 'i', 9, 1),
    ("out-del-col-before", "Out", 'c', 'd', 1, 1),
    ("out-del-col-first", "Out", 'c', 'd', 3, 1),
    ("out-del-col-inside", "Out", 'c', 'd', 5, 1),
    ("out-del-col-last", "Out", 'c', 'd', 8, 1),
    ("out-del-cols-across-left", "Out", 'c', 'd', 2, 3),
    ("out-del-cols-across-right", "Out", 'c', 'd', 7, 4),
    ("src-ins-row-above", "Src", 'r', 'i', 1, 1),
    ("src-ins-row-header", "Src", 'r', 'i', 2, 1),
    ("src-ins-row-first-data", "Src", 'r', 'i', 3, 1),
    ("src-ins-row-inside", "Src", 'r', 'i', 8, 1),
    ("src-ins-row-last", "Src", 'r', 'i', 14, 1),
    ("src-ins-row-after", "Src", 'r', 'i', 15, 1),
    ("src-del-row-above", "Src", 'r', 'd', 1, 1),
    ("src-del-row-first-data", "Src", 'r', 'd', 3, 1),
    ("src-del-row-inside", "Src", 'r', 'd', 8, 1),
    ("src-del-row-last", "Src", 'r', 'd', 14, 1),
    ("src-del-row-after", "Src", 'r', 'd', 15, 1),
    ("src-del-rows-across-end", "Src", 'r', 'd', 12, 5),
    ("src-ins-col-before", "Src", 'c', 'i', 1, 1),
    ("src-ins-col-first", "Src", 'c', 'i', 2, 1),
    ("src-ins-col-after", "Src", 'c', 'i', 6, 1),
    ("src-del-col-before", "Src", 'c', 'd', 1, 1),
    ("src-del-col-after", "Src", 'c', 'd', 6, 1),
];

fn edit(book: &mut HostSession, case: &(&str, &str, char, char, u32, u32)) -> calc_host::IntentReceipt {
    let target = sheet(book, case.1);
    let axis = if case.2 == 'r' { Axis::Rows } else { Axis::Columns };
    let change = if case.3 == 'i' { AxisChange::Insert } else { AxisChange::Delete };
    book.dispatch(SkinIntent::EditAxis(AxisEdit::new(target, axis, change, case.4, case.5).unwrap()))
}

#[test]
/// Native-only: opens the Excel-saved workbook files of the fixture set.
fn dc_8wp_16__row_and_column_edits_place_pivot_outputs_and_sources_as_excel_does() {
    let mut failures = Vec::new();
    for case in EDITS {
        let want = excel(case.0);
        let mut book = open(BASE_RANGE);
        let before = pivot_now(&book);
        assert_eq!(before, want.pivots[0], "{}: the start workbook", case.0);
        let receipt = edit(&mut book, case);
        if want.act != "ok" {
            // Excel refused: so must DNA Calc, with Excel's text, leaving the pivot as it was.
            if receipt.accepted() {
                failures.push(format!("{}: Excel refused, DNA Calc accepted", case.0));
            } else if !format!("{receipt:?}").contains("will affect a PivotTable") {
                failures.push(format!("{}: refused without Excel's text: {receipt:?}", case.0));
            }
            assert_eq!(pivot_now(&book), before, "{}: a refused edit changes nothing", case.0);
            continue;
        }
        if !receipt.accepted() {
            failures.push(format!("{}: Excel accepted, DNA Calc refused: {receipt:?}", case.0));
            continue;
        }
        let moved = pivot_now(&book);
        if moved != want.pivots[1] {
            failures.push(format!("{}: after the edit want {} got {moved}", case.0, want.pivots[1]));
        }
        // The refresh (a source edit moves what is read; an output edit must keep the pivot refreshable).
        let result = refresh(&mut book);
        let after = pivot_now(&book);
        let want_after = &want.pivots[2];
        let area = area_on(want_after, sheet(&book, "Out"));
        match result {
            Err(why) => failures.push(format!("{}: refresh refused ({why}); Excel gave {want_after}", case.0)),
            Ok(()) => {
                if &after != want_after {
                    failures.push(format!("{}: after the refresh want {want_after} got {after}", case.0));
                } else {
                    let (d, e) = (dna_cells(&book, area), excel_cells(case.0, area));
                    if d != e {
                        failures.push(format!("{}: refreshed cells differ ({} vs {})", case.0, d.len(), e.len()));
                    }
                }
            }
        }
        // The save a person would make: kept for the Excel replay when DNACALC_PIVOT_SAVES names a folder.
        if let Some(folder) = std::env::var_os("DNACALC_PIVOT_SAVES").map(std::path::PathBuf::from) {
            let mut package = Vec::new();
            if book.save(&SavePolicy::default(), &mut package).is_ok() {
                std::fs::create_dir_all(&folder).unwrap();
                std::fs::write(folder.join(format!("p8-{}.xlsx", case.0)), &package).unwrap();
                let lines: Vec<String> = dna_cells(&book, area)
                    .into_iter()
                    .map(|(a, v)| format!("Out	{a}	{v}"))
                    .collect();
                std::fs::write(folder.join(format!("p8-{}.dna.tsv", case.0)), lines.join("
") + "
").unwrap();
            }
        }
        // Save and reopen keeps what the edit made.
        let mut saved = Vec::new();
        match book.save(&SavePolicy::default(), &mut saved) {
            Ok(_) => {
                let reopened = open(&saved);
                if pivot_now(&reopened) != after {
                    failures.push(format!("{}: reopened {} want {after}", case.0, pivot_now(&reopened)));
                }
            }
            Err(why) => failures.push(format!("{}: save refused: {why:?}", case.0)),
        }
    }
    assert!(failures.is_empty(), "{} of {} scenarios differ:\n{}", failures.len(), EDITS.len(), failures.join("\n"));
}

#[test]
/// Native-only: opens the Excel-saved workbook file of the fixture set and saves it to a package.
fn dc_8wp_16__deleting_all_of_an_output_removes_the_pivot_as_excel_does() {
    // out-del-rows-all and out-del-cols-all: Excel removes the pivot (no table is left on the sheet) and the rows or
    // columns go with the other cells of the sheet, which move up or left.
    for (id, axis, first, count) in [("out-del-rows-all", Axis::Rows, 4, 6), ("out-del-cols-all", Axis::Columns, 3, 6)] {
        let want = excel(id);
        assert_eq!(want.pivots[0], "m5@C4:H9@Src!R2C2:R14C5", "{id}");
        assert_eq!(want.pivots[1], "-", "{id}");
        let mut book = open(BASE_RANGE);
        let out = sheet(&book, "Out");
        let receipt = book.dispatch(SkinIntent::EditAxis(AxisEdit::new(out, axis, AxisChange::Delete, first, count).unwrap()));
        assert!(receipt.accepted(), "{id}: {receipt:?}");
        assert_eq!(pivot_now(&book), want.pivots[1], "{id}");
        let sheet_area = GridArea::new(at(out, "A1"), at(out, "P40")).unwrap();
        assert_eq!(dna_cells(&book, sheet_area), excel_cells_at(id, "1", sheet_area), "{id}: the other cells of the sheet");
        // There is nothing left to refresh, and the save carries no pivot table.
        let mut saved = Vec::new();
        book.save(&SavePolicy::default(), &mut saved).unwrap_or_else(|why| panic!("{id}: save refused: {why:?}"));
        let reopened = open(&saved);
        assert_eq!(pivot_now(&reopened), "-", "{id}: reopened");
        assert_eq!(dna_cells(&reopened, sheet_area), excel_cells_at(id, "1", sheet_area), "{id}: reopened cells");
        let text = String::from_utf8_lossy(&saved);
        assert!(!text.contains("pivotTables/pivotTable"), "{id}: the saved package still names a pivot table part");
    }
}

#[test]
/// Native-only: opens the Excel-saved workbook file of the fixture set.
fn dc_8wp_16__table_source_grows_with_its_table_and_the_next_refresh_reads_the_new_row() {
    // tbl-resize-grow: the row under the table is filled in and the table resized to B2:E15.
    let want = excel("tbl-resize-grow");
    let mut book = open(BASE_TABLE);
    let src = sheet(&book, "Src");
    for (address, text) in [("B15", "West"), ("C15", "Alpha"), ("D15", "500"), ("E15", "9")] {
        assert!(book.dispatch(SkinIntent::EnterCell { at: at(src, address), text: text.into() }).accepted());
    }
    let (part, area) = {
        let table = &book.host().model().definitions().tables()[0];
        (table.part.clone(), table.area)
    };
    assert_eq!(a1(area), "B2:E14");
    let grown = GridArea::new(at(src, "B2"), at(src, "E15")).unwrap();
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::Table {
        revision: book.host().revision().get(),
        change: calc_host::TableChange::Resize { part, area: grown },
    }));
    assert!(receipt.accepted(), "{receipt:?}");
    assert_eq!(want.tables[2], "TblSrc=B2:E15");
    assert_eq!(pivot_now(&book), "m5@C4:H9@TblSrc");
    refresh(&mut book).unwrap();
    assert_eq!(pivot_now(&book), want.pivots[2]);
    let area = area_on(&want.pivots[2], sheet(&book, "Out"));
    assert_eq!(dna_cells(&book, area), excel_cells("tbl-resize-grow", area));
}

#[test]
/// Native-only: opens the Excel-saved workbook files of the fixture set.
fn dc_8wp_16__table_source_follows_row_edits_on_its_sheet() {
    // Rows inserted inside the table, deleted from it, and inserted above its header: Excel's table range and
    // the pivot over it (the pivot's source is the table's name, so it needs no change).
    for (id, change, first, count) in [
        ("tbl-ins-row-inside", AxisChange::Insert, 8, 1),
        ("tbl-ins-row-header", AxisChange::Insert, 2, 1),
        ("tbl-ins-row-after", AxisChange::Insert, 15, 1),
        ("tbl-del-row-inside", AxisChange::Delete, 8, 1),
        ("tbl-del-row-last", AxisChange::Delete, 14, 1),
        ("tbl-ins-col-before", AxisChange::Insert, 1, 1),
    ] {
        let want = excel(id);
        let mut book = open(BASE_TABLE);
        let src = sheet(&book, "Src");
        let axis = if id.contains("-col-") { Axis::Columns } else { Axis::Rows };
        let receipt = book.dispatch(SkinIntent::EditAxis(AxisEdit::new(src, axis, change, first, count).unwrap()));
        assert!(receipt.accepted(), "{id}: {receipt:?}");
        let table = &book.host().model().definitions().tables()[0];
        assert_eq!(format!("{}={}", table.name, a1(table.area)), want.tables[1], "{id}");
        assert_eq!(pivot_now(&book), want.pivots[1], "{id}");
        refresh(&mut book).unwrap_or_else(|why| panic!("{id}: {why}"));
        assert_eq!(pivot_now(&book), want.pivots[2], "{id}");
        let area = area_on(&want.pivots[2], sheet(&book, "Out"));
        assert_eq!(dna_cells(&book, area), excel_cells(id, area), "{id}");
    }
}

#[test]
/// Native-only: opens the Excel-saved workbook files of the fixture set.
fn dc_8wp_16__open_items_a_changed_source_field_count_and_table_column_edits_are_refused() {
    // Excel adds or drops a cache field on the next refresh when a column is inserted into, or deleted from,
    // the source; DNA Calc follows the source range (checked here) but refuses the refresh (open: dc-8wp.16).
    for (id, first, count, change) in [
        ("src-ins-col-inside", 4, 1, AxisChange::Insert),
        ("src-ins-col-last", 5, 1, AxisChange::Insert),
        ("src-del-col-inside-unused", 5, 1, AxisChange::Delete),
    ] {
        let want = excel(id);
        let mut book = open(BASE_RANGE);
        let src = sheet(&book, "Src");
        let receipt = book.dispatch(SkinIntent::EditAxis(AxisEdit::new(src, Axis::Columns, change, first, count).unwrap()));
        assert!(receipt.accepted(), "{id}: {receipt:?}");
        assert_eq!(pivot_now(&book), want.pivots[1], "{id}");
        assert!(refresh(&mut book).unwrap_err().contains("source column count"), "{id}");
    }
    // Deleting a table column through the sheet is refused by the table rules, not by the pivot.
    let mut book = open(BASE_TABLE);
    let src = sheet(&book, "Src");
    let receipt = book.dispatch(SkinIntent::EditAxis(AxisEdit::new(src, Axis::Columns, AxisChange::Delete, 5, 1).unwrap()));
    assert!(!receipt.accepted());
    assert!(format!("{receipt:?}").contains("TablePart"), "{receipt:?}");
}

fn enter(book: &mut HostSession, sheet_name: &str, address: &str, text: &str) {
    let target = sheet(book, sheet_name);
    assert!(book.dispatch(SkinIntent::EnterCell { at: at(target, address), text: text.into() }).accepted(), "{address}");
}
fn set_layout(book: &mut HostSession, rows: Vec<i32>, columns: Vec<i32>) -> Result<(), String> {
    let (sheet, part) = {
        let table = book.host().model().definitions().pivots().tables().next().unwrap().clone();
        (table.area.sheet(), table.part)
    };
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotSetLayout {
        sheet, revision: book.host().revision().get(), part, rows, columns,
    }));
    if receipt.accepted() { Ok(()) } else { Err(format!("{:?}", receipt.error)) }
}

#[test]
/// Native-only: opens the Excel-saved workbook file of the fixture set.
fn dc_8wp_16__output_growing_into_nonblank_cells_collapses_to_one_error_cell_as_excel_does() {
    // Excel (alerts on or off, no dialog over COM) shows the pivot as its top-left cell holding an error when a
    // refresh or a layout change needs cells that are not blank; the pivot keeps its fields and the blocking cells
    // stay. (Decision D4 chose a confirmation; the capture shows Excel has none, so the capture rules.)
    type Setup = fn(&mut HostSession);
    let cases: [(&str, &str, Setup); 6] = [
        ("ovl-nonblank-right", "refresh", |b| { enter(b, "Out", "I5", "blocker-I5"); enter(b, "Src", "C14", "Epsilon"); }),
        ("ovl-nonblank-below", "refresh", |b| { enter(b, "Out", "C10", "blocker-C10"); enter(b, "Src", "B14", "West"); }),
        ("ovl-nonblank-below-two", "refresh", |b| {
            enter(b, "Out", "C11", "blocker-C11"); enter(b, "Src", "B14", "West"); enter(b, "Src", "B13", "Far");
        }),
        ("ovl-nonblank-formula", "refresh", |b| { enter(b, "Out", "I5", "=1+1"); enter(b, "Src", "C14", "Epsilon"); }),
        ("ovl-layout-two-rows-into-nonblank", "layout", |b| { enter(b, "Out", "C14", "blocker-C14"); }),
        ("ovl-layout-into-nonblank-right", "layout", |b| { enter(b, "Out", "K5", "blocker-K5"); }),
    ];
    let mut failures = Vec::new();
    for (id, how, setup) in cases {
        let want = excel(id);
        let mut book = open(BASE_RANGE);
        setup(&mut book);
        assert_eq!(pivot_now(&book), want.pivots[0], "{id}: the start");
        let (stage, result) = if how == "refresh" {
            assert_eq!(pivot_now(&book), want.pivots[1], "{id}: edits alone do not move the pivot");
            (2, refresh(&mut book))
        } else {
            let columns = if id.ends_with("right") { vec![3] } else { vec![] };
            (1, set_layout(&mut book, vec![0, 1], columns))
        };
        if let Err(why) = result {
            failures.push(format!("{id}: refused where Excel collapsed: {why}"));
            continue;
        }
        let want_now = &want.pivots[stage];
        if &pivot_now(&book) != want_now {
            failures.push(format!("{id}: want {want_now} got {}", pivot_now(&book)));
            continue;
        }
        let out = sheet(&book, "Out");
        let area = GridArea::new(at(out, "A1"), at(out, "P40")).unwrap();
        let (dna, excel) = (dna_cells(&book, area), excel_cells_at(id, &stage.to_string(), area));
        if dna != excel {
            failures.push(format!("{id}: cells differ\n  dna   {dna:?}\n  excel {excel:?}"));
        }
        let table = book.host().model().definitions().pivots().tables().next().unwrap().clone();
        assert!(table.collapsed, "{id}");
    }
    assert!(failures.is_empty(), "{} of {} differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
}

#[test]
/// Native-only: opens the Excel-saved workbook file of the fixture set.
fn dc_8wp_16__a_collapsed_pivot_expands_when_room_is_made_and_is_not_saved_collapsed() {
    let mut book = open(BASE_RANGE);
    enter(&mut book, "Out", "I5", "blocker-I5");
    enter(&mut book, "Src", "C14", "Epsilon");
    refresh(&mut book).unwrap();
    assert_eq!(pivot_now(&book), "m5@C4@Src!R2C2:R14C5");
    // The file Excel writes for such a pivot was not captured: the save refuses (its output cell is an error).
    let mut saved = Vec::new();
    let why = book.save(&SavePolicy::default(), &mut saved).unwrap_err();
    assert!(format!("{why:?}").contains("Spill"), "{why:?}");
    // Clearing the blocker makes room: the next refresh gives the pivot its whole output again.
    let out = sheet(&book, "Out");
    enter(&mut book, "Out", "I5", "");
    let _ = out;
    refresh(&mut book).unwrap();
    let table = book.host().model().definitions().pivots().tables().next().unwrap().clone();
    assert!(!table.collapsed);
    assert_eq!(a1(table.area), "C4:I9");
    let mut saved = Vec::new();
    book.save(&SavePolicy::default(), &mut saved).expect("an expanded pivot saves");
}

#[test]
/// Native-only: opens the Excel-saved workbook file of the fixture set.
fn dc_8wp_16__a_new_pivot_takes_one_cell_when_nonblank_cells_are_in_its_way_and_is_refused_over_a_pivot() {
    // ovl-create-onto-nonblank: Row Region, Column Product, Sum of Units at C21 with a blocker in E22.
    let want = excel("ovl-create-onto-nonblank");
    assert!(want.pivots[1].starts_with("other@C21@"), "{}", want.pivots[1]);
    let mut book = open(BASE_RANGE);
    enter(&mut book, "Out", "E22", "blocker-E22");
    let (src, out) = (sheet(&book, "Src"), sheet(&book, "Out"));
    let source = GridArea::new(at(src, "B2"), at(src, "E14")).unwrap();
    let create = |book: &mut HostSession, destination: &str| {
        let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::PivotCreateLayout {
            revision: book.host().revision().get(), source, destination: destination.into(),
            rows: vec![0], columns: vec![1], values: vec![(3, "sum".into())],
        }));
        if receipt.accepted() { Ok(()) } else { Err(format!("{:?}", receipt.error)) }
    };
    create(&mut book, "Out!C21").unwrap();
    let state = book.host().model().definitions().pivots();
    let other = state.tables().find(|t| t.area.first() == at(out, "C21")).expect("the new pivot");
    assert!(other.collapsed);
    assert_eq!(a1(other.area), "C21");
    let area = GridArea::new(at(out, "A1"), at(out, "P40")).unwrap();
    let cells = dna_cells(&book, area);
    assert_eq!(cells.get("C21").map(String::as_str), Some("e\t#SPILL!"));
    assert_eq!(cells.get("E22").map(String::as_str), Some("s\tblocker-E22"));
    // ovl-create-onto-pivot: the destination is inside pvt_m5.
    let want = excel("ovl-create-onto-pivot");
    let mut book = open(BASE_RANGE);
    let before = book.host().revision();
    let why = create(&mut book, "Out!E6").unwrap_err();
    assert!(why.replace("\\\"", "\"").contains(&want.act), "want {:?}, got {why}", want.act);
    assert_eq!(book.host().revision(), before);
}
