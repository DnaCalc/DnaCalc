//! dc-jnt: the typed-input rows of the jnt oracle workbook (`fixtures/jnt/sheet-jnt_*.txt`) reach the workbook
//! exactly as their section files say. A line of such a file is a formula and up to six
//! tab-separated inputs; `make-catalogue-fixtures.ps1` writes each input as a constant in column A,
//! B, C, E, F or G of the line's row (a number with 17 significant digits) and the formula, with `{A}`, `{B}`, ...
//! replaced by that row's cells, in column D. This test reads the committed
//! `jnt/oracle.xlsx` back and checks every row of every such sheet: the formula text, and
//! each input cell's kind and value (a number's binary64 bits, which decimal text with 17
//! significant digits keeps), and that a `-` input leaves its cell empty. Live Excel keeping those
//! constants exactly is measured separately: Excel 16.0.20527 read back the inputs of all 3,108 rows
//! of `jnt_w111_elementary` bit for bit (dc-jnt.5).

use std::collections::BTreeMap;

/// The input columns, in the order of a line's inputs (column D holds the formula).
const COLUMNS: [&str; 6] = ["A", "B", "C", "E", "F", "G"];

const WORKBOOK: &[u8] = include_bytes!("fixtures/jnt/oracle.xlsx");

/// The section captures (`captures/jnt/sections`), whose environment `jnt_import.py environment` sets.
const CAPTURES: &[(&str, &str)] = &[
    ("jnt_dates_random", include_str!("captures/jnt/sections/jnt_dates_random.capture.json")),
    ("jnt_operators", include_str!("captures/jnt/sections/jnt_operators.capture.json")),
    ("jnt_parity_complex", include_str!("captures/jnt/sections/jnt_parity_complex.capture.json")),
    ("jnt_logical_info", include_str!("captures/jnt/sections/jnt_logical_info.capture.json")),
    ("jnt_lookup", include_str!("captures/jnt/sections/jnt_lookup.capture.json")),
    ("jnt_text_formats", include_str!("captures/jnt/sections/jnt_text_formats.capture.json")),
    ("jnt_w111_elementary", include_str!("captures/jnt/sections/jnt_w111_elementary.capture.json")),
    ("jnt_w111_rounding", include_str!("captures/jnt/sections/jnt_w111_rounding.capture.json")),
    ("jnt_w111_text_dates", include_str!("captures/jnt/sections/jnt_w111_text_dates.capture.json")),
];

/// The sheets whose section files carry typed inputs, each with its section file.
const SECTIONS: &[(&str, &str)] = &[
    ("jnt_w111_elementary", include_str!("fixtures/jnt/sheet-jnt_w111_elementary.txt")),
    ("jnt_w111_text_dates", include_str!("fixtures/jnt/sheet-jnt_w111_text_dates.txt")),
    ("jnt_dates_random", include_str!("fixtures/jnt/sheet-jnt_dates_random.txt")),
    ("jnt_operators", include_str!("fixtures/jnt/sheet-jnt_operators.txt")),
    ("jnt_parity_complex", include_str!("fixtures/jnt/sheet-jnt_parity_complex.txt")),
    ("jnt_logical_info", include_str!("fixtures/jnt/sheet-jnt_logical_info.txt")),
    ("jnt_lookup", include_str!("fixtures/jnt/sheet-jnt_lookup.txt")),
    ("jnt_text_formats", include_str!("fixtures/jnt/sheet-jnt_text_formats.txt")),
];

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&")
}

/// The cells of a worksheet part: reference to (formula text, constant number text, constant text).
#[derive(Default, Debug)]
struct Cell {
    formula: Option<String>,
    number: Option<String>,
    text: Option<String>,
    logical: Option<String>,
    error: Option<String>,
}

fn cells_of(sheet: &str) -> BTreeMap<String, Cell> {
    let mut cells = BTreeMap::new();
    let mut rest = sheet;
    while let Some(at) = rest.find("<c r=\"") {
        rest = &rest[at + 6..];
        let reference = &rest[..rest.find('"').unwrap()];
        let end = rest.find("</c>").unwrap();
        let body = &rest[..end];
        let open = &body[..body.find('>').unwrap()];
        let mut cell = Cell::default();
        if let Some(f) = body.find("<f>") {
            cell.formula = Some(unescape(&body[f + 3..body.find("</f>").unwrap()]));
        } else if open.contains("t=\"inlineStr\"") {
            let t = body.find("<t").unwrap();
            let start = t + body[t..].find('>').unwrap() + 1;
            cell.text = Some(unescape(&body[start..body.find("</t>").unwrap()]));
        } else if let Some(v) = body.find("<v>") {
            let value = body[v + 3..body.find("</v>").unwrap()].to_owned();
            if open.contains("t=\"b\"") {
                cell.logical = Some(value);
            } else if open.contains("t=\"e\"") {
                cell.error = Some(value);
            } else {
                cell.number = Some(value);
            }
        }
        cells.insert(reference.to_owned(), cell);
        rest = &rest[end..];
    }
    cells
}

fn sheet_part(name: &str) -> String {
    let parts = calc_verdict::stored_package_parts(WORKBOOK).expect("a stored package");
    let workbook = std::str::from_utf8(&parts["xl/workbook.xml"]).unwrap();
    let marker = format!("<sheet name=\"{name}\" sheetId=\"");
    let at = workbook.find(&marker).unwrap_or_else(|| panic!("the workbook has no sheet {name}"));
    let id: &str = &workbook[at + marker.len()..];
    let id = &id[..id.find('"').unwrap()];
    String::from_utf8(parts[&format!("xl/worksheets/sheet{id}.xml")].clone()).unwrap()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn typed_input_rows_reach_the_workbook_exactly() {
    for (name, section) in SECTIONS {
        let cells = cells_of(&sheet_part(name));
        let lines: Vec<&str> = section.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
        assert!(!lines.is_empty(), "{name} has no rows");
        for (index, line) in lines.iter().enumerate() {
            let row = index + 1;
            let fields: Vec<&str> = line.split('\t').collect();
            let mut formula = fields[0].to_owned();
            for column in COLUMNS {
                formula = formula.replace(&format!("{{{column}}}"), &format!("{column}{row}"));
            }
            let written = cells.get(&format!("D{row}")).and_then(|c| c.formula.clone());
            assert_eq!(written.as_deref(), Some(formula.as_str()), "{name} row {row}");
            for (k, column) in COLUMNS.iter().enumerate() {
                let cell = cells.get(&format!("{column}{row}"));
                let Some(token) = fields.get(k + 1) else {
                    assert!(cell.is_none(), "{name} {column}{row} should be empty");
                    continue;
                };
                let here = format!("{name} {column}{row} {token:?}");
                match (token.split_at(token.len().min(2)), cell) {
                    (("-", ""), cell) => assert!(cell.is_none(), "{here}"),
                    (("n:", hex), Some(cell)) => {
                        let want = u64::from_str_radix(hex, 16).unwrap();
                        let text = cell.number.as_deref().unwrap_or_else(|| panic!("{here}: no number"));
                        let got: f64 = text.parse().unwrap();
                        // -0 is not written (the importer leaves it out): a zero is +0.
                        assert_eq!(got.to_bits(), want, "{here}: <v>{text}</v>");
                    }
                    (("s:", text), Some(cell)) => assert_eq!(cell.text.as_deref(), Some(text), "{here}"),
                    (("b:", flag), Some(cell)) => {
                        assert_eq!(cell.logical.as_deref(), Some(if flag == "TRUE" { "1" } else { "0" }), "{here}")
                    }
                    (("e:", literal), Some(cell)) => assert_eq!(cell.error.as_deref(), Some(literal), "{here}"),
                    (("f:", formula), Some(cell)) => assert_eq!(cell.formula.as_deref(), Some(formula), "{here}"),
                    _ => panic!("{here}: the cell does not match the input"),
                }
            }
        }
    }
}

/// Every jnt section capture records the Excel environment with its user culture and separators: the
/// corpus harness builds the locale profile from them, and without a profile number text such as
/// "50%" or "(2)" given to a number argument is refused as not modelled (189 rows of the workbook).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn section_captures_record_the_locale_of_their_excel() {
    for (name, capture) in CAPTURES {
        assert!(!capture.contains("\"environment\": null"), "{name} has no environment");
        for setting in ["user_culture\": \"en-ZA", "decimal_separator\": \".", "thousands_separator\": \" ", "calculation_clock"] {
            assert!(capture.contains(setting), "{name} lacks the setting {setting}");
        }
    }
}
