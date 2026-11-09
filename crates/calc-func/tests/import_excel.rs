//! IMPORTCSV and IMPORTTEXT against Excel 16.0.20527's answers (dc-z8c1): every case of
//! `fixtures/import/excel-answers.json` is one call whose arguments and answer were taken from an
//! owned Excel session over the files in `fixtures/import/files` (the path `C:\\data\\` stands
//! for the folder they were in). A host fake serves the files from memory, so no test touches the
//! file system.

use calc_func::value::{CalcArray, CalcValue, CoreRef, ExcelText, WorksheetErrorCode};
use calc_func::{ImportFunction, ImportRoute, UserFunction};
use std::rc::Rc;

const FILES: &[(&str, &[u8])] = &[
    (
        "basic.csv",
        include_bytes!("fixtures/import/files/basic.csv"),
    ),
    // Windows resolves this spelling to the same file; keep the distinct path in the test.
    (
        "basic.csv ",
        include_bytes!("fixtures/import/files/basic.csv"),
    ),
    ("big.csv", include_bytes!("fixtures/import/files/big.csv")),
    (
        "blank.csv",
        include_bytes!("fixtures/import/files/blank.csv"),
    ),
    ("bom.csv", include_bytes!("fixtures/import/files/bom.csv")),
    (
        "bools.csv",
        include_bytes!("fixtures/import/files/bools.csv"),
    ),
    ("cr.csv", include_bytes!("fixtures/import/files/cr.csv")),
    ("crlf.csv", include_bytes!("fixtures/import/files/crlf.csv")),
    (
        "crlfq.csv",
        include_bytes!("fixtures/import/files/crlfq.csv"),
    ),
    (
        "dates.csv",
        include_bytes!("fixtures/import/files/dates.csv"),
    ),
    ("de.csv", include_bytes!("fixtures/import/files/de.csv")),
    ("de2.csv", include_bytes!("fixtures/import/files/de2.csv")),
    (
        "de2_ok.csv",
        include_bytes!("fixtures/import/files/de2_ok.csv"),
    ),
    (
        "dec_ok.csv",
        include_bytes!("fixtures/import/files/dec_ok.csv"),
    ),
    (
        "empty.csv",
        include_bytes!("fixtures/import/files/empty.csv"),
    ),
    (
        "enc1252.txt",
        include_bytes!("fixtures/import/files/enc1252.txt"),
    ),
    (
        "fixed.txt",
        include_bytes!("fixtures/import/files/fixed.txt"),
    ),
    (
        "fixed2.txt",
        include_bytes!("fixtures/import/files/fixed2.txt"),
    ),
    ("fr.csv", include_bytes!("fixtures/import/files/fr.csv")),
    (
        "latin1.csv",
        include_bytes!("fixtures/import/files/latin1.csv"),
    ),
    (
        "lonelyquote.csv",
        include_bytes!("fixtures/import/files/lonelyquote.csv"),
    ),
    (
        "mixed_nl.csv",
        include_bytes!("fixtures/import/files/mixed_nl.csv"),
    ),
    ("nonl.csv", include_bytes!("fixtures/import/files/nonl.csv")),
    (
        "num_ok.csv",
        include_bytes!("fixtures/import/files/num_ok.csv"),
    ),
    (
        "onlyhdr.csv",
        include_bytes!("fixtures/import/files/onlyhdr.csv"),
    ),
    (
        "pipe2.txt",
        include_bytes!("fixtures/import/files/pipe2.txt"),
    ),
    ("qq.csv", include_bytes!("fixtures/import/files/qq.csv")),
    (
        "quoted_hdr.csv",
        include_bytes!("fixtures/import/files/quoted_hdr.csv"),
    ),
    (
        "quotes.csv",
        include_bytes!("fixtures/import/files/quotes.csv"),
    ),
    (
        "ragged.csv",
        include_bytes!("fixtures/import/files/ragged.csv"),
    ),
    (
        "ragged2.csv",
        include_bytes!("fixtures/import/files/ragged2.csv"),
    ),
    (
        "ragged3.csv",
        include_bytes!("fixtures/import/files/ragged3.csv"),
    ),
    ("semi.txt", include_bytes!("fixtures/import/files/semi.txt")),
    (
        "spaces.csv",
        include_bytes!("fixtures/import/files/spaces.csv"),
    ),
    (
        "spacesq.csv",
        include_bytes!("fixtures/import/files/spacesq.csv"),
    ),
    ("tab.txt", include_bytes!("fixtures/import/files/tab.txt")),
    (
        "tabtest.txt",
        include_bytes!("fixtures/import/files/tabtest.txt"),
    ),
    (
        "trail.csv",
        include_bytes!("fixtures/import/files/trail.csv"),
    ),
    (
        "types.csv",
        include_bytes!("fixtures/import/files/types.csv"),
    ),
    (
        "utf16.csv",
        include_bytes!("fixtures/import/files/utf16.csv"),
    ),
    (
        "utf16be.csv",
        include_bytes!("fixtures/import/files/utf16be.csv"),
    ),
    (
        "utf8acc.csv",
        include_bytes!("fixtures/import/files/utf8acc.csv"),
    ),
    ("wide.csv", include_bytes!("fixtures/import/files/wide.csv")),
];

#[derive(Debug)]
struct Files;

impl ImportRoute for Files {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        let name = path.strip_prefix("C:\\data\\")?;
        FILES
            .iter()
            .find(|(file, _)| *file == name)
            .map(|(_, bytes)| bytes.to_vec())
    }
}

const ANSWERS: &str = include_str!("fixtures/import/excel-answers.json");

fn error(spelling: &str) -> WorksheetErrorCode {
    [
        WorksheetErrorCode::Null,
        WorksheetErrorCode::Div0,
        WorksheetErrorCode::Value,
        WorksheetErrorCode::Ref,
        WorksheetErrorCode::Name,
        WorksheetErrorCode::Num,
        WorksheetErrorCode::NA,
        WorksheetErrorCode::Spill,
        WorksheetErrorCode::Calc,
    ]
    .into_iter()
    .find(|code| code.excel_spelling() == spelling)
    .expect("an error spelling")
}

fn argument(json: &serde_json::Value) -> CalcValue {
    if json.get("missing").is_some() {
        return CalcValue::missing();
    }
    if let Some(text) = json.get("s") {
        let units = text.as_str().unwrap().encode_utf16().collect();
        return CalcValue::text(ExcelText::from_utf16_code_units(units));
    }
    if let Some(number) = json.get("n") {
        return CalcValue::number(number.as_f64().unwrap());
    }
    if let Some(flag) = json.get("b") {
        return CalcValue::logical(flag.as_bool().unwrap());
    }
    if let Some(code) = json.get("e") {
        return CalcValue::error(error(code.as_str().unwrap()));
    }
    let rows = json["a"].as_array().expect("an argument form");
    let rows = rows
        .iter()
        .map(|row| row.as_array().unwrap().iter().map(argument).collect())
        .collect();
    CalcValue::array(CalcArray::from_rows(rows).expect("a rectangular array"))
}

/// A value as the answers write it.
fn shown(value: &CalcValue) -> String {
    match value.core_ref() {
        CoreRef::Number(number) => format!("n{:016x}", number.to_bits()),
        CoreRef::Text(text) => format!("s{:?}", text.to_string_lossy()),
        CoreRef::Logical(flag) => format!("b{flag}"),
        CoreRef::Error(code) => format!("e{}", code.excel_spelling()),
        other => format!("{other:?}"),
    }
}

fn expected(json: &serde_json::Value) -> String {
    if let Some(number) = json.get("n") {
        return format!("n{:016x}", number.as_f64().unwrap().to_bits());
    }
    if let Some(text) = json.get("s") {
        return format!("s{:?}", text.as_str().unwrap());
    }
    if let Some(flag) = json.get("b") {
        return format!("b{}", flag.as_bool().unwrap());
    }
    if let Some(code) = json.get("e") {
        return format!("e{}", code.as_str().unwrap());
    }
    "empty".to_owned()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn imports_match_excels_answers() {
    let answers: serde_json::Value = serde_json::from_str(ANSWERS).expect("the answers");
    let route: Rc<dyn ImportRoute> = Rc::new(Files);
    let cases = answers["cases"].as_array().expect("cases");
    let mut differ = Vec::new();
    for case in cases {
        let name = case["function"].as_str().unwrap();
        let function = ImportFunction::new(name, route.clone()).expect("an import function");
        let arguments: Vec<CalcValue> = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(argument)
            .collect();
        let want: Vec<Vec<String>> = case["excel"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row.as_array().unwrap().iter().map(expected).collect())
            .collect();
        let got: Result<Vec<Vec<String>>, String> =
            function
                .call(&arguments)
                .map(|value| match value.as_array() {
                    Some(array) => {
                        let shape = array.shape();
                        (0..shape.rows)
                            .map(|r| {
                                (0..shape.cols)
                                    .map(|c| shown(array.get(r, c).unwrap()))
                                    .collect()
                            })
                            .collect()
                    }
                    None => vec![vec![shown(&value)]],
                });
        if got.as_ref() != Ok(&want) {
            differ.push(format!(
                "{} {}: Excel {want:?}, here {got:?}",
                case["id"], case["formula"]
            ));
        }
    }
    assert!(
        differ.is_empty(),
        "{} of {} differ:
{}",
        differ.len(),
        cases.len(),
        differ.join(
            "
"
        )
    );
}
