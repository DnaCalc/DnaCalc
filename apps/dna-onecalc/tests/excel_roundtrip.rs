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
    let mut session = file::open(ONECALC_SAVED)
        .expect("OneCalc reads its own file")
        .0;
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
    let mut original = file::open(ONECALC_SAVED).unwrap().0;
    let mut reopened = file::open(EXCEL_SAVED)
        .expect("Excel's file reopens in OneCalc")
        .0;
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
    assert_eq!(
        file::read_item(&kept).unwrap(),
        file::read_item(&written).unwrap()
    );
    let strip = |bytes: &[u8]| {
        String::from_utf8(bytes.to_vec())
            .unwrap()
            .replace([chr_cr(), chr_lf()], "")
    };
    assert_eq!(strip(&kept), strip(&written));
}

fn chr_cr() -> char {
    char::from(13)
}

fn chr_lf() -> char {
    char::from(10)
}

// The look of a session (dc-0f1k.20): `onecalc-look.xlsx` is what OneCalc saved
// (`cargo run -p dna-onecalc --example session_look`), `excel-saved.xlsx` what an owned Excel session
// saved after opening it, and `excel-reading.json` what Excel reported for the first (cell formats,
// conditional formats and each cell's display format; no repair log). Rerun with
// `tools/excel-session/onecalc-look-job.ps1`.
const LOOK_SAVED: &[u8] = include_bytes!("fixtures/excel-look/onecalc-look.xlsx");
const LOOK_EXCEL_SAVED: &[u8] = include_bytes!("fixtures/excel-look/excel-saved.xlsx");
const LOOK_EXCEL_READING: &str = include_str!("fixtures/excel-look/excel-reading.json");

/// Excel opened the file without repair and showed the formats and rules the panels held.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn excel_shows_the_saved_formatting_and_rules_it_was_given() {
    let reading: serde_json::Value = serde_json::from_str(LOOK_EXCEL_READING).unwrap();
    let row = &reading["rows"][0];
    assert_eq!(row["errors"].as_array().unwrap().len(), 0);
    assert_eq!(
        row["new_error_logs"].as_array().unwrap().len(),
        0,
        "Excel repaired the file"
    );
    assert_eq!(row["range"], "K4:M7");
    let (style, rules) = {
        let session = file::open(LOOK_SAVED).unwrap().0;
        let (style, rules) = session.look();
        (style.clone(), rules.to_vec())
    };
    // Every cell of the result carries the cell format (BGR integers: #336699, #ffee99, #112233).
    let bgr = |hex: &str| {
        let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap();
        i64::from(((v & 255) << 16) | (v & 0xff00) | (v >> 16))
    };
    let cells = row["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 12);
    for cell in cells {
        let font = &cell["font"];
        assert_eq!(font["name"], style.font_family);
        assert_eq!(font["size"], f64::from(style.font_size));
        assert_eq!(
            (&font["bold"], &font["italic"], &font["strike"]),
            (&true.into(), &true.into(), &true.into())
        );
        assert_eq!(font["underline"], 2);
        assert_eq!(font["color"], bgr(&style.text_color));
        assert_eq!(cell["interior"]["color"], bgr(&style.fill));
        assert_eq!(cell["horizontal"], -4108);
        assert_eq!(cell["wrap"], true);
    }
    // Borders: the top, bottom and right edges are drawn in the border colour; the left is not (the
    // inner edges of the area show the neighbour's edge, so only the outer ones are asserted).
    let first = &cells[0];
    assert_eq!(first["borders"]["top"]["color"], bgr(&style.border_color));
    assert_eq!(first["borders"]["left"]["style"], -4142);
    assert_eq!(
        cells[2]["borders"]["right"]["color"],
        bgr(&style.border_color)
    );
    assert_eq!(cells[11]["borders"]["bottom"]["style"], 1);
    // The rules, in order: Excel's rule types (xlCellValue 1, expression 2, colour scale 3, data bar
    // 4, top 10 5, icon set 6, unique/duplicate values 8, text 9, time period 11, above/below
    // average 12, no blanks 13, errors 16) with the formulas stored for K4.
    let conditions = row["conditions"].as_array().unwrap();
    let types: Vec<i64> = conditions
        .iter()
        .map(|c| c["Type"].as_i64().unwrap())
        .collect();
    assert_eq!(types, [1, 9, 13, 16, 11, 2, 5, 12, 8, 8, 3, 3, 4, 6]);
    assert_eq!(conditions.len(), rules.len());
    for condition in conditions {
        assert_eq!(condition["applies_to"], "K4:M7");
    }
    assert_eq!(conditions[0]["Formula1"], "=2");
    assert_eq!(conditions[0]["Formula2"], "=L4+5");
    assert_eq!(conditions[0]["StopIfTrue"], true);
    assert_eq!(conditions[0]["NumberFormat"], "0.00\"x\"");
    assert_eq!(conditions[5]["Formula1"], "=AND(K4>2,L4<$C$3)");
    assert_eq!(
        (conditions[6]["Rank"].as_i64(), &conditions[6]["Percent"]),
        (Some(25), &true.into())
    );
    assert_eq!(conditions[7]["AboveBelow"], 3);
    assert_eq!(conditions[12]["bar"]["color"], 255);
    assert_eq!(conditions[13]["icons"]["id"], 13);
    assert_eq!(conditions[13]["ReverseOrder"], true);
    // What the rules give the first cell, from Excel's display format: the first rule's colours
    // (font #9c0006, fill #ffc7ce) and its bold.
    assert_eq!(first["display"]["font_color"], bgr("#9c0006"));
    assert_eq!(first["display"]["interior_color"], bgr("#ffc7ce"));
}

/// The file Excel saved reopens in OneCalc with the same panels.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn the_file_excel_saved_reopens_with_the_same_formatting_and_rules() {
    let original = file::open(LOOK_SAVED).unwrap().0;
    let reopened = file::open(LOOK_EXCEL_SAVED)
        .expect("Excel's file reopens in OneCalc")
        .0;
    assert_eq!(reopened.look().0, original.look().0);
    assert_eq!(reopened.look().1, original.look().1);
    assert_eq!(reopened.formula(), original.formula());
    assert_eq!(reopened.formula_cell(), original.formula_cell());
    assert_eq!(original.look().1.len(), 14);
    assert!(original.look().0.bold && original.look().0.border_right);
}
