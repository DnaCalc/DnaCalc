//! dc-6yw: data validation as calc-host decides it, against what Excel 16.0.20430 decides for the
//! same workbook (tests/fixtures/dc_6yw: data-validation.xlsx, authored in Excel by make-dv.ps1,
//! which wrote each candidate entry and recorded Range.Validation.Value in dv-excel.txt).

use calc_host::value::{CellKey, CoreValue, SheetId};
use calc_host::{
    ErrorStyle, HostOptions, HostSession, IntentError, MemorySource, OpenPolicy, PackageRead,
    SkinIntent, interpret_entry_in,
};
use std::sync::Arc;

const WORKBOOK: &[u8] = include_bytes!("fixtures/dc_6yw/data-validation.xlsx");
const EXCEL: &str = include_str!("fixtures/dc_6yw/dv-excel.txt");

fn session() -> HostSession {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(WORKBOOK.to_vec()));
    // The en-US profile DNA Calc opens with, under which a typed 6/1/2024 is a date.
    let options = HostOptions {
        locale: calc_host::FormatProfile::for_culture("en-US"),
        ..HostOptions::default()
    };
    let mut session = HostSession::open(source, &OpenPolicy::default(), options).expect("it opens");
    assert!(session.dispatch(SkinIntent::Recalculate).accepted());
    session
}

fn at(sheet: SheetId, address: &str) -> CellKey {
    let split = address.find(|c: char| c.is_ascii_digit()).unwrap();
    let (letters, digits) = address.split_at(split);
    let column = letters
        .bytes()
        .fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1));
    CellKey::new(sheet, digits.parse().unwrap(), column).unwrap()
}

/// Every candidate entry Excel judged: whole numbers, decimals, text length, dates, lists written
/// in the rule (case kept) and read from cells and a name (case ignored), blanks with and without
/// allowBlank.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_6yw__entries_allowed_as_excel_allows_them() {
    let session = session();
    let host = session.host();
    let sheet = host.sheets()[0].id();
    let environment = host.entry_environment();
    let mut compared = 0;
    for line in EXCEL.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let [address, candidate, valid] = fields[..] else {
            panic!("a line of three fields: {line}");
        };
        let input = interpret_entry_in(candidate, Some(&environment)).expect("an entry");
        let allowed = host.check_entry(at(sheet, address), input.as_ref()).is_ok();
        assert_eq!(allowed, valid == "1", "{address} {candidate:?}");
        compared += 1;
    }
    assert_eq!(compared, 27);
}

/// The rules as a skin shows them, the lists' items, and the session refusing a stop rule's
/// disallowed entry while taking a warning rule's.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_6yw__lists_prompts_and_refusals() {
    let mut session = session();
    let sheet = session.host().sheets()[0].id();
    let host = session.host();
    let texts = |address: &str| -> Vec<String> {
        host.validation_list(at(sheet, address))
            .expect("a list")
            .iter()
            .map(|value| match value.core() {
                CoreValue::Text(text) => text.to_string_lossy(),
                CoreValue::Number(n) => format!("{n}"),
                other => format!("{other:?}"),
            })
            .collect()
    };
    assert_eq!(texts("A2"), vec!["Red", "Green", "Blue"]);
    assert_eq!(texts("B3"), vec!["North", "South", "East"]);
    assert_eq!(texts("G1"), vec!["10", "20", "30"]);
    let rule = host.validation(at(sheet, "A1")).expect("a rule");
    assert!(rule.dropdown);
    assert_eq!(
        rule.prompt,
        Some(("Colour".to_owned(), "Pick a colour".to_owned()))
    );
    assert_eq!(
        host.validation(at(sheet, "D1")).map(|r| r.style),
        Some(ErrorStyle::Warning)
    );
    assert!(host.validation(at(sheet, "J1")).is_none());

    let receipt = session.dispatch(SkinIntent::EnterCell {
        at: at(sheet, "A1"),
        text: "Purple".to_owned(),
    });
    match receipt.error {
        Some(IntentError::Validation(failure)) => {
            assert_eq!(failure.title, "Not a colour");
            assert_eq!(failure.message, "Choose Red, Green or Blue");
        }
        other => panic!("refused by the rule: {other:?}"),
    }
    assert!(
        session
            .dispatch(SkinIntent::EnterCell {
                at: at(sheet, "A1"),
                text: "Blue".to_owned(),
            })
            .accepted()
    );
    // D1's rule is a warning: the entry is taken.
    assert!(
        session
            .dispatch(SkinIntent::EnterCell {
                at: at(sheet, "D1"),
                text: "-1".to_owned(),
            })
            .accepted()
    );
}
