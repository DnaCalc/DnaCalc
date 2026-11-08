//! The constrained session workbook: files outside its constraints are refused with the reason,
//! and a file the codec wrote reads back (the round trips are in `src/session.rs`).

use calc_doc::{SessionRefusal, SessionVba, read_session, read_vba_file, write_session};

const MANY_SHEETS: &[u8] = include_bytes!("fixtures/cdoc_m1_2/d1.saved.xlsx");
const WITH_VBA: &[u8] = include_bytes!("fixtures/vba_udf_names/case-only.xlsm");
// DNA Calc's example add-in project as Excel saved it (crates/calc-xll-reference/fixture/vba).
const EXAMPLES_XLSM: &[u8] = include_bytes!("../../calc-xll-reference/fixture/vba/DnaExamples.xlsm");
const EXAMPLES_XLAM: &[u8] = include_bytes!("../../calc-xll-reference/fixture/vba/DnaExamples.xlam");

fn opened(package: &[u8]) -> calc_doc::OpenedDocument {
    calc_doc::open(
        std::sync::Arc::new(calc_doc::MemorySource::new(package.to_vec())),
        &Default::default(),
    )
    .unwrap()
}

/// The bytes of a stored or deflated part of a package.
fn part(package: &[u8], name: &str) -> Vec<u8> {
    opened(package).package().read_part(name).unwrap()
}
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
fn a_workbook_with_a_vba_project_is_refused_only_for_what_else_it_holds() {
    // A VBA project is no reason to refuse a session file (it is then an .xlsm); this one also
    // holds defined names, which a session file has no place for.
    assert_eq!(
        read_session(WITH_VBA.to_vec(), "urn:example:none"),
        Err(SessionRefusal::Content { what: "defined names" })
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn an_xlsm_session_saves_with_its_project_part_unchanged_and_reads_back() {
    let file = read_session(EXAMPLES_XLSM.to_vec(), "urn:example:none").expect("an empty session");
    assert_eq!(file.sheet, "Sheet1");
    assert!(file.formula.is_none());
    let vba = file.vba.clone().expect("the example project");
    assert_eq!(
        vba,
        SessionVba {
            project: part(EXAMPLES_XLSM, "xl/vbaProject.bin"),
            workbook_code_name: Some("ThisWorkbook".into()),
            sheet_code_name: Some("Sheet1".into()),
        }
    );
    let mut session = file.clone();
    session.formula = Some(calc_doc::SessionFormula {
        row: 1,
        column: 8,
        text: "DnaAdd(40,2)".into(),
        array: calc_doc::SessionArray::Plain,
    });
    let saved = write_session(&session).expect("saved as .xlsm");
    // The part goes back byte for byte, with its content type, its relationship and the
    // macro-enabled workbook content type.
    assert_eq!(part(&saved, "xl/vbaProject.bin"), vba.project);
    let document = opened(&saved);
    let package = document.package();
    assert_eq!(
        package.content_types().of("xl/vbaProject.bin"),
        Some(calc_doc::VBA_PROJECT_CONTENT_TYPE)
    );
    assert_eq!(
        package.content_types().of("xl/workbook.xml"),
        Some("application/vnd.ms-excel.sheet.macroEnabled.main+xml")
    );
    let links = package.relationships_of(Some("xl/workbook.xml")).unwrap().items();
    assert!(links.iter().any(|link| link.kind
        == "http://schemas.microsoft.com/office/2006/relationships/vbaProject"
        && link.resolved.as_deref() == Some("xl/vbaProject.bin")));
    let read = read_session(saved, "urn:example:none").expect("read back");
    assert_eq!(read.vba, Some(vba));
    assert_eq!(read.formula, session.formula);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn an_add_in_file_gives_its_project_part_and_code_name() {
    let vba = read_vba_file(EXAMPLES_XLAM.to_vec()).unwrap().expect("the add-in's project");
    assert_eq!(vba.project, part(EXAMPLES_XLAM, "xl/vbaProject.bin"));
    assert_eq!(vba.workbook_code_name.as_deref(), Some("ThisWorkbook"));
    assert_eq!(read_vba_file(MANY_SHEETS.to_vec()).unwrap(), None);
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
