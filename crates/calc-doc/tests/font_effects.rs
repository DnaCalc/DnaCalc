//! dc-971p.6: a font's `vertAlign` (superscript, subscript) and `charset` read into the style table.

#![allow(non_snake_case)]

mod cdoc_m1_1_support;

use cdoc_m1_1_support::{RawEntry, build_zip, d1::*};

fn package(fonts: &str) -> Vec<u8> {
    let main = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    let rel = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    let parts = [
        ("[Content_Types].xml", format!("<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/></Types>")),
        ("_rels/.rels", format!("<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{rel}/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>")),
        ("xl/workbook.xml", format!("<workbook xmlns=\"{main}\" xmlns:r=\"{rel}\"><sheets><sheet name=\"S\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>")),
        ("xl/_rels/workbook.xml.rels", format!("<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{rel}/worksheet\" Target=\"worksheets/sheet1.xml\"/><Relationship Id=\"rId2\" Type=\"{rel}/styles\" Target=\"styles.xml\"/></Relationships>")),
        ("xl/worksheets/sheet1.xml", format!("<worksheet xmlns=\"{main}\"><sheetData><row r=\"1\"><c r=\"A1\" s=\"1\"><v>1</v></c></row></sheetData></worksheet>")),
        ("xl/styles.xml", format!("<styleSheet xmlns=\"{main}\">{fonts}<fills count=\"1\"><fill><patternFill patternType=\"none\"/></fill></fills><borders count=\"1\"><border/></borders><cellXfs count=\"2\"><xf fontId=\"0\"/><xf fontId=\"1\" applyFont=\"1\"/></cellXfs></styleSheet>")),
    ];
    let entries: Vec<RawEntry> = parts.iter().map(|(n, c)| RawEntry::stored(n, c.as_bytes())).collect();
    build_zip(&entries)
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_6__vert_align_reads_as_a_font_field_not_an_unread_child() {
    let loaded = load(&package(
        "<fonts count=\"4\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font><font><sz val=\"11\"/><vertAlign val=\"superscript\"/><name val=\"Calibri\"/><charset val=\"1\"/></font><font><vertAlign val=\"subscript\"/></font><font><vertAlign val=\"baseline\"/></font></fonts>",
    ));
    let fonts = &loaded.styles().fonts;
    assert_eq!(fonts[0].vert_align, None);
    assert_eq!(fonts[1].vert_align.as_deref(), Some("superscript"));
    assert_eq!(fonts[2].vert_align.as_deref(), Some("subscript"));
    // `baseline` is the absence of a script.
    assert_eq!(fonts[3].vert_align, None);
    // vertAlign is no longer an unread child; the charset still is.
    assert_eq!(fonts[1].other_children, ["charset"]);
    assert!(fonts[2].other_children.is_empty());
}
