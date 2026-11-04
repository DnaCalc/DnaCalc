//! Change source streams in an existing test project. This creates synthetic
//! source-ingestion controls, never a new Excel observation or a captured answer.
use super::support;
use std::{
    io::{Cursor, Read, Write},
    sync::Arc,
};

pub fn replace_sources(bytes: &[u8], sources: &[(&str, &str)]) -> Vec<u8> {
    replace_project(bytes, sources, None)
}

/// A fresh namespace-edit control with the exact supplied project binary. The
/// original captured package has rich metadata whose sheet rename is refused.
/// This authors an empty sheet, without changing or filtering any VBA module.
#[allow(dead_code)]
pub fn minimal_control(bytes: &[u8]) -> Vec<u8> {
    let binary = support::d1::part(bytes, "xl/vbaProject.bin").unwrap();
    let parts: &[(&str, &[u8])] = &[
        ("[Content_Types].xml", br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.ms-excel.sheet.macroEnabled.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/vbaProject.bin" ContentType="application/vnd.ms-office.vbaProject"/></Types>"#),
        ("_rels/.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#),
        ("xl/workbook.xml", br#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><workbookPr codeName="ThisWorkbook"/><sheets><sheet name="Cases" sheetId="1" r:id="sheet"/></sheets></workbook>"#),
        ("xl/_rels/workbook.xml.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="sheet" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="vba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin"/></Relationships>"#),
        ("xl/worksheets/sheet1.xml", br#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetPr codeName="Sheet1"/><sheetData/></worksheet>"#),
        ("xl/vbaProject.bin", &binary),
    ];
    support::build_zip(&parts.iter().map(|(name, data)| support::RawEntry::stored(name, data)).collect::<Vec<_>>())
}

#[allow(dead_code)]
pub fn replace_project_declaration(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
    replace_project(bytes, &[], Some((from, to)))
}

fn replace_project(
    bytes: &[u8],
    sources: &[(&str, &str)],
    declaration: Option<(&str, &str)>,
) -> Vec<u8> {
    let source = Arc::new(calc_doc::MemorySource::new(bytes.to_vec()));
    let document = calc_doc::open(source, &Default::default()).unwrap();
    let loaded = calc_doc::load(&document, &mut Default::default()).unwrap();
    let package = document.package();
    let project = loaded
        .read_vba_project(&Default::default())
        .unwrap()
        .unwrap();
    let mut compound =
        cfb::CompoundFile::open(Cursor::new(package.read_part(&project.part).unwrap())).unwrap();
    for (name, text) in sources {
        let module = project
            .modules
            .iter()
            .find(|module| module.name == *name)
            .unwrap();
        let path = format!("/VBA/{}", module.stream_name);
        let mut bytes = Vec::new();
        compound
            .open_stream(&path)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        bytes.truncate(module.source_offset);
        // MS-OVBA literal-only compressed chunks. No copied caches or p-code is
        // executed by the source frontend; the original directory/offset stays valid.
        bytes.push(1);
        for chunk in text.as_bytes().chunks(3600) {
            let mut literals = Vec::new();
            for group in chunk.chunks(8) {
                literals.push(0);
                literals.extend_from_slice(group);
            }
            bytes.extend_from_slice(&(0xb000u16 | (literals.len() as u16 - 1)).to_le_bytes());
            bytes.extend_from_slice(&literals);
        }
        compound
            .create_stream(&path)
            .unwrap()
            .write_all(&bytes)
            .unwrap();
    }
    if let Some((from, to)) = declaration {
        let mut text = String::new();
        compound
            .open_stream("/PROJECT")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert!(text.contains(from), "test declaration is present");
        compound
            .create_stream("/PROJECT")
            .unwrap()
            .write_all(text.replace(from, to).as_bytes())
            .unwrap();
    }
    let binary = compound.into_inner().into_inner();
    let entries = support::parse_zip(bytes)
        .entries
        .iter()
        .map(|entry| {
            let content = if entry.name == project.part {
                binary.clone()
            } else {
                match entry.method {
                    0 => entry.payload(bytes).to_vec(),
                    8 => miniz_oxide::inflate::decompress_to_vec(entry.payload(bytes)).unwrap(),
                    _ => panic!("unsupported test package compression"), // ubs:ignore[rust.ownership.panic-macro] An unknown authored fixture encoding must fail its test.
                }
            };
            support::RawEntry::stored(&entry.name, &content)
        })
        .collect::<Vec<_>>();
    support::build_zip(&entries)
}
