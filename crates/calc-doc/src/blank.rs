//! A new, empty workbook package: what Excel's New Workbook makes, one empty worksheet named
//! `Sheet1`, with an empty shared-string table and the default cell style, so a save of an edited
//! revision has the parts it extends. The archive is written by the save's own writer
//! (`save::new_archive`), each part deflated as a save adds one.
//!
//! [`blank_package`] returns the bytes; the caller opens them as it opens any package
//! ([`crate::open`] over a [`crate::MemorySource`]).

const CONTENT_TYPES: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
    r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
    r#"<Default Extension="xml" ContentType="application/xml"/>"#,
    r#"<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>"#,
    r#"<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#,
    r#"<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>"#,
    r#"<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>"#,
    r#"</Types>"#
);

const PACKAGE_RELATIONSHIPS: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>"#,
    r#"</Relationships>"#
);

const WORKBOOK: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
    r#"<DATEPR/><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets>"#,
    r#"<calcPr calcId="191029"/>"#,
    r#"</workbook>"#
);

const WORKBOOK_RELATIONSHIPS: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>"#,
    r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
    r#"<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>"#,
    r#"</Relationships>"#
);

const WORKSHEET: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
    r#"<sheetData/>"#,
    r#"</worksheet>"#
);

/// The new workbook's default font, fill, border and cell format.
/// This minimal package has no theme part, so its font colour is automatic.
const STYLES: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
    r#"<fonts count="1"><font><sz val="11"/><color auto="1"/><name val="Calibri"/><family val="2"/><scheme val="minor"/></font></fonts>"#,
    r#"<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>"#,
    r#"<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>"#,
    r#"<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>"#,
    r#"<cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>"#,
    r#"<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>"#,
    r#"</styleSheet>"#
);

const SHARED_STRINGS: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="0" uniqueCount="0"/>"#
);

/// The package of a new workbook with one empty worksheet, `Sheet1`.
pub fn blank_package() -> Vec<u8> {
    blank_package_named("Sheet1")
}

/// The package of a new workbook with one empty worksheet named `sheet`, which the caller has
/// checked is a sheet name Excel accepts (`calc_doc_model::SheetName`).
pub fn blank_package_named(sheet: &str) -> Vec<u8> {
    blank_package_named_in(sheet, false)
}

/// [`blank_package_named`] counting serial dates from 1904 (`workbookPr date1904="1"`) when
/// `date_1904`, as Excel's date system option writes it.
pub fn blank_package_named_in(sheet: &str, date_1904: bool) -> Vec<u8> {
    blank_package_with(sheet, date_1904, None)
}

/// A VBA project a new workbook carries: its `vbaProject.bin` part, and the code names the
/// workbook and its sheet are given so the project's document modules bind to them.
pub(crate) struct Project<'a> {
    pub part: &'a [u8],
    pub workbook_code_name: Option<&'a str>,
    pub sheet_code_name: Option<&'a str>,
}

fn escaped(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// [`blank_package_named_in`], as an .xlsm carrying `project` when there is one: the macro-enabled
/// workbook content type, the project part with its content type and its relationship from the
/// workbook part, and the code names.
pub(crate) fn blank_package_with(
    sheet: &str,
    date_1904: bool,
    project: Option<Project<'_>>,
) -> Vec<u8> {
    let name = escaped(sheet);
    let workbook_code_name = project
        .as_ref()
        .and_then(|p| p.workbook_code_name)
        .map(|n| format!(r#" codeName="{}""#, escaped(n)))
        .unwrap_or_default();
    let date = if date_1904 { r#" date1904="1""# } else { "" };
    let workbook_pr = if workbook_code_name.is_empty() && date.is_empty() {
        String::new()
    } else {
        format!("<workbookPr{date}{workbook_code_name}/>")
    };
    let workbook = WORKBOOK
        .replace(r#"name="Sheet1""#, &format!(r#"name="{name}""#))
        .replace("<DATEPR/>", &workbook_pr);
    let Some(project) = project else {
        return crate::save::new_archive(&[
            ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
            ("_rels/.rels", PACKAGE_RELATIONSHIPS.as_bytes()),
            ("xl/workbook.xml", workbook.as_bytes()),
            (
                "xl/_rels/workbook.xml.rels",
                WORKBOOK_RELATIONSHIPS.as_bytes(),
            ),
            ("xl/worksheets/sheet1.xml", WORKSHEET.as_bytes()),
            ("xl/styles.xml", STYLES.as_bytes()),
            ("xl/sharedStrings.xml", SHARED_STRINGS.as_bytes()),
        ])
        .expect("seven small parts fit a ZIP archive");
    };
    let content_types = CONTENT_TYPES
        .replace(
            crate::SpreadsheetFormat::Xlsx.content_type(),
            crate::SpreadsheetFormat::Xlsm.content_type(),
        )
        .replace(
            "</Types>",
            &format!(
                r#"<Override PartName="/xl/vbaProject.bin" ContentType="{}"/></Types>"#,
                crate::VBA_PROJECT_CONTENT_TYPE
            ),
        );
    let relationships = WORKBOOK_RELATIONSHIPS.replace(
        "</Relationships>",
        &format!(
            r#"<Relationship Id="rId4" Type="{}" Target="vbaProject.bin"/></Relationships>"#,
            crate::vba::VBA_PROJECT
        ),
    );
    let worksheet = match project.sheet_code_name {
        Some(code_name) => WORKSHEET.replace(
            "<sheetData/>",
            &format!(
                r#"<sheetPr codeName="{}"/><sheetData/>"#,
                escaped(code_name)
            ),
        ),
        None => WORKSHEET.to_owned(),
    };
    crate::save::new_archive(&[
        ("[Content_Types].xml", content_types.as_bytes()),
        ("_rels/.rels", PACKAGE_RELATIONSHIPS.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", relationships.as_bytes()),
        ("xl/worksheets/sheet1.xml", worksheet.as_bytes()),
        ("xl/styles.xml", STYLES.as_bytes()),
        ("xl/sharedStrings.xml", SHARED_STRINGS.as_bytes()),
        ("xl/vbaProject.bin", project.part),
    ])
    .expect("eight parts fit a ZIP archive")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemorySource, OpenPolicy, load, open};
    use calc_doc_model::value::{IdAllocator, SheetId};
    use std::sync::Arc;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_blank_package_opens_with_one_empty_sheet() {
        let source = Arc::new(MemorySource::new(blank_package()));
        let document = open(source, &OpenPolicy::default()).expect("the blank package opens");
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let loaded = load(&document, &mut ids).expect("its workbook loads");
        assert!(loaded.styles().fonts[0].color.as_ref().unwrap().auto);
        assert!(loaded.theme_colors().is_empty());
        let sheets = loaded.model().sheets();
        assert_eq!(sheets.len(), 1);
        assert_eq!(sheets[0].name().text().to_string_lossy(), "Sheet1");
        let cells = loaded
            .model()
            .sheet_cells(sheets[0].id())
            .expect("the sheet is in the model");
        assert_eq!(cells.count(), 0);
    }
}
