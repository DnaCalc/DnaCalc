//! dc-ndi (D1): authored formulas and values saved in Excel's file form: function prefixes,
//! LET/LAMBDA parameters, the spill and intersection operators, dynamic arrays with their cell
//! metadata (found, appended to a metadata part, or added with a new part), and error constants
//! Excel cannot hold refused.

#![cfg(not(target_arch = "wasm32"))]

mod cdoc_m1_1_support;

use calc_doc::model::value::WorksheetErrorCode;
use calc_doc::model::{ArrayKind, CellInput, OpBatch, WorkbookOp};
use calc_doc::{EditError, prepare_edit};
use cdoc_m1_1_support::d1::*;

#[test]
fn d1_authoring__formulas_are_written_in_file_form() {
    let bytes = fixture("../cdoc_m1_2/d1.saved.xlsx");
    let loaded = load(&bytes);
    let calc = sheet(&loaded, "Calc");
    let set = |address: &str, text: &str| WorkbookOp::SetCell {
        at: at(calc, address),
        input: formula(text),
    };
    let model = edited(
        &loaded,
        vec![
            set("Y1", "SEQUENCE(2,2)"),
            set("Y4", "XLOOKUP(3,Data!A1:A5,Data!B1:B5)"),
            set("Y5", "LET(x,2,x*3)"),
            set("Y6", "SUM(A1#)"),
            set("Y7", "Data!A1:A3*2"),
            set("Y10", "@Data!A1:A3"),
            set("Y11", "STDEV.P(Data!A1:A5)"),
        ],
    );
    let (saved, _) = save_model(&loaded, &model);
    keep_for_excel("d1_authored_formulas.xlsx", &saved);
    let xml = part_text(&saved, "xl/worksheets/sheet2.xml");
    for expected in [
        r#"<c r="Y1" cm="1"><f t="array" ref="Y1">_xlfn.SEQUENCE(2,2)</f></c>"#,
        r#"<c r="Y4"><f>_xlfn.XLOOKUP(3,Data!A1:A5,Data!B1:B5)</f></c>"#,
        r#"<c r="Y5"><f>_xlfn.LET(_xlpm.x,2,_xlpm.x*3)</f></c>"#,
        r#"<c r="Y6"><f>SUM(_xlfn.ANCHORARRAY(A1))</f></c>"#,
        r#"<c r="Y7" cm="1"><f t="array" ref="Y7">Data!A1:A3*2</f></c>"#,
        r#"<c r="Y10"><f>Data!A1:A3</f></c>"#,
        r#"<c r="Y11"><f>_xlfn.STDEV.P(Data!A1:A5)</f></c>"#,
    ] {
        assert!(xml.contains(expected), "{expected} in {xml}");
    }
    // The workbook's metadata part already holds the dynamic-array record (cm 1): it is kept.
    assert_eq!(
        part(&saved, "xl/metadata.xml"),
        part(&bytes, "xl/metadata.xml")
    );
    let reloaded = load(&saved);
    let calc = sheet(&reloaded, "Calc");
    for (address, kind) in [("Y1", true), ("Y7", true), ("Y4", false)] {
        assert_eq!(
            reloaded
                .model()
                .definitions()
                .array(at(calc, address))
                .map(|array| array.kind()),
            kind.then_some(ArrayKind::Dynamic { collapsed: false }),
            "{address}"
        );
    }
}

#[test]
fn d1_authoring__a_metadata_part_without_the_record_gains_it() {
    // OxDoc's rich-error fixture: a metadata part with only XLRICHVALUE value metadata.
    let bytes = package_from_dir(
        &fixtures()
            .join("oxdoc")
            .join("w010-rich-error-cell-edit")
            .join("parts"),
    );
    let loaded = load(&bytes);
    let first = loaded.model().sheets()[0].id();
    let model = edited(
        &loaded,
        vec![
            WorkbookOp::SetCell {
                at: at(first, "H1"),
                input: formula("SEQUENCE(3)"),
            },
            // D1 calls IMAGE, which the capture adapter's isolation refuses to open, so the
            // saved package's Excel reopen goes through `cargo xtask capture` without it.
            WorkbookOp::Clear {
                area: calc_doc::model::value::GridArea::cell(at(first, "D1")),
            },
        ],
    );
    let (saved, _) = save_model(&loaded, &model);
    keep_for_excel("d1_authored_metadata_appended.xlsx", &saved);
    let metadata = part_text(&saved, "xl/metadata.xml");
    assert!(
        metadata.contains(r#"<metadataTypes count="2">"#)
            && metadata.contains(r#"<metadataType name="XLDAPR""#)
            && metadata.contains(
                r#"<cellMetadata count="1"><bk><rc t="2" v="0"/></bk></cellMetadata><valueMetadata"#
            ),
        "{metadata}"
    );
    let xml = part_text(&saved, "xl/worksheets/sheet1.xml");
    assert!(
        xml.contains(r#"<c r="H1" cm="1"><f t="array" ref="H1">_xlfn.SEQUENCE(3)</f></c>"#),
        "{xml}"
    );
    // The appended record resolves, and the rich errors' records still do.
    let reloaded = load(&saved);
    let first = reloaded.model().sheets()[0].id();
    assert_eq!(
        reloaded
            .model()
            .definitions()
            .array(at(first, "H1"))
            .map(|array| array.kind()),
        Some(ArrayKind::Dynamic { collapsed: false })
    );
}

#[test]
fn d1_authoring__error_constants_excel_cannot_hold_are_refused() {
    let loaded = load(&fixture("../cdoc_m1_2/d1.saved.xlsx"));
    let calc = sheet(&loaded, "Calc");
    let model = loaded.model();
    for code in [
        WorksheetErrorCode::Spill,
        WorksheetErrorCode::Calc,
        WorksheetErrorCode::Field,
        WorksheetErrorCode::Blocked,
        WorksheetErrorCode::Connect,
        WorksheetErrorCode::Busy,
        WorksheetErrorCode::GettingData,
    ] {
        let batch = OpBatch {
            base: model.revision(),
            operations: vec![WorkbookOp::SetCell {
                at: at(calc, "Z9"),
                input: CellInput::Error(code),
            }],
        };
        assert_eq!(
            prepare_edit(model, &batch).map(|_| ()),
            Err(EditError::ErrorConstant { operation: 0, code })
        );
    }
    // The classic errors are written as their literals.
    let model = edited(
        &loaded,
        vec![WorkbookOp::SetCell {
            at: at(calc, "Z9"),
            input: CellInput::Error(WorksheetErrorCode::NA),
        }],
    );
    let (saved, _) = save_model(&loaded, &model);
    assert!(
        part_text(&saved, "xl/worksheets/sheet2.xml")
            .contains(r#"<c r="Z9" t="e"><v>#N/A</v></c>"#)
    );
}
