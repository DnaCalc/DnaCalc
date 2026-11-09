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
// The M7 set (names and saves that M5 took from memory): the same two start workbooks.
const M7_NAMES: &str = include_str!("fixtures/pivot/m7-names/names.tsv");
const M7_CELLS: &str = include_str!("fixtures/pivot/m7-names/cells.tsv");
const M7_CORNERS: &str = include_str!("fixtures/pivot/m7-names/corners.tsv");

struct Excel {
    act: String,
    pivots: [String; 5],
    tables: [String; 5],
}

fn excel(id: &str) -> Excel {
    excel_in(SCENARIOS, id)
}
/// The scenario `id` of a scenarios table (m5's `scenarios.tsv` or m7's `names.tsv`).
fn excel_in(table: &str, id: &str) -> Excel {
    let header: Vec<&str> = table.lines().next().unwrap().split('\t').collect();
    let line = table
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
    format!(
        "{}{}:{}{}",
        letters(f.column()),
        f.row(),
        letters(l.column()),
        l.row()
    )
}
fn at(sheet: SheetId, address: &str) -> CellKey {
    let split = address.find(|c: char| c.is_ascii_digit()).unwrap();
    let (l, d) = address.split_at(split);
    CellKey::new(
        sheet,
        d.parse().unwrap(),
        l.bytes()
            .fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1)),
    )
    .unwrap()
}
fn open(bytes: &[u8]) -> HostSession {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    let mut book =
        HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    assert!(book.dispatch(SkinIntent::Recalculate).accepted());
    book
}
fn sheet(book: &HostSession, name: &str) -> SheetId {
    book.host()
        .sheets()
        .iter()
        .find(|s| s.name().text().to_string_lossy() == name)
        .unwrap()
        .id()
}
/// `m5@C4:H9@Src!R2C2:R14C5` as DNA Calc holds it: the output area and the cache source in Excel's spelling.
fn pivot_now(book: &HostSession) -> String {
    let state = book.host().model().definitions().pivots();
    let Some(table) = state.tables().next() else {
        return "-".into();
    };
    let cache = &state.caches.caches[&table.cache_id];
    let source = match (&cache.source_name, &cache.source_sheet, &cache.source_range) {
        (Some(name), _, _) => name.clone(),
        (None, Some(sheet), Some(range)) => {
            let (a, b) = range.split_once(':').unwrap_or((range, range));
            let rc = |cell: &str| {
                let split = cell.find(|c: char| c.is_ascii_digit()).unwrap();
                let col = cell[..split]
                    .bytes()
                    .fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1));
                format!("R{}C{}", &cell[split..], col)
            };
            format!("{sheet}!{}:{}", rc(a), rc(b))
        }
        _ => "?".into(),
    };
    format!(
        "{}@{}@{}",
        table.name.trim_start_matches("pvt_"),
        a1(table.area),
        source
    )
}
fn refresh(book: &mut HostSession) -> Result<(), String> {
    let (sheet, part) = {
        let state = book.host().model().definitions().pivots();
        let table = state.tables().next().unwrap();
        (table.area.sheet(), table.part.clone())
    };
    let receipt = book.dispatch(SkinIntent::Command(
        calc_host::WorkbookCommand::PivotRefresh {
            sheet,
            revision: book.host().revision().get(),
            part,
        },
    ));
    if receipt.accepted() {
        Ok(())
    } else {
        Err(format!("{:?}", receipt.error))
    }
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
    let page = project(
        book.host(),
        &ProjectionRequest {
            cells: keys.clone(),
        },
    )
    .unwrap();
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
    excel_cells_in(CELLS, id, stage, area)
}
fn excel_cells_in(data: &str, id: &str, stage: &str, area: GridArea) -> BTreeMap<String, String> {
    let (top, bottom, left, right) = (
        area.first().row(),
        area.last().row(),
        area.first().column(),
        area.last().column(),
    );
    let mut cells = BTreeMap::new();
    for line in data.lines().skip(1) {
        let f: Vec<&str> = line.splitn(6, '\t').collect();
        if f[0] != id || f[1] != stage || f[2] != "Out" {
            continue;
        }
        let split = f[3].find(|c: char| c.is_ascii_digit()).unwrap();
        let col = f[3][..split]
            .bytes()
            .fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1));
        let row: u32 = f[3][split..].parse().unwrap();
        if (top..=bottom).contains(&row) && (left..=right).contains(&col) {
            let value = if f[4] == "n" {
                // Excel's repr of a float, compared as a number below.
                format!("n\t{}", f[5].parse::<f64>().unwrap())
            } else if f[4] == "e" {
                // The capture holds Excel's raw error code (HRESULT 0x800A0000 + the CVErr number).
                let code = f[5].parse::<i64>().unwrap() + (1 << 32) - 0x800A_0000;
                let name = match code {
                    2000 => "#NULL!",
                    2007 => "#DIV/0!",
                    2015 => "#VALUE!",
                    2023 => "#REF!",
                    2029 => "#NAME?",
                    2036 => "#NUM!",
                    2042 => "#N/A",
                    2045 => "#SPILL!",
                    other => panic!("error code {other}"),
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

fn edit(
    book: &mut HostSession,
    case: &(&str, &str, char, char, u32, u32),
) -> calc_host::IntentReceipt {
    let target = sheet(book, case.1);
    let axis = if case.2 == 'r' {
        Axis::Rows
    } else {
        Axis::Columns
    };
    let change = if case.3 == 'i' {
        AxisChange::Insert
    } else {
        AxisChange::Delete
    };
    book.dispatch(SkinIntent::EditAxis(
        AxisEdit::new(target, axis, change, case.4, case.5).unwrap(),
    ))
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
                failures.push(format!(
                    "{}: refused without Excel's text: {receipt:?}",
                    case.0
                ));
            }
            assert_eq!(
                pivot_now(&book),
                before,
                "{}: a refused edit changes nothing",
                case.0
            );
            continue;
        }
        if !receipt.accepted() {
            failures.push(format!(
                "{}: Excel accepted, DNA Calc refused: {receipt:?}",
                case.0
            ));
            continue;
        }
        let moved = pivot_now(&book);
        if moved != want.pivots[1] {
            failures.push(format!(
                "{}: after the edit want {} got {moved}",
                case.0, want.pivots[1]
            ));
        }
        // The refresh (a source edit moves what is read; an output edit must keep the pivot refreshable).
        let result = refresh(&mut book);
        let after = pivot_now(&book);
        let want_after = &want.pivots[2];
        let area = area_on(want_after, sheet(&book, "Out"));
        match result {
            Err(why) => failures.push(format!(
                "{}: refresh refused ({why}); Excel gave {want_after}",
                case.0
            )),
            Ok(()) => {
                if &after != want_after {
                    failures.push(format!(
                        "{}: after the refresh want {want_after} got {after}",
                        case.0
                    ));
                } else {
                    let (d, e) = (dna_cells(&book, area), excel_cells(case.0, area));
                    if d != e {
                        failures.push(format!(
                            "{}: refreshed cells differ ({} vs {})",
                            case.0,
                            d.len(),
                            e.len()
                        ));
                    }
                }
            }
        }
        // The save a person would make: kept for the Excel replay when DNACALC_PIVOT_SAVES names a folder.
        if let Some(folder) = std::env::var_os("DNACALC_PIVOT_SAVES").map(std::path::PathBuf::from)
        {
            let mut package = Vec::new();
            if book.save(&SavePolicy::default(), &mut package).is_ok() {
                std::fs::create_dir_all(&folder).unwrap();
                std::fs::write(folder.join(format!("p8-{}.xlsx", case.0)), &package).unwrap();
                let lines: Vec<String> = dna_cells(&book, area)
                    .into_iter()
                    .map(|(a, v)| format!("Out	{a}	{v}"))
                    .collect();
                std::fs::write(
                    folder.join(format!("p8-{}.dna.tsv", case.0)),
                    lines.join(
                        "
",
                    ) + "
",
                )
                .unwrap();
            }
        }
        // Save and reopen keeps what the edit made.
        let mut saved = Vec::new();
        match book.save(&SavePolicy::default(), &mut saved) {
            Ok(_) => {
                let reopened = open(&saved);
                if pivot_now(&reopened) != after {
                    failures.push(format!(
                        "{}: reopened {} want {after}",
                        case.0,
                        pivot_now(&reopened)
                    ));
                }
            }
            Err(why) => failures.push(format!("{}: save refused: {why:?}", case.0)),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} scenarios differ:\n{}",
        failures.len(),
        EDITS.len(),
        failures.join("\n")
    );
}

#[test]
/// Native-only: opens the Excel-saved workbook file of the fixture set and saves it to a package.
fn dc_8wp_16__deleting_all_of_an_output_removes_the_pivot_as_excel_does() {
    // out-del-rows-all and out-del-cols-all: Excel removes the pivot (no table is left on the sheet) and the rows or
    // columns go with the other cells of the sheet, which move up or left.
    for (id, axis, first, count) in [
        ("out-del-rows-all", Axis::Rows, 4, 6),
        ("out-del-cols-all", Axis::Columns, 3, 6),
    ] {
        let want = excel(id);
        assert_eq!(want.pivots[0], "m5@C4:H9@Src!R2C2:R14C5", "{id}");
        assert_eq!(want.pivots[1], "-", "{id}");
        let mut book = open(BASE_RANGE);
        let out = sheet(&book, "Out");
        let receipt = book.dispatch(SkinIntent::EditAxis(
            AxisEdit::new(out, axis, AxisChange::Delete, first, count).unwrap(),
        ));
        assert!(receipt.accepted(), "{id}: {receipt:?}");
        assert_eq!(pivot_now(&book), want.pivots[1], "{id}");
        let sheet_area = GridArea::new(at(out, "A1"), at(out, "P40")).unwrap();
        assert_eq!(
            dna_cells(&book, sheet_area),
            excel_cells_at(id, "1", sheet_area),
            "{id}: the other cells of the sheet"
        );
        // There is nothing left to refresh, and the save carries no pivot table.
        let mut saved = Vec::new();
        book.save(&SavePolicy::default(), &mut saved)
            .unwrap_or_else(|why| panic!("{id}: save refused: {why:?}"));
        if let Some(folder) = std::env::var_os("DNACALC_PIVOT_SAVES").map(std::path::PathBuf::from)
        {
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join(format!("p8-{id}.xlsx")), &saved).unwrap();
        }
        let reopened = open(&saved);
        assert_eq!(pivot_now(&reopened), "-", "{id}: reopened");
        assert_eq!(
            dna_cells(&reopened, sheet_area),
            excel_cells_at(id, "1", sheet_area),
            "{id}: reopened cells"
        );
        let text = String::from_utf8_lossy(&saved);
        assert!(
            !text.contains("pivotTables/pivotTable"),
            "{id}: the saved package still names a pivot table part"
        );
    }
}

#[test]
/// Native-only: opens the Excel-saved workbook file of the fixture set.
fn dc_8wp_16__table_source_grows_with_its_table_and_the_next_refresh_reads_the_new_row() {
    // tbl-resize-grow: the row under the table is filled in and the table resized to B2:E15.
    let want = excel("tbl-resize-grow");
    let mut book = open(BASE_TABLE);
    let src = sheet(&book, "Src");
    for (address, text) in [
        ("B15", "West"),
        ("C15", "Alpha"),
        ("D15", "500"),
        ("E15", "9"),
    ] {
        assert!(
            book.dispatch(SkinIntent::EnterCell {
                at: at(src, address),
                text: text.into()
            })
            .accepted()
        );
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
        let axis = if id.contains("-col-") {
            Axis::Columns
        } else {
            Axis::Rows
        };
        let receipt = book.dispatch(SkinIntent::EditAxis(
            AxisEdit::new(src, axis, change, first, count).unwrap(),
        ));
        assert!(receipt.accepted(), "{id}: {receipt:?}");
        let table = &book.host().model().definitions().tables()[0];
        assert_eq!(
            format!("{}={}", table.name, a1(table.area)),
            want.tables[1],
            "{id}"
        );
        assert_eq!(pivot_now(&book), want.pivots[1], "{id}");
        refresh(&mut book).unwrap_or_else(|why| panic!("{id}: {why}"));
        assert_eq!(pivot_now(&book), want.pivots[2], "{id}");
        let area = area_on(&want.pivots[2], sheet(&book, "Out"));
        assert_eq!(dna_cells(&book, area), excel_cells(id, area), "{id}");
    }
}

#[test]
/// Native-only: opens the Excel-saved workbook file of the fixture set.
fn dc_8wp_16__deleting_every_column_of_a_table_is_still_refused_by_the_table_rule() {
    // Columns inserted into or deleted from a table are followed now (the test below). Deleting all of its columns
    // deletes the table, which the table rule still refuses and leaves the workbook as it was.
    let mut book = open(BASE_TABLE);
    let src = sheet(&book, "Src");
    let before = book.host().revision();
    let receipt = book.dispatch(SkinIntent::EditAxis(
        AxisEdit::new(src, Axis::Columns, AxisChange::Delete, 2, 4).unwrap(),
    ));
    assert!(!receipt.accepted());
    assert!(format!("{receipt:?}").contains("TablePart"), "{receipt:?}");
    assert_eq!(book.host().revision(), before);
}

fn enter(book: &mut HostSession, sheet_name: &str, address: &str, text: &str) {
    let target = sheet(book, sheet_name);
    assert!(
        book.dispatch(SkinIntent::EnterCell {
            at: at(target, address),
            text: text.into()
        })
        .accepted(),
        "{address}"
    );
}
fn set_layout(book: &mut HostSession, rows: Vec<i32>, columns: Vec<i32>) -> Result<(), String> {
    let (sheet, part) = {
        let table = book
            .host()
            .model()
            .definitions()
            .pivots()
            .tables()
            .next()
            .unwrap()
            .clone();
        (table.area.sheet(), table.part)
    };
    let receipt = book.dispatch(SkinIntent::Command(
        calc_host::WorkbookCommand::PivotSetLayout {
            sheet,
            revision: book.host().revision().get(),
            part,
            rows,
            columns,
        },
    ));
    if receipt.accepted() {
        Ok(())
    } else {
        Err(format!("{:?}", receipt.error))
    }
}

#[test]
/// Native-only: opens the Excel-saved workbook file of the fixture set.
fn dc_8wp_16__output_growing_into_nonblank_cells_collapses_to_one_error_cell_as_excel_does() {
    // Excel (alerts on or off, no dialog over COM) shows the pivot as its top-left cell holding an error when a
    // refresh or a layout change needs cells that are not blank; the pivot keeps its fields and the blocking cells
    // stay. (Decision D4 chose a confirmation; the capture shows Excel has none, so the capture rules.)
    type Setup = fn(&mut HostSession);
    let cases: [(&str, &str, Setup); 6] = [
        ("ovl-nonblank-right", "refresh", |b| {
            enter(b, "Out", "I5", "blocker-I5");
            enter(b, "Src", "C14", "Epsilon");
        }),
        ("ovl-nonblank-below", "refresh", |b| {
            enter(b, "Out", "C10", "blocker-C10");
            enter(b, "Src", "B14", "West");
        }),
        ("ovl-nonblank-below-two", "refresh", |b| {
            enter(b, "Out", "C11", "blocker-C11");
            enter(b, "Src", "B14", "West");
            enter(b, "Src", "B13", "Far");
        }),
        ("ovl-nonblank-formula", "refresh", |b| {
            enter(b, "Out", "I5", "=1+1");
            enter(b, "Src", "C14", "Epsilon");
        }),
        ("ovl-layout-two-rows-into-nonblank", "layout", |b| {
            enter(b, "Out", "C14", "blocker-C14");
        }),
        ("ovl-layout-into-nonblank-right", "layout", |b| {
            enter(b, "Out", "K5", "blocker-K5");
        }),
    ];
    let mut failures = Vec::new();
    for (id, how, setup) in cases {
        let want = excel(id);
        let mut book = open(BASE_RANGE);
        setup(&mut book);
        assert_eq!(pivot_now(&book), want.pivots[0], "{id}: the start");
        let (stage, result) = if how == "refresh" {
            assert_eq!(
                pivot_now(&book),
                want.pivots[1],
                "{id}: edits alone do not move the pivot"
            );
            (2, refresh(&mut book))
        } else {
            let columns = if id.ends_with("right") {
                vec![3]
            } else {
                vec![]
            };
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
        let (dna, excel) = (
            dna_cells(&book, area),
            excel_cells_at(id, &stage.to_string(), area),
        );
        if dna != excel {
            failures.push(format!(
                "{id}: cells differ\n  dna   {dna:?}\n  excel {excel:?}"
            ));
        }
        let table = book
            .host()
            .model()
            .definitions()
            .pivots()
            .tables()
            .next()
            .unwrap()
            .clone();
        assert!(table.collapsed, "{id}");
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

#[test]
/// Native-only: opens the Excel-saved workbook file of the fixture set.
fn dc_8wp_16__a_collapsed_pivot_expands_when_room_is_made_and_saves_as_excel_saves_it() {
    let mut book = open(BASE_RANGE);
    enter(&mut book, "Out", "I5", "blocker-I5");
    enter(&mut book, "Src", "C14", "Epsilon");
    refresh(&mut book).unwrap();
    assert_eq!(pivot_now(&book), "m5@C4@Src!R2C2:R14C5");
    // The file shape is Excel's (m7-names col-*): the pivot's location is its one cell with no header row, data
    // row or data column, and the fields and items stay; the output cell holds an error.
    let mut saved = Vec::new();
    book.save(&SavePolicy::default(), &mut saved)
        .expect("a collapsed pivot saves");
    if let Some(folder) = std::env::var_os("DNACALC_PIVOT_SAVES").map(std::path::PathBuf::from) {
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("m7-collapsed-refresh.xlsx"), &saved).unwrap();
    }
    let definition = pivot_part(&saved);
    let location = &definition[definition.find("<location").expect("a location")..];
    let location = &location[..location.find("/>").expect("an empty element")];
    for attribute in [
        "ref=\"C4\"",
        "firstHeaderRow=\"0\"",
        "firstDataRow=\"0\"",
        "firstDataCol=\"0\"",
    ] {
        assert!(location.contains(attribute), "{attribute} in {location}");
    }
    assert!(
        definition.contains("<rowFields")
            && definition.contains("<colFields")
            && definition.contains("<dataFields"),
        "{definition}"
    );
    // Reopened, it is the same collapsed pivot with the same corner.
    let mut again = open(&saved);
    assert_eq!(pivot_now(&again), "m5@C4@Src!R2C2:R14C5");
    assert!(
        again
            .host()
            .model()
            .definitions()
            .pivots()
            .tables()
            .next()
            .unwrap()
            .collapsed
    );
    let out = sheet(&again, "Out");
    let corner = dna_cells(&again, GridArea::new(at(out, "C4"), at(out, "C4")).unwrap());
    assert_eq!(corner.get("C4").map(String::as_str), Some("e	#SPILL!"));
    assert_eq!(
        dna_cells(&again, GridArea::new(at(out, "I5"), at(out, "I5")).unwrap())
            .get("I5")
            .map(String::as_str),
        Some("s	blocker-I5")
    );
    // Clearing the blocker makes room: the next refresh gives the pivot its whole output again.
    enter(&mut again, "Out", "I5", "");
    refresh(&mut again).unwrap();
    let table = again
        .host()
        .model()
        .definitions()
        .pivots()
        .tables()
        .next()
        .unwrap()
        .clone();
    assert!(!table.collapsed);
    assert_eq!(a1(table.area), "C4:I9");
    let mut saved = Vec::new();
    again
        .save(&SavePolicy::default(), &mut saved)
        .expect("an expanded pivot saves");
}

/// The text of the first pivot definition part of a saved package.
fn pivot_part(package: &[u8]) -> String {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(package.to_vec()));
    let document = calc_doc::open(source, &OpenPolicy::default()).expect("a package");
    String::from_utf8(
        document
            .package()
            .read_part("xl/pivotTables/pivotTable1.xml")
            .expect("its pivot part"),
    )
    .unwrap()
}

#[test]
/// Native-only: opens the Excel-saved workbook file of the fixture set.
fn dc_8wp_16__a_new_pivot_takes_one_cell_when_nonblank_cells_are_in_its_way_and_is_refused_over_a_pivot()
 {
    // ovl-create-onto-nonblank: Row Region, Column Product, Sum of Units at C21 with a blocker in E22.
    let want = excel("ovl-create-onto-nonblank");
    assert!(
        want.pivots[1].starts_with("other@C21@"),
        "{}",
        want.pivots[1]
    );
    let mut book = open(BASE_RANGE);
    enter(&mut book, "Out", "E22", "blocker-E22");
    let (src, out) = (sheet(&book, "Src"), sheet(&book, "Out"));
    let source = GridArea::new(at(src, "B2"), at(src, "E14")).unwrap();
    let create = |book: &mut HostSession, destination: &str| {
        let receipt = book.dispatch(SkinIntent::Command(
            calc_host::WorkbookCommand::PivotCreateLayout {
                revision: book.host().revision().get(),
                source,
                destination: destination.into(),
                rows: vec![0],
                columns: vec![1],
                values: vec![(3, "sum".into())],
            },
        ));
        if receipt.accepted() {
            Ok(())
        } else {
            Err(format!("{:?}", receipt.error))
        }
    };
    create(&mut book, "Out!C21").unwrap();
    let state = book.host().model().definitions().pivots();
    let other = state
        .tables()
        .find(|t| t.area.first() == at(out, "C21"))
        .expect("the new pivot");
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
    assert!(
        why.replace("\\\"", "\"").contains(&want.act),
        "want {:?}, got {why}",
        want.act
    );
    assert_eq!(book.host().revision(), before);
}

/// (scenario, workbook, change, first column, count): a column edit on the source sheet, then a refresh.
const COLUMN_EDITS: &[(&str, bool, char, u32, u32)] = &[
    // A worksheet-range source Src!B2:E14 (Region, Product, Amount, Units).
    ("src-ins-col-inside", false, 'i', 4, 1),
    ("src-ins-col-last", false, 'i', 5, 1),
    ("src-del-col-inside-unused", false, 'd', 5, 1),
    ("src-del-col-first", false, 'd', 2, 1),
    ("src-del-col-inside-used", false, 'd', 3, 1),
    // A table source TblSrc over the same cells: the table follows the edit, the pivot follows the table.
    ("tbl-ins-col-inside", true, 'i', 4, 1),
    ("tbl-ins-col-first", true, 'i', 2, 1),
    ("tbl-ins-col-last", true, 'i', 5, 1),
    ("tbl-del-col-unused", true, 'd', 5, 1),
    ("tbl-del-col-first", true, 'd', 2, 1),
    ("tbl-del-col-inside-used", true, 'd', 3, 1),
];

#[test]
/// Native-only: opens the Excel-saved workbook files of the fixture set and saves them to packages.
fn dc_8wp_16__a_source_column_inserted_or_deleted_remaps_the_pivot_fields_on_refresh_as_excel_does()
{
    let mut failures = Vec::new();
    for &(id, table, change, first, count) in COLUMN_EDITS {
        let want = excel(id);
        let mut book = open(if table { BASE_TABLE } else { BASE_RANGE });
        let src = sheet(&book, "Src");
        let change = if change == 'i' {
            AxisChange::Insert
        } else {
            AxisChange::Delete
        };
        let receipt = book.dispatch(SkinIntent::EditAxis(
            AxisEdit::new(src, Axis::Columns, change, first, count).unwrap(),
        ));
        if !receipt.accepted() {
            failures.push(format!(
                "{id}: Excel accepted, DNA Calc refused: {receipt:?}"
            ));
            continue;
        }
        if pivot_now(&book) != want.pivots[1] {
            failures.push(format!(
                "{id}: after the edit want {} got {}",
                want.pivots[1],
                pivot_now(&book)
            ));
            continue;
        }
        if table {
            let t = &book.host().model().definitions().tables()[0];
            let now = format!("{}={}", t.name, a1(t.area));
            if now != want.tables[1] {
                failures.push(format!("{id}: the table want {} got {now}", want.tables[1]));
                continue;
            }
        }
        if let Err(why) = refresh(&mut book) {
            failures.push(format!(
                "{id}: refresh refused ({why}); Excel gave {}",
                want.pivots[2]
            ));
            continue;
        }
        let after = pivot_now(&book);
        if after != want.pivots[2] {
            failures.push(format!(
                "{id}: after the refresh want {} got {after}",
                want.pivots[2]
            ));
            continue;
        }
        let area = area_on(&want.pivots[2], sheet(&book, "Out"));
        let (dna, excel) = (dna_cells(&book, area), excel_cells(id, area));
        if dna != excel {
            failures.push(format!(
                "{id}: refreshed cells differ\n  dna   {dna:?}\n  excel {excel:?}"
            ));
        }
        // The save a person would make reopens to the same pivot and cells.
        let mut saved = Vec::new();
        match book.save(&SavePolicy::default(), &mut saved) {
            Ok(_) => {
                if let Some(folder) =
                    std::env::var_os("DNACALC_PIVOT_SAVES").map(std::path::PathBuf::from)
                {
                    std::fs::create_dir_all(&folder).unwrap();
                    std::fs::write(folder.join(format!("p8-{id}.xlsx")), &saved).unwrap();
                }
                let reopened = open(&saved);
                if pivot_now(&reopened) != after {
                    failures.push(format!(
                        "{id}: reopened {} want {after}",
                        pivot_now(&reopened)
                    ));
                } else if dna_cells(&reopened, area) != excel {
                    failures.push(format!("{id}: reopened cells differ"));
                }
            }
            Err(why) => failures.push(format!("{id}: save refused: {why:?}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} scenarios differ:\n{}",
        failures.len(),
        COLUMN_EDITS.len(),
        failures.join("\n")
    );
}

#[test]
/// Native-only: opens the Excel-saved workbook file of the fixture set.
fn dc_8wp_16__a_source_that_leaves_no_value_field_refreshes_to_the_one_cell_error_stub_as_excel_does()
 {
    // src-del-cols-all, src-clear-all, src-del-row-header: the refresh is accepted and the pivot is its top-left cell
    // holding an error; the cells around it stay.
    type Setup = fn(&mut HostSession);
    let cases: [(&str, Setup); 3] = [
        ("src-del-cols-all", |b| {
            let src = sheet(b, "Src");
            assert!(
                b.dispatch(SkinIntent::EditAxis(
                    AxisEdit::new(src, Axis::Columns, AxisChange::Delete, 2, 4).unwrap()
                ))
                .accepted()
            );
        }),
        ("src-clear-all", |b| {
            for row in 2..=14u32 {
                for column in ["B", "C", "D", "E"] {
                    enter(b, "Src", &format!("{column}{row}"), "");
                }
            }
        }),
        ("src-del-row-header", |b| {
            let src = sheet(b, "Src");
            assert!(
                b.dispatch(SkinIntent::EditAxis(
                    AxisEdit::new(src, Axis::Rows, AxisChange::Delete, 2, 1).unwrap()
                ))
                .accepted()
            );
        }),
    ];
    let mut failures = Vec::new();
    for (id, setup) in cases {
        let want = excel(id);
        let mut book = open(BASE_RANGE);
        setup(&mut book);
        if pivot_now(&book) != want.pivots[1] {
            failures.push(format!(
                "{id}: after the edit want {} got {}",
                want.pivots[1],
                pivot_now(&book)
            ));
            continue;
        }
        if let Err(why) = refresh(&mut book) {
            failures.push(format!("{id}: refused where Excel left the stub: {why}"));
            continue;
        }
        if pivot_now(&book) != want.pivots[2] {
            failures.push(format!(
                "{id}: want {} got {}",
                want.pivots[2],
                pivot_now(&book)
            ));
            continue;
        }
        let out = sheet(&book, "Out");
        let area = GridArea::new(at(out, "A1"), at(out, "P40")).unwrap();
        let (dna, excel) = (dna_cells(&book, area), excel_cells(id, area));
        if dna != excel {
            failures.push(format!(
                "{id}: cells differ\n  dna   {dna:?}\n  excel {excel:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

#[test]
/// Native-only: opens the Excel-saved workbook file of the fixture set.
fn dc_8wp_16__a_source_cut_to_its_header_row_refuses_the_refresh_with_excels_text_and_changes_nothing()
 {
    // src-del-rows-all-data: the range is left as the header row alone.
    let want = excel("src-del-rows-all-data");
    let mut book = open(BASE_RANGE);
    let src = sheet(&book, "Src");
    assert!(
        book.dispatch(SkinIntent::EditAxis(
            AxisEdit::new(src, Axis::Rows, AxisChange::Delete, 3, 12).unwrap()
        ))
        .accepted()
    );
    assert_eq!(pivot_now(&book), want.pivots[1]);
    let before = book.host().revision();
    let why = refresh(&mut book).unwrap_err();
    let first_sentence = "This command requires at least two rows of source data. You cannot use the command on a selection in only one row.";
    assert!(why.contains(first_sentence), "{why}");
    assert_eq!(book.host().revision(), before);
    assert_eq!(pivot_now(&book), want.pivots[2]);
    let out = sheet(&book, "Out");
    let area = GridArea::new(at(out, "A1"), at(out, "P40")).unwrap();
    assert_eq!(
        dna_cells(&book, area),
        excel_cells("src-del-rows-all-data", area)
    );
}

// ---- M7: names and saves that M5 took from memory (bead dc-8wp, tests/fixtures/pivot/m7-names) ----

/// `Field1|Field2|...` the cache's field names, as the m7 capture lists a pivot's fields.
fn cache_field_names(book: &HostSession) -> String {
    let state = book.host().model().definitions().pivots();
    let table = state.tables().next().expect("a pivot");
    state.caches.caches[&table.cache_id]
        .fields
        .iter()
        .map(|f| f.name.clone())
        .collect::<Vec<_>>()
        .join("|")
}

#[test]
/// Native-only: opens the Excel-saved workbook files of the fixture set.
fn dc_8wp_16__a_blank_source_header_is_the_field_named_by_its_position_as_excel_names_it() {
    // Excel (16.0.20527, m7-names hdr-*): a pivot over a range whose header cell is blank names that field
    // `Field` and the column's position in the range, not `Column<n>`; a space is a name, a number is its text, and a
    // header that repeats a name is that name with 2.
    type Act = fn(&mut HostSession);
    let cases: [(&str, Act); 7] = [
        ("hdr-blank-unused-last", |b| enter(b, "Src", "E2", "")),
        ("hdr-blank-used-product", |b| enter(b, "Src", "C2", "")),
        ("hdr-blank-first", |b| enter(b, "Src", "B2", "")),
        ("hdr-blank-then-column-before", |b| {
            enter(b, "Src", "E2", "");
            let src = sheet(b, "Src");
            assert!(
                b.dispatch(SkinIntent::EditAxis(
                    AxisEdit::new(src, Axis::Columns, AxisChange::Insert, 1, 1).unwrap()
                ))
                .accepted()
            );
        }),
        ("hdr-space", |b| enter(b, "Src", "E2", " ")),
        ("hdr-number", |b| enter(b, "Src", "E2", "42")),
        ("hdr-duplicate-name", |b| enter(b, "Src", "E2", "Region")),
    ];
    let mut failures = Vec::new();
    for (id, act) in cases {
        let want = excel_in(M7_NAMES, id);
        let mut book = open(BASE_RANGE);
        act(&mut book);
        if let Err(why) = refresh(&mut book) {
            failures.push(format!(
                "{id}: refresh refused ({why}); Excel gave {}",
                want.pivots[2]
            ));
            continue;
        }
        let now = format!("{}@{}", pivot_now(&book), cache_field_names(&book));
        if now != want.pivots[2] {
            failures.push(format!("{id}: want {} got {now}", want.pivots[2]));
            continue;
        }
        let area = area_on(&want.pivots[2], sheet(&book, "Out"));
        let (dna, excel) = (
            dna_cells(&book, area),
            excel_cells_in(M7_CELLS, id, "2", area),
        );
        if dna != excel {
            failures.push(format!(
                "{id}: cells differ\n  dna   {dna:?}\n  excel {excel:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

#[test]
/// Native-only: opens the Excel-saved workbook files of the fixture set.
fn dc_8wp_16__a_source_that_keeps_row_fields_but_loses_its_value_fields_is_refused_not_guessed() {
    // Excel keeps the row and column fields and draws labels without values (m7-names stub-delete-value-column,
    // stub-rename-value-header, stub-table-delete-value-column, hdr-blank-used-amount, hdr-blank-two). That layout is
    // not modelled, so the refresh is refused with its reason and changes nothing; it used to give the one-cell
    // stub, which Excel does not.
    type Act = fn(&mut HostSession);
    let cases: [(&str, bool, Act); 5] = [
        ("stub-delete-value-column", false, |b| {
            let src = sheet(b, "Src");
            assert!(
                b.dispatch(SkinIntent::EditAxis(
                    AxisEdit::new(src, Axis::Columns, AxisChange::Delete, 4, 1).unwrap()
                ))
                .accepted()
            );
        }),
        ("stub-rename-value-header", false, |b| {
            enter(b, "Src", "D2", "Total")
        }),
        ("stub-table-delete-value-column", true, |b| {
            let src = sheet(b, "Src");
            assert!(
                b.dispatch(SkinIntent::EditAxis(
                    AxisEdit::new(src, Axis::Columns, AxisChange::Delete, 4, 1).unwrap()
                ))
                .accepted()
            );
        }),
        ("hdr-blank-used-amount", false, |b| {
            enter(b, "Src", "D2", "")
        }),
        ("hdr-blank-two", false, |b| {
            enter(b, "Src", "D2", "");
            enter(b, "Src", "E2", "");
        }),
    ];
    for (id, table, act) in cases {
        let want = excel_in(M7_NAMES, id);
        assert!(
            want.pivots[2].contains("@Region|") || want.pivots[2].contains("@Region"),
            "{id}: Excel kept the Region field"
        );
        let mut book = open(if table { BASE_TABLE } else { BASE_RANGE });
        act(&mut book);
        let before = book.host().revision();
        let why = refresh(&mut book).expect_err(id);
        assert!(why.contains("labels without values"), "{id}: {why}");
        assert_eq!(book.host().revision(), before, "{id}");
    }
}

#[test]
/// Native-only: opens the Excel-saved workbook files of the fixture set.
fn dc_8wp_16__a_source_that_loses_every_field_gives_the_empty_pivot_frame_unless_cells_are_in_the_way()
 {
    // Excel (m7-names stub-clear-source, stub-delete-all-columns, markers far away): the pivot has no field left and
    // is the frame C4:E21 of empty cells. The M5 scenarios src-clear-all and src-del-cols-all had markers in the
    // frame, which is why Excel gave one error cell there (tested above).
    type Act = fn(&mut HostSession);
    let cases: [(&str, Act); 2] = [
        ("stub-clear-source", |b| {
            for row in 2..=14u32 {
                for column in ["B", "C", "D", "E"] {
                    enter(b, "Src", &format!("{column}{row}"), "");
                }
            }
        }),
        ("stub-delete-all-columns", |b| {
            let src = sheet(b, "Src");
            assert!(
                b.dispatch(SkinIntent::EditAxis(
                    AxisEdit::new(src, Axis::Columns, AxisChange::Delete, 2, 4).unwrap()
                ))
                .accepted()
            );
        }),
    ];
    for (id, act) in cases {
        let want = excel_in(M7_NAMES, id);
        let mut book = open(BASE_RANGE);
        // The m7 start workbook has its markers far from the pivot.
        for marker in ["C2", "B5", "J5", "C12"] {
            enter(&mut book, "Out", marker, "");
        }
        enter(&mut book, "Out", "A60", "m-far-below");
        enter(&mut book, "Out", "Z5", "m-far-right");
        act(&mut book);
        refresh(&mut book).unwrap_or_else(|why| panic!("{id}: {why}"));
        let now = format!("{}@{}", pivot_now(&book), cache_field_names(&book));
        assert_eq!(now, want.pivots[2], "{id}");
        let table = book
            .host()
            .model()
            .definitions()
            .pivots()
            .tables()
            .next()
            .unwrap()
            .clone();
        assert!(!table.collapsed, "{id}");
        assert_eq!(a1(table.area), "C4:E21", "{id}");
        let out = sheet(&book, "Out");
        let area = GridArea::new(at(out, "A1"), at(out, "P40")).unwrap();
        assert_eq!(
            dna_cells(&book, area),
            excel_cells_in(M7_CELLS, id, "2", area),
            "{id}"
        );
        // The frame saves, and the file reopens to the same empty pivot.
        let mut saved = Vec::new();
        book.save(&SavePolicy::default(), &mut saved)
            .unwrap_or_else(|why| panic!("{id}: {why:?}"));
        if let Some(folder) = std::env::var_os("DNACALC_PIVOT_SAVES").map(std::path::PathBuf::from)
        {
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join(format!("m7-{id}.xlsx")), &saved).unwrap();
        }
        let again = open(&saved);
        assert_eq!(
            format!("{}@{}", pivot_now(&again), cache_field_names(&again)),
            want.pivots[2],
            "{id}: reopened"
        );
    }
}

#[test]
/// Native-only: reads the m7 capture of the collapsed pivot's corner.
fn dc_8wp_16__the_collapsed_corner_is_spill_live_and_reopened_and_value_in_the_file_as_excel_has_it()
 {
    // m7-names corners.tsv: in every stage of every collapsed scenario (built, refreshed, saved and reopened,
    // reopened and refreshed) the corner reads Value2 -2146826243 (2045, #SPILL!), Text and Formula "#SPILL!". The
    // `#VALUE!` the UI28 capture holds for it is how Excel's file spells the corner (`t="e"`, `<v>#VALUE!</v>` and
    // `vm`, with the value metadata of an `_error` rich value), which the UI28 comparison reads.
    let mut collapsed = 0;
    for line in M7_CORNERS.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        if !f[0].starts_with("col-") || f[6] != "#SPILL!" {
            continue;
        }
        collapsed += 1;
        assert_eq!(
            (f[3], f[4], f[5]),
            ("e", "-2146826243", "#SPILL!"),
            "{line}"
        );
    }
    assert!(collapsed >= 20, "{collapsed}");
}

/// `TblSrc=B2:F14=Region|Product|...` the first table of the workbook, as the m7 capture lists a table.
fn table_now(book: &HostSession) -> String {
    let t = &book.host().model().definitions().tables()[0];
    format!("{}={}={}", t.name, a1(t.area), t.columns.join("|"))
}

#[test]
/// Native-only: opens the Excel-saved workbook files of the fixture set.
fn dc_8wp_16__a_column_inserted_into_a_table_is_named_after_its_left_neighbour_as_excel_names_it() {
    // m7-names tbl-col-sheet-insert-*: Excel names the column `Column<n>` after the column on its left; the
    // pivot over the table takes the name at its next refresh. (The naming rule over more neighbours is tested
    // on the rule itself, calc-doc-model's table_names, from the same capture.)
    let cases: [(&str, &[u32]); 2] = [
        ("tbl-col-sheet-insert-inside", &[4]),
        ("tbl-col-sheet-insert-first", &[3, 3]),
    ];
    for (id, inserts) in cases {
        let want = excel_in(M7_NAMES, id);
        let mut book = open(BASE_TABLE);
        let src = sheet(&book, "Src");
        for column in inserts {
            let receipt = book.dispatch(SkinIntent::EditAxis(
                AxisEdit::new(src, Axis::Columns, AxisChange::Insert, *column, 1).unwrap(),
            ));
            assert!(receipt.accepted(), "{id}: {receipt:?}");
        }
        assert_eq!(table_now(&book), want.tables[1], "{id}: after the edit");
        refresh(&mut book).unwrap_or_else(|why| panic!("{id}: {why}"));
        assert_eq!(
            format!("{}@{}", pivot_now(&book), cache_field_names(&book)),
            want.pivots[2],
            "{id}: refreshed"
        );
    }
}

#[test]
/// Native-only: opens the Excel-saved workbook files of the fixture set.
fn dc_8wp_16__blank_headers_of_a_new_table_take_the_smallest_free_column_numbers() {
    // Excel names a cleared table header `Column` and the smallest number no other header has, in column order
    // (m7-names hdr-table-*, tbln-blank-*; the rule itself is tested in calc-doc-model's table_names). Creating a
    // table over blank headers was not captured: it follows the same rule here, and was `Column<position>`.
    let mut book = open(BASE_RANGE);
    for (cell, text) in [
        ("G5", "a"),
        ("H5", ""),
        ("I5", "Column1"),
        ("J5", ""),
        ("G6", "1"),
        ("H6", "2"),
        ("I6", "3"),
        ("J6", "4"),
    ] {
        enter(&mut book, "Src", cell, text);
    }
    let src = sheet(&book, "Src");
    let area = GridArea::new(at(src, "G5"), at(src, "J6")).unwrap();
    let revision = book.host().revision().get();
    let receipt = book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::Table {
        revision,
        change: calc_host::TableChange::Create {
            area,
            name: "TblNew".into(),
        },
    }));
    assert!(receipt.accepted(), "{receipt:?}");
    let table = book
        .host()
        .model()
        .definitions()
        .tables()
        .iter()
        .find(|t| t.name == "TblNew")
        .unwrap()
        .clone();
    assert_eq!(table.columns.join("|"), "a|Column2|Column1|Column3");
}
