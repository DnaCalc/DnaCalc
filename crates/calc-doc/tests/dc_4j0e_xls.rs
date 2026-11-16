//! dc-4j0e: legacy Excel workbooks (.xls) open as the SpreadsheetML package they stand for.
//!
//! The fixtures are built in the test (`xls_kit`): an OLE compound file holding BIFF8 records.
//! Each test states what the file holds and what the loaded workbook must show.

mod xls_kit;

use calc_doc::model::value::{IdAllocator, SheetId};
use calc_doc::model::{ModelRoot, SheetVisibility};
use calc_doc::{
    DateSystem, LoadedWorkbook, MemorySource, OpenError, OpenPolicy, PackageRead, legacy_workbook_report,
};
use std::sync::Arc;
use xls_kit::*;

fn source(bytes: &[u8]) -> Arc<dyn PackageRead> {
    Arc::new(MemorySource::new(bytes.to_vec()))
}

fn load(bytes: &[u8]) -> LoadedWorkbook {
    let document = calc_doc::open(source(bytes), &OpenPolicy::default()).expect("the file opens");
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    calc_doc::load(&document, &mut ids).expect("the workbook loads")
}

fn names(model: &ModelRoot) -> Vec<(String, SheetVisibility)> {
    model
        .sheets()
        .iter()
        .map(|s| (s.name().text().to_string_lossy(), s.visibility()))
        .collect()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sheets_in_order_with_state_and_date_system() {
    let bytes = xls(
        true,
        &[],
        &[
            Sheet::worksheet("First", Vec::new()),
            Sheet {
                state: 1,
                ..Sheet::worksheet("Hidden one", Vec::new())
            },
            Sheet {
                state: 2,
                ..Sheet::worksheet("Très secret ✓", Vec::new())
            },
        ],
    );
    let loaded = load(&bytes);
    assert_eq!(
        names(loaded.model()),
        vec![
            ("First".to_string(), SheetVisibility::Visible),
            ("Hidden one".to_string(), SheetVisibility::Hidden),
            ("Très secret ✓".to_string(), SheetVisibility::VeryHidden),
        ]
    );
    assert_eq!(loaded.date_system(), DateSystem::Date1904);
    let plain = load(&xls(false, &[], &[Sheet::worksheet("S", Vec::new())]));
    assert_eq!(plain.date_system(), DateSystem::Date1900);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_large_workbook_stream_is_read_from_regular_sectors_and_unknown_records_are_counted() {
    // Padding records of an unknown type push the stream past the mini stream cutoff.
    let mut filler = Vec::new();
    for _ in 0..600 {
        filler.extend(record(0x01C1, &[0; 8]));
    }
    let bytes = xls(false, &[], &[Sheet::worksheet("Big", filler)]);
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default())
        .unwrap()
        .expect("a legacy workbook");
    assert_eq!(report.biff_version, 8);
    assert_eq!(report.sheets, 1);
    assert_eq!(report.unknown_records.get(&0x01C1), Some(&600));
    assert_eq!(load(&bytes).model().sheets().len(), 1);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_chart_sheet_and_a_macro_sheet_keep_their_kind_and_a_module_sheet_is_not_a_sheet() {
    let bytes = xls(
        false,
        &[],
        &[
            Sheet::worksheet("Data", Vec::new()),
            Sheet {
                kind: 2,
                ..Sheet::worksheet("Chart1", Vec::new())
            },
            Sheet {
                kind: 1,
                ..Sheet::worksheet("Macro1", Vec::new())
            },
            Sheet {
                kind: 6,
                ..Sheet::worksheet("Module1", Vec::new())
            },
        ],
    );
    let loaded = load(&bytes);
    let sheets = loaded.sheets();
    assert_eq!(sheets.len(), 3);
    assert_eq!(sheets[1].preserved_kind(), Some("chart sheet"));
    assert!(sheets[2].is_macro_sheet());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn the_source_bytes_are_kept() {
    let bytes = xls(false, &[], &[Sheet::worksheet("S", Vec::new())]);
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    assert_eq!(document.legacy_source(), Some(&bytes[..]));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn files_that_are_not_read_are_typed_refusals() {
    // A compound file with no Workbook stream.
    let refused = |bytes: &[u8]| {
        match calc_doc::open(source(bytes), &OpenPolicy::default()) {
            Err(OpenError::LegacyWorkbook { stream, detail }) => (stream, detail),
            other => panic!("expected a legacy refusal, got {:?}", other.err()),
        }
    };
    let (_, detail) = refused(&compound(&[("WordDocument", vec![1, 2, 3])]));
    assert!(detail.contains("no Workbook"), "{detail}");
    // BIFF5/7, which dc-4j0e.8 reads.
    let mut stream = record(0x0809, &u16s(&[0x0500, 0x0005, 0x1234, 0x07CC]));
    stream.extend(eof());
    let (name, detail) = refused(&compound(&[("Book", stream)]));
    assert_eq!(name, "Book");
    assert!(detail.contains("BIFF5/7"), "{detail}");
    // FILEPASS: an encrypted workbook, which dc-4j0e.7 reads.
    let encrypted = xls(false, &record(0x002F, &[0, 0, 1, 0, 1, 0]), &[Sheet::worksheet("S", Vec::new())]);
    let (_, detail) = refused(&encrypted);
    assert!(detail.contains("encrypted"), "{detail}");
    // A truncated compound file and a sheet offset into nowhere.
    let good = xls(false, &[], &[Sheet::worksheet("S", Vec::new())]);
    refused(&good[..600]);
    let mut stream = workbook_stream(false, &[], &[Sheet::worksheet("S", Vec::new())]);
    // BOUNDSHEET's offset field: point it one byte into a record.
    let at = stream.windows(2).position(|w| w == [0x85, 0x00]).unwrap();
    stream[at + 4] = stream[at + 4].wrapping_add(1);
    let (_, detail) = refused(&compound(&[("Workbook", stream)]));
    assert!(detail.contains("where no record does"), "{detail}");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_zip_package_and_other_bytes_are_unchanged() {
    // A source that is neither a compound file nor a ZIP archive is still the ZIP refusal.
    let result = calc_doc::open(source(b"plain text, nothing else"), &OpenPolicy::default());
    assert_eq!(result.err(), Some(OpenError::NotAZipArchive));
    assert!(legacy_workbook_report(source(b"PK\x03\x04 and so on"), &OpenPolicy::default()).unwrap().is_none());
}

fn column_letters(column: u32) -> String {
    let mut n = column;
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push(b'A' + (n % 26) as u8);
        n /= 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

/// The occupied cells of the sheet at `index` as `A1 n:1.5`, `B2 s:text`, `C3 b:true`, `D4 e:Div0`.
fn cells(model: &ModelRoot, index: usize) -> Vec<String> {
    use calc_doc::model::CellInput;
    let id = model.sheets()[index].id();
    model
        .sheet_cells(id)
        .into_iter()
        .flatten()
        .map(|(at, input)| {
            let what = match input {
                CellInput::Number(n) => format!("n:{}", format!("{n:?}").trim_start_matches("FiniteNumber(").trim_end_matches(')')),
                CellInput::Text(t) => format!("s:{}", t.text().to_string_lossy()),
                CellInput::Logical(b) => format!("b:{b}"),
                CellInput::Error(e) => format!("e:{e:?}"),
                CellInput::Formula(f) => format!("f:{f:?}"),
            };
            format!("{}{} {what}", column_letters(at.column()), at.row())
        })
        .collect()
}

fn sheet_cells(bytes: &[u8]) -> Vec<String> {
    cells(load(bytes).model(), 0)
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn numbers_rk_forms_logicals_errors_and_strings() {
    // RK: integer 512; 123 divided by 100; the double 1.5 in the high 30 bits; negative integer
    // -7; a divided double 0.25 stored as 25/100.
    let rk_int = |v: i32| ((v as u32) << 2) | 2;
    let mut r = Vec::new();
    r.extend(number(0, 0, 0.1));
    r.extend(number(0, 1, -1.0e300));
    r.extend(rk_cell(1, 0, rk_int(512)));
    r.extend(rk_cell(1, 1, (123 << 2) | 3));
    r.extend(rk_cell(1, 2, (1.5f64.to_bits() >> 32) as u32 & 0xFFFF_FFFC));
    r.extend(rk_cell(1, 3, rk_int(-7)));
    r.extend(mulrk(2, 0, &[rk_int(1), rk_int(2), (25 << 2) | 3]));
    r.extend(bool_err(3, 0, 1, false));
    r.extend(bool_err(3, 1, 0, false));
    r.extend(bool_err(3, 2, 0x07, true));
    r.extend(bool_err(3, 3, 0x2A, true));
    r.extend(blank(4, 0));
    let globals = sst(&[("alpha", &[]), (" padded ", &[]), ("Ünïcode ✓", &[]), ("a & <b>", &[])], None);
    r.extend(label_sst(5, 0, 0));
    r.extend(label_sst(5, 1, 1));
    r.extend(label_sst(5, 2, 2));
    r.extend(label_sst(5, 3, 3));
    let bytes = xls(false, &globals, &[Sheet::worksheet("S", r)]);
    assert_eq!(
        sheet_cells(&bytes),
        vec![
            "A1 n:0.1", "B1 n:-1e300", "A2 n:512.0", "B2 n:1.23", "C2 n:1.5", "D2 n:-7.0",
            "A3 n:1.0", "B3 n:2.0", "C3 n:0.25", "A4 b:true", "B4 b:false", "C4 e:Div0", "D4 e:NA",
            "A6 s:alpha", "B6 s: padded ", "C6 s:Ünïcode ✓", "D6 s:a & <b>",
        ]
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn the_shared_string_table_split_across_continue_records() {
    // A cut inside the characters of a compressed string, and inside a UTF-16 one, each
    // continuing with its own flag byte; and a cut between strings.
    let strings: [(&str, &[(u16, u16)]); 4] = [
        ("first string", &[]),
        ("second ✓ string", &[(0, 1), (3, 2)]),
        ("third", &[]),
        ("fourth", &[]),
    ];
    let probe = sst(&strings, None);
    let body_len = probe.len() - 4;
    for cut in [8 + 5, 8 + 2 + 1 + 12 + 5, body_len - 3, 8 + 2 + 1 + 12] {
        let globals = sst(&strings, Some(cut));
        let mut r = Vec::new();
        for k in 0..4u16 {
            r.extend(label_sst(k, 0, u32::from(k)));
        }
        let bytes = xls(false, &globals, &[Sheet::worksheet("S", r)]);
        assert_eq!(
            sheet_cells(&bytes),
            vec!["A1 s:first string", "A2 s:second ✓ string", "A3 s:third", "A4 s:fourth"],
            "cut at {cut}"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn rows_and_columns_keep_height_hidden_and_outline() {
    let mut r = Vec::new();
    r.extend(colinfo(1, 2, 3072, 0x0002));
    r.extend(colinfo(4, 4, 2560, 0x0001 | (2 << 8)));
    r.extend(row(0, 255, 0));
    r.extend(row(1, 600, 0x40));
    r.extend(row(2, 255, 0x20 | 1));
    r.extend(number(1, 0, 1.0));
    let bytes = xls(false, &[], &[Sheet::worksheet("S", r)]);
    let loaded = load(&bytes);
    let geometry = loaded.sheets()[0].geometry();
    assert_eq!(geometry.columns.len(), 2);
    let c = &geometry.columns[0];
    assert_eq!((c.first, c.last), (2, 3));
    assert_eq!(c.width, Some(12.0));
    assert!(geometry.columns[1].hidden);
    assert_eq!(geometry.rows.get(&2).map(|r| r.height), Some(Some(30.0)));
    assert!(geometry.rows.get(&3).is_some_and(|r| r.hidden));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_formula_cell_is_a_typed_refusal_until_formulas_are_read() {
    let formula = record(0x0006, &[0; 24]);
    let bytes = xls(false, &[], &[Sheet::worksheet("S", formula)]);
    match calc_doc::open(source(&bytes), &OpenPolicy::default()) {
        Err(OpenError::LegacyWorkbook { detail, .. }) => assert!(detail.contains("dc-4j0e.3"), "{detail}"),
        other => panic!("{:?}", other.err()),
    }
}
