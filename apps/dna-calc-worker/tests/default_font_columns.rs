//! dc-piw0.7: the display draws columns from the worker's geometry as wide as calc-host places
//! shapes (Q9, Excel build 20527).
use dna_calc_worker::server::Server;
use dna_calc_worker::wire::Sheet;
use dna_calc_worker::workbook::Workbook;
#[allow(dead_code)]
#[path = "../../../crates/calc-doc/tests/cdoc_m1_1_support/mod.rs"]
mod zip;

/// dc-piw0.7 (Q9, Excel build 20527): the display draws columns as wide as Excel does, from the
/// maximum digit width of the workbook's default font that the worker puts on the wire.

/// The blank workbook with Excel's current default font (Aptos Narrow 11).
fn aptos_narrow_package() -> Vec<u8> {
    let blank = calc_host::blank_package();
    let archive = zip::parse_zip(&blank);
    let entries = archive
        .entries
        .iter()
        .map(|entry| {
            let payload = entry.payload(&blank);
            if entry.name == "xl/styles.xml" {
                let bytes = if entry.method == 0 {
                    payload.to_vec()
                } else {
                    miniz_oxide::inflate::decompress_to_vec(payload).unwrap()
                };
                let xml = String::from_utf8(bytes)
                    .unwrap()
                    .replace("<name val=\"Calibri\"/>", "<name val=\"Aptos Narrow\"/>");
                assert!(xml.contains("Aptos Narrow"));
                zip::RawEntry::stored(&entry.name, xml.as_bytes())
            } else if entry.method == 0 {
                zip::RawEntry::stored(&entry.name, payload)
            } else {
                zip::RawEntry::deflated(&entry.name, payload, entry.size, entry.crc32)
            }
        })
        .collect::<Vec<_>>();
    zip::build_zip(&entries)
}

/// The columns the display draws from the wire's geometry, against the columns calc-host puts
/// shapes in (points; 96 DPI pixels are 4/3 of a point).
fn columns_match_the_host(package: Vec<u8>, first_column: f64) {
    let mut server = Server::new();
    *server.book_mut() = Workbook::open("a.xlsx", package).expect("it opens");
    let sheet = server.book().host().sheets()[0].id();
    let geometry = server.geometry(Sheet::of(sheet));
    let metrics = dna_calc_model::geometry::sheet_metrics(Some(&geometry));
    assert_eq!(metrics.width(1), first_column);
    let host = server.book().host();
    for column in 1..=12u32 {
        let left = host.column_left(sheet, column - 1);
        assert_eq!(metrics.col_start(column), left * 96.0 / 72.0, "column {column}");
    }
    // A shape at Left 200 pt lands in the column calc-host puts it in.
    let by_host = (1..=12u32)
        .rev()
        .find(|c| host.column_left(sheet, c - 1) <= 200.0)
        .unwrap();
    let x = 200.0 * 96.0 / 72.0;
    let by_display = (1..=12u32).rev().find(|c| metrics.col_start(*c) <= x).unwrap();
    assert_eq!(by_display, by_host);
    // Rows follow the font too.
    assert_eq!(metrics.height(1), (host.row_top(sheet, 1) * 96.0 / 72.0).floor());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_7__aptos_narrow_columns_draw_at_72_pixels() {
    columns_match_the_host(aptos_narrow_package(), 72.0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_7__calibri_columns_draw_at_64_pixels() {
    columns_match_the_host(calc_host::blank_package(), 64.0);
}

