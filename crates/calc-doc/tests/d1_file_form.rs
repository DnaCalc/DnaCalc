//! dc-ndi (D1): the file form calc_doc writes for authored formulas, compared with what Excel
//! 16.0 build 20430 stored for the same entries (the measurements under tests/fixtures/d1/).

mod cdoc_m1_1_support;

use calc_doc::file_form;
use calc_doc::model::value::ExcelText;
use calc_doc::model::{DefinedName, FormulaText, NameScope};
use cdoc_m1_1_support::d1::fixture;
use serde_json::Value;

fn measurement(name: &str) -> Value {
    let bytes = fixture(name);
    let text = std::str::from_utf8(&bytes).expect("a measurement");
    serde_json::from_str(text.trim_start_matches('\u{feff}')).expect("JSON")
}

fn text(formula: &str) -> FormulaText {
    FormulaText::new(ExcelText::from_utf16_code_units(
        formula.encode_utf16().collect(),
    ))
    .expect("a formula")
}

fn name(name: &str, formula: &str) -> DefinedName {
    DefinedName {
        name: name.to_owned(),
        scope: NameScope::Workbook,
        formula: formula.to_owned(),
        hidden: false,
        comment: None,
        function: false,
        vb_procedure: false,
        xlm: false,
    }
}

fn form(formula: &str, names: &[DefinedName]) -> (String, bool) {
    let form =
        file_form(&text(formula), names).unwrap_or_else(|error| panic!("{formula}: {error:?}"));
    (String::from_utf16(&form.text).unwrap(), form.dynamic)
}

fn flags(value: &Value) -> Vec<Option<bool>> {
    match value {
        Value::Array(items) => items.iter().map(Value::as_bool).collect(),
        Value::Bool(flag) => vec![Some(*flag)],
        _ => Vec::new(),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d1_file_form__table_matches_the_measurements() {
    let prefixes = measurement("excel-prefixes.json");
    let positions = measurement("excel-array-positions.json");
    let mut checked = 0;
    for row in prefixes["rows"].as_array().unwrap() {
        let (Some(function), Some(entered), Some(stored)) =
            (row[0].as_str(), row[1].as_str(), row[2].as_str())
        else {
            continue;
        };
        let (written, _) = form(&entered[1..], &[]);
        assert_eq!(written, stored, "{function}: the prefix Excel stored");
        checked += 1;
    }
    assert!(checked > 500, "{checked} prefixes");
    let mut entries = 0;
    for row in positions["rows"].as_array().unwrap() {
        let function = row[0].as_str().unwrap();
        let first = row[1].as_u64().unwrap() as usize;
        for (offset, base) in flags(&row[2]).into_iter().enumerate() {
            let Some(base) = base else { continue };
            let count = first + offset;
            let formula = format!("{function}({})", vec!["Z1"; count].join(","));
            assert_eq!(form(&formula, &[]).1, base, "{formula}");
            entries += 1;
        }
        let most = flags(&row[3]).len();
        for (position, flag) in flags(&row[3]).into_iter().enumerate() {
            let Some(flag) = flag else { continue };
            let mut arguments = vec!["Z1"; most];
            arguments[position] = "Z1:Z3";
            let formula = format!("{function}({})", arguments.join(","));
            assert_eq!(form(&formula, &[]).1, flag, "{formula}");
            entries += 1;
        }
    }
    assert!(entries > 1500, "{entries} entries");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d1_file_form__entered_formulas_match_excel() {
    let measured = measurement("excel-file-form.json");
    let names = [
        name("RangeName", "Sheet1!$A$1:$A$3"),
        name("ConstName", "5"),
    ];
    let mut checked = 0;
    for row in measured["rows"].as_array().unwrap() {
        let entered = row[0].as_str().unwrap();
        let Some(stored) = row[1].as_str() else {
            panic!("{entered}: Excel refused the entry");
        };
        let dynamic = row[2].as_str() == Some("array") && row[4].as_str().is_some();
        assert_eq!(
            form(&entered[1..], &names),
            (stored.to_owned(), dynamic),
            "{entered}"
        );
        checked += 1;
    }
    assert_eq!(checked, 70);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d1_file_form__prefixed_text_is_left_as_written() {
    for written in [
        "_xlfn.XLOOKUP(1,A1:A3,B1:B3)",
        "_xlfn._xlws.FILTER(A1:A3,A1:A3>1)",
        "_xlfn.LET(_xlpm.x,1,_xlpm.x+1)",
        "SUM(_xlfn.ANCHORARRAY(E1))",
    ] {
        assert_eq!(form(written, &[]).0, written);
    }
}

/// The save's prefix table is measured over every function Excel 16 knows; calc-func's
/// file-prefix metadata covers the functions it catalogues. Every catalogued function's prefix
/// agrees with what Excel stored for it in the measurement.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d1_file_form__catalogue_prefixes_agree() {
    let prefixes = measurement("excel-prefixes.json");
    let stored: std::collections::BTreeMap<String, String> = prefixes["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| Some((row[0].as_str()?.to_owned(), row[2].as_str()?.to_owned())))
        .collect();
    let mut agreed = 0;
    for target in calc_func::catalogue().surfaces() {
        let Some(prefix) = target.file_prefix() else {
            continue;
        };
        let name = target.spelling();
        let Some(text) = stored.get(name) else {
            continue;
        };
        let measured = text
            .strip_suffix(&text[text.find('(').unwrap_or(text.len())..])
            .unwrap_or(text)
            .strip_suffix(name)
            .unwrap_or_else(|| panic!("{name}: {text}"));
        assert_eq!(measured, prefix.text(), "{name}");
        agreed += 1;
    }
    assert!(agreed > 100, "{agreed} catalogued functions");
}
