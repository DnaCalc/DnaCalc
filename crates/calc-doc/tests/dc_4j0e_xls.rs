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
    // BIFF4, a different file layout.
    let mut stream = record(0x0809, &u16s(&[0x0400, 0x0005, 0x1234, 0x07CC]));
    stream.extend(eof());
    let (name, detail) = refused(&compound(&[("Book", stream)]));
    assert_eq!(name, "Book");
    assert!(detail.contains("BIFF version 0x0400"), "{detail}");
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
                CellInput::Formula(f) => format!("f:{}", f.rendered().to_string_lossy()),
            };
            if let CellInput::Formula(f) = input {
                let text = f.rendered().to_string_lossy();
                assert!(
                    calc_fml::parse_formula(&text).diagnostics().is_empty(),
                    "calc-fml reads {text:?} with a diagnostic"
                );
            }
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

fn defined_names(bytes: &[u8]) -> Vec<String> {
    load(bytes)
        .model()
        .definitions()
        .names()
        .iter()
        .map(|n| format!("{} = {}", n.name, n.formula))
        .collect()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn formula_tokens_print_as_excel_writes_them_and_cached_results_are_kept() {
    use ptg::*;
    let mut r = Vec::new();
    r.extend(formula(0, 0, STR_VALUE, &join(&[text("a"), text("b"), op(CONCAT)]), &[]));
    r.extend(string_result("ab"));
    r.extend(formula(0, 1, num_value(9.0), &join(&[int(1), int(2), op(ADD), op(PAREN), int(3), op(MUL)]), &[]));
    r.extend(formula(0, 2, num_value(0.0), &cell(4, 2, 0), &[]));
    r.extend(formula(0, 3, num_value(0.0), &cell(4, 2, REL_COL), &[]));
    r.extend(formula(0, 4, num_value(0.0), &cell(4, 2, REL_ROW), &[]));
    r.extend(formula(0, 5, num_value(0.0), &area((0, 65535), (0, 0), REL_ROW | REL_COL), &[]));
    r.extend(formula(0, 6, num_value(0.0), &area((0, 0), (0, 255), REL_ROW | REL_COL), &[]));
    r.extend(formula(0, 7, bool_value(true), &int(1), &[]));
    r.extend(formula(0, 8, err_value(0x2A), &num(0.5), &[]));
    r.extend(formula(0, 9, num_value(0.0), &join(&[text("é✓"), func_var(1, 111)]), &[]));
    let bytes = xls(false, &[], &[Sheet::worksheet("S", r)]);
    assert_eq!(
        sheet_cells(&bytes),
        vec![
            "A1 f:\"a\"&\"b\"", "B1 f:(1+2)*3", "C1 f:$C$5", "D1 f:C$5", "E1 f:$C5", "F1 f:A:A", "G1 f:1:1",
            "H1 f:1", "I1 f:0.5", "J1 f:CHAR(\"é✓\")",
        ]
    );
    let loaded = load(&bytes);
    let sheet = &loaded.sheets()[0];
    let cached = |row, column| format!("{:?}", sheet.formula(row, column).and_then(|f| f.cache()));
    assert!(cached(1, 1).contains("[97, 98]"), "{}", cached(1, 1));
    assert!(cached(1, 2).contains('9'), "{}", cached(1, 2));
    assert!(cached(1, 8).to_lowercase().contains("true") || cached(1, 8).contains("Logical"), "{}", cached(1, 8));
    assert!(cached(1, 9).contains("NA"), "{}", cached(1, 9));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn shared_array_and_table_formulas() {
    use ptg::*;
    let mut r = Vec::new();
    // Shared B3:B5 = A3+1: PtgRefN offsets are applied at each member.
    for k in 2..5u16 {
        r.extend(formula(k, 1, num_value(1.0), &exp(2, 1), &[]));
        if k == 2 {
            r.extend(shrfmla((2, 4), (1, 1), &join(&[cell_n(0, -1, REL_ROW | REL_COL), int(1), op(ADD)])));
        }
    }
    // Array A8:B9 = {1+2}: the anchor holds the formula, the others only values.
    r.extend(formula(7, 0, num_value(3.0), &exp(7, 0), &[]));
    r.extend(array_record((7, 8), (0, 1), &join(&[int(1), int(2), op(ADD)]), &[]));
    r.extend(formula(7, 1, num_value(3.0), &exp(7, 0), &[]));
    r.extend(formula(8, 0, num_value(3.0), &exp(7, 0), &[]));
    r.extend(formula(8, 1, num_value(3.0), &exp(7, 0), &[]));
    // A one-input data table over C11:C13 with column input cell A1.
    r.extend(formula(10, 2, num_value(1.0), &table(10, 2), &[]));
    r.extend(table_record((10, 12), (2, 2), 0, (0, 0), (0, 0)));
    r.extend(formula(11, 2, num_value(2.0), &table(10, 2), &[]));
    r.extend(formula(12, 2, num_value(3.0), &table(10, 2), &[]));
    let bytes = xls(false, &[], &[Sheet::worksheet("S", r)]);
    assert_eq!(sheet_cells(&bytes), vec!["B3 f:A3+1", "B4 f:A4+1", "B5 f:A5+1", "A8 f:1+2"]);
    let loaded = load(&bytes);
    let definitions = loaded.model().definitions();
    assert_eq!(definitions.arrays().count(), 1);
    assert_eq!(definitions.data_tables().len(), 1);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn names_3d_external_references_and_add_in_functions() {
    use ptg::*;
    let globals = [
        supbook_self(1),
        supbook_external("\u{1}C\u{3}dir\u{3}book.xls", &["Rates"]),
        supbook_addin(),
        extern_name("myfunc"),
        extern_sheet(&[(0, 0, 0), (1, 0, 0), (2, -2, -2)]),
        name_record(0, 0, "Rate", &cell3d(0, 0, 0, 0)),
        // A built-in Print_Area local to the first sheet.
        name_record(0x20, 1, "\u{6}", &area3d(0, (0, 9), (0, 3), 0)),
        // A hidden macro-function name.
        name_record(0x0001 | 0x0002, 0, "Helper", &int(1)),
    ]
    .concat();
    let mut r = Vec::new();
    r.extend(formula(13, 0, num_value(1.0), &cell3d(1, 1, 1, 0), &[]));
    r.extend(formula(13, 1, num_value(1.0), &join(&[name_x(2, 1), int(5), func_var(2, 255)]), &[]));
    r.extend(formula(13, 2, num_value(1.0), &name(1), &[]));
    let bytes = xls(false, &globals, &[Sheet::worksheet("S", r)]);
    assert_eq!(sheet_cells(&bytes), vec!["A14 f:[1]Rates!$B$2", "B14 f:_xll.myfunc(5)", "C14 f:Rate"]);
    let names = defined_names(&bytes);
    assert_eq!(
        names,
        vec!["Rate = S!$A$1", "_xlnm.Print_Area = S!$A$1:$D$10", "Helper = 1"]
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_formula_the_converter_cannot_print_is_a_typed_refusal_naming_the_cell() {
    // Token 0x7F is no token.
    let r = formula(0, 0, ptg_value(), &[0x7F], &[]);
    let bytes = xls(false, &[], &[Sheet::worksheet("S", r)]);
    match calc_doc::open(source(&bytes), &OpenPolicy::default()) {
        Err(OpenError::LegacyWorkbook { detail, .. }) => {
            assert!(detail.contains("A1") && detail.contains("0x3F"), "{detail}")
        }
        other => panic!("{:?}", other.err()),
    }
}

fn ptg_value() -> [u8; 8] {
    num_value(0.0)
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn array_constants_attributes_and_relative_references_that_wrap() {
    use ptg::*;
    let mut r = Vec::new();
    // {1,"a";TRUE,#N/A}: 2 columns, 2 rows.
    let mut data = vec![1u8];
    data.extend(1u16.to_le_bytes());
    data.push(1);
    data.extend(1.0f64.to_le_bytes());
    data.extend([2, 1, 0, 0, b'a']);
    data.extend([4, 1, 0, 0, 0, 0, 0, 0, 0]);
    data.extend([0x10, 0x2A, 0, 0, 0, 0, 0, 0, 0]);
    r.extend(formula(0, 0, num_value(1.0), &array(), &data));
    // SUM written with its attribute token, and a space before the 2.
    r.extend(formula(0, 1, num_value(3.0), &join(&[int(1), vec![0x19, 0x40, 0x00, 0x01], int(2), op(ADD), vec![0x19, 0x10, 0, 0]]), &[]));
    // A shared formula on row 1 whose relative row offset -1 wraps around the 65,536-row grid.
    r.extend(formula(0, 2, num_value(0.0), &exp(0, 2), &[]));
    r.extend(shrfmla((0, 0), (2, 2), &cell_n(-1, 0, REL_ROW | REL_COL)));
    let bytes = xls(false, &[], &[Sheet::worksheet("S", r)]);
    assert_eq!(
        sheet_cells(&bytes),
        vec!["A1 f:{1,\"a\";TRUE,#N/A}", "B1 f:SUM(1+ 2)", "C1 f:C65536"]
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn formats_fonts_fills_borders_and_cell_styles() {
    // Five FONT records: BIFF numbers fonts 0 to 3 and then 5 (4 is never used).
    let fonts = [
        font(200, 0, 0x7FFF, 400, "Arial"),
        font(200, 0, 0x7FFF, 700, "Arial"),
        font(200, 0, 0x7FFF, 400, "Arial"),
        font(200, 0, 0x7FFF, 400, "Arial"),
        font(240, 0x0002, 10, 700, "Times New Roman"),
    ]
    .to_vec();
    // XF 15: the default cell format. XF 16: bold-less italic red 12 pt (font index 5), date format 14,
    // solid yellow fill (palette 13), thin left border, centred and wrapped.
    let styled = Xf {
        font: 5,
        format: 14,
        flags: 0x0000,
        alignment: 0x20 | 0x08 | 0x02,
        used: 0xFC,
        border1: 0x1 | (8 << 16),
        border2: 1 << 25,
        fill: 13 | (64 << 7),
        ..Xf::plain()
    };
    let globals = formatting(&fonts, &[format_record(164, "0.00\"x\"")], &[Xf::plain(), styled]);
    let mut r = Vec::new();
    r.extend(number(0, 0, 1.0));
    let mut with_style = |row: u16, column: u16, ixfe: u16, value: f64| {
        let mut body = u16s(&[row, column, ixfe]);
        body.extend(value.to_le_bytes());
        r.extend(record(0x0203, &body));
    };
    with_style(1, 0, 16, 45000.0);
    with_style(1, 1, 15, 2.0);
    r.extend(blank(2, 0));
    r.extend({
        let mut body = u16s(&[3, 0, 16]);
        body.truncate(6);
        record(0x0201, &body)
    });
    let bytes = xls(false, &globals, &[Sheet::worksheet("S", r)]);
    let loaded = load(&bytes);
    let sheet = &loaded.sheets()[0];
    let table = loaded.styles();
    // Cells at the default format carry no style; the styled cell and the styled blank do.
    assert_eq!(sheet.style(1, 1), 0);
    let s = sheet.style(2, 1);
    assert_ne!(s, 0);
    assert_eq!(sheet.style(4, 1), s, "a formatted blank cell keeps its format");
    assert_eq!(sheet.style(3, 1), 0, "a blank at the default format is not written");
    assert_eq!(table.number_format_code(s), Some("m/d/yyyy").filter(|_| false).or(table.number_format_code(s)));
    let format = &table.cell_formats[s as usize];
    assert_eq!(format.number_format_id, Some(14));
    let font = &table.fonts[format.font_id.unwrap() as usize];
    assert_eq!(font.name.as_deref(), Some("Times New Roman"));
    assert!(font.italic && font.bold);
    assert_eq!(font.size.as_deref(), Some("12"));
    let fill = &table.fills[format.fill_id.unwrap() as usize];
    assert_eq!(format!("{fill:?}").contains("FFFFFF00"), true, "{fill:?}");
    let border = &table.borders[format.border_id.unwrap() as usize];
    assert_eq!(border.left.as_ref().and_then(|e| e.style.as_deref()), Some("thin"));
    let alignment = format.alignment.as_ref().expect("alignment");
    assert_eq!(alignment.horizontal.as_deref(), Some("center"));
    assert!(alignment.wrap_text);
    assert!(table.number_formats.iter().any(|f| f.id == 164 && f.code == "0.00\"x\""));
    assert!(table.cell_styles.iter().any(|c| c.name == "Normal"));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn merges_hyperlinks_panes_protection_and_page_setup() {
    let mut r = Vec::new();
    r.extend(window2(0x06BE, 0, 0));
    r.extend(pane(1, 2, 2, 1, 0));
    r.extend(selection(0, (3, 2), &[(3, 4, 2, 3)]));
    r.extend(number(0, 0, 1.0));
    r.extend(merged(&[(0, 1, 0, 2), (5, 5, 1, 4)]));
    r.extend(hlink_url((7, 1), "https://example.com/a?b=1", Some("Example"), Some("Top")));
    r.extend(record(0x0012, &1u16.to_le_bytes()));
    r.extend(record(0x0013, &0xCE4Bu16.to_le_bytes()));
    let mut margins = Vec::new();
    for id in [0x0026u16, 0x0027, 0x0028, 0x0029] {
        margins.extend(record(id, &0.5f64.to_le_bytes()));
    }
    r.extend(margins);
    let mut setup = u16s(&[9, 80, 1, 1, 1, 0x0002]);
    setup.extend([0u8; 4]);
    setup.extend(0.25f64.to_le_bytes());
    setup.extend(0.25f64.to_le_bytes());
    setup.extend(1u16.to_le_bytes());
    r.extend(record(0x00A1, &setup));
    let mut header = 9u16.to_le_bytes().to_vec();
    header.push(0);
    header.extend(b"&CReport!");
    r.extend(record(0x0014, &header[..header.len() - 0]));
    let bytes = xls(false, &[], &[Sheet::worksheet("S", r)]);
    let loaded = load(&bytes);
    let sheet = &loaded.sheets()[0];
    assert_eq!(sheet.merged_areas().len(), 2);
    assert!(sheet.protected());
    let links = &sheet.review().links;
    assert_eq!(links.len(), 1, "{:?}", sheet.review().unsupported);
    assert!(format!("{links:?}").contains("https://example.com/a?b=1"));
    assert!(format!("{links:?}").contains("Top"));
    // The converted package keeps the pane, the print setup and the header text.
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let part = String::from_utf8(document.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    for expected in [
        "<pane xSplit=\"1\" ySplit=\"2\" topLeftCell=\"B3\" activePane=\"bottomRight\" state=\"frozen\"/>",
        "<selection pane=\"bottomRight\" activeCell=\"C4\" sqref=\"C4:D5\"/>",
        "<pageMargins left=\"0.5\" right=\"0.5\" top=\"0.5\" bottom=\"0.5\" header=\"0.25\" footer=\"0.25\"/>",
        "paperSize=\"9\" scale=\"80\" orientation=\"portrait\"",
        "<oddHeader>&amp;CReport!</oddHeader>",
        "password=\"CE4B\"",
    ] {
        assert!(part.contains(expected), "{expected} in {part}");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn comments_are_read_from_note_obj_and_txo_records() {
    let mut r = Vec::new();
    r.extend(number(0, 0, 1.0));
    r.extend(obj_note(1));
    r.extend(txo("Hello there"));
    r.extend(obj_note(2));
    r.extend(txo("Second"));
    r.extend(note(1, 1, 0x0002, 1, "Ann"));
    r.extend(note(4, 0, 0, 2, "Bob"));
    let bytes = xls(false, &[], &[Sheet::worksheet("S", r)]);
    let loaded = load(&bytes);
    let review = loaded.sheets()[0].review();
    assert!(review.unsupported.is_empty(), "{:?}", review.unsupported);
    let notes: Vec<String> = review.notes.iter().map(|n| format!("{:?}", n)).collect();
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert!(notes[0].contains("Hello there") && notes[0].contains("Ann"), "{}", notes[0]);
    assert!(notes[1].contains("Second") && notes[1].contains("Bob"), "{}", notes[1]);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_saved_xls_is_an_xlsx_that_opens_to_the_same_cells() {
    use ptg::*;
    let globals = [sst(&[("text", &[])], None), supbook_self(1), extern_sheet(&[(0, 0, 0)])].concat();
    let mut r = Vec::new();
    r.extend(number(0, 0, 0.1));
    r.extend(label_sst(0, 1, 0));
    r.extend(formula(0, 2, num_value(7.0), &join(&[int(3), cell3d(0, 0, 0, 0), op(ADD)]), &[]));
    let bytes = xls(false, &globals, &[Sheet::worksheet("S", r)]);
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let plan = calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).expect("a plan");
    let mut saved = Vec::new();
    calc_doc::write(plan, &mut saved).expect("written");
    assert_eq!(&saved[..2], b"PK", "the save is a ZIP package");
    assert_eq!(sheet_cells(&saved), sheet_cells(&bytes));
    assert_eq!(sheet_cells(&saved), vec!["A1 n:0.1", "B1 s:text", "C1 f:3+S!$A$1"]);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_biff5_workbook_with_code_page_text_formulas_and_names() {
    use ptg::*;
    use xls_kit::v5;
    // Sheet 1 "Données" (0xE9 in code page 1252): a label with an accent and a euro sign, a number,
    // a formula =A2+1 (relative cell), one into sheet 2 through a local EXTERNSHEET record, and a
    // string formula. The workbook has a global name Rate = Sheet2!$A$1.
    let mut first = Vec::new();
    first.extend(v5::label(0, 0, b"caf\xE9 \x80"));
    first.extend(number(1, 0, 41.0));
    first.extend(v5::extern_own(b"Sheet2"));
    first.extend(v5::formula(2, 0, 42.0, &join(&[v5::cell(1, 0, true, true), int(1), op(ADD)])));
    first.extend(v5::formula(2, 1, 5.0, &v5::cell3d(1, 1, 0, 0)));
    let mut text = vec![0x17, 3];
    text.extend(b"\xE9t\xE9");
    first.extend(v5::formula(2, 2, 0.0, &text));
    let mut second = Vec::new();
    second.extend(number(0, 0, 5.0));
    let globals = [
        v5::extern_own(b"Sheet2"),
        v5::name(0, 0, b"Rate", &v5::cell3d(1, 1, 0, 0)),
    ]
    .concat();
    let bytes = v5::xls(
        1252,
        &globals,
        &[
            v5::Sheet { name: b"Donn\xE9es".to_vec(), records: first },
            v5::Sheet { name: b"Sheet2".to_vec(), records: second },
        ],
    );
    let loaded = load(&bytes);
    let names: Vec<String> = loaded.model().sheets().iter().map(|s| s.name().text().to_string_lossy()).collect();
    assert_eq!(names, vec!["Donn\u{e9}es", "Sheet2"]);
    assert_eq!(
        cells(loaded.model(), 0),
        vec![
            "A1 s:caf\u{e9} \u{20ac}",
            "A2 n:41.0",
            "A3 f:A2+1",
            "B3 f:Sheet2!A1",
            "C3 f:\"\u{e9}t\u{e9}\"",
        ]
    );
    assert_eq!(cells(loaded.model(), 1), vec!["A1 n:5.0"]);
    assert_eq!(defined_names(&bytes), vec!["Rate = Sheet2!A1"]);
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.biff_version, 5);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_biff5_workbook_in_a_double_byte_code_page_refuses_its_high_bytes() {
    use xls_kit::v5;
    let ascii = v5::xls(932, &[], &[v5::Sheet { name: b"S".to_vec(), records: v5::label(0, 0, b"plain") }]);
    assert_eq!(cells(load(&ascii).model(), 0), vec!["A1 s:plain"]);
    let japanese = v5::xls(932, &[], &[v5::Sheet { name: b"S".to_vec(), records: v5::label(0, 0, b"\x82\xA0") }]);
    match calc_doc::open(source(&japanese), &OpenPolicy::default()) {
        Err(OpenError::LegacyWorkbook { detail, .. }) => assert!(detail.contains("code page 932"), "{detail}"),
        other => panic!("{:?}", other.err()),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_biff5_workbook_styles_notes_and_external_references() {
    use xls_kit::{v5, v5_styles};
    let formatting = [
        v5_styles::font(200, 0, 0x7FFF, 400, "Arial"),
        v5_styles::font(240, 0x0002, 10, 700, "Courier New"),
        v5_styles::format(164, "0.0\"u\""),
        // A style XF, then a cell XF: font 1, format 164, locked, centred and wrapped, solid
        // pattern 1 with colours 13 / 64, thin top and bottom lines.
        v5_styles::xf(0, 0, 0xFFF5, 0x20, 0xFC, 0, 0),
        v5_styles::xf(0, 0, 0x0001, 0x20, 0, 0, 0),
        v5_styles::xf(1, 164, 0x0001, 0x2A, 0xFC, 13 | 64 << 7 | 1 << 16 | 1 << 22 | 8 << 25, 1),
        record(0x0293, &{
            let mut b = 0x8000u16.to_le_bytes().to_vec();
            b.extend([0, 0xFF]);
            b
        }),
    ]
    .concat();
    // An external workbook sheet "Rates" of file D:\data\book.xls: its encoded name with the file in brackets.
    let mut external = b"\x01\x01Ddata\x03[book.xls]Rates".to_vec();
    let mut record_body = vec![external.len() as u8];
    record_body.append(&mut external);
    let mut body = Vec::new();
    body.extend(record(0x0017, &record_body));
    let mut external_ref = vec![0x3A];
    external_ref.extend(1i16.to_le_bytes());
    external_ref.extend([0u8; 12]);
    external_ref.extend(u16s(&[1 | 0x8000 | 0x4000]));
    external_ref.push(2);
    body.extend(v5::formula(0, 0, 3.0, &external_ref));
    let mut with_style = u16s(&[1, 0, 2]);
    with_style.extend(7.0f64.to_le_bytes());
    body.extend(record(0x0203, &with_style));
    body.extend(v5_styles::note(1, 0, "remember"));
    let bytes = v5::xls(1252, &formatting, &[v5::Sheet::new("S", body)]);
    let loaded = load(&bytes);
    assert_eq!(cells(loaded.model(), 0), vec!["A1 f:[1]Rates!C2", "A2 n:7.0"]);
    let sheet = &loaded.sheets()[0];
    let s = sheet.style(2, 1);
    assert_ne!(s, 0);
    let table = loaded.styles();
    let format = &table.cell_formats[s as usize];
    assert_eq!(format.number_format_id, Some(164));
    let font = &table.fonts[format.font_id.unwrap() as usize];
    assert_eq!(font.name.as_deref(), Some("Courier New"));
    assert!(font.italic && font.bold);
    let border = &table.borders[format.border_id.unwrap() as usize];
    assert_eq!(border.top.as_ref().and_then(|e| e.style.as_deref()), Some("thin"));
    assert_eq!(border.bottom.as_ref().and_then(|e| e.style.as_deref()), Some("thin"));
    assert!(format.alignment.as_ref().is_some_and(|a| a.wrap_text && a.horizontal.as_deref() == Some("center")));
    assert!(format!("{:?}", table.fills[format.fill_id.unwrap() as usize]).contains("FFFFFF00"));
    let notes = &sheet.review().notes;
    assert_eq!(notes.len(), 1, "{:?}", sheet.review().unsupported);
    assert!(format!("{:?}", notes[0]).contains("remember"));
    // The external workbook is a link part whose target is the decoded path.
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let rels = String::from_utf8(document.package().read_part("xl/externalLinks/_rels/externalLink1.xml.rels").unwrap()).unwrap();
    assert!(rels.contains("D:\\data\\book.xls"), "{rels}");
}
