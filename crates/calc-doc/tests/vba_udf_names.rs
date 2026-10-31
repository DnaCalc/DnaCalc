//! Regressions against the Excel-authored dc-snz.7 file witnesses.

mod cdoc_m1_1_support;

use calc_doc::model::value::CellKey;
use calc_doc::model::{CellInput, OpBatch, WorkbookOp};
use cdoc_m1_1_support::d1::{load, part, part_text, save_model, save_untouched};

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_snz_8__reentered_forced_udf_keeps_ordinary_arguments() {
    for bytes in [
        include_bytes!("fixtures/vba_udf_names/collision.xlsm").as_slice(),
        include_bytes!("fixtures/vba_udf_names/collision.xlsb").as_slice(),
    ] {
        let loaded = load(bytes);
        let root = loaded.model();
        let sheet = root.sheets()[0].id();
        let original = CellKey::new(sheet, 1, 2).unwrap();
        let input = root.cell(original).unwrap().clone();
        let CellInput::Formula(formula) = &input else {
            panic!("a formula")
        };
        assert_eq!(formula.text().to_string_lossy(), "_xludf.LAMBDA(Rate,3)");
        let target = CellKey::new(sheet, 1, 4).unwrap();
        let edit = calc_doc::prepare_edit(
            root,
            &OpBatch {
                base: root.revision(),
                operations: vec![WorkbookOp::SetCell { at: target, input }],
            },
        )
        .unwrap();
        let (saved, _) = save_model(&loaded, edit.model());
        let reopened = load(&saved);
        assert_eq!(reopened.model().cell(target), root.cell(original));
        // Ordinary language LAMBDA still receives parameter prefixes, as in local.xlsm.
        let local = load(include_bytes!("fixtures/vba_udf_names/local.xlsm"));
        let named = local
            .model()
            .definitions()
            .names()
            .iter()
            .find(|name| name.name == "NamedLambda")
            .unwrap();
        let CellInput::Formula(text) = cdoc_m1_1_support::d1::formula("LAMBDA(x,x+1400)") else {
            unreachable!()
        };
        assert_eq!(
            String::from_utf16(
                &calc_doc::file_form(&text, root.definitions().names())
                    .unwrap()
                    .text
            )
            .unwrap(),
            named.formula
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_snz_12__binary_links_keep_the_measured_absolute_targets() {
    let xml = include_bytes!("fixtures/vba_udf_names/consumer.xlsx");
    let saved = save_untouched(include_bytes!("fixtures/vba_udf_names/consumer.xlsb"));
    // The saved source folder is also needed: Excel rebases relative links from it.
    let workbook = part_text(xml, "xl/workbook.xml");
    let location = workbook
        .split("<x15ac:absPath")
        .nth(1)
        .unwrap()
        .split("url=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let converted = part_text(&saved, "xl/workbook.xml");
    assert!(converted.contains("<x15ac:absPath"));
    assert!(converted.contains(&format!("url=\"{location}\"")));
    for index in 1..=2 {
        let link = format!("xl/externalLinks/externalLink{index}.xml");
        let expected = part_text(xml, &link);
        let written = part_text(&saved, &link);
        // Both the relative main link and its absolute alternate relationship survive.
        for fragment in [
            "r:id=\"rId1\"",
            "<xxl21:alternateUrls>",
            "<xxl21:absoluteUrl r:id=\"rId2\"/>",
            "</xxl21:alternateUrls>",
        ] {
            assert!(
                expected.contains(fragment),
                "Excel witness contains {fragment}"
            );
            assert!(
                written.contains(fragment),
                "converted {link} contains {fragment}: {written}"
            );
        }
        let relationships = format!("xl/externalLinks/_rels/externalLink{index}.xml.rels");
        assert_eq!(part(&saved, &relationships), part(xml, &relationships));
    }
}
