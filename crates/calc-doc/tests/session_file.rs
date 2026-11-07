//! The constrained session workbook: files outside its constraints are refused with the reason,
//! and a file the codec wrote reads back (the round trips are in `src/session.rs`).

use calc_doc::{SessionRefusal, read_session};

const MANY_SHEETS: &[u8] = include_bytes!("fixtures/cdoc_m1_2/d1.saved.xlsx");
const WITH_VBA: &[u8] = include_bytes!("fixtures/vba_udf_names/case-only.xlsm");
const PIVOT: &[u8] = include_bytes!("fixtures/cdoc_m1_1/pivot-saved.xlsx");

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_workbook_with_several_sheets_is_refused_with_their_number() {
    match read_session(MANY_SHEETS.to_vec(), "urn:example:none") {
        Err(SessionRefusal::Sheets { count }) => assert!(count > 1, "{count}"),
        other => panic!("{other:?}"),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_workbook_with_a_vba_project_is_refused_as_one() {
    assert_eq!(
        read_session(WITH_VBA.to_vec(), "urn:example:none"),
        Err(SessionRefusal::VbaProject)
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_workbook_with_pivot_tables_is_refused_with_a_reason_naming_content_or_sheets() {
    // Whichever constraint the workbook breaks first, the refusal says which, never a value.
    let refusal = read_session(PIVOT.to_vec(), "urn:example:none").expect_err("outside a session");
    assert!(
        matches!(
            refusal,
            SessionRefusal::Sheets { .. }
                | SessionRefusal::Content { .. }
                | SessionRefusal::Formulas { .. }
        ),
        "{refusal}"
    );
}
