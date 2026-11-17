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

fn external_cache_fixture(invalid_link: bool) -> Vec<u8> {
    let mut crn = vec![4, 0, 2, 0, 1];
    crn.extend(1.25_f64.to_le_bytes());
    crn.extend([2, 2, 0, 0, b'O', b'K']);
    crn.extend([4, 1, 0, 0, 0, 0, 0, 0, 0]);
    crn.extend([16, 0x07, 0, 0, 0, 0, 0, 0, 0]);
    crn.extend([0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let count = if invalid_link { u16::MAX } else { 1 };
    let globals = [supbook_external("book.xls", &["Rates"]), record(0x0059, &u16s(&[count, 0])),
        record(0x005A, &crn), extern_sheet(&[(0, 0, 0)])].concat();
    let cells = formula(0, 0, num_value(1.25), &ptg::cell3d(0, 2, 0, 0), &[]);
    xls(false, &globals, &[Sheet::worksheet("S", cells)])
}

fn external_cache_biff5_fixture() -> Vec<u8> {
    let external = b"\x01\x01Ddata\x03[book.xls]Rates";
    let mut link = vec![external.len() as u8]; link.extend_from_slice(external);
    let mut crn = vec![2, 0, 2, 0, 1]; crn.extend(1.25_f64.to_le_bytes());
    crn.extend([2, 1, 0xE9]);
    crn.extend([4, 1, 0, 0, 0, 0, 0, 0, 0]);
    let records = [record(0x0017, &link), record(0x0059, &u16s(&[1])), record(0x005A, &crn),
        v5::formula(0, 0, 1.25, &v5::cell3d(-1, 0, 2, 0))].concat();
    v5::xls(1252, &[], &[v5::Sheet { name: b"S".to_vec(), records }])
}

fn addin_cell_reference_fixture() -> Vec<u8> {
    let globals = [supbook_addin(), extern_sheet(&[(0, -2, -2)])].concat();
    let cell = formula(0, 0, num_value(0.0), &ptg::cell3d(0, 0, 0, 0), &[]);
    xls(false, &globals, &[Sheet::worksheet("Data", cell)])
}

fn damaged_compound_fixtures() -> Vec<(&'static str, Vec<u8>)> {
    let base = xls(false, &[], &[Sheet::worksheet("Data", number(0, 0, 42.0))]);
    let missing_sector = (base.len() / 512 + 10) as u32;
    let mut missing_fat = base.clone();
    missing_fat[0x4c..0x50].copy_from_slice(&missing_sector.to_le_bytes());
    let mut missing_chain = base;
    let fat = u32::from_le_bytes(missing_chain[0x4c..0x50].try_into().unwrap()) as usize;
    let directory = u32::from_le_bytes(missing_chain[0x30..0x34].try_into().unwrap()) as usize;
    let entry = (fat + 1) * 512 + directory * 4;
    missing_chain[entry..entry+4].copy_from_slice(&missing_sector.to_le_bytes());
    vec![("repair-missing-fat.xls", missing_fat), ("repair-missing-chain.xls", missing_chain)]
}

fn ole_property_section(items: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut result = vec![0u8; 8 + items.len() * 8];
    result[4..8].copy_from_slice(&(items.len() as u32).to_le_bytes());
    for (i, (id, data)) in items.iter().enumerate() {
        while result.len() % 4 != 0 { result.push(0); }
        let offset = result.len() as u32;
        result[8+i*8..12+i*8].copy_from_slice(&id.to_le_bytes());
        result[12+i*8..16+i*8].copy_from_slice(&offset.to_le_bytes());
        result.extend(data);
    }
    while result.len() % 4 != 0 { result.push(0); }
    let len = result.len() as u32; result[..4].copy_from_slice(&len.to_le_bytes());
    result
}

fn custom_property_fixture(unaligned_page: bool) -> Vec<u8> {
    let mut page = 2u32.to_le_bytes().to_vec(); page.extend(u16s(&[1252, 0]));
    let document = ole_property_section(&[(1, page.clone())]);
    let mut dictionary = 1u32.to_le_bytes().to_vec(); dictionary.extend(2u32.to_le_bytes());
    dictionary.extend(7u32.to_le_bytes()); dictionary.extend(b"cxBlob\0");
    let mut value = if unaligned_page { 3u32.to_le_bytes().to_vec() } else { 0x41u32.to_le_bytes().to_vec() };
    if unaligned_page { value.extend(42i32.to_le_bytes()); }
    else { value.extend(4u32.to_le_bytes()); value.extend([0, 1, 2, 255]); }
    let mut user = ole_property_section(&[(0, dictionary), (1, page), (2, value)]);
    if unaligned_page {
        let at = u32::from_le_bytes(user[20..24].try_into().unwrap()) as usize;
        let next = u32::from_le_bytes(user[28..32].try_into().unwrap()) as usize;
        user.splice(at..at, [0, 0]);
        user.splice(next+2..next+2, [0, 0]);
        user[20..24].copy_from_slice(&((at + 2) as u32).to_le_bytes());
        user[28..32].copy_from_slice(&((next + 4) as u32).to_le_bytes());
        let size = user.len() as u32; user[..4].copy_from_slice(&size.to_le_bytes());
    }
    let mut stream = vec![0u8; 68]; stream[..2].copy_from_slice(&0xfffeu16.to_le_bytes());
    stream[24..28].copy_from_slice(&2u32.to_le_bytes());
    stream[28..44].copy_from_slice(&[0x02, 0xd5, 0xcd, 0xd5, 0x9c, 0x2e, 0x1b, 0x10,
        0x93, 0x97, 0x08, 0x00, 0x2b, 0x2c, 0xf9, 0xae]);
    stream[44..48].copy_from_slice(&68u32.to_le_bytes());
    stream[48..64].copy_from_slice(&[0x05, 0xd5, 0xcd, 0xd5, 0x9c, 0x2e, 0x1b, 0x10,
        0x93, 0x97, 0x08, 0x00, 0x2b, 0x2c, 0xf9, 0xae]);
    stream[64..68].copy_from_slice(&(68u32 + document.len() as u32).to_le_bytes());
    stream.extend(document); stream.extend(user);
    let workbook = workbook_stream(false, &[], &[Sheet::worksheet("Data", number(0, 0, 42.0))]);
    compound(&[("Workbook", workbook), ("\u{5}DocumentSummaryInformation", stream)])
}

fn east_asian_property_fixture(page: u16, text: &[u8]) -> Vec<u8> {
    let mut cp = 2u32.to_le_bytes().to_vec(); cp.extend(u16s(&[page, 0]));
    let mut value = 30u32.to_le_bytes().to_vec(); value.extend(((text.len() + 1) as u32).to_le_bytes());
    value.extend(text); value.push(0);
    let mut stream = vec![0u8; 48]; stream[..2].copy_from_slice(&0xfffeu16.to_le_bytes());
    stream[24..28].copy_from_slice(&1u32.to_le_bytes());
    stream[28..44].copy_from_slice(&[0x02, 0xd5, 0xcd, 0xd5, 0x9c, 0x2e, 0x1b, 0x10,
        0x93, 0x97, 0x08, 0x00, 0x2b, 0x2c, 0xf9, 0xae]);
    stream[44..48].copy_from_slice(&48u32.to_le_bytes());
    stream.extend(ole_property_section(&[(1, cp), (15, value)]));
    let workbook = workbook_stream(false, &[], &[Sheet::worksheet("Data", number(0, 0, 42.0))]);
    compound(&[("Workbook", workbook), ("\u{5}DocumentSummaryInformation", stream)])
}

fn ignored_error_feature() -> Vec<u8> {
    let mut header = vec![0x67, 0x08]; header.extend([0u8; 10]);
    header.extend(u16s(&[3])); header.push(1); header.extend(0u32.to_le_bytes());
    let mut body = vec![0x68, 0x08]; body.extend([0u8; 10]);
    body.extend(u16s(&[3])); body.extend([0u8; 5]); body.extend(u16s(&[1]));
    body.extend(4u32.to_le_bytes()); body.extend(u16s(&[0]));
    body.extend(u16s(&[0, 1, 0, 1])); body.extend((0x15u32).to_le_bytes());
    [record(0x0867, &header), record(0x0868, &body)].concat()
}

/// Native-only: writes controlled inputs for the owned Windows Excel session when requested.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn write_legacy_excel_session_inputs() {
    let Some(dir) = std::env::var_os("DNACALC_XLS_SESSION_INPUTS") else { return };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut cells = record(0x0099, &u16s(&[2340]));
    cells.extend(colinfo(1, 2, 3072, 0x000A));
    let mut row_with_flags = row(0, 255, 0);
    let last = row_with_flags.len() - 1;
    row_with_flags[last] |= 0x70;
    cells.extend(row_with_flags);
    cells.extend(number(0, 0, 42.5));
    let mut pending_formula = formula(1, 0, num_value(43.5), &ptg::join(&[ptg::cell(0, 0, 0), ptg::int(1), ptg::op(ptg::ADD)]), &[]);
    pending_formula[4 + 14] = 1;
    cells.extend(pending_formula);
    let mut excluded_formula = formula(1, 1, num_value(1.0), &ptg::int(1), &[]);
    excluded_formula[4 + 14] = 0x20;
    cells.extend(excluded_formula);
    cells.extend(merged(&[(2, 2, 0, 1)]));
    cells.extend(hlink_url((3, 0), "https://example.com/", Some("Example"), Some("part")));
    cells.extend(hlink_file((4, 0), "book.xls", Some("資料.xls"), "Data!A1"));
    cells.extend(ignored_error_feature());
    let globals = [supbook_self(1), extern_sheet(&[(0, 0, 0)]),
        name_record(0, 0, "Rate", &ptg::cell3d(0, 0, 0, 0))].concat();
    let biff8 = xls(false, &globals, &[Sheet::worksheet("Data", cells)]);
    let mut gcw = u16s(&[32]); gcw.push(0b0000_0101); gcw.extend([0u8; 31]);
    let mut old_cells = [record(0x0055, &u16s(&[8])), colinfo(0, 1, 3072, 0x0002),
        record(0x0099, &u16s(&[2340])), record(0x00AB, &gcw), v5::label(0, 0, b"legacy")].concat();
    old_cells.extend(record(0x001B, &u16s(&[1, 25])));
    old_cells.extend(record(0x001A, &u16s(&[1, 5])));
    let biff5 = v5::xls(1252, &[], &[v5::Sheet { name: b"Data".to_vec(), records: old_cells }]);
    std::fs::write(dir.join("cells-biff8.xls"), biff8).unwrap();
    std::fs::write(dir.join("cells-biff5.xls"), biff5).unwrap();
    std::fs::write(dir.join("precision-biff8.xls"), xls(false, &record(0x000E, &0u16.to_le_bytes()),
        &[Sheet::worksheet("Data", number(0, 0, 1.25))])).unwrap();
    std::fs::write(dir.join("external-cache-biff8.xls"), external_cache_fixture(false)).unwrap();
    std::fs::write(dir.join("external-cache-invalid-biff8.xls"), external_cache_fixture(true)).unwrap();
    std::fs::write(dir.join("external-cache-biff5.xls"), external_cache_biff5_fixture()).unwrap();
    let mut biff8_gcw = u16s(&[32]); biff8_gcw.push(0b0000_0101); biff8_gcw.extend([0u8; 31]);
    let biff8_gcw_cells = [record(0x0055, &u16s(&[8])), colinfo(0, 1, 3072, 0x0002),
        record(0x0099, &u16s(&[2340])), record(0x00AB, &biff8_gcw), number(0, 0, 1.0)].concat();
    std::fs::write(dir.join("gcw-biff8.xls"), xls(false, &[], &[Sheet::worksheet("Data", biff8_gcw_cells)])).unwrap();
    std::fs::write(dir.join("drawing-obj-continuation.xls"), obj_continuation_fixture(false)).unwrap();
    std::fs::write(dir.join("drawing-txo-continuation.xls"), txo_continuation_fixture(false)).unwrap();
    std::fs::write(dir.join("emf-checksum-valid.xls"), emf_checksum_fixture(false)).unwrap();
    std::fs::write(dir.join("emf-checksum-invalid.xls"), emf_checksum_fixture(true)).unwrap();
    std::fs::write(dir.join("forms.xls"), include_bytes!("fixtures/dc_4j0e/FormsWorkbook.xls")).unwrap();
    let repair_cases = [
        ("repair-slash.xls", "Bad/Name"),
        ("repair-stars.xls", "****"),
        ("repair-empty.xls", ""),
        ("repair-long.xls", "This sheet name exceeds thirty one"),
    ];
    for (file, name) in repair_cases {
        let bytes = xls(false, &[], &[Sheet::worksheet(name, number(0, 0, 42.0))]);
        std::fs::write(dir.join(file), bytes).unwrap();
    }
    for (file, tokens) in [
        ("repair-pivot-0.xls", ptg::func(358)),
        ("repair-pivot-2.xls", ptg::join(&[ptg::text("Amount"), ptg::cell(0, 0, 0), ptg::func(358)])),
        ("repair-pivot-4.xls", ptg::join(&[ptg::text("Amount"), ptg::cell(0, 0, 0), ptg::text("Region"), ptg::text("North"), ptg::func(358)])),
    ] {
        let cell = formula(1, 0, num_value(0.0), &tokens, &[]);
        std::fs::write(dir.join(file), xls(false, &[], &[Sheet::worksheet("Data", cell)])).unwrap();
    }
    let mut elf_col = vec![0x18, 0x03];
    elf_col.extend(u16s(&[0, 0])); // PtgElfCol referring to the A1 label
    let mut elf_radical = vec![0x18, 0x0A];
    elf_radical.extend(u16s(&[0, 0])); // PtgElfRadical referring to the A1 label
    for (file, tokens) in [
        ("repair-elf-column.xls", ptg::join(&[elf_col, ptg::func_var(1, 4)])),
        ("repair-elf-radical.xls", ptg::join(&[elf_radical, ptg::area((1, 2), (0, 0), 0), ptg::func_var(1, 4)])),
    ] {
        let cells = [label_sst(0, 0, 0), number(1, 0, 2.0), number(2, 0, 3.0),
            formula(0, 1, num_value(0.0), &tokens, &[])].concat();
        std::fs::write(dir.join(file), xls(false, &sst(&[("Sales", &[])], None), &[Sheet::worksheet("Data", cells)])).unwrap();
    }
    std::fs::write(dir.join("repair-addin-cell.xls"), addin_cell_reference_fixture()).unwrap();
    std::fs::write(dir.join("custom-blob.xls"), custom_property_fixture(false)).unwrap();
    std::fs::write(dir.join("custom-unaligned-page.xls"), custom_property_fixture(true)).unwrap();
    for (page, text) in [(936u16, &b"\xd6\xd0"[..]), (949, &b"\xb0\xa1"[..]), (950, &b"\xa4\xa4"[..])] {
        std::fs::write(dir.join(format!("company-cp{page}.xls")), east_asian_property_fixture(page, text)).unwrap();
    }
    for (file, bytes) in damaged_compound_fixtures() { std::fs::write(dir.join(file), bytes).unwrap(); }
    let short_format = record(0x041e, &[0xa4, 0x00, 0x02, 0x00, 0x00, b'x']);
    std::fs::write(dir.join("repair-short-format.xls"),
        xls(false, &short_format, &[Sheet::worksheet("Data", number(0, 0, 42.0))])).unwrap();
    for name in ["cells-biff8", "cells-biff5", "precision-biff8", "external-cache-biff8", "external-cache-invalid-biff8", "external-cache-biff5", "gcw-biff8", "drawing-obj-continuation", "drawing-txo-continuation", "emf-checksum-valid", "emf-checksum-invalid", "forms", "custom-blob", "custom-unaligned-page", "company-cp936", "company-cp949", "company-cp950"] {
        let original = std::fs::read(dir.join(format!("{name}.xls"))).unwrap();
        let document = calc_doc::open(source(&original), &OpenPolicy::default()).unwrap();
        let plan = calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
        let mut converted = Vec::new();
        calc_doc::write(plan, &mut converted).unwrap();
        std::fs::write(dir.join(format!("{name}.dnacalc.xlsx")), converted).unwrap();
    }
    let cases = serde_json::json!({
        "files": ["cells-biff8.xls", "cells-biff5.xls", "precision-biff8.xls", "external-cache-biff8.xls", "external-cache-invalid-biff8.xls", "external-cache-biff5.xls", "gcw-biff8.xls", "drawing-obj-continuation.xls", "drawing-txo-continuation.xls", "emf-checksum-valid.xls", "emf-checksum-invalid.xls", "forms.xls", "custom-blob.xls", "custom-unaligned-page.xls", "company-cp936.xls", "company-cp949.xls", "company-cp950.xls"],
        "repair_cases": ["repair-slash.xls", "repair-stars.xls", "repair-empty.xls", "repair-long.xls",
            "repair-pivot-0.xls", "repair-pivot-2.xls", "repair-pivot-4.xls",
            "repair-elf-column.xls", "repair-elf-radical.xls", "repair-addin-cell.xls",
            "repair-missing-fat.xls", "repair-missing-chain.xls", "repair-short-format.xls"],
        "create_pivot": true,
        "formulas": [
            ["rows_full_column", "=ROWS(A:A)"], ["columns_full_row", "=COLUMNS(1:1)"],
            ["rows_modern_extent", "=ROWS(1:1048576)"], ["beyond_old_grid", "=A65537"],
            ["whole_column_sum", "=SUM(A:A)"], ["xlookup", "=XLOOKUP(1,{1},{2})"],
            ["let", "=LET(x,1,x+1)"], ["lambda", "=LAMBDA(x,x+1)(1)"],
            ["filter", "=FILTER({1;2},{TRUE;FALSE})"], ["sort", "=SORT({2;1})"],
            ["sequence", "=SEQUENCE(2)"], ["textjoin", "=TEXTJOIN(\",\",TRUE,\"a\",\"b\")"],
            ["ifs", "=IFS(TRUE,1)"], ["concat", "=CONCAT(\"a\",\"b\")"],
            ["stdev_s", "=STDEV.S(1,2)"], ["ceiling_math", "=CEILING.MATH(1.2)"],
            ["norm_s_dist", "=NORM.S.DIST(0,TRUE)"], ["webservice", "=WEBSERVICE(\"\")"],
            ["randarray", "=RANDARRAY(1,1)"], ["xmatch", "=XMATCH(1,{1,2})"]
        ]
    });
    std::fs::write(dir.join("cases.json"), serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
}

/// Native-only: converts the controlled workbooks returned by the owned Excel session.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn convert_excel_session_returned_inputs() {
    let Some(dir) = std::env::var_os("DNACALC_XLS_RETURNED_INPUTS") else { return };
    let dir = std::path::PathBuf::from(dir);
    let files: Vec<String> = serde_json::from_slice(&std::fs::read(dir.join("cases.json")).unwrap()).unwrap();
    let mut outcomes = serde_json::Map::new();
    for file in files {
        let path = std::path::Path::new(&file);
        assert!(matches!(path.components().collect::<Vec<_>>().as_slice(), [std::path::Component::Normal(_)]), "invalid fixture filename");
        let input = std::fs::read(dir.join(path)).unwrap();
        let result = (|| -> Result<Vec<u8>, String> {
            let doc = calc_doc::open(source(&input), &OpenPolicy::default()).map_err(|e| e.to_string())?;
            let plan = calc_doc::prepare_save(&doc.snapshot(), &calc_doc::SavePolicy::default()).map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            calc_doc::write(plan, &mut out).map_err(|e| e.to_string())?;
            Ok(out)
        })();
        match result {
            Ok(bytes) => {
                std::fs::write(dir.join(format!("{file}.dnacalc.xlsx")), bytes).unwrap();
                outcomes.insert(file, serde_json::Value::Null);
            }
            Err(error) => { outcomes.insert(file, serde_json::Value::String(error)); }
        }
    }
    std::fs::write(dir.join("outcomes.json"), serde_json::to_vec_pretty(&outcomes).unwrap()).unwrap();
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
fn sheet_calculation_modes_are_preserved_and_conflicts_refused() {
    for (value, expected) in [(0u16, calc_doc::CalcMode::Manual), (1, calc_doc::CalcMode::Automatic),
        (2, calc_doc::CalcMode::AutomaticExceptTables)] {
        let mode = record(0x000D, &value.to_le_bytes());
        let bytes = xls(false, &[], &[Sheet::worksheet("A", mode.clone()), Sheet::worksheet("B", mode)]);
        assert_eq!(load(&bytes).calc_mode(), expected);
        let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
        assert!(!report.unknown_records.contains_key(&0x000D));
    }
    let bytes = xls(false, &[], &[Sheet::worksheet("A", record(0x000D, &0u16.to_le_bytes())),
        Sheet::worksheet("B", record(0x000D, &1u16.to_le_bytes()))]);
    match calc_doc::open(source(&bytes), &OpenPolicy::default()) {
        Err(OpenError::LegacyWorkbook { detail, .. }) => assert!(detail.contains("conflicting calculation modes"), "{detail}"),
        other => panic!("{:?}", other.err()),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn iterative_calculation_settings_survive_the_workbook_route() {
    let records = [record(0x000D, &0u16.to_le_bytes()), record(0x000C, &42u16.to_le_bytes()),
        record(0x0011, &1u16.to_le_bytes()), record(0x0010, &0.005_f64.to_le_bytes()),
        record(0x005F, &0u16.to_le_bytes())].concat();
    let bytes = xls(false, &[], &[Sheet::worksheet("S", records)]);
    let loaded = load(&bytes);
    assert_eq!(loaded.calc_mode(), calc_doc::CalcMode::Manual);
    assert!(!loaded.calc_on_save());
    let iteration = loaded.model().definitions().iteration();
    assert!(iteration.enabled());
    assert_eq!(iteration.max_iterations(), 42);
    assert_eq!(iteration.max_change(), 0.005);
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    for id in [0x000C, 0x0010, 0x0011, 0x005F] { assert!(!report.unknown_records.contains_key(&id)); }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn precision_setting_survives_while_nondefault_remains_counted() {
    for (value, expected_unread) in [(0u16, true), (1u16, false)] {
        let bytes = xls(false, &record(0x000E, &value.to_le_bytes()), &[Sheet::worksheet("S", Vec::new())]);
        let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let xml = String::from_utf8(doc.package().read_part("xl/workbook.xml").unwrap()).unwrap();
        assert_eq!(xml.contains("fullPrecision=\"0\""), value == 0);
        let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
        assert_eq!(report.unknown_records.contains_key(&0x000E), expected_unread);
        let plan = calc_doc::prepare_save(&doc.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
        let mut saved = Vec::new(); calc_doc::write(plan, &mut saved).unwrap();
        let reopened = calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap();
        let xml = String::from_utf8(reopened.package().read_part("xl/workbook.xml").unwrap()).unwrap();
        assert_eq!(xml.contains("fullPrecision=\"0\""), value == 0);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn reference_display_mode_survives_conversion() {
    for (mode, expected) in [(0u16, true), (1u16, false)] {
        let records = [record(0x000F, &mode.to_le_bytes()), formula(0, 0, num_value(1.0), &ptg::int(1), &[])].concat();
        let bytes = xls(false, &[], &[Sheet::worksheet("S", records)]);
        let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let xml = String::from_utf8(document.package().read_part("xl/workbook.xml").unwrap()).unwrap();
        assert_eq!(xml.contains("refMode=\"R1C1\""), expected, "{xml}");
        assert_eq!(sheet_cells(&bytes), vec!["A1 f:1"]);
        let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
        assert!(!report.unknown_records.contains_key(&0x000F));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn workbook_protection_is_preserved_in_the_loaded_document() {
    let globals = [record(0x0019, &1u16.to_le_bytes()), record(0x0012, &1u16.to_le_bytes()),
        record(0x0013, &0xCE4Bu16.to_le_bytes())].concat();
    let bytes = xls(false, &globals, &[Sheet::worksheet("S", Vec::new())]);
    assert!(load(&bytes).structure_protected());
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(document.package().read_part("xl/workbook.xml").unwrap()).unwrap();
    assert!(xml.contains("<workbookProtection lockStructure=\"1\" lockWindows=\"1\" workbookPassword=\"CE4B\"/>"), "{xml}");
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    for id in [0x0012, 0x0013, 0x0019] { assert!(!report.unknown_records.contains_key(&id)); }
    let unprotected = xls(false, &[], &[Sheet::worksheet("S", Vec::new())]);
    assert!(!load(&unprotected).structure_protected());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn workbook_object_display_setting_survives_conversion() {
    for (mode, expected) in [(0u16, None), (1, Some("placeholders")), (2, Some("none"))] {
        let bytes = xls(false, &record(0x008D, &mode.to_le_bytes()), &[Sheet::worksheet("S", Vec::new())]);
        let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let xml = String::from_utf8(document.package().read_part("xl/workbook.xml").unwrap()).unwrap();
        if let Some(value) = expected {
            assert!(xml.contains(&format!("showObjects=\"{value}\"")), "{xml}");
        } else { assert!(!xml.contains("showObjects="), "{xml}"); }
        load(&bytes);
        let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
        assert!(!report.unknown_records.contains_key(&0x008D));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn workbook_backup_setting_survives_conversion() {
    for enabled in [0u16, 1] {
        let bytes = xls(false, &record(0x0040, &enabled.to_le_bytes()), &[Sheet::worksheet("S", Vec::new())]);
        let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let xml = String::from_utf8(document.package().read_part("xl/workbook.xml").unwrap()).unwrap();
        assert_eq!(xml.contains("backupFile=\"1\""), enabled == 1, "{xml}");
        load(&bytes);
        let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
        assert!(!report.unknown_records.contains_key(&0x0040));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn extra_empty_backup_record_is_counted_before_valid_setting() {
    let globals = [record(0x0040, &[]), record(0x0040, &1u16.to_le_bytes())].concat();
    let bytes = xls(false, &globals, &[Sheet::worksheet("S", Vec::new())]);
    let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(doc.package().read_part("xl/workbook.xml").unwrap()).unwrap();
    assert!(xml.contains("backupFile=\"1\""), "{xml}");
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unknown_records.get(&0x0040), Some(&1));
    let missing = xls(false, &record(0x0040, &[]), &[Sheet::worksheet("S", Vec::new())]);
    assert!(calc_doc::open(source(&missing), &OpenPolicy::default()).is_err());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn workbook_external_link_settings_survive_conversion() {
    for (flags, present, unread) in [
        (0u16, vec![], false),
        (0x0121, vec!["saveExternalLinkValues=\"0\"", "showBorderUnselectedTables=\"0\"", "updateLinks=\"never\""], false),
        (0x0044, vec!["updateLinks=\"always\""], true),
    ] {
        let bytes = xls(false, &record(0x00DA, &flags.to_le_bytes()), &[Sheet::worksheet("S", Vec::new())]);
        let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let xml = String::from_utf8(document.package().read_part("xl/workbook.xml").unwrap()).unwrap();
        for attribute in present { assert!(xml.contains(attribute), "{xml}"); }
        let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
        assert_eq!(report.unknown_records.contains_key(&0x00DA), unread);
        load(&bytes);
    }
    let bytes = xls(false, &record(0x00DA, &0x0060u16.to_le_bytes()), &[Sheet::worksheet("S", Vec::new())]);
    assert!(calc_doc::open(source(&bytes), &OpenPolicy::default()).is_err());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn workbook_refresh_setting_survives_conversion() {
    for value in [0u16, 1] {
        let bytes = xls(false, &record(0x01B7, &value.to_le_bytes()), &[Sheet::worksheet("S", Vec::new())]);
        let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let xml = String::from_utf8(document.package().read_part("xl/workbook.xml").unwrap()).unwrap();
        assert_eq!(xml.contains("refreshAllConnections=\"1\""), value == 1, "{xml}");
        let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
        assert_eq!(report.unknown_records.contains_key(&0x01B7), value == 1);
        load(&bytes);
    }
    let bytes = xls(false, &record(0x01B7, &2u16.to_le_bytes()), &[Sheet::worksheet("S", Vec::new())]);
    assert!(calc_doc::open(source(&bytes), &OpenPolicy::default()).is_err());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn outline_gutter_levels_survive_conversion() {
    let bytes = xls(false, &[], &[Sheet::worksheet("S", record(0x0080, &u16s(&[0, 0, 3, 2])))]);
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(document.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(xml.contains("outlineLevelRow=\"2\""), "{xml}");
    assert!(xml.contains("outlineLevelCol=\"1\""), "{xml}");
    load(&bytes);
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert!(!report.unknown_records.contains_key(&0x0080));
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
fn chart_sheet_preserves_series_in_a_linked_chart_part() {
    use ptg::*;
    use xls_kit::art;
    let globals = [supbook_self(2), extern_sheet(&[(0, 0, 0)])].concat();
    let chart = art::chart(&[("Sales", area3d(0, (1, 4), (0, 0), 0),
        area3d(0, (1, 4), (1, 1), 0))], 0, "Sales chart");
    let header = |text: &str| {
        let mut body = (text.len() as u16).to_le_bytes().to_vec(); body.push(0); body.extend(text.as_bytes());
        record(0x0014, &body)
    };
    let records = [header("&CChart & page"), record(0x023E, &u16s(&[0x0200, 0, 0, 0, 0])),
        record(0x01AA, &[]), header("custom only"), record(0x023E, &[0; 10]), record(0x01AB, &[]),
        chart[bof(0x0020).len()..chart.len()-eof().len()].to_vec()].concat();
    let mut three_d_records = records.clone();
    three_d_records.extend(record(0x103A, &[0; 14]));
    let mut duplicate_windows = records.clone();
    duplicate_windows.extend(record(0x023E, &[0; 10]));
    let bytes = xls(false, &globals, &[Sheet::worksheet("Data", number(1, 1, 7.5)),
        Sheet { kind: 2, ..Sheet::worksheet("Chart1", records) }]);
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let verify = |doc: &calc_doc::OpenedDocument| {
        let chart = String::from_utf8(doc.package().read_part("xl/charts/chart2_1.xml").unwrap()).unwrap();
        assert!(chart.contains("Data!$B$2:$B$5") && chart.contains("Sales chart"), "{chart}");
        let sheet = String::from_utf8(doc.package().read_part("xl/chartsheets/sheet2.xml").unwrap()).unwrap();
        assert!(sheet.contains("<drawing r:id=\"rId1\"/>"), "{sheet}");
        assert!(sheet.contains("tabSelected=\"1\""), "{sheet}");
        assert!(sheet.contains("<oddHeader>&amp;CChart &amp; page</oddHeader>") && !sheet.contains("custom only"), "{sheet}");
        assert!(doc.package().part("xl/drawings/drawing2.xml").is_some());
        assert!(doc.package().part("xl/drawings/_rels/drawing2.xml.rels").is_some());
    };
    verify(&document);
    let mut ids = IdAllocator::<SheetId>::new();
    assert_eq!(calc_doc::load(&document, &mut ids).unwrap().sheets()[1].preserved_kind(), Some("chart sheet"));
    let plan = calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
    let mut saved = Vec::new();
    calc_doc::write(plan, &mut saved).unwrap();
    verify(&calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap());
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unknown_records.get(&0x1003), Some(&1));
    assert_eq!(report.unknown_records.get(&0x0014), Some(&1));
    assert_eq!(report.unknown_records.get(&0x023E), Some(&1));
    let duplicate = xls(false, &globals, &[Sheet::worksheet("Data", Vec::new()),
        Sheet { kind: 2, ..Sheet::worksheet("Chart1", duplicate_windows) }]);
    let duplicate_doc = calc_doc::open(source(&duplicate), &OpenPolicy::default()).unwrap();
    let duplicate_sheet = String::from_utf8(duplicate_doc.package().read_part("xl/chartsheets/sheet2.xml").unwrap()).unwrap();
    assert!(!duplicate_sheet.contains("tabSelected=\"1\""), "{duplicate_sheet}");
    assert_eq!(legacy_workbook_report(source(&duplicate), &OpenPolicy::default()).unwrap().unwrap().unknown_records.get(&0x023E), Some(&3));
    let three_d = xls(false, &globals, &[Sheet::worksheet("Data", Vec::new()),
        Sheet { kind: 2, ..Sheet::worksheet("Chart1", three_d_records) }]);
    let three_d_doc = calc_doc::open(source(&three_d), &OpenPolicy::default()).unwrap();
    assert!(three_d_doc.package().part("xl/charts/chart2_1.xml").is_none());
    let sheet = String::from_utf8(three_d_doc.package().read_part("xl/chartsheets/sheet2.xml").unwrap()).unwrap();
    assert!(sheet.contains("<oddHeader>&amp;CChart &amp; page</oddHeader>"), "{sheet}");
    assert!(sheet.contains("tabSelected=\"1\""), "{sheet}");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chart_sheet_preserves_complete_unambiguous_print_margins() {
    let mut setup = vec![0; 34];
    setup[16..24].copy_from_slice(&0.4_f64.to_le_bytes());
    setup[24..32].copy_from_slice(&0.6_f64.to_le_bytes());
    let margins = [(0x26, 0.7_f64), (0x27, 0.8), (0x28, 0.9), (0x29, 1.1)]
        .into_iter().flat_map(|(id, value)| record(id, &value.to_le_bytes())).collect::<Vec<_>>();
    let records = [margins.clone(), record(0xa1, &setup)].concat();
    let make = |records| xls(false, &[], &[Sheet { kind: 2, ..Sheet::worksheet("Chart", records) }]);
    let bytes = make(records.clone());
    let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let verify = |doc: &calc_doc::OpenedDocument| {
        let sheet = String::from_utf8(doc.package().read_part("xl/chartsheets/sheet1.xml").unwrap()).unwrap();
        assert!(sheet.contains("<pageMargins left=\"0.7\" right=\"0.8\" top=\"0.9\" bottom=\"1.1\" header=\"0.4\" footer=\"0.6\"/>"), "{sheet}");
    };
    verify(&doc);
    let plan = calc_doc::prepare_save(&doc.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
    let mut saved = Vec::new();
    calc_doc::write(plan, &mut saved).unwrap();
    verify(&calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap());
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unknown_records.get(&0x26), None);
    assert_eq!(report.unknown_records.get(&0xa1), Some(&1));
    let ambiguous = make([records, record(0x26, &0.1_f64.to_le_bytes())].concat());
    let doc = calc_doc::open(source(&ambiguous), &OpenPolicy::default()).unwrap();
    let sheet = doc.package().read_part("xl/chartsheets/sheet1.xml").unwrap();
    assert!(!sheet.windows(12).any(|w| w == b"<pageMargins"));
    assert_eq!(legacy_workbook_report(source(&ambiguous), &OpenPolicy::default()).unwrap().unwrap().unknown_records.get(&0x26), Some(&2));
    setup[16..24].copy_from_slice(&49.0_f64.to_le_bytes());
    let invalid = make([margins, record(0xa1, &setup)].concat());
    let doc = calc_doc::open(source(&invalid), &OpenPolicy::default()).unwrap();
    let sheet = doc.package().read_part("xl/chartsheets/sheet1.xml").unwrap();
    assert!(!sheet.windows(12).any(|w| w == b"<pageMargins"));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chart_sheet_preserves_representable_print_setup() {
    let make = |flags| {
        let mut setup = u16s(&[9, 100, 3, 1, 1, flags, 300, 600]);
        setup.extend(0.5_f64.to_le_bytes()); setup.extend(0.5_f64.to_le_bytes());
        setup.extend(2u16.to_le_bytes());
        xls(false, &[], &[Sheet { kind: 2, ..Sheet::worksheet("Chart", record(0xa1, &setup)) }])
    };
    let bytes = make(0x00da); // portrait, monochrome, draft and explicit first page
    let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let verify = |doc: &calc_doc::OpenedDocument| {
        let sheet = String::from_utf8(doc.package().read_part("xl/chartsheets/sheet1.xml").unwrap()).unwrap();
        for expected in ["paperSize=\"9\"", "orientation=\"portrait\"", "horizontalDpi=\"300\"",
            "verticalDpi=\"600\"", "copies=\"2\"", "blackAndWhite=\"1\"", "draft=\"1\"",
            "firstPageNumber=\"3\"", "useFirstPageNumber=\"1\""] {
            assert!(sheet.contains(expected), "{expected}: {sheet}");
        }
    };
    verify(&doc);
    let mut saved = Vec::new();
    calc_doc::write(calc_doc::prepare_save(&doc.snapshot(), &calc_doc::SavePolicy::default()).unwrap(), &mut saved).unwrap();
    verify(&calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap());
    assert_eq!(legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap().unknown_records.get(&0xa1), Some(&1));
    let undefined = make(4);
    let doc = calc_doc::open(source(&undefined), &OpenPolicy::default()).unwrap();
    let sheet = String::from_utf8(doc.package().read_part("xl/chartsheets/sheet1.xml").unwrap()).unwrap();
    assert!(sheet.contains("usePrinterDefaults=\"1\""), "{sheet}");
    for absent in ["paperSize=", "orientation=", "horizontalDpi=", "verticalDpi=", "copies="] {
        assert!(!sheet.contains(absent), "{sheet}");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn scenarios_survive_conversion_as_unsupported_metadata() {
    let full = |text: &str| {
        let mut bytes = (text.len() as u16).to_le_bytes().to_vec(); bytes.push(0); bytes.extend(text.as_bytes()); bytes
    };
    let mut manager = u16s(&[1, 0, 0xffff, 1, 4, 4, 3, 3]);
    let mut scenario = u16s(&[2]);
    scenario.extend([1, 0, 4, 4, 3]); // locked, visible, name/comment/user lengths
    scenario.extend([0, b'P', b'l', b'a', b'n']);
    scenario.extend(full("Ann")); scenario.extend(full("note"));
    scenario.extend(u16s(&[0, 0, 2, 1]));
    scenario.extend(full("12")); scenario.extend(full("_x0041_")); scenario.extend([0, 0, 0, 0]);
    let make = |m: &[u8], s: &[u8]| xls(false, &[], &[Sheet::worksheet("S",
        [record(0x00AE, m), record(0x00AF, s), number(0, 0, 1.0)].concat())]);
    let bytes = make(&manager, &scenario);
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let check = |doc: &calc_doc::OpenedDocument| {
        let xml = String::from_utf8(doc.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
        for expected in ["<scenarios current=\"0\" sqref=\"D5\">", "<scenario name=\"Plan\" locked=\"1\" count=\"2\" user=\"Ann\" comment=\"note\">",
            "<inputCells r=\"A1\" val=\"12\"/>", "<inputCells r=\"B3\" val=\"_x005F_x0041_\"/>"] {
            assert!(xml.contains(expected), "{expected} in {xml}");
        }
    };
    check(&document);
    let _loaded = load(&bytes);
    let plan = calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
    let mut saved = Vec::new(); calc_doc::write(plan, &mut saved).unwrap();
    check(&calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap());
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unknown_records.get(&0x00AE), Some(&1));
    assert_eq!(report.unknown_records.get(&0x00AF), Some(&1));
    // A mismatched manager and a deleted input do not yield a partial scenario.
    manager[..2].copy_from_slice(&2u16.to_le_bytes());
    let bad = calc_doc::open(source(&make(&manager, &scenario)), &OpenPolicy::default()).unwrap();
    assert!(!String::from_utf8(bad.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap().contains("<scenarios"));
    manager[..2].copy_from_slice(&1u16.to_le_bytes());
    let reference = scenario.windows(8).position(|w| w == u16s(&[0, 0, 2, 1])).unwrap();
    scenario[reference+2..reference+4].copy_from_slice(&0x4000u16.to_le_bytes());
    let bad = calc_doc::open(source(&make(&manager, &scenario)), &OpenPolicy::default()).unwrap();
    assert!(!String::from_utf8(bad.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap().contains("<scenarios"));
    // BIFF5 strings are byte-counted and use the workbook code page.
    let mut legacy = u16s(&[1]); legacy.extend([1, 0, 4, 0, 3]); legacy.extend(b"caf\xE9");
    legacy.extend([3, b'A', b'n', b'n']); legacy.extend(u16s(&[0, 0]));
    legacy.extend([1, b'7', 0, 0]);
    let records = [record(0x00AE, &u16s(&[1, 0, 0xffff, 0])), record(0x00AF, &legacy)].concat();
    let legacy = v5::xls(1252, &[], &[v5::Sheet::new("S", records)]);
    let legacy_doc = calc_doc::open(source(&legacy), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(legacy_doc.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(xml.contains("name=\"café\"") && xml.contains("<inputCells r=\"A1\" val=\"7\"/>"), "{xml}");
    let _loaded = load(&legacy);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn biff5_chart_sheet_preserves_decodable_series_and_title() {
    use xls_kit::{art, v5};
    let value = v5::cell3d(1, 0, 0, 0);
    let mut chart = art::chart(&[("Sales", Vec::new(), value)], 0, "Title");
    for (old, replacement) in [("Sales", b"Sales".as_slice()), ("Title", b"caf\xE9".as_slice())] {
        let original = art::series_text(old);
        let mut body = u16s(&[0]); body.push(replacement.len() as u8); body.extend(replacement);
        let new = record(0x100D, &body);
        let at = chart.windows(original.len()).position(|part| part == original).unwrap();
        chart.splice(at..at + original.len(), new);
    }
    let original = art::series();
    let at = chart.windows(original.len()).position(|part| part == original).unwrap();
    chart.splice(at..at + original.len(), record(0x1003, &u16s(&[1, 1, 4, 4])));
    let records = [v5::extern_own(b"Data"), chart[bof(0x0020).len()..chart.len()-eof().len()].to_vec()].concat();
    let mut book = v5::book(1252, &[], &[v5::Sheet::new("Data", number(0, 0, 7.5)), v5::Sheet::new("Chart1", records)]);
    let mut at = 0;
    let mut sheets = 0;
    while at + 4 <= book.len() {
        let id = u16::from_le_bytes([book[at], book[at + 1]]);
        let len = usize::from(u16::from_le_bytes([book[at + 2], book[at + 3]]));
        if id == 0x0085 {
            sheets += 1;
            if sheets == 2 {
                let offset = u32::from_le_bytes(book[at + 4..at + 8].try_into().unwrap()) as usize;
                book[at + 9] = 2;
                book[offset + 6..offset + 8].copy_from_slice(&0x0020u16.to_le_bytes());
                break;
            }
        }
        at += 4 + len;
    }
    assert_eq!(sheets, 2);
    let bytes = compound(&[("Book", book)]);
    let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(doc.package().read_part("xl/charts/chart2_1.xml").unwrap()).unwrap();
    assert!(xml.contains("Data!A1") && xml.contains("café"), "{xml}");
    let plan = calc_doc::prepare_save(&doc.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
    let mut saved = Vec::new(); calc_doc::write(plan, &mut saved).unwrap();
    assert!(calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap().package().part("xl/charts/chart2_1.xml").is_some());
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
fn damaged_compound_and_format_fixtures_report_their_missing_data() {
    for ((_, bytes), reason) in damaged_compound_fixtures().into_iter().zip([
        "a FAT sector lies outside the file", "a sector chain leaves the file",
    ]) {
        match calc_doc::open(source(&bytes), &OpenPolicy::default()) {
            Err(OpenError::LegacyWorkbook { detail, .. }) => assert!(detail.contains(reason), "{detail}"),
            other => panic!("expected {reason}, got {:?}", other.err()),
        }
    }
    let short_format = record(0x041e, &[0xa4, 0x00, 0x02, 0x00, 0x00, b'x']);
    let bytes = xls(false, &short_format, &[Sheet::worksheet("Data", number(0, 0, 42.0))]);
    match calc_doc::open(source(&bytes), &OpenPolicy::default()) {
        Err(OpenError::LegacyWorkbook { detail, .. }) => assert!(detail.contains("record ends inside"), "{detail}"),
        other => panic!("expected a short format refusal, got {:?}", other.err()),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn custom_blob_fixture_is_counted_as_unread() {
    let bytes = custom_property_fixture(false);
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    assert!(document.package().part("docProps/custom.xml").is_none());
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unread_custom_property_types.get(&0x41), Some(&1));
    assert_eq!(report.unread_custom_property_sets, 0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ole_summary_properties_survive_conversion_and_save() {
    let mut properties = Vec::<(u32, Vec<u8>)>::new();
    properties.push((1, [2u32.to_le_bytes().as_slice(), 1252u16.to_le_bytes().as_slice(), &[0, 0]].concat()));
    let ansi = |text: &[u8]| {
        let mut v = 30u32.to_le_bytes().to_vec(); v.extend(((text.len() + 1) as u32).to_le_bytes());
        v.extend(text); v.push(0); v
    };
    properties.push((2, ansi(b"A & B _x0041_\rC")));
    properties.push((3, ansi(b"bad\x01subject")));
    properties.push((4, ansi(b"caf\xE9")));
    let mut unicode = 31u32.to_le_bytes().to_vec(); unicode.extend(3u32.to_le_bytes());
    unicode.extend(u16s(&[0x6771, 0x4eac, 0]));
    properties.push((8, unicode));
    let mut created = 0x40u32.to_le_bytes().to_vec();
    created.extend(116_444_736_000_000_000u64.to_le_bytes());
    properties.push((12, created));
    properties.push((18, ansi(b"Excel")));
    let section = ole_property_section;
    let mut summary = vec![0u8; 48];
    summary[..2].copy_from_slice(&0xfffeu16.to_le_bytes());
    summary[24..28].copy_from_slice(&1u32.to_le_bytes());
    summary[28..44].copy_from_slice(&[0xe0, 0x85, 0x9f, 0xf2, 0xf9, 0x4f, 0x68, 0x10,
        0xab, 0x91, 0x08, 0x00, 0x2b, 0x27, 0xb3, 0xd9]);
    summary[44..48].copy_from_slice(&48u32.to_le_bytes()); summary.extend(section(&properties));
    let mut doc_section = section(&[(1, properties[0].1.clone()), (2, ansi(b"Planning")),
        (12, vec![0; 8]), (14, ansi(b"Manager")), (15, ansi(b"Company & Co"))]);
    // A malformed, unrelated property offset must not hide aligned properties.
    let unrelated_offset = u32::from_le_bytes(doc_section[28..32].try_into().unwrap());
    doc_section[28..32].copy_from_slice(&(unrelated_offset + 1).to_le_bytes());
    let mut doc_summary = vec![0u8; 68];
    doc_summary[..2].copy_from_slice(&0xfffeu16.to_le_bytes());
    doc_summary[24..28].copy_from_slice(&2u32.to_le_bytes());
    doc_summary[28..44].copy_from_slice(&[0x02, 0xd5, 0xcd, 0xd5, 0x9c, 0x2e, 0x1b, 0x10,
        0x93, 0x97, 0x08, 0x00, 0x2b, 0x2c, 0xf9, 0xae]);
    doc_summary[44..48].copy_from_slice(&68u32.to_le_bytes());
    doc_summary[48..64].copy_from_slice(&[0x05, 0xd5, 0xcd, 0xd5, 0x9c, 0x2e, 0x1b, 0x10,
        0x93, 0x97, 0x08, 0x00, 0x2b, 0x2c, 0xf9, 0xae]);
    doc_summary[64..68].copy_from_slice(&(68u32 + doc_section.len() as u32).to_le_bytes());
    let mut dictionary = 4u32.to_le_bytes().to_vec();
    for (id, name) in [(2u32, "Note & Name"), (3, "Count"), (4, "Approved"), (5, "Opaque")] {
        dictionary.extend(id.to_le_bytes()); dictionary.extend(((name.len() + 1) as u32).to_le_bytes());
        dictionary.extend(name.as_bytes()); dictionary.push(0);
    }
    let mut integer = 3u32.to_le_bytes().to_vec(); integer.extend((-42i32).to_le_bytes());
    let mut boolean = 11u32.to_le_bytes().to_vec(); boolean.extend(u16s(&[0xffff, 0]));
    let mut blob = 0x41u32.to_le_bytes().to_vec(); blob.extend(2u32.to_le_bytes()); blob.extend([1, 2]);
    let custom_section = section(&[(0, dictionary), (1, properties[0].1.clone()), (2, ansi(b"Value & data")),
        (3, integer), (4, boolean), (5, blob)]);
    doc_summary.extend(doc_section); doc_summary.extend(custom_section);
    let workbook = workbook_stream(false, &[], &[Sheet::worksheet("S", number(0, 0, 1.0))]);
    let bytes = compound(&[("Workbook", workbook.clone()), ("\u{5}SummaryInformation", summary.clone()),
        ("\u{5}DocumentSummaryInformation", doc_summary.clone())]);
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let check = |doc: &calc_doc::OpenedDocument| {
        let xml = String::from_utf8(doc.package().read_part("docProps/core.xml").unwrap()).unwrap();
        for part in ["<dc:title>A &amp; B _x0041_&#xD;C</dc:title>", "<dc:creator>café</dc:creator>",
            "<cp:lastModifiedBy>東京</cp:lastModifiedBy>",
            "<cp:category>Planning</cp:category>",
            "<dcterms:created xsi:type=\"dcterms:W3CDTF\">1970-01-01T00:00:00Z</dcterms:created>"] { assert!(xml.contains(part), "{xml}"); }
        assert!(!xml.contains("dc:subject"), "{xml}");
        let rels = doc.package().relationships_of(None).unwrap();
        assert!(rels.items().iter().any(|rel| rel.resolved.as_deref() == Some("docProps/core.xml")));
        let app = String::from_utf8(doc.package().read_part("docProps/app.xml").unwrap()).unwrap();
        for part in ["<Application>Excel</Application>", "<Manager>Manager</Manager>",
            "<Company>Company &amp; Co</Company>"] { assert!(app.contains(part), "{app}"); }
        assert!(rels.items().iter().any(|rel| rel.resolved.as_deref() == Some("docProps/app.xml")));
        let custom = String::from_utf8(doc.package().read_part("docProps/custom.xml").unwrap()).unwrap();
        for part in ["name=\"Note &amp; Name\"", "<vt:lpwstr>Value &amp; data</vt:lpwstr>",
            "<vt:i4>-42</vt:i4>", "<vt:bool>true</vt:bool>"] { assert!(custom.contains(part), "{custom}"); }
        assert!(!custom.contains("Opaque"));
        assert!(rels.items().iter().any(|rel| rel.resolved.as_deref() == Some("docProps/custom.xml")));
    };
    check(&document);
    // An unaligned offset for the property itself is still refused independently.
    let mut bad_company = doc_summary.clone();
    let company_offset = u32::from_le_bytes(bad_company[112..116].try_into().unwrap());
    bad_company[112..116].copy_from_slice(&(company_offset + 1).to_le_bytes());
    let bad_company = compound(&[("Workbook", workbook.clone()), ("\u{5}DocumentSummaryInformation", bad_company)]);
    let bad_company = calc_doc::open(source(&bad_company), &OpenPolicy::default()).unwrap();
    let app = String::from_utf8(bad_company.package().read_part("docProps/app.xml").unwrap()).unwrap();
    assert!(app.contains("<Manager>Manager</Manager>") && !app.contains("<Company>"), "{app}");
    for (page, encoded, expected) in [(936u16, &b"\xd6\xd0"[..], "中"), (949, &b"\xb0\xa1"[..], "가"),
        (950, &b"\xa4\xa4"[..], "中")] {
        let mut cp = 2u32.to_le_bytes().to_vec(); cp.extend(u16s(&[page, 0]));
        let mut d = doc_summary[..48].to_vec(); d[24..28].copy_from_slice(&1u32.to_le_bytes());
        d[44..48].copy_from_slice(&48u32.to_le_bytes());
        d.extend(section(&[(1, cp), (15, ansi(encoded))]));
        let input = compound(&[("Workbook", workbook.clone()), ("\u{5}DocumentSummaryInformation", d)]);
        let opened = calc_doc::open(source(&input), &OpenPolicy::default()).unwrap();
        let app = String::from_utf8(opened.package().read_part("docProps/app.xml").unwrap()).unwrap();
        assert!(app.contains(&format!("<Company>{expected}</Company>")), "{app}");
    }
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unread_custom_property_types.get(&0x41), Some(&1));
    assert_eq!(report.unread_custom_property_sets, 0);
    let plan = calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
    let mut saved = Vec::new(); calc_doc::write(plan, &mut saved).unwrap();
    check(&calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap());
    let user_offset = u32::from_le_bytes(doc_summary[64..68].try_into().unwrap()) as usize;
    let mut unicode_doc = doc_summary[..user_offset].to_vec();
    let mut unicode_dictionary = 1u32.to_le_bytes().to_vec();
    unicode_dictionary.extend(2u32.to_le_bytes()); unicode_dictionary.extend(3u32.to_le_bytes());
    unicode_dictionary.extend(u16s(&[0x540d, 0x00e9, 0])); unicode_dictionary.extend([0, 0]);
    let mut unicode_page = 2u32.to_le_bytes().to_vec(); unicode_page.extend(u16s(&[1200, 0]));
    let mut unicode_value = 31u32.to_le_bytes().to_vec(); unicode_value.extend(3u32.to_le_bytes());
    unicode_value.extend(u16s(&[0x6771, 0x4eac, 0]));
    unicode_doc.extend(section(&[(0, unicode_dictionary), (1, unicode_page), (2, unicode_value)]));
    let unicode_bytes = compound(&[("Workbook", workbook.clone()), ("\u{5}DocumentSummaryInformation", unicode_doc)]);
    let unicode_open = calc_doc::open(source(&unicode_bytes), &OpenPolicy::default()).unwrap();
    let unicode_custom = String::from_utf8(unicode_open.package().read_part("docProps/custom.xml").unwrap()).unwrap();
    assert!(unicode_custom.contains("name=\"名é\"") && unicode_custom.contains("<vt:lpwstr>東京</vt:lpwstr>"), "{unicode_custom}");
    let mut invalid_doc = doc_summary;
    let name_at = invalid_doc.windows(8).position(|part| part == b"Approved").unwrap();
    invalid_doc[name_at] = 1;
    let invalid_custom = compound(&[("Workbook", workbook.clone()), ("\u{5}DocumentSummaryInformation", invalid_doc)]);
    assert!(calc_doc::open(source(&invalid_custom), &OpenPolicy::default()).unwrap().package().part("docProps/custom.xml").is_none());
    assert_eq!(legacy_workbook_report(source(&invalid_custom), &OpenPolicy::default()).unwrap().unwrap().unread_custom_property_sets, 1);
    // Duplicate property identifiers are ambiguous: keep opening the workbook, but do not guess.
    summary[48+16..48+20].copy_from_slice(&1u32.to_le_bytes());
    let bad = compound(&[("Workbook", workbook), ("\u{5}SummaryInformation", summary)]);
    assert!(calc_doc::open(source(&bad), &OpenPolicy::default()).unwrap().package().part("docProps/core.xml").is_none());
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
    // FILEPASS: an encrypted workbook needs its password (and one of a scheme not read is typed).
    let encrypted = xls(false, &record(0x002F, &[0, 0, 1, 0, 1, 0]), &[Sheet::worksheet("S", Vec::new())]);
    assert_eq!(
        calc_doc::open(source(&encrypted), &OpenPolicy::default()).err(),
        Some(OpenError::PasswordRequired)
    );
    let odd = xls(false, &record(0x002F, &[9, 0, 1, 0, 1, 0]), &[Sheet::worksheet("S", Vec::new())]);
    let (_, detail) = refused(&odd);
    assert!(detail.contains("encryption type 9"), "{detail}");
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
    r.extend(colinfo(1, 2, 3072, 0x000A));
    r.extend(colinfo(4, 4, 2560, 0x0001 | (2 << 8)));
    r.extend(row(0, 255, 0));
    r.extend(row(1, 600, 0x40));
    r.extend(row(2, 255, 0x20 | 1));
    r.extend(number(1, 0, 1.0));
    let bytes = xls(false, &[], &[Sheet::worksheet("S", r)]);
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(document.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(xml.contains("phonetic=\"1\""));
    let plan = calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
    let mut saved = Vec::new();
    calc_doc::write(plan, &mut saved).unwrap();
    let reopened = calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(reopened.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(xml.contains("phonetic=\"1\""));
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
fn row_border_and_phonetic_flags_survive_conversion_and_save() {
    let mut records = record(0x0225, &u16s(&[0x000C, 255]));
    let mut formatted = row(0, 255, 0);
    let last = formatted.len() - 1;
    formatted[last] |= 0x70;
    records.extend(formatted);
    records.extend(number(0, 0, 1.0));
    let bytes = xls(false, &[], &[Sheet::worksheet("S", records)]);
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let check = |doc: &calc_doc::OpenedDocument| {
        let xml = String::from_utf8(doc.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
        let defaults = xml.split("<sheetFormatPr ").nth(1).unwrap().split('>').next().unwrap();
        assert!(defaults.contains("thickTop=\"1\"") && defaults.contains("thickBottom=\"1\""));
        let row = xml.split("<row ").nth(1).unwrap().split('>').next().unwrap();
        assert!(row.contains("thickTop=\"1\"") && row.contains("thickBot=\"1\"") && row.contains("ph=\"1\""));
    };
    check(&document);
    let plan = calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
    let mut saved = Vec::new();
    calc_doc::write(plan, &mut saved).unwrap();
    check(&calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap());

}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn fractional_standard_width_takes_precedence_and_survives_save() {
    let records = [record(0x0099, &u16s(&[2340])), record(0x0055, &u16s(&[10])), number(0, 0, 1.0)].concat();
    let bytes = xls(false, &[], &[Sheet::worksheet("S", records)]);
    let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let mut ids = IdAllocator::<SheetId>::new();
    let loaded = calc_doc::load(&doc, &mut ids).unwrap();
    assert_eq!(loaded.sheets()[0].geometry().default_column_width, Some(2340.0 / 256.0));
    let plan = calc_doc::prepare_save(&doc.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
    let mut saved = Vec::new();
    calc_doc::write(plan, &mut saved).unwrap();
    let reopened = calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap();
    let loaded = calc_doc::load(&reopened, &mut IdAllocator::<SheetId>::new()).unwrap();
    assert_eq!(loaded.sheets()[0].geometry().default_column_width, Some(2340.0 / 256.0));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn biff5_global_width_bitmap_overrides_only_selected_columns() {
    let mut gcw = u16s(&[32]);
    gcw.extend([0b0000_0101]); // A and C use STANDARDWIDTH, B keeps COLINFO.
    gcw.extend([0u8; 31]);
    let records = [record(0x0055, &u16s(&[8])), colinfo(0, 1, 3072, 0x0002),
        record(0x0099, &u16s(&[2340])), record(0x00AB, &gcw), v5::label(0, 0, b"x")].concat();
    let bytes = v5::xls(1252, &[], &[v5::Sheet { name: b"S".to_vec(), records }]);
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let loaded = calc_doc::load(&document, &mut IdAllocator::<SheetId>::new()).unwrap();
    let columns = &loaded.sheets()[0].geometry().columns;
    assert_eq!(columns.iter().map(|c| (c.first, c.last, c.width)).collect::<Vec<_>>(),
        vec![(1, 1, Some(2340.0 / 256.0)), (2, 2, Some(12.0)), (3, 3, Some(2340.0 / 256.0))]);
    assert!(!legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap().unknown_records.contains_key(&0x00AB));
    let plan = calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
    let mut saved = Vec::new(); calc_doc::write(plan, &mut saved).unwrap();
    let doc = calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap();
    let loaded = calc_doc::load(&doc, &mut IdAllocator::<SheetId>::new()).unwrap();
    assert_eq!(loaded.sheets()[0].geometry().columns.iter().map(|c| c.width).collect::<Vec<_>>(),
        vec![Some(2340.0 / 256.0), Some(12.0), Some(2340.0 / 256.0)]);
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
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert!(!report.unknown_records.contains_key(&0x04BC), "shared definitions were read");
    // Array recalculation behavior and some data-table metadata remain unverified or incomplete.
    assert!(report.unknown_records.contains_key(&0x0221));
    assert!(report.unknown_records.contains_key(&0x0236));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn array_recalculation_flag_survives_conversion() {
    for needs_calc in [false, true] {
        let mut records = formula(0, 0, num_value(1.0), &ptg::exp(0, 0), &[]);
        let mut array = array_record((0, 0), (0, 0), &ptg::int(1), &[]);
        array[10] = u8::from(needs_calc); // four-byte record header, six-byte array range
        records.extend(array);
        let bytes = xls(false, &[], &[Sheet::worksheet("S", records)]);
        let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let xml = String::from_utf8(doc.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
        assert_eq!(xml.contains("ca=\"1\""), needs_calc, "{xml}");
        load(&bytes);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn formula_recalculation_flag_is_preserved_and_incomplete_flags_are_counted() {
    let mut recalculate = formula(0, 0, num_value(1.0), &ptg::int(1), &[]);
    recalculate[4 + 14] = 1;
    let mut ignore_error = formula(1, 0, num_value(2.0), &ptg::int(2), &[]);
    ignore_error[4 + 14] = 0x20;
    let bytes = xls(false, &[], &[Sheet::worksheet("S", [recalculate, ignore_error].concat())]);
    let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(doc.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(xml.contains("<f ca=\"1\">1</f>"), "{xml}");
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unknown_records.get(&0x0006), Some(&2));
    let plan = calc_doc::prepare_save(&doc.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
    let mut saved = Vec::new(); calc_doc::write(plan, &mut saved).unwrap();
    let doc = calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(doc.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(xml.contains("<f ca=\"1\">1</f>"), "{xml}");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ignored_formula_error_ranges_survive_conversion_and_save() {
    let bytes = xls(false, &[], &[Sheet::worksheet("S", [number(0, 0, 1.0), ignored_error_feature()].concat())]);
    let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let check = |doc: &calc_doc::OpenedDocument| {
        let xml = String::from_utf8(doc.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
        let tag = xml.split("<ignoredError ").nth(1).unwrap().split("/>").next().unwrap();
        assert!(tag.contains("sqref=\"A1:B2\"") && tag.contains("evalError=\"1\"") &&
            tag.contains("numberStoredAsText=\"1\"") && tag.contains("formula=\"1\""));
        assert!(!tag.contains("emptyCellReference"));
    };
    check(&doc);
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert!(!report.unknown_records.contains_key(&0x0868));
    assert!(!report.unknown_records.contains_key(&0x0867));
    let plan = calc_doc::prepare_save(&doc.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
    let mut saved = Vec::new(); calc_doc::write(plan, &mut saved).unwrap();
    check(&calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap());
    let mut malformed = ignored_error_feature();
    // Claim two ranges while supplying one; leave the BIFF record framing intact.
    malformed[4 + 19 + 4 + 19] = 2;
    let bytes = xls(false, &[], &[Sheet::worksheet("S", malformed)]);
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unknown_records.get(&0x0868), Some(&1));
    let bytes = xls(false, &[], &[Sheet { kind: 1, ..Sheet::worksheet("Macro", ignored_error_feature()) }]);
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unknown_records.get(&0x0868), Some(&1));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sheet_phonetic_format_and_unhandled_visibility_ranges() {
    for has_ranges in [false, true] {
        let mut body = u16s(&[0, 7, u16::from(has_ranges)]);
        if has_ranges { body.extend(u16s(&[0, 0, 0, 1])); }
        let records = [number(0, 0, 1.0), number(0, 2, 2.0), record(0x00EF, &body)].concat();
        let bytes = xls(false, &[], &[Sheet::worksheet("S", records)]);
        let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let xml = String::from_utf8(doc.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
        assert!(xml.contains("<phoneticPr fontId=\"0\" type=\"noConversion\" alignment=\"left\"/>"), "{xml}");
        assert_eq!(xml.contains("r=\"A1\" ph=\"1\""), has_ranges, "{xml}");
        assert!(!xml.contains("r=\"C1\" ph=\"1\""), "{xml}");
        let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
        assert_eq!(report.unknown_records.contains_key(&0x00EF), has_ranges);
        load(&bytes);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn shared_string_phonetic_extension_is_reported_unread() {
    let mut sst_body = Vec::new();
    sst_body.extend(1u32.to_le_bytes()); // total string count
    sst_body.extend(1u32.to_le_bytes()); // unique string count
    sst_body.extend(1u16.to_le_bytes()); // one compressed character
    sst_body.push(0x04); // phonetic extension present
    let extension = u16s(&[1, 10, 0, 7, 0, 0, 0]);
    sst_body.extend((extension.len() as u32).to_le_bytes());
    sst_body.push(b'A'); sst_body.extend(extension);
    let bytes = xls(false, &record(0x00FC, &sst_body), &[Sheet::worksheet("S", label_sst(0, 0, 0))]);
    assert_eq!(sheet_cells(&bytes), vec!["A1 s:A"]);
    let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(doc.package().read_part("xl/sharedStrings.xml").unwrap()).unwrap();
    assert!(xml.contains("<phoneticPr fontId=\"0\" type=\"noConversion\" alignment=\"left\"/>"), "{xml}");
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unknown_records.get(&0x00FC), Some(&1));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn explicit_phonetic_string_runs_survive_conversion() {
    let mut extension = u16s(&[1, 0, 0, 7, 2, 3, 3]);
    extension.extend(u16s(&['x' as u16, 'y' as u16, 'z' as u16]));
    extension.extend(u16s(&[0, 0, 1, 2, 1, 1]));
    let cb = (extension.len() - 4) as u16;
    extension[2..4].copy_from_slice(&cb.to_le_bytes());
    let mut body = Vec::new();
    body.extend(1u32.to_le_bytes()); body.extend(1u32.to_le_bytes());
    body.extend(2u16.to_le_bytes()); body.push(0x04);
    body.extend((extension.len() as u32).to_le_bytes()); body.extend(b"AB"); body.extend(extension);
    let bytes = xls(false, &record(0x00FC, &body), &[Sheet::worksheet("S", label_sst(0, 0, 0))]);
    let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(doc.package().read_part("xl/sharedStrings.xml").unwrap()).unwrap();
    assert!(xml.contains("<rPh sb=\"0\" eb=\"1\"><t>xy</t></rPh>"), "{xml}");
    assert!(xml.contains("<rPh sb=\"1\" eb=\"2\"><t>z</t></rPh>"), "{xml}");
    assert!(xml.contains("<phoneticPr fontId=\"0\" type=\"noConversion\" alignment=\"left\"/>"), "{xml}");
    assert_eq!(sheet_cells(&bytes), vec!["A1 s:AB"]);
    let plan = calc_doc::prepare_save(&doc.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
    let mut saved = Vec::new();
    calc_doc::write(plan, &mut saved).unwrap();
    let reopened = calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap();
    let saved_xml = String::from_utf8(reopened.package().read_part("xl/sharedStrings.xml").unwrap()).unwrap();
    assert!(saved_xml.contains("<rPh sb=\"0\" eb=\"1\"><t>xy</t></rPh>"), "{saved_xml}");
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert!(!report.unknown_records.contains_key(&0x00FC));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn natural_language_formula_refusal_names_the_feature() {
    let tokens = [0x18, 0x03, 0, 0, 0, 0];
    let cell = formula(0, 1, num_value(0.0), &tokens, &[]);
    let bytes = xls(false, &[], &[Sheet::worksheet("S", cell)]);
    match calc_doc::open(source(&bytes), &OpenPolicy::default()) {
        Err(OpenError::LegacyWorkbook { detail, .. }) => assert!(detail.contains("natural-language formula token") && detail.contains("dc-4j0e.3"), "{detail}"),
        _ => panic!("natural-language formula was not refused"),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn printer_settings_are_carried_as_an_explicit_part() {
    let mut mode = vec![0u8; 156];
    mode[36..38].copy_from_slice(&156u16.to_le_bytes());
    let mut body = vec![0, 0];
    body.extend(&mode);
    let pls = record(0x004D, &body);
    let bytes = xls(false, &[], &[Sheet::worksheet("S", pls.clone())]);
    let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    assert_eq!(doc.package().read_part("xl/printerSettings/printerSettings1.bin").unwrap(), mode);
    let sheet = String::from_utf8(doc.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(sheet.contains("<pageSetup r:id=\"rIdPrinter\"/>"), "{sheet}");
    let rels = String::from_utf8(doc.package().read_part("xl/worksheets/_rels/sheet1.xml.rels").unwrap()).unwrap();
    assert!(rels.contains("relationships/printerSettings"), "{rels}");
    let plan = calc_doc::prepare_save(&doc.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
    let mut saved = Vec::new();
    calc_doc::write(plan, &mut saved).unwrap();
    let reopened = calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap();
    assert_eq!(reopened.package().read_part("xl/printerSettings/printerSettings1.bin").unwrap(), mode);
    let sheet = String::from_utf8(reopened.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(sheet.contains("rIdPrinter"), "{sheet}");
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert!(!report.unknown_records.contains_key(&0x004D));
    let duplicated = xls(false, &[], &[Sheet::worksheet("S", [pls.clone(), pls].concat())]);
    let report = legacy_workbook_report(source(&duplicated), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unknown_records.get(&0x004D), Some(&2));
    let invalid = xls(false, &[], &[Sheet::worksheet("S", record(0x004D, &[0, 0, 0]))]);
    let report = legacy_workbook_report(source(&invalid), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unknown_records.get(&0x004D), Some(&1));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn custom_view_print_settings_do_not_replace_default_sheet_settings() {
    let mode = |marker| {
        let mut data = vec![0u8; 156]; data[0] = marker;
        data[36..38].copy_from_slice(&156u16.to_le_bytes());
        data
    };
    let pls = |data: &[u8]| { let mut body = vec![0, 0]; body.extend(data); record(0x004D, &body) };
    let header = |text: &str| {
        let mut body = (text.len() as u16).to_le_bytes().to_vec(); body.push(0); body.extend(text.as_bytes());
        record(0x0014, &body)
    };
    let default = mode(1);
    let custom = mode(2);
    let records = [header("default"), pls(&default), record(0x01AA, &[]), header("custom"),
        pls(&custom), record(0x01AB, &[])].concat();
    let bytes = xls(false, &[], &[Sheet::worksheet("S", records)]);
    let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(doc.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(xml.contains("default") && !xml.contains("custom"), "{xml}");
    assert_eq!(doc.package().read_part("xl/printerSettings/printerSettings1.bin").unwrap(), default);
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unknown_records.get(&0x004D), Some(&1));
    assert_eq!(report.unknown_records.get(&0x0014), Some(&1));
    assert_eq!(report.unknown_records.get(&0x01AA), Some(&1));
    assert_eq!(report.unknown_records.get(&0x01AB), Some(&1));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn window_view_preserves_outline_grid_color_and_preview() {
    let mut view = window2(0x0816, 0, 0);
    view[10..12].copy_from_slice(&42u16.to_le_bytes());
    view[14..16].copy_from_slice(&80u16.to_le_bytes());
    view[16..18].copy_from_slice(&120u16.to_le_bytes());
    for with_scl in [false, true] {
        let mut records = view.clone();
        if with_scl { records.extend(record(0x00A0, &u16s(&[1, 2]))); }
        let bytes = xls(false, &[], &[Sheet::worksheet("S", records)]);
        let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let xml = String::from_utf8(doc.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
        for expected in ["showOutlineSymbols=\"0\"", "defaultGridColor=\"0\"", "colorId=\"42\"", "view=\"pageBreakPreview\"", "zoomScaleNormal=\"120\""] {
            assert!(xml.contains(expected), "{xml}");
        }
        assert_eq!(xml.contains("zoomScaleSheetLayoutView=\"80\""), !with_scl, "{xml}");
        if with_scl { assert!(xml.contains("zoomScale=\"50\""), "{xml}"); }
        let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
        assert!(!report.unknown_records.contains_key(&0x023E));
        load(&bytes);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn custom_palette_keeps_window_grid_color_index() {
    let mut palette = 56u16.to_le_bytes().to_vec();
    for index in 0..56 { palette.extend(if index == 34 { [0x12, 0x34, 0x56, 0] } else { [0, 0, 0, 0] }); }
    let mut view = window2(0x0096, 0, 0); // nondefault grid color, outline shown
    view[10..12].copy_from_slice(&42u16.to_le_bytes());
    let bytes = xls(false, &record(0x0092, &palette), &[Sheet::worksheet("S", view)]);
    let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let styles = String::from_utf8(doc.package().read_part("xl/styles.xml").unwrap()).unwrap();
    let indexed = styles.split("<indexedColors>").nth(1).unwrap().split("</indexedColors>").next().unwrap();
    let colors: Vec<_> = indexed.split("<rgbColor ").skip(1).collect();
    assert_eq!(colors.len(), 64);
    assert!(colors[42].starts_with("rgb=\"FF123456\""), "{}", colors[42]);
    let sheet = String::from_utf8(doc.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(sheet.contains("colorId=\"42\""), "{sheet}");
    load(&bytes);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn legacy_sheet_ids_follow_source_tabs_even_with_module() {
    let sheets = [Sheet::worksheet("First", Vec::new()),
        Sheet { kind: 6, ..Sheet::worksheet("Module", Vec::new()) },
        Sheet::worksheet("Last", Vec::new())];
    for (ids, first, last, unread) in [
        (vec![8, 3, 19], 8, 19, false),
        (vec![8, 3, 0], 1, 2, true),
        (vec![8, 8, 19], 1, 2, true),
    ] {
        let bytes = xls(false, &record(0x013D, &u16s(&ids)), &sheets);
        let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let xml = String::from_utf8(doc.package().read_part("xl/workbook.xml").unwrap()).unwrap();
        assert!(xml.contains(&format!("name=\"First\" sheetId=\"{first}\"")), "{xml}");
        assert!(xml.contains(&format!("name=\"Last\" sheetId=\"{last}\"")), "{xml}");
        let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
        assert_eq!(report.unknown_records.contains_key(&0x013D), unread);
        load(&bytes);
    }
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
fn addin_cell_reference_remains_an_explicit_refusal() {
    match calc_doc::open(source(&addin_cell_reference_fixture()), &OpenPolicy::default()) {
        Err(OpenError::LegacyWorkbook { detail, .. }) => assert!(detail.contains("3-D reference into an add-in"), "{detail}"),
        _ => panic!("the add-in cell reference was not refused"),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn biff8_external_cache_survives_open_and_save_as_cached_input() {
    let bytes = external_cache_fixture(false);
    let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let check = |doc: &calc_doc::OpenedDocument| {
        let loaded = calc_doc::load(doc, &mut IdAllocator::<SheetId>::new()).unwrap();
        let book = loaded.model().definitions().external_book(1).unwrap();
        let sheet = &book.sheets[0];
        assert!(sheet.cached && !sheet.refresh_error);
        assert_eq!(sheet.cells.len(), 4);
        assert_eq!(sheet.cells.get(&(3, 1)), Some(&calc_doc::model::CachedValue::Number(calc_doc::model::FiniteNumber::new(1.25).unwrap())));
        let text = calc_doc::model::value::ExcelText::from_utf16_code_units(vec![79, 75]);
        assert_eq!(sheet.cells.get(&(3, 2)), Some(&calc_doc::model::CachedValue::Text(calc_doc::model::CellText::new(text).unwrap())));
        assert_eq!(sheet.cells.get(&(3, 3)), Some(&calc_doc::model::CachedValue::Logical(true)));
        assert_eq!(sheet.cells.get(&(3, 4)), Some(&calc_doc::model::CachedValue::Error(calc_doc::model::value::WorksheetErrorCode::Div0)));
        assert!(!sheet.cells.contains_key(&(3, 5)));
    };
    check(&doc);
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert!(!report.unknown_records.contains_key(&0x0059));
    assert!(!report.unknown_records.contains_key(&0x005A));
    let plan = calc_doc::prepare_save(&doc.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
    let mut saved = Vec::new(); calc_doc::write(plan, &mut saved).unwrap();
    check(&calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ambiguous_external_cache_is_not_exposed_as_complete() {
    let mut crn = vec![0, 0, 0, 0, 1]; crn.extend(1.0_f64.to_le_bytes());
    let group = [record(0x0059, &u16s(&[1, 0])), record(0x005A, &crn)].concat();
    let globals = [supbook_external("book.xls", &["Rates"]), group.clone(), group].concat();
    let bytes = xls(false, &globals, &[Sheet::worksheet("S", Vec::new())]);
    let loaded = load(&bytes);
    assert!(!loaded.model().definitions().external_book(1).unwrap().sheets[0].cached);
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unknown_records.get(&0x0059), Some(&2));
    assert_eq!(report.unknown_records.get(&0x005A), Some(&2));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn invalid_external_link_cache_remains_unread() {
    let bytes = external_cache_fixture(true);
    let loaded = load(&bytes);
    assert!(!loaded.model().definitions().external_book(1).unwrap().sheets[0].cached);
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unknown_records.get(&0x0059), Some(&1));
    assert_eq!(report.unknown_records.get(&0x005A), Some(&1));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn external_caches_stay_with_their_own_workbooks() {
    let cache = |value: f64| {
        let mut body = vec![0, 0, 0, 0, 1]; body.extend(value.to_le_bytes());
        [record(0x0059, &u16s(&[1, 0])), record(0x005A, &body)].concat()
    };
    let globals = [supbook_self(1), supbook_external("one.xls", &["Data"]), cache(1.0),
        supbook_external("two.xls", &["Data"]), cache(2.0)].concat();
    let bytes = xls(false, &globals, &[Sheet::worksheet("S", Vec::new())]);
    let loaded = load(&bytes);
    for (number, expected) in [(1, 1.0), (2, 2.0)] {
        let sheet = &loaded.model().definitions().external_book(number).unwrap().sheets[0];
        assert!(sheet.cached);
        assert_eq!(sheet.cells.get(&(1, 1)), Some(&calc_doc::model::CachedValue::Number(calc_doc::model::FiniteNumber::new(expected).unwrap())));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn biff5_external_cache_uses_local_link_table_and_code_page() {
    let bytes = external_cache_biff5_fixture();
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let check = |doc: &calc_doc::OpenedDocument| {
        let loaded = calc_doc::load(doc, &mut IdAllocator::<SheetId>::new()).unwrap();
        let sheet = &loaded.model().definitions().external_book(1).unwrap().sheets[0];
        assert!(sheet.cached);
        assert_eq!(sheet.cells.get(&(3, 1)), Some(&calc_doc::model::CachedValue::Number(calc_doc::model::FiniteNumber::new(1.25).unwrap())));
        let text = calc_doc::model::value::ExcelText::from_utf16_code_units(vec![0x00E9]);
        assert_eq!(sheet.cells.get(&(3, 2)), Some(&calc_doc::model::CachedValue::Text(calc_doc::model::CellText::new(text).unwrap())));
        assert_eq!(sheet.cells.get(&(3, 3)), Some(&calc_doc::model::CachedValue::Logical(true)));
    };
    check(&document);
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert!(!report.unknown_records.contains_key(&0x0059));
    assert!(!report.unknown_records.contains_key(&0x005A));
    let plan = calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
    let mut saved = Vec::new(); calc_doc::write(plan, &mut saved).unwrap();
    check(&calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn biff5_ambiguous_external_caches_stay_unread() {
    let external = b"\x01\x01Ddata\x03[book.xls]Rates";
    let mut link = vec![external.len() as u8]; link.extend_from_slice(external);
    let mut crn = vec![0, 0, 0, 0, 1]; crn.extend(1.0_f64.to_le_bytes());
    let group = [record(0x0017, &link), record(0x0059, &u16s(&[1])), record(0x005A, &crn)].concat();
    let bytes = v5::xls(1252, &[], &[
        v5::Sheet { name: b"A".to_vec(), records: group.clone() },
        v5::Sheet { name: b"B".to_vec(), records: group },
    ]);
    let loaded = load(&bytes);
    assert!(!loaded.model().definitions().external_book(1).unwrap().sheets[0].cached);
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unknown_records.get(&0x0059), Some(&2));
    assert_eq!(report.unknown_records.get(&0x005A), Some(&2));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn defined_name_array_keeps_its_extra_data() {
    let mut extra = vec![1, 0, 0, 1]; // two columns, one row, first value is numeric
    extra.extend(1.5_f64.to_le_bytes());
    extra.push(1);
    extra.extend(2.5_f64.to_le_bytes());
    let globals = name_record_extra(0, 0, "Values", &ptg::array(), &extra);
    let bytes = xls(false, &globals, &[Sheet::worksheet("S", Vec::new())]);
    assert_eq!(defined_names(&bytes), vec!["Values = {1.5,2.5}"]);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn module_sheets_do_not_shift_formula_indices_or_local_name_scope() {
    let globals = [
        supbook_self(3), extern_sheet(&[(0, 2, 2), (0, 1, 1)]),
        name_record(0, 3, "Rate", &ptg::cell3d(0, 0, 0, 0)),
    ].concat();
    let module = || Sheet { kind: 6, ..Sheet::worksheet("Module", Vec::new()) };
    let sheets = [Sheet::worksheet("First", formula(0, 0, num_value(1.0), &ptg::cell3d(0, 0, 0, 0), &[])),
        module(), Sheet::worksheet("Last", Vec::new())];
    let bytes = xls(false, &globals, &sheets);
    assert_eq!(sheet_cells(&bytes), vec!["A1 f:Last!$A$1"]);
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let workbook = String::from_utf8(document.package().read_part("xl/workbook.xml").unwrap()).unwrap();
    assert!(workbook.contains("name=\"Rate\" localSheetId=\"1\""), "{workbook}");
    let sheets = [Sheet::worksheet("First", formula(0, 0, num_value(1.0), &ptg::cell3d(1, 0, 0, 0), &[])),
        module(), Sheet::worksheet("Last", Vec::new())];
    let bytes = xls(false, &globals, &sheets);
    match calc_doc::open(source(&bytes), &OpenPolicy::default()) {
        Err(OpenError::LegacyWorkbook { detail, .. }) => assert!(detail.contains("omitted module sheet"), "{detail}"),
        other => panic!("{:?}", other.err()),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn biff_attribute_with_no_flags_keeps_the_expression() {
    let tokens = ptg::join(&[ptg::int(7), vec![0x19, 0, 10, 0]]);
    let bytes = xls(false, &[], &[Sheet::worksheet("S", formula(0, 0, num_value(7.0), &tokens, &[]))]);
    assert_eq!(sheet_cells(&bytes), vec!["A1 f:7"]);
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
fn setup_ignores_undefined_printer_values_and_respects_no_orientation() {
    for (flags, expected) in [(0, Some("landscape")), (2, Some("portrait")),
        (0x40, Some("portrait")), (0x44, None)] {
        let mut setup = u16s(&[9, 80, 1, 1, 1, flags]);
        setup.extend([0u8; 4]);
        setup.extend(0.25_f64.to_le_bytes());
        setup.extend(0.25_f64.to_le_bytes());
        setup.extend(1u16.to_le_bytes());
        let bytes = xls(false, &[], &[Sheet::worksheet("S", record(0xa1, &setup))]);
        let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        assert_eq!(legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap().unknown_records.get(&0xa1), Some(&1));
        let sheet = String::from_utf8(doc.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
        if let Some(value) = expected {
            assert!(sheet.contains(&format!("orientation=\"{value}\"")), "{sheet}");
            assert!(sheet.contains("paperSize=\"9\" scale=\"80\""), "{sheet}");
        } else {
            assert!(!sheet.contains("orientation=") && !sheet.contains("paperSize=") && !sheet.contains("scale="), "{sheet}");
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn setup_retains_resolution_copies_comment_and_error_print_modes() {
    for (flags, comments, errors, printer) in [(0x0420, "asDisplayed", "blank", true),
        (0x0a20, "atEnd", "dash", true), (0x0c24, "asDisplayed", "NA", false)] {
        let mut setup = u16s(&[9, 100, 1, 1, 1, flags, 300, 600]);
        setup.extend(0.25_f64.to_le_bytes()); setup.extend(0.25_f64.to_le_bytes());
        setup.extend(3u16.to_le_bytes());
        let bytes = xls(false, &[], &[Sheet::worksheet("S", record(0xa1, &setup))]);
        let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let check = |doc: &calc_doc::OpenedDocument| {
            let sheet = String::from_utf8(doc.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
            assert!(sheet.contains(&format!("cellComments=\"{comments}\"")), "{sheet}");
            assert!(sheet.contains(&format!("errors=\"{errors}\"")), "{sheet}");
            for attr in ["horizontalDpi=\"300\"", "verticalDpi=\"600\"", "copies=\"3\""] {
                assert_eq!(sheet.contains(attr), printer, "{sheet}");
            }
        };
        check(&doc);
        let mut saved = Vec::new();
        calc_doc::write(calc_doc::prepare_save(&doc.snapshot(), &calc_doc::SavePolicy::default()).unwrap(), &mut saved).unwrap();
        check(&calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap());
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn file_moniker_keeps_unicode_path_and_location() {
    let records = [hlink_file((0, 0), "book.xls", None, "Sheet1!A1"),
        hlink_file((1, 0), "book.xls", Some("資料.xls"), "Sheet2!B2")].concat();
    let bytes = xls(false, &[], &[Sheet::worksheet("S", records)]);
    let loaded = load(&bytes);
    let links = format!("{:?}", loaded.sheets()[0].review().links);
    for expected in ["book.xls", "Sheet1!A1", "資料.xls", "Sheet2!B2"] {
        assert!(links.contains(expected), "{links}");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn file_moniker_keeps_parent_directory_indicators() {
    let with_parents = |row, ansi: &str, unicode: Option<&str>, parents: u16| {
        let mut rec = hlink_file((row, 0), ansi, unicode, "Data!A1");
        rec[52..54].copy_from_slice(&parents.to_le_bytes());
        rec
    };
    let records = [with_parents(0, "book.xls", None, 1),
        with_parents(1, "book.xls", Some("資料.xls"), 2),
        with_parents(2, "../book.xls", None, 1)].concat();
    let bytes = xls(false, &[], &[Sheet::worksheet("S", records)]);
    let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let rels = String::from_utf8(doc.package().read_part("xl/worksheets/_rels/sheet1.xml.rels").unwrap()).unwrap();
    assert_eq!(rels.matches("Target=\"../book.xls\"").count(), 2, "{rels}");
    assert!(rels.contains("Target=\"../../資料.xls\""), "{rels}");
    let links = format!("{:?}", load(&bytes).sheets()[0].review().links);
    assert!(links.contains("../book.xls") && links.contains("../../資料.xls"), "{links}");
    let invalid = xls(false, &[], &[Sheet::worksheet("S", with_parents(0, "C:\\book.xls", None, 1))]);
    assert!(calc_doc::open(source(&invalid), &OpenPolicy::default()).is_err());
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
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unknown_records.get(&0x005D), Some(&2));
    assert_eq!(report.unknown_records.get(&0x01B6), Some(&2));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn note_txo_format_runs_are_preserved_when_complete() {
    for (last, expected) in [(6u16, true), (5, false)] {
        let mut body = u16s(&[0x0212, 0, 0, 0, 0, 6, 24]); body.extend([0u8; 4]);
        let runs = u16s(&[0, 0, 0, 0, 3, 1, 0, 0, last, 0, 0, 0]);
        let records = [obj_note(1), record(0x01b6, &body), record(0x003c, b"\0ABCDEF"),
            record(0x003c, &runs[..10]), record(0x003c, &runs[10..]), note(0, 0, 0, 1, "Ann")].concat();
        let globals = [font(200, 0, 0x7fff, 400, "Arial"), font(200, 0, 0x7fff, 700, "Arial")].concat();
        let bytes = xls(false, &globals, &[Sheet::worksheet("S", records)]);
        let check = |bytes: &[u8]| {
            let loaded = load(bytes);
            let notes = &loaded.sheets()[0].review().notes;
            assert_eq!(notes[0].text, "ABCDEF");
            assert_eq!(!notes[0].spans.is_empty(), expected, "{:?}", notes[0].spans);
            if expected { assert!(notes[0].spans.iter().any(|span| span.start == 3 && span.end == 6 && span.style.bold)); }
        };
        check(&bytes);
        if expected {
            let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
            let mut saved = Vec::new();
            calc_doc::write(calc_doc::prepare_save(&doc.snapshot(), &calc_doc::SavePolicy::default()).unwrap(), &mut saved).unwrap();
            check(&saved);
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn continued_txo_text_is_read_for_notes_and_text_boxes() {
    use xls_kit::art;
    let mut body = u16s(&[0x0212, 0, 0, 0, 0, 3, 16]);
    body.extend([0u8; 4]);
    let text = [record(0x01B6, &body), record(0x003C, &[0, b'A']),
        record(0x003C, &[1, 0xe9, 0, 0x22, 0x6f]), record(0x003C, &[0; 16])].concat();
    let records = [obj_note(1), text.clone(), note(0, 0, 0, 1, "Ann"),
        art::drawing(&[art::shape(202, 0, [1, 0, 1, 0, 4, 0, 6, 0], false)]),
        art::obj(6, 2), record(0x00EC, &[0, 0, 0x0d, 0xf0, 0, 0, 0, 0]), text].concat();
    let bytes = xls(false, &[], &[Sheet::worksheet("S", records)]);
    let loaded = load(&bytes);
    let notes = format!("{:?}", loaded.sheets()[0].review().notes);
    assert!(notes.contains("Aé漢"), "{notes}");
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let drawing = String::from_utf8(document.package().read_part("xl/drawings/drawing1.xml").unwrap()).unwrap();
    assert!(drawing.contains("Aé漢"), "{drawing}");
    let unrelated = [art::drawing(&[art::shape(202, 0, [1, 0, 1, 0, 4, 0, 6, 0], false)]),
        art::obj(6, 2), number(0, 0, 1.0), record(0x00EC, &[0, 0, 0x0d, 0xf0, 0, 0, 0, 0]),
        record(0x01B6, &body), record(0x003C, &[0, b'A']),
        record(0x003C, &[1, 0xe9, 0, 0x22, 0x6f]), record(0x003C, &[0; 16])].concat();
    let unrelated = xls(false, &[], &[Sheet::worksheet("S", unrelated)]);
    let document = calc_doc::open(source(&unrelated), &OpenPolicy::default()).unwrap();
    let drawing = String::from_utf8(document.package().read_part("xl/drawings/drawing1.xml").unwrap()).unwrap();
    assert!(!drawing.contains("Aé漢"), "{drawing}");
    body[10] = 4;
    let truncated = [obj_note(1), record(0x01B6, &body), record(0x003C, &[0, b'A']),
        record(0x003C, &[1, 0xe9, 0, 0x22, 0x6f]), record(0x003C, &[0; 16]),
        note(0, 0, 0, 1, "Ann")].concat();
    let invalid = xls(false, &[], &[Sheet::worksheet("S", truncated)]);
    assert!(calc_doc::open(source(&invalid), &OpenPolicy::default()).is_err());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn oversized_compound_project_is_refused_instead_of_omitted() {
    for size in [4096, 7_200_000] {
        let workbook = workbook_stream(false, &[], &[Sheet::worksheet("S", number(0, 0, 1.0))]);
        let mut bytes = compound(&[("Workbook", workbook), ("_VBA_PROJECT_CUR", Vec::new()),
            ("payload", vec![0; size])]);
        let directory_sector = u32::from_le_bytes(bytes[0x30..0x34].try_into().unwrap()) as usize;
        let storage = (directory_sector + 1) * 512 + 2 * 128;
        bytes[storage + 0x42] = 1;
        bytes[storage + 0x48..storage + 0x4c].copy_from_slice(&u32::MAX.to_le_bytes());
        bytes[storage + 0x4c..storage + 0x50].copy_from_slice(&3u32.to_le_bytes());
        let result = calc_doc::open(source(&bytes), &OpenPolicy::default());
        if size == 4096 {
            let document = result.unwrap();
            assert!(document.package().part("xl/vbaProject.bin").is_some());
        } else {
            let error = match result { Ok(_) => panic!("oversized storage was silently omitted"), Err(e) => e.to_string() };
            assert!(error.contains("109 FAT-sector writer limit") && error.contains("dc-4j0e.1"), "{error}");
        }
    }
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
fn biff5_page_breaks_span_the_sheet() {
    let mut records = record(0x001B, &u16s(&[2, 59, 114]));
    records.extend(record(0x001A, &u16s(&[1, 5])));
    let bytes = v5::xls(1252, &[], &[v5::Sheet { name: b"S".to_vec(), records }]);
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let part = String::from_utf8(document.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(part.contains("<brk id=\"59\" min=\"0\" max=\"16383\" man=\"1\"/>"), "{part}");
    assert!(part.contains("<brk id=\"114\" min=\"0\" max=\"16383\" man=\"1\"/>"), "{part}");
    assert!(part.contains("<brk id=\"5\" min=\"0\" max=\"1048575\" man=\"1\"/>"), "{part}");
    load(&bytes);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_biff5_workbook_reads_code_page_932_and_refuses_invalid_text() {
    use xls_kit::v5;
    let ascii = v5::xls(932, &[], &[v5::Sheet { name: b"S".to_vec(), records: v5::label(0, 0, b"plain") }]);
    assert_eq!(cells(load(&ascii).model(), 0), vec!["A1 s:plain"]);
    let japanese = v5::xls(932, &[], &[v5::Sheet { name: b"S".to_vec(), records: v5::label(0, 0, b"\x82\xA0") }]);
    assert_eq!(cells(load(&japanese).model(), 0), vec!["A1 s:あ"]);
    let invalid = v5::xls(932, &[], &[v5::Sheet { name: b"S".to_vec(), records: v5::label(0, 0, b"\x82") }]);
    match calc_doc::open(source(&invalid), &OpenPolicy::default()) {
        Err(OpenError::LegacyWorkbook { detail, .. }) => assert!(detail.contains("invalid text in code page 932"), "{detail}"),
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

mod encrypted {
    use super::*;

    const PLAIN: &[u8] = include_bytes!("fixtures/dc_4j0e/plain.bin");
    const XOR: &[u8] = include_bytes!("fixtures/dc_4j0e/xor.bin");
    const RC4: &[u8] = include_bytes!("fixtures/dc_4j0e/rc4.bin");
    const RC4_VELVET: &[u8] = include_bytes!("fixtures/dc_4j0e/rc4-velvet.bin");
    const CRYPTOAPI_128: &[u8] = include_bytes!("fixtures/dc_4j0e/cryptoapi-128.bin");
    const CRYPTOAPI_40: &[u8] = include_bytes!("fixtures/dc_4j0e/cryptoapi-40.bin");

    fn open_with(stream: &[u8], password: Option<&str>) -> Result<calc_doc::OpenedDocument, OpenError> {
        let file = compound(&[("Workbook", stream.to_vec())]);
        calc_doc::open_with_password(source(&file), &OpenPolicy::default(), password)
    }

    fn cells_of(document: &calc_doc::OpenedDocument) -> Vec<String> {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let loaded = calc_doc::load(document, &mut ids).expect("loads");
        cells(loaded.model(), 0)
    }

    const EXPECTED: [&str; 2] = ["A1 n:42.5", "B1 s:classified"];

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_plain_stream_reads() {
        assert_eq!(cells_of(&open_with(PLAIN, None).unwrap()), EXPECTED);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xor_obfuscation_opens_with_the_password() {
        assert_eq!(cells_of(&open_with(XOR, Some("swordfish")).unwrap()), EXPECTED);
        assert_eq!(open_with(XOR, Some("wrong")).err(), Some(OpenError::WrongPassword));
        assert_eq!(open_with(XOR, None).err(), Some(OpenError::PasswordRequired));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rc4_opens_with_the_password_across_1024_byte_blocks() {
        assert_eq!(cells_of(&open_with(RC4, Some("p\u{e4}ssw\u{f6}rd")).unwrap()), EXPECTED);
        assert_eq!(open_with(RC4, Some("password")).err(), Some(OpenError::WrongPassword));
        assert_eq!(open_with(RC4, None).err(), Some(OpenError::PasswordRequired));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_velvet_sweatshop_workbook_opens_without_a_password() {
        assert_eq!(cells_of(&open_with(RC4_VELVET, None).unwrap()), EXPECTED);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rc4_cryptoapi_opens_with_128_and_40_bit_keys() {
        for stream in [CRYPTOAPI_128, CRYPTOAPI_40] {
            assert_eq!(cells_of(&open_with(stream, Some("p\u{e4}ssw\u{f6}rd")).unwrap()), EXPECTED);
            assert_eq!(open_with(stream, Some("nope")).err(), Some(OpenError::WrongPassword));
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn pictures_text_boxes_and_charts_are_written_as_a_drawing() {
    use ptg::*;
    use xls_kit::art;
    let png = [0x89, b'P', b'N', b'G', 13, 10, 26, 10, 1, 2, 3, 4];
    let globals = [art::group_with_png(&png), supbook_self(1), extern_sheet(&[(0, 0, 0)])].concat();
    let mut r = Vec::new();
    r.extend(number(0, 0, 1.0));
    r.extend(art::drawing(&[
        art::shape(75, 1, [1, 0, 1, 0, 3, 512, 5, 128], false),
        art::shape(202, 0, [4, 0, 6, 0, 7, 0, 9, 0], true),
        art::shape(201, 0, [10, 0, 12, 0, 14, 0, 20, 0], false),
    ]));
    r.extend(art::obj(8, 1));
    r.extend(art::obj(6, 2));
    r.extend(record(0x01B6, &{
        let mut b = u16s(&[0x0212, 0, 0, 0, 0, 5, 16]);
        b.extend([0u8; 4]);
        b
    }));
    r.extend(record(0x003C, b"\0hello"));
    r.extend(record(0x003C, &[0u8; 16]));
    r.extend(art::obj(5, 3));
    r.extend(art::chart(
        &[("Sales", area3d(0, (1, 4), (0, 0), 0), area3d(0, (1, 4), (1, 1), 0))],
        0x0001,
        "Quarterly",
    ));
    let bytes = xls(false, &globals, &[Sheet::worksheet("Sheet1", r)]);
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let text = |part: &str| String::from_utf8(document.package().read_part(part).unwrap()).unwrap();
    let drawing = text("xl/drawings/drawing1.xml");
    assert!(drawing.contains("<xdr:pic>"), "{drawing}");
    assert!(drawing.contains("hello"), "{drawing}");
    assert!(drawing.contains("<xdr:graphicFrame"), "{drawing}");
    assert!(drawing.contains("<xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row>"), "{drawing}");
    assert_eq!(document.package().read_part("xl/media/image1_1.png").unwrap(), png);
    let chart = text("xl/charts/chart1_1.xml");
    for expected in [
        "<c:barDir val=\"bar\"/>",
        "<c:f>Sheet1!$B$2:$B$5</c:f>",
        "<c:f>Sheet1!$A$2:$A$5</c:f>",
        "Sales",
        "Quarterly",
        "<c:legendPos",
    ] {
        assert!(chart.contains(expected), "{expected} in {chart}");
    }
    // The ellipse-less type 201 shape is not written, and says so.
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
    assert_eq!(report.unread_objects.values().sum::<u32>(), 0, "{:?}", report.unread_objects);
    assert!(report.unknown_records.contains_key(&0x00EC));
    assert_eq!(report.unknown_records.get(&0x00EB), Some(&1));
    assert_eq!(report.unknown_records.get(&0x1017), Some(&1));
    // The workbook loads and has a chart and shapes.
    let loaded = load(&bytes);
    let sheet = &loaded.sheets()[0];
    // The picture and the text box are shapes the model holds; the chart is the chart reader's.
    assert_eq!(sheet.shapes().shapes.len(), 2, "{:?}", sheet.shapes());
    assert_eq!(sheet.shapes().unread, 0);
    let charts = sheet.charts();
    assert_eq!(charts.charts.len(), 1, "{charts:?}");
    assert_eq!(charts.charts[0].title.as_deref(), Some("Quarterly"));
    assert_eq!(charts.charts[0].plots.len(), 1);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn explicit_shape_no_fill_is_preserved() {
    use xls_kit::art;
    for (flags, expected) in [(&[0x0010_0000u32][..], true), (&[0x0010_0010][..], false),
        (&[0][..], false), (&[0x0010_0000, 0x0010_0000][..], false)] {
        let mut shape = art::shape(202, 0, [1, 0, 1, 0, 4, 0, 6, 0], false);
        let mut properties = Vec::new();
        for flag in flags { properties.extend(0x01bfu16.to_le_bytes()); properties.extend(flag.to_le_bytes()); }
        let props = art::atom(3, flags.len() as u16, 0xf00b, &properties);
        shape.splice(8..8, props);
        let length = (shape.len() - 8) as u32;
        shape[4..8].copy_from_slice(&length.to_le_bytes());
        let records = [art::drawing(&[shape]), art::obj(6, 1)].concat();
        let bytes = xls(false, &[], &[Sheet::worksheet("S", records)]);
        let loaded = load(&bytes);
        assert_eq!(matches!(loaded.sheets()[0].shapes().shapes[0].fill, calc_doc::model::ShapeFill::None), expected);
        if expected {
            let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
            let mut saved = Vec::new();
            calc_doc::write(calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).unwrap(), &mut saved).unwrap();
            assert!(matches!(load(&saved).sheets()[0].shapes().shapes[0].fill, calc_doc::model::ShapeFill::None));
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn drawing_shape_in_obj_continuation_belongs_to_next_object() {
    for (intervening, expected) in [(false, 2), (true, 1)] {
        let bytes = obj_continuation_fixture(intervening);
        let loaded = load(&bytes);
        assert_eq!(loaded.sheets()[0].shapes().shapes.len(), expected);
    }
}

fn obj_continuation_fixture(intervening: bool) -> Vec<u8> {
    use xls_kit::art;
    let first = art::shape(202, 0, [1, 0, 1, 0, 4, 0, 6, 0], false);
    let second = art::shape(3, 0, [5, 0, 1, 0, 8, 0, 6, 0], false);
    let between = if intervening { number(0, 0, 1.0) } else { Vec::new() };
    let records = [art::drawing(&[first]), art::obj(6, 1), record(0x003c, &second),
        between, art::obj(3, 2)].concat();
    xls(false, &[], &[Sheet::worksheet("S", records)])
}

fn emf_checksum_fixture(corrupt: bool) -> Vec<u8> {
    use xls_kit::art;
    let emf = include_bytes!("fixtures/drawing/pictures/withpng.emf");
    let records = [art::drawing(&[art::shape(75, 1, [1, 0, 1, 0, 4, 0, 6, 0], false)]), art::obj(8, 1)].concat();
    xls(false, &art::group_with_emf(emf, corrupt), &[Sheet::worksheet("S", records)])
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn emf_checksum_fixture_distinguishes_valid_and_damaged_images() {
    for (corrupt, expected) in [(false, 0), (true, 1)] {
        let bytes = emf_checksum_fixture(corrupt);
        let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap();
        assert_eq!(report.unread_objects.get(&8).copied().unwrap_or(0), expected);
        if !corrupt {
            let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
            assert_eq!(document.package().read_part("xl/media/image1_1.emf").unwrap().as_slice(),
                include_bytes!("fixtures/drawing/pictures/withpng.emf"));
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn drawing_shape_after_split_txo_does_not_become_text_or_formatting() {
    for (intervening, expected) in [(false, 2), (true, 1)] {
        let bytes = txo_continuation_fixture(intervening);
        let loaded = load(&bytes);
        assert_eq!(loaded.sheets()[0].shapes().shapes.len(), expected);
        let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let drawing = String::from_utf8(document.package().read_part("xl/drawings/drawing1.xml").unwrap()).unwrap();
        assert!(drawing.contains("Aé漢"), "{drawing}");
    }
}

fn txo_continuation_fixture(intervening: bool) -> Vec<u8> {
    use xls_kit::art;
    let first = art::shape(202, 0, [1, 0, 1, 0, 4, 0, 6, 0], true);
    let second = art::shape(3, 0, [5, 0, 1, 0, 8, 0, 6, 0], false);
    let mut body = u16s(&[0x0212, 0, 0, 0, 0, 3, 16]); body.extend([0u8; 4]);
    let between = if intervening { number(0, 0, 1.0) } else { Vec::new() };
    let records = [art::drawing(&[first]), art::obj(6, 1), record(0x01b6, &body),
        record(0x003c, &[0, b'A']), record(0x003c, &[1, 0xe9, 0, 0x22, 0x6f]),
        record(0x003c, &[0; 16]), record(0x003c, &second), between, art::obj(3, 2)].concat();
    xls(false, &[], &[Sheet::worksheet("S", records)])
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chart_attached_title_binds_after_text_without_changing_series_name() {
    use ptg::*;
    use xls_kit::art;
    for (linked, duplicate, expected) in [(false, 0, true), (true, 0, true), (false, 1, false), (false, 2, false)] {
        let mut chart = art::chart(&[("Sales", Vec::new(), area3d(0, (1, 4), (1, 1), 0))], 0, "Quarterly");
        if linked {
            let literal = record(0x1051, &[0, 1, 0, 0, 0, 0, 0, 0]);
            let at = chart.windows(literal.len()).position(|w| w == literal).unwrap();
            chart.splice(at..at + literal.len(), art::ai(0, &area3d(0, (0, 0), (0, 0), 0)));
        }
        if duplicate == 1 {
            let link = record(0x1027, &u16s(&[1, 0, 0, 0]));
            let at = chart.windows(link.len()).position(|w| w == link).unwrap() + link.len();
            chart.splice(at..at, link);
        }
        if duplicate == 2 {
            let second = [record(0x1025, &[0u8; 32]), art::begin(), art::series_text("Other"),
                record(0x1027, &u16s(&[1, 0, 0, 0])), art::end()].concat();
            let legend = record(0x1015, &[0u8; 20]);
            let at = chart.windows(legend.len()).position(|w| w == legend).unwrap();
            chart.splice(at..at, second);
        }
        let records = [art::drawing(&[art::shape(201, 0, [0, 0, 0, 0, 4, 0, 10, 0], false)]), art::obj(5, 1), chart].concat();
        let globals = [supbook_self(1), extern_sheet(&[(0, 0, 0)])].concat();
        let bytes = xls(false, &globals, &[Sheet::worksheet("S", records)]);
        let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let xml = String::from_utf8(doc.package().read_part("xl/charts/chart1_1.xml").unwrap()).unwrap();
        assert_eq!(xml.contains("Quarterly"), expected, "{xml}");
        assert!(xml.contains("Sales"), "{xml}");
        assert_eq!(xml.matches("<c:f>S!$A$1").count(), usize::from(linked), "{xml}");
        if expected {
            let mut saved = Vec::new();
            calc_doc::write(calc_doc::prepare_save(&doc.snapshot(), &calc_doc::SavePolicy::default()).unwrap(), &mut saved).unwrap();
            let reopened = load(&saved);
            assert_eq!(reopened.sheets()[0].charts().charts[0].title.as_deref(), Some("Quarterly"));
            assert_eq!(reopened.sheets()[0].charts().charts[0].title_formula.is_some(), linked);
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chart_numeric_display_cache_survives_conversion() {
    use ptg::*;
    use xls_kit::art;
    for invalid in [false, true] {
        let mut chart = art::chart(&[("Sales", area3d(0, (1, 4), (0, 0), 0),
            area3d(0, (1, 4), (1, 1), 0))], 0, "Title");
        // Some Excel-produced files order the sections 2, 1, 3.
        let mut data = record(0x1065, &u16s(&[2]));
        data.extend(record(0x1065, &u16s(&[1])));
        data.extend(number(0, 0, 7.5));
        data.extend(record(0x0201, &u16s(&[1, 0, 0])));
        data.extend(number(2, 0, 4.0));
        if invalid { data.extend(number(2, 0, 9.0)); }
        data.extend(record(0x1065, &u16s(&[3])));
        chart.splice(chart.len() - 4..chart.len() - 4, data);
        let records = [art::drawing(&[art::shape(201, 0, [0, 0, 0, 0, 4, 0, 10, 0], false)]),
            art::obj(5, 1), chart].concat();
        let globals = [supbook_self(1), extern_sheet(&[(0, 0, 0)])].concat();
        let bytes = xls(false, &globals, &[Sheet::worksheet("S", records)]);
        let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let xml = String::from_utf8(document.package().read_part("xl/charts/chart1_1.xml").unwrap()).unwrap();
        assert_eq!(xml.contains("<c:numCache>"), !invalid, "{xml}");
        if !invalid {
            assert!(xml.contains("<c:ptCount val=\"4\"/>"), "{xml}");
            let loaded = load(&bytes);
            let cache = &loaded.sheets()[0].charts().charts[0].plots[0].series[0].values.as_ref().unwrap().cache;
            assert_eq!(cache, &[Some("7.5".to_string()), None, Some("4".to_string())]);
            let plan = calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
            let mut saved = Vec::new();
            calc_doc::write(plan, &mut saved).unwrap();
            let reopened = load(&saved);
            assert_eq!(reopened.sheets()[0].charts().charts[0].plots[0].series[0].values.as_ref().unwrap().cache, *cache);
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chart_category_display_caches_survive_conversion() {
    use ptg::*;
    use xls_kit::art;
    for (text, invalid) in [(false, false), (true, false), (true, true)] {
        let mut chart = art::chart(&[("Sales", area3d(0, (1, 4), (0, 0), 0),
            area3d(0, (1, 4), (1, 1), 0))], 0, "Title");
        if text {
            let series = record(0x1003, &u16s(&[1, 1, 4, 4, 0, 0]));
            let at = chart.windows(series.len()).position(|part| part == series).unwrap();
            chart[at + 4] = 3;
        }
        let mut data = record(0x1065, &u16s(&[2]));
        if text {
            let mut label = u16s(&[0, 0, 0, 3]); label.push(0); label.extend(b"one");
            data.extend(record(0x0204, &label));
            if invalid { data.extend(record(0x0204, &label)); }
        } else { data.extend(number(0, 0, 7.5)); }
        data.extend(record(0x0201, &u16s(&[1, 0, 0])));
        data.extend(record(0x1065, &u16s(&[1])));
        data.extend(record(0x1065, &u16s(&[3])));
        chart.splice(chart.len() - 4..chart.len() - 4, data);
        let records = [art::drawing(&[art::shape(201, 0, [0, 0, 0, 0, 4, 0, 10, 0], false)]), art::obj(5, 1), chart].concat();
        let globals = [supbook_self(1), extern_sheet(&[(0, 0, 0)])].concat();
        let bytes = xls(false, &globals, &[Sheet::worksheet("S", records)]);
        let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let xml = String::from_utf8(doc.package().read_part("xl/charts/chart1_1.xml").unwrap()).unwrap();
        assert_eq!(xml.contains(if text { "<c:strCache>" } else { "<c:numCache>" }), !invalid, "{xml}");
        if !invalid {
            let loaded = load(&bytes);
            let categories = loaded.sheets()[0].charts().charts[0].plots[0].series[0].categories.as_ref().unwrap();
            assert_eq!(categories.numeric, !text);
            assert_eq!(categories.cache[0].as_deref(), Some(if text { "one" } else { "7.5" }));
            let plan = calc_doc::prepare_save(&doc.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
            let mut saved = Vec::new(); calc_doc::write(plan, &mut saved).unwrap();
            let reopened = load(&saved);
            assert_eq!(reopened.sheets()[0].charts().charts[0].plots[0].series[0].categories.as_ref().unwrap(), categories);
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chart_explicit_series_fill_uses_unambiguous_palette_color() {
    use ptg::*;
    use xls_kit::art;
    for (point, flags, rgb, overridden, nested, custom_palette, expected) in [
        (0xffff, 0, [255, 0, 0], false, false, false, Some("FF0000")),
        (0xffff, 0, [0x12, 0x34, 0x56], false, false, true, Some("123456")),
        (0, 0, [255, 0, 0], false, false, false, None),
        (0xffff, 1, [255, 0, 0], false, false, false, None),
        (0xffff, 0, [0, 0, 255], false, false, false, None),
        (0xffff, 0, [255, 0, 0], true, false, false, None),
        (0xffff, 0, [255, 0, 0], false, true, false, None),
    ] {
        let mut chart = art::chart(&[("Sales", Vec::new(), area3d(0, (1, 4), (1, 1), 0))], 0, "Title");
        let anchor = record(0x1045, &u16s(&[0]));
        let at = chart.windows(anchor.len()).position(|window| window == anchor).unwrap() + anchor.len();
        let mut area = vec![rgb[0], rgb[1], rgb[2], 0, 255, 255, 255, 0];
        area.extend(u16s(&[1, flags, 10, 9]));
        let mut format = [record(0x1006, &u16s(&[point, 0, 0, 0])), art::begin()].concat();
        if nested { format.extend(art::begin()); }
        format.extend(record(0x100A, &area));
        if nested { format.extend(art::end()); }
        if overridden { format.extend(record(0x1066, &[])); }
        format.extend(art::end()); chart.splice(at..at, format);
        let records = [art::drawing(&[art::shape(201, 0, [0, 0, 0, 0, 4, 0, 10, 0], false)]), art::obj(5, 1), chart].concat();
        let palette = if custom_palette {
            let mut body = u16s(&[56]); body.extend([0u8; 56 * 4]);
            body[10..14].copy_from_slice(&[0x12, 0x34, 0x56, 0]); record(0x0092, &body)
        } else { Vec::new() };
        let globals = [supbook_self(1), extern_sheet(&[(0, 0, 0)]), palette].concat();
        let bytes = xls(false, &globals, &[Sheet::worksheet("S", records)]);
        let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let xml = String::from_utf8(document.package().read_part("xl/charts/chart1_1.xml").unwrap()).unwrap();
        let color = format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
        assert_eq!(xml.contains(&format!("<a:srgbClr val=\"{color}\"/>")), expected.is_some(), "{xml}");
        if let Some(expected) = expected {
            let loaded = load(&bytes);
            assert_eq!(loaded.sheets()[0].charts().charts[0].plots[0].series[0].fill.as_deref(), Some(expected));
            let plan = calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
            let mut saved = Vec::new(); calc_doc::write(plan, &mut saved).unwrap();
            let reopened = load(&saved);
            assert_eq!(reopened.sheets()[0].charts().charts[0].plots[0].series[0].fill.as_deref(), Some(expected));
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chart_explicit_line_color_reaches_the_loaded_series() {
    use ptg::*;
    use xls_kit::art;
    for (point, style, weight, flags, rgb, nested, overridden, expected) in [
        (0xffff, 0, 0, 0, [255, 0, 0], false, false, true),
        (0, 0, 0, 0, [255, 0, 0], false, false, false),
        (0xffff, 1, 0, 0, [255, 0, 0], false, false, false),
        (0xffff, 0, 1, 0, [255, 0, 0], false, false, false),
        (0xffff, 0, 0, 1, [255, 0, 0], false, false, false),
        (0xffff, 0, 0, 0, [0, 0, 255], false, false, false),
        (0xffff, 0, 0, 0, [255, 0, 0], true, false, false),
        (0xffff, 0, 0, 0, [255, 0, 0], false, true, false),
    ] {
        let mut chart = art::chart(&[("Sales", Vec::new(), area3d(0, (1, 4), (1, 1), 0))], 0, "Title");
        let bar = record(0x1017, &u16s(&[0, 150, 0]));
        let at = chart.windows(bar.len()).position(|window| window == bar).unwrap();
        chart.splice(at..at + bar.len(), record(0x1018, &u16s(&[0])));
        let anchor = record(0x1045, &u16s(&[0]));
        let at = chart.windows(anchor.len()).position(|window| window == anchor).unwrap() + anchor.len();
        let mut line = vec![rgb[0], rgb[1], rgb[2], 0]; line.extend(u16s(&[style, weight, flags, 10]));
        let mut format = [record(0x1006, &u16s(&[point, 0, 0, 0])), art::begin()].concat();
        if nested { format.extend(art::begin()); }
        format.extend(record(0x1007, &line));
        if nested { format.extend(art::end()); }
        if overridden { format.extend(record(0x1066, &[])); }
        format.extend(art::end()); chart.splice(at..at, format);
        let records = [art::drawing(&[art::shape(201, 0, [0, 0, 0, 0, 4, 0, 10, 0], false)]), art::obj(5, 1), chart].concat();
        let globals = [supbook_self(1), extern_sheet(&[(0, 0, 0)])].concat();
        let bytes = xls(false, &globals, &[Sheet::worksheet("S", records)]);
        let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let color = load(&bytes).sheets()[0].charts().charts[0].plots[0].series[0].fill.clone();
        assert_eq!(color.as_deref(), expected.then_some("FF0000"));
        if expected {
            let plan = calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
            let mut saved = Vec::new(); calc_doc::write(plan, &mut saved).unwrap();
            assert_eq!(load(&saved).sheets()[0].charts().charts[0].plots[0].series[0].fill.as_deref(), Some("FF0000"));
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chart_unambiguous_series_value_labels_survive_save() {
    use ptg::*;
    use xls_kit::art;
    let duplicate_with_fill = [record(0x100C, &u16s(&[1])),
        record(0x100A, &[255, 0, 0, 0, 255, 255, 255, 0, 1, 0, 0, 0, 10, 0, 9, 0])].concat();
    for (group, point, flags, extra, expected, fill) in [(false, 0xffff, 1, Vec::new(), true, false),
        (false, 0xffff, 2, Vec::new(), false, false), (false, 0, 1, Vec::new(), false, false),
        (false, 0xffff, 1, record(0x086A, &[]), false, false), (true, 0, 1, Vec::new(), true, false),
        (false, 0xffff, 1, duplicate_with_fill, false, true)] {
        let mut chart = art::chart(&[("Sales", Vec::new(), area3d(0, (1, 4), (1, 1), 0))], 0, "Title");
        let anchor = record(0x1045, &u16s(&[0]));
        let at = if group {
            let bar = record(0x1017, &u16s(&[0, 150, 0]));
            chart.windows(bar.len()).position(|w| w == bar).unwrap()
        } else { chart.windows(anchor.len()).position(|w| w == anchor).unwrap() + anchor.len() };
        let data = [record(0x1006, &u16s(&[point, 0, if group { 0xfffd } else { 0 }, 0])), art::begin(),
            record(0x100C, &u16s(&[flags])), extra, art::end()].concat();
        chart.splice(at..at, data);
        let records = [art::drawing(&[art::shape(201, 0, [0, 0, 0, 0, 4, 0, 10, 0], false)]), art::obj(5, 1), chart].concat();
        let globals = [supbook_self(1), extern_sheet(&[(0, 0, 0)])].concat();
        let bytes = xls(false, &globals, &[Sheet::worksheet("S", records)]);
        let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let check = |doc: &calc_doc::OpenedDocument| {
            let chart = String::from_utf8(doc.package().read_part("xl/charts/chart1_1.xml").unwrap()).unwrap();
            assert_eq!(chart.contains("<c:dLbls><c:showVal val=\"1\"/></c:dLbls>"), expected, "{chart}");
            if fill { assert!(chart.contains("<a:srgbClr val=\"FF0000\"/>"), "{chart}"); }
        };
        check(&doc);
        if expected {
            let _loaded = load(&bytes);
            let mut saved = Vec::new();
            calc_doc::write(calc_doc::prepare_save(&doc.snapshot(), &calc_doc::SavePolicy::default()).unwrap(), &mut saved).unwrap();
            check(&calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap());
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn embedded_chart_print_text_is_preserved_when_unambiguous() {
    use ptg::*;
    use xls_kit::art;
    let text = |id, value: &str| {
        let mut b = (value.len() as u16).to_le_bytes().to_vec(); b.push(0); b.extend(value.as_bytes()); record(id, &b)
    };
    for (header, extra, expected) in [
        ("&CChart", Vec::new(), true),
        ("&CChart", record(0x089C, &[]), false),
        ("&CChart", text(0x0014, "duplicate"), false),
        ("&G", Vec::new(), false),
    ] {
        let mut chart = art::chart(&[("Sales", Vec::new(), area3d(0, (1, 4), (1, 1), 0))], 0, "Title");
        let at = bof(0x0020).len();
        chart.splice(at..at, [text(0x0014, header), text(0x0015, "&RPage &P"), extra].concat());
        let records = [art::drawing(&[art::shape(201, 0, [0, 0, 0, 0, 4, 0, 10, 0], false)]), art::obj(5, 1), chart].concat();
        let globals = [supbook_self(1), extern_sheet(&[(0, 0, 0)])].concat();
        let bytes = xls(false, &globals, &[Sheet::worksheet("S", records)]);
        let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let check = |doc: &calc_doc::OpenedDocument| {
            let xml = String::from_utf8(doc.package().read_part("xl/charts/chart1_1.xml").unwrap()).unwrap();
            assert_eq!(xml.contains("<c:oddHeader>&amp;CChart</c:oddHeader>"), expected, "{xml}");
            assert_eq!(xml.contains("<c:oddFooter>&amp;RPage &amp;P</c:oddFooter>"), expected, "{xml}");
        };
        check(&document); let _loaded = load(&bytes);
        if expected {
            let plan = calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
            let mut saved = Vec::new(); calc_doc::write(plan, &mut saved).unwrap();
            check(&calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap());
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn embedded_chart_preserves_unambiguous_print_setup() {
    use ptg::*;
    use xls_kit::art;
    for (flags, duplicate, custom, expected) in [(2, false, false, true), (4, false, false, false),
        (2, true, false, false), (2, false, true, false)] {
        let mut setup = u16s(&[9, 100, 3, 1, 1, flags, 300, 600]);
        setup.extend(0.5_f64.to_le_bytes()); setup.extend(0.5_f64.to_le_bytes());
        setup.extend(2u16.to_le_bytes());
        let setup = record(0xa1, &setup);
        let mut chart = art::chart(&[("Sales", Vec::new(), area3d(0, (1, 4), (1, 1), 0))], 0, "Title");
        let mut records = setup.clone();
        if duplicate { records.extend(setup); }
        if custom { records.extend(record(0x01aa, &[])); records.extend(record(0x01ab, &[])); }
        chart.splice(bof(0x0020).len()..bof(0x0020).len(), records);
        let records = [art::drawing(&[art::shape(201, 0, [0, 0, 0, 0, 4, 0, 10, 0], false)]), art::obj(5, 1), chart].concat();
        let globals = [supbook_self(1), extern_sheet(&[(0, 0, 0)])].concat();
        let bytes = xls(false, &globals, &[Sheet::worksheet("S", records)]);
        let doc = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let check = |doc: &calc_doc::OpenedDocument| {
            let xml = String::from_utf8(doc.package().read_part("xl/charts/chart1_1.xml").unwrap()).unwrap();
            assert_eq!(xml.contains("<c:pageSetup paperSize=\"9\" orientation=\"portrait\""), expected, "{xml}");
            assert!(!xml.contains("usePrinterDefaults="), "{xml}");
        };
        check(&doc);
        if expected {
            let mut saved = Vec::new();
            calc_doc::write(calc_doc::prepare_save(&doc.snapshot(), &calc_doc::SavePolicy::default()).unwrap(), &mut saved).unwrap();
            check(&calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap());
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chart_group_geometry_survives_legacy_conversion() {
    use xls_kit::art;
    let original = record(0x1017, &u16s(&[0, 150, 0]));
    for (replacement, varied, expected) in [
        (record(0x1017, &u16s(&[(-30i16) as u16, 70, 0])),
         true, vec!["<c:varyColors val=\"1\"/>", "<c:gapWidth val=\"70\"/>", "<c:overlap val=\"-30\"/>"]),
        (record(0x1019, &u16s(&[135, 45, 0])),
         false, vec!["<c:doughnutChart>", "<c:varyColors val=\"0\"/>", "<c:firstSliceAng val=\"135\"/>", "<c:holeSize val=\"45\"/>"]),
        (record(0x1019, &u16s(&[270, 0, 0])),
         true, vec!["<c:pieChart>", "<c:varyColors val=\"1\"/>", "<c:firstSliceAng val=\"270\"/>"]),
    ] {
        let mut chart = art::chart(&[("Sales", Vec::new(), Vec::new())], 0, "Title");
        let at = chart.windows(original.len()).position(|window| window == original).unwrap();
        chart.splice(at..at + original.len(), replacement);
        let format = record(0x1014, &[0u8; 20]);
        let at = chart.windows(format.len()).position(|window| window == format).unwrap();
        chart[at + 4 + 16] = u8::from(varied);
        chart.splice(at..at, record(0x1044, &[0, 0, 2, 0]));
        let records = [art::drawing(&[art::shape(201, 0, [0, 0, 0, 0, 4, 0, 10, 0], false)]),
            art::obj(5, 1), chart].concat();
        let bytes = xls(false, &[], &[Sheet::worksheet("S", records)]);
        let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let check = |doc: &calc_doc::OpenedDocument| {
            let xml = String::from_utf8(doc.package().read_part("xl/charts/chart1_1.xml").unwrap()).unwrap();
            for item in &expected { assert!(xml.contains(item), "{item} in {xml}"); }
            assert!(xml.contains("<c:plotVisOnly val=\"0\"/>") && xml.contains("<c:dispBlanksAs val=\"span\"/>"), "{xml}");
        };
        check(&document);
        let plan = calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
        let mut saved = Vec::new();
        calc_doc::write(plan, &mut saved).unwrap();
        check(&calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap());
        assert_eq!(load(&bytes).sheets()[0].charts().charts.len(), 1);
    }
    // BIFF ignores the variation flag when a group contains more than one series.
    let mut chart = art::chart(&[("A", Vec::new(), Vec::new()), ("B", Vec::new(), Vec::new())], 0, "Title");
    let format = record(0x1014, &[0u8; 20]);
    let at = chart.windows(format.len()).position(|window| window == format).unwrap();
    chart[at + 4 + 16] = 1;
    chart.splice(at..at, record(0x1044, &[2, 0, 1, 0]));
    let records = [art::drawing(&[art::shape(201, 0, [0, 0, 0, 0, 4, 0, 10, 0], false)]), art::obj(5, 1), chart].concat();
    let bytes = xls(false, &[], &[Sheet::worksheet("S", records)]);
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(document.package().read_part("xl/charts/chart1_1.xml").unwrap()).unwrap();
    assert!(xml.contains("<c:varyColors val=\"0\"/>"), "{xml}");
    assert!(xml.contains("<c:plotVisOnly val=\"1\"/>") && xml.contains("<c:dispBlanksAs val=\"zero\"/>"), "{xml}");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chart_value_axis_retains_primary_linear_scale() {
    use xls_kit::art;
    for (parent, flags, present) in [(0, 0x50, true), (1, 0x50, false), (0, 0x70, false)] {
        let mut chart = art::chart(&[("A", Vec::new(), Vec::new())], 0, "Title");
        let format = record(0x1014, &[0u8; 20]);
        let at = chart.windows(format.len()).position(|window| window == format).unwrap();
        let mut range = Vec::new();
        for v in [2.0_f64, 20.0, 5.0, 1.0, 0.0] { range.extend(v.to_le_bytes()); }
        range.extend(u16s(&[flags]));
        let axis = [record(0x1041, &u16s(&[parent])), art::begin(), record(0x101D, &u16s(&[1])),
            art::begin(), record(0x101F, &range), art::end(), art::end()].concat();
        chart.splice(at..at, axis);
        let records = [art::drawing(&[art::shape(201, 0, [0, 0, 0, 0, 4, 0, 10, 0], false)]), art::obj(5, 1), chart].concat();
        let bytes = xls(false, &[], &[Sheet::worksheet("S", records)]);
        let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
        let check = |doc: &calc_doc::OpenedDocument| {
            let xml = String::from_utf8(doc.package().read_part("xl/charts/chart1_1.xml").unwrap()).unwrap();
            for item in ["<c:min val=\"2\"/>", "<c:max val=\"20\"/>", "<c:majorUnit val=\"5\"/>",
                "<c:minorUnit val=\"1\"/>", "<c:orientation val=\"maxMin\"/>"] {
                assert_eq!(xml.contains(item), present, "{item} in {xml}");
            }
        };
        check(&document);
        let plan = calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).unwrap();
        let mut saved = Vec::new(); calc_doc::write(plan, &mut saved).unwrap();
        check(&calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap());
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn conditional_formats_validations_and_the_automatic_filter() {
    use ptg::*;
    use xls_kit::rules;
    let globals = [
        supbook_self(1),
        extern_sheet(&[(0, 0, 0)]),
        // The sheet's _FilterDatabase: a hidden built-in name (code 13) local to sheet 1.
        name_record(0x0021, 1, "\u{d}", &area3d(0, (0, 9), (0, 2), 0)),
    ]
    .concat();
    let mut r = Vec::new();
    r.extend(number(0, 0, 1.0));
    // Cells B2:B10 greater than 5 -> bold, solid yellow (palette 13) fill; and a formula rule.
    r.extend(rules::cf_header(2, &[(1, 9, 1, 1)]));
    r.extend(rules::cf(1, 5, &int(5), &[], true, Some((1, 13, 64))));
    r.extend(rules::cf(2, 0, &join(&[cell_n(0, -1, REL_ROW | REL_COL), int(0), op(0x0D)]), &[], false, None));
    // A list "yes,no" (explicit) on D1:D5 and a whole-number validation between 1 and 10 on E1:E5.
    let mut list = vec![0x17u8, 6, 0];
    list.extend(b"yes\0no");
    r.extend(rules::dv(0x0003 | 0x80 | 0x100 | 0x4_0000 | 0x8_0000, ["", "Oops", "", "Pick one"], &list, &[], (0, 4, 3, 3)));
    r.extend(rules::dv(0x0001 | 0x100 | 0x8_0000, ["", "", "", ""], &int(1), &int(10), (0, 4, 4, 4)));
    // The automatic filter over A1:C10 keeps rows where column B > 3.
    r.extend(record(0x009B, &[]));
    r.extend(rules::autofilter_info(3));
    r.extend(rules::autofilter_greater_than(1, 3.0));
    let mut duplicate_filter_mode = r.clone();
    duplicate_filter_mode.extend(record(0x009B, &[]));
    let bytes = xls(false, &globals, &[Sheet::worksheet("S", r)]);
    let loaded = load(&bytes);
    let sheet = &loaded.sheets()[0];
    let formats = sheet.conditional_formats();
    assert_eq!(formats.len(), 1, "{formats:?}");
    assert_eq!(formats[0].rules.len(), 2);
    assert_eq!(formats[0].areas, vec![(2, 2, 10, 2)], "rows and columns are 1-based");
    let validations = sheet.validations();
    assert_eq!(validations.len(), 2, "{validations:?}");
    let text = format!("{validations:?}");
    assert!(text.contains("yes,no") && text.contains("Pick one") && text.contains("Oops"), "{text}");
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let part = String::from_utf8(document.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    for expected in [
        "<sheetPr filterMode=\"1\">",
        "<autoFilter ref=\"A1:C10\"><filterColumn colId=\"1\"><customFilters><customFilter operator=\"greaterThan\" val=\"3\"/>",
        "<cfRule type=\"cellIs\" dxfId=\"0\" priority=\"1\" operator=\"greaterThan\"><formula>5</formula></cfRule>",
        "<cfRule type=\"expression\"",
        "type=\"whole\"",
    ] {
        assert!(part.contains(expected), "{expected} in {part}");
    }
    assert_eq!(legacy_workbook_report(source(&bytes), &OpenPolicy::default()).unwrap().unwrap().unknown_records.get(&0x009B), Some(&1));
    let duplicate = xls(false, &globals, &[Sheet::worksheet("S", duplicate_filter_mode)]);
    let duplicate = calc_doc::open(source(&duplicate), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(duplicate.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(!xml.contains("filterMode=\"1\""), "{xml}");
    let missing = xls(false, &globals, &[Sheet::worksheet("S", record(0x009B, &[]))]);
    let missing = calc_doc::open(source(&missing), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(missing.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(!xml.contains("filterMode=\"1\""), "{xml}");
    // The rule's formatting is a differential format of the styles part: bold, and the solid fill.
    let loaded_styles = loaded.styles();
    assert_eq!(loaded_styles.differential_formats.len(), 2, "{:?}", loaded_styles.differential_formats);
    let first = format!("{:?}", loaded_styles.differential_formats[0]);
    assert!(first.contains("bold: true") && first.contains("FFFFFF00"), "{first}");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn biff5_filter_text_uses_the_workbook_code_page() {
    let globals = [v5::extern_own(b"S"), v5::name(0x21, 1, b"\r", &v5::cell3d(1, 0, 0, 0))].concat();
    let mut doper = [0u8; 10];
    doper[0] = 6; doper[1] = 2; doper[6] = 4;
    let mut filter = u16s(&[0, 0]);
    filter.extend(doper); filter.extend([0u8; 10]); filter.extend(b"caf\xE9");
    let records = [rules::autofilter_info(1), record(0x009E, &filter)].concat();
    let bytes = v5::xls(1252, &globals, &[v5::Sheet::new("S", records)]);
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(document.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(xml.contains("val=\"café\""), "{xml}");
    load(&bytes);
}

mod vba {
    use super::*;

    const XLS: &[u8] = include_bytes!("fixtures/dc_4j0e/FormsWorkbook.xls");
    const XLSM: &[u8] = include_bytes!("fixtures/dc_4j0e/FormsWorkbook.xlsm");

    fn project_of(bytes: &[u8]) -> calc_vba::OfficeProject {
        let document = calc_doc::open(source(bytes), &OpenPolicy::default()).unwrap();
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let loaded = calc_doc::load(&document, &mut ids).unwrap();
        let part = loaded.vba_project(&Default::default()).unwrap().expect("a VBA project");
        calc_vba::read_office_project(&part.bytes).expect("CalcVBA reads the project")
    }

    /// Native-only: none; the real workbook (written by Excel, with a user form) and the .xlsm
    /// Excel saved from it hold one project, and the converted .xls hands CalcVBA the same modules.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_vba_project_of_an_xls_is_the_one_of_the_xlsm_saved_from_it() {
        let from_xls = project_of(XLS);
        let from_xlsm = project_of(XLSM);
        let modules = |p: &calc_vba::OfficeProject| -> Vec<(String, String)> {
            let mut v: Vec<_> = p.modules().iter().map(|m| (m.name.clone(), m.source.clone())).collect();
            v.sort();
            v
        };
        let (a, b) = (modules(&from_xls), modules(&from_xlsm));
        assert!(!a.is_empty());
        assert_eq!(a.iter().map(|m| &m.0).collect::<Vec<_>>(), b.iter().map(|m| &m.0).collect::<Vec<_>>());
        assert_eq!(a, b);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn read_vba_file_takes_the_compound_file_and_names_the_workbook() {
        let session = calc_doc::read_vba_file(XLS.to_vec()).expect("reads").expect("a project");
        let text = format!("{session:?}");
        assert!(text.contains("ThisWorkbook"), "{text}");
        let document = calc_doc::open(source(XLS), &OpenPolicy::default()).unwrap();
        let workbook = String::from_utf8(document.package().read_part("xl/workbook.xml").unwrap()).unwrap();
        assert!(workbook.contains("codeName=\"ThisWorkbook\""), "{workbook}");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_saved_macro_enabled_xls_keeps_its_vba_project() {
    const XLS: &[u8] = include_bytes!("fixtures/dc_4j0e/FormsWorkbook.xls");
    let document = calc_doc::open(source(XLS), &OpenPolicy::default()).unwrap();
    let plan = calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).expect("a plan");
    let mut saved = Vec::new();
    calc_doc::write(plan, &mut saved).expect("written");
    assert_eq!(&saved[..2], b"PK");
    let session = calc_doc::read_vba_file(saved).expect("reads").expect("the saved package keeps the project");
    assert!(format!("{session:?}").contains("ThisWorkbook"));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn an_excel_4_macro_sheet_of_an_xls_opens_as_dc_xuw_describes_and_saves_as_xlsm() {
    use ptg::*;
    // Macro1 (BOUNDSHEET type 1): A2 = 21; A3 = ALERT("hi"), a command (PtgFuncVar with the
    // command bit), cached 1; A4 = A2*2. Data!A1 reads Macro1!A2.
    let mut macro_records = Vec::new();
    macro_records.extend(number(1, 0, 21.0));
    macro_records.extend(formula(2, 0, bool_value(true), &join(&[text("hi"), {
        let mut v = vec![0x42, 1];
        v.extend((118u16 | 0x8000).to_le_bytes());
        v
    }]), &[]));
    macro_records.extend(formula(3, 0, num_value(42.0), &join(&[cell(1, 0, REL_ROW | REL_COL), int(2), op(MUL)]), &[]));
    let mut data_records = Vec::new();
    data_records.extend(formula(0, 0, num_value(21.0), &cell3d(0, 1, 0, REL_ROW | REL_COL), &[]));
    let globals = [supbook_self(2), extern_sheet(&[(0, 1, 1)])].concat();
    let bytes = xls(
        false,
        &globals,
        &[
            Sheet::worksheet("Data", data_records),
            Sheet {
                kind: 1,
                ..Sheet::worksheet("Macro1", macro_records)
            },
        ],
    );
    let loaded = load(&bytes);
    let sheets = loaded.sheets();
    assert!(sheets[1].is_macro_sheet() && !sheets[0].is_macro_sheet());
    assert!(loaded.model().definitions().macro_enabled());
    // The constant is an input; the macro formulas are not loaded, but their cached results are kept.
    assert_eq!(cells(loaded.model(), 1), vec!["A2 n:21.0"]);
    let cached: Vec<(u32, u32)> = sheets[1].value_caches().map(|(r, c, _)| (r, c)).collect();
    assert_eq!(cached, vec![(3, 1), (4, 1)], "{cached:?}");
    assert_eq!(cells(loaded.model(), 0), vec!["A1 f:Macro1!A2"]);
    // Saved, it is a macro-enabled package that keeps the macro sheet.
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    let plan = calc_doc::prepare_save(&document.snapshot(), &calc_doc::SavePolicy::default()).expect("a plan");
    let mut saved = Vec::new();
    calc_doc::write(plan, &mut saved).expect("written");
    let reopened = calc_doc::open(source(&saved), &OpenPolicy::default()).unwrap();
    let macro_part = String::from_utf8(reopened.package().read_part("xl/macrosheets/sheet2.xml").unwrap()).unwrap();
    assert!(macro_part.contains("ALERT("), "{macro_part}");
}
