//! The Excel round trip of the OneCalc file (dc-0f1k.5), from files kept beside this test:
//! `onecalc-saved.xlsx` is what OneCalc saved (`cargo run -p dna-onecalc --example session_file`),
//! `excel-saved.xlsx` is what an owned Excel session saved after opening it, and
//! `excel-reading.json` is what Excel reported for the first (formula, value, constants, and that
//! it holds no array formula). Rerun with `tools/excel-session/onecalc-roundtrip-job.ps1`.

use dna_onecalc::file;
use dna_onecalc::shown::{authored_of, evaluate};

const ONECALC_SAVED: &[u8] = include_bytes!("fixtures/excel-roundtrip/onecalc-saved.xlsx");
const EXCEL_SAVED: &[u8] = include_bytes!("fixtures/excel-roundtrip/excel-saved.xlsx");
const EXCEL_READING: &str = include_str!("fixtures/excel-roundtrip/excel-reading.json");

fn customs(package: &[u8]) -> Vec<u8> {
    let loaded = calc_doc::read_session(package.to_vec(), file::NAMESPACE).expect("a session file");
    loaded.custom.expect("its custom XML part").content
}

/// Excel's own reading of what OneCalc saved: the same formula text and value, and the constants.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn excel_read_the_saved_session_as_the_same_formula_and_constants() {
    let reading: serde_json::Value = serde_json::from_str(EXCEL_READING).unwrap();
    let mut session = file::open(ONECALC_SAVED).expect("OneCalc reads its own file");
    assert_eq!(reading["formula2"], session.formula().to_owned());
    assert_eq!(reading["sheets"], 1);
    assert_eq!(reading["has_array"], false);
    let authored = authored_of(&session);
    assert_eq!(reading["cells"]["A1"], 1.5);
    assert_eq!(authored.grid[0], "1.5");
    assert_eq!(reading["cells"]["B1"], "é & <text>");
    assert_eq!(authored.grid[1], "é & <text>");
    assert_eq!(reading["cells"]["B2"], true);
    assert_eq!(authored.grid[9], "TRUE");
    assert_eq!(reading["cells"]["B3"], -2146826246); // #N/A
    assert_eq!(authored.grid[17], "#N/A");
    // OneCalc's calculation agrees with the value Excel calculated.
    let shown = evaluate(&mut session, &authored);
    assert_eq!(shown.result, format!("number {}", reading["value2"]));
}

/// The file Excel saved reopens in OneCalc with the same session, and the custom XML part is kept:
/// Excel drops only the line break after the XML declaration, so the part is compared as the
/// options and add-in list it holds, and by its bytes without that break.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn the_file_excel_saved_reopens_with_the_same_session_and_the_custom_part() {
    let mut original = file::open(ONECALC_SAVED).unwrap();
    let mut reopened = file::open(EXCEL_SAVED).expect("Excel's file reopens in OneCalc");
    assert_eq!(reopened.formula(), original.formula());
    assert_eq!(reopened.formula_cell(), original.formula_cell());
    assert_eq!(reopened.options(), original.options());
    assert_eq!(reopened.addins(), original.addins());
    assert_eq!(authored_of(&reopened), authored_of(&original));
    let (state_a, state_b) = (authored_of(&reopened), authored_of(&original));
    assert_eq!(
        evaluate(&mut reopened, &state_a).result,
        evaluate(&mut original, &state_b).result
    );
    let (kept, written) = (customs(EXCEL_SAVED), customs(ONECALC_SAVED));
    assert_eq!(file::read_item(&kept).unwrap(), file::read_item(&written).unwrap());
    let strip = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).unwrap().replace([chr_cr(), chr_lf()], "");
    assert_eq!(strip(&kept), strip(&written));
}

fn chr_cr() -> char {
    char::from(13)
}

fn chr_lf() -> char {
    char::from(10)
}
