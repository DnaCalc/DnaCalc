//! The save route over a package built here: a worksheet and a shared strings table whose
//! attributes the rewrite reads (`r`, `s`, `t`, `si`, `count`) are written as character
//! references, which the loader replaces and the rewrite must read as the loader does.

use crate::xml::XmlEvent;
use crate::zip::crc32::crc32;
use crate::{MemorySource, OpenPolicy, SavePolicy, load, open, prepare_edit, prepare_save, write};
use calc_doc_model::value::{CellKey, ExcelText, GridArea, IdAllocator, SheetId};
use calc_doc_model::{CellInput, FiniteNumber, FormulaText, OpBatch, WorkbookOp};
use std::sync::Arc;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const KIND: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// A ZIP archive of stored entries, in the order given.
fn stored_zip(entries: &[(&str, String)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, content) in entries {
        let offset = out.len() as u32;
        let (crc, size) = (crc32(content.as_bytes()), content.len() as u32);
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        for field in [20u16, 0, 0, 0, 0x21] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        for field in [crc, size, size] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(content.as_bytes());
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        for field in [20u16, 20, 0, 0, 0, 0x21] {
            central.extend_from_slice(&field.to_le_bytes());
        }
        for field in [crc, size, size] {
            central.extend_from_slice(&field.to_le_bytes());
        }
        for field in [name.len() as u16, 0, 0, 0, 0] {
            central.extend_from_slice(&field.to_le_bytes());
        }
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let (directory_offset, directory_size) = (out.len() as u32, central.len() as u32);
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    let count = entries.len() as u16;
    for field in [0u16, 0, count, count] {
        out.extend_from_slice(&field.to_le_bytes());
    }
    out.extend_from_slice(&directory_size.to_le_bytes());
    out.extend_from_slice(&directory_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn declared(root: &str) -> String {
    format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>{root}"#)
}

/// The package: A1 a shared string (`r` and `t` as references), B1 the master and B2 the member
/// of a shared formula group whose `si` (and B2's `t`) are references, C1 a value whose style
/// index 0 is a reference, row 1's `r` a reference, and an sst whose `count` is a reference.
fn package() -> Vec<u8> {
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}"><dimension ref="A1:C2"/><sheetData><row r="&#49;"><c r="&#65;1" t="&#115;"><v>0</v></c><c r="B1"><f t="shared" ref="B1:B2" si="&#48;">A1</f><v>0</v></c><c r="C1" s="&#48;"><v>1</v></c></row><row r="2"><c r="B2"><f t="&#115;hared" si="&#48;"/><v>0</v></c></row></sheetData></worksheet>"#
    );
    stored_zip(&[
        (
            "[Content_Types].xml",
            declared(&format!(
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/sharedStrings.xml" ContentType="{}"/></Types>"#,
                super::strings::CONTENT_TYPE
            )),
        ),
        (
            "_rels/.rels",
            declared(&format!(
                r#"<Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{KIND}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
            )),
        ),
        (
            "xl/workbook.xml",
            declared(&format!(
                r#"<workbook xmlns="{MAIN}" xmlns:r="{KIND}"><sheets><sheet name="One" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            )),
        ),
        (
            "xl/_rels/workbook.xml.rels",
            declared(&format!(
                r#"<Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{KIND}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{KIND}/sharedStrings" Target="sharedStrings.xml"/></Relationships>"#
            )),
        ),
        ("xl/worksheets/sheet1.xml", declared(&worksheet)),
        (
            "xl/sharedStrings.xml",
            declared(&format!(
                r#"<sst xmlns="{MAIN}" count="&#49;" uniqueCount="1"><si><t>a</t></si></sst>"#
            )),
        ),
    ])
}

fn number(value: f64) -> CellInput {
    CellInput::Number(FiniteNumber::new(value).expect("finite"))
}

fn key(sheet: SheetId, row: u32, column: u32) -> CellKey {
    CellKey::new(sheet, row, column).expect("in the grid")
}

/// The cells of a model's first sheet, as (row, column, input).
fn cells(model: &calc_doc_model::ModelRoot) -> Vec<(u32, u32, CellInput)> {
    let sheet = model.sheets()[0].id();
    model
        .sheet_cells(sheet)
        .expect("the sheet")
        .map(|(at, input)| (at.row(), at.column(), input.clone()))
        .collect()
}

/// The rewrite reads every attribute with its references replaced, as the loader does: the row
/// and the cell are where `r` says, the replaced shared string is uncounted, the group whose
/// master changed is written as individual formulas, and the emptied cell of style 0 is removed.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn attributes_written_as_references() {
    let bytes = package();
    let document = open(
        Arc::new(MemorySource::new(bytes.clone())),
        &OpenPolicy::default(),
    )
    .expect("the package opens");
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let loaded = load(&document, &mut ids).expect("the package loads");
    let sheet = loaded.model().sheets()[0].id();
    let member = loaded.sheets()[0].formula(2, 2).expect("B2 is a formula");
    assert_eq!(member.shared().map(|group| group.index()), Some(0));
    let batch = OpBatch {
        base: loaded.model().revision(),
        operations: vec![
            WorkbookOp::SetCell {
                at: key(sheet, 1, 1),
                input: number(7.0),
            },
            WorkbookOp::SetCell {
                at: key(sheet, 1, 2),
                input: number(5.0),
            },
            WorkbookOp::Clear {
                area: GridArea::new(key(sheet, 1, 3), key(sheet, 1, 3)).expect("C1"),
            },
        ],
    };
    let model = prepare_edit(loaded.model(), &batch)
        .expect("the edit")
        .into_model();
    let plan = prepare_save(&loaded.snapshot(&model), &SavePolicy::default())
        .expect("the rewrite reads the attributes as the loader does");
    let mut out = Vec::new();
    let receipt = write(plan, &mut out).expect("a write to memory");
    assert_eq!(receipt.caches_dropped, 1, "B2's cached result");

    let saved = open(Arc::new(MemorySource::new(out)), &OpenPolicy::default())
        .expect("the saved package opens");
    let mut reload_ids: IdAllocator<SheetId> = IdAllocator::new();
    let reloaded = load(&saved, &mut reload_ids).expect("the saved package loads");
    assert_eq!(cells(reloaded.model()), cells(&model));
    let text = FormulaText::new(ExcelText::from_utf16_code_units(
        "A2".encode_utf16().collect(),
    ))
    .expect("a formula");
    assert_eq!(
        reloaded
            .model()
            .cell(key(reloaded.model().sheets()[0].id(), 2, 2)),
        Some(&CellInput::Formula(text)),
        "B2 holds its own text"
    );
    assert_eq!(
        reloaded.sheets()[0]
            .formula(2, 2)
            .map(|record| record.shared()),
        Some(None),
        "B2 is no longer a member of a group"
    );

    let sheet_xml = saved
        .package()
        .read_xml("xl/worksheets/sheet1.xml")
        .expect("the sheet");
    let sheet_text = String::from_utf8(sheet_xml.bytes().to_vec()).expect("UTF-8");
    assert!(
        sheet_text.contains(r#"<row r="&#49;"><c r="A1"><v>7</v></c><c r="B1"><v>5</v></c></row>"#),
        "row 1 keeps its start tag, A1 is rewritten without its t, and C1 is removed: {sheet_text}"
    );
    assert!(
        sheet_text.contains(r#"<c r="B2"><f>A2</f></c>"#),
        "{sheet_text}"
    );

    let strings = saved
        .package()
        .read_xml("xl/sharedStrings.xml")
        .expect("the shared strings");
    let mut reader = strings.reader();
    let Ok(Some(XmlEvent::Start(root))) = reader.next_event() else {
        panic!("the sst element");
    };
    assert_eq!(
        (root.attribute("count"), root.attribute("uniqueCount")),
        (Some("0"), Some("1")),
        "one reference to the table, A1's, removed"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn conditional_rules_keep_namespaces_escapes_and_container_extensions() {
    for container_extension in [false, true] {
        let source = open(
            Arc::new(MemorySource::new(package())),
            &OpenPolicy::default(),
        )
        .unwrap();
        let extra = if container_extension {
            "<x:extLst><x:ext uri=\"held\"><held xmlns=\"urn:test\"/></x:ext></x:extLst>"
        } else {
            ""
        };
        let worksheet = format!(
            "<x:worksheet xmlns:x=\"{MAIN}\"><x:sheetData/><x:conditionalFormatting sqref=\"A1:A3\"><x:cfRule type=\"expression\" priority=\"1\"><x:formula xmlns:v=\"urn:formula-retention\" v:keep=\"yes\">LEN(\"_x005F_x0041_\")=7</x:formula></x:cfRule>{extra}</x:conditionalFormatting></x:worksheet>"
        );
        let entries = source
            .package()
            .archive()
            .entries
            .iter()
            .map(|entry| {
                let text = if entry.name == "xl/worksheets/sheet1.xml" {
                    worksheet.clone()
                } else {
                    String::from_utf8(
                        crate::zip::read_content(source.package().source(), entry).unwrap(),
                    )
                    .unwrap()
                };
                (entry.name.as_str(), text)
            })
            .collect::<Vec<_>>();
        let document = open(
            Arc::new(MemorySource::new(stored_zip(&entries))),
            &OpenPolicy::default(),
        )
        .unwrap();
        let mut ids = IdAllocator::new();
        let loaded = load(&document, &mut ids).unwrap();
        let sheet = loaded.model().sheets()[0].id();
        let existing = loaded.model().definitions().conditional_rules(sheet)[0].clone();
        assert_eq!(existing.formulas, vec!["LEN(\"_x0041_\")=7"]);
        let area = GridArea::new(key(sheet, 1, 1), key(sheet, 3, 1)).unwrap();
        let mut rules = vec![
            crate::new_conditional_rule(vec![area], "dataBar", "327A94").unwrap(),
            existing,
        ];
        rules[1].priority = 2;
        if container_extension {
            rules.clear();
        }
        let batch = OpBatch {
            base: loaded.model().revision(),
            operations: vec![WorkbookOp::SetConditionalRules { sheet, rules }],
        };
        let edited = prepare_edit(loaded.model(), &batch).unwrap().into_model();
        let plan = prepare_save(&loaded.snapshot(&edited), &SavePolicy::default());
        if container_extension {
            assert!(plan.is_err(), "container metadata must never be discarded");
            continue;
        }
        let mut bytes = Vec::new();
        write(plan.unwrap(), &mut bytes).unwrap();
        let document = open(Arc::new(MemorySource::new(bytes)), &OpenPolicy::default()).unwrap();
        let mut ids = IdAllocator::new();
        let loaded = load(&document, &mut ids).unwrap();
        let sheet = loaded.model().sheets()[0].id();
        let rules = loaded.model().definitions().conditional_rules(sheet);
        assert_eq!(rules.len(), 2);
        assert_eq!(crate::conditional_rule(&rules[0]).unwrap().kind, "dataBar");
        assert_eq!(rules[1].formulas, vec!["LEN(\"_x0041_\")=7"]);
        assert!(crate::conditional_rule(&rules[1]).unwrap().unread);
        assert!(String::from_utf8_lossy(&rules[1].source).contains("v:keep=\"yes\""));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_41__filter_rewrite_owns_its_prefix_after_source_rebinding() {
    let document = open(
        Arc::new(MemorySource::new(package())),
        &OpenPolicy::default(),
    )
    .unwrap();
    let loaded = load(&document, &mut IdAllocator::new()).unwrap();
    let sheet = loaded.model().sheets()[0].id();
    let area = GridArea::new(key(sheet, 1, 1), key(sheet, 3, 2)).unwrap();
    let source = format!(
        r#"<s:worksheet xmlns:s="{MAIN}"><s:sheetData/><m:autoFilter xmlns:m="{MAIN}" xmlns:s="http://schemas.microsoft.com/office/spreadsheetml/2014/revision" s:uid="keep-me" ref="A1:B3"/></s:worksheet>"#
    );
    let part = crate::xml::XmlPart::new(
        "sheet.xml".into(),
        source.into_bytes(),
        crate::xml::XmlLimits::default(),
    );
    let output = super::filter::rewrite(
        &part,
        Some(&calc_doc_model::AutoFilter::new(area)),
        loaded.model().definitions(),
    )
    .unwrap();
    let part =
        crate::xml::XmlPart::new("sheet.xml".into(), output, crate::xml::XmlLimits::default());
    let mut reader = part.reader();
    let mut filters = 0;
    while let Some(event) = reader.next_event().unwrap() {
        if let XmlEvent::Start(e) = event {
            if e.name.local == "autoFilter" {
                assert!(e.name.is(MAIN, "autoFilter"));
                assert!(e.attributes.iter().any(|a| a.name.is(
                    "http://schemas.microsoft.com/office/spreadsheetml/2014/revision",
                    "uid"
                ) && a.value == "keep-me"));
                filters += 1;
            }
        }
    }
    assert_eq!(filters, 1);
}
