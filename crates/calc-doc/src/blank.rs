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
    r#"<sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets>"#,
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

/// The default font, fill, border and cell format Excel writes for a new workbook.
const STYLES: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
    r#"<fonts count="1"><font><sz val="11"/><color theme="1"/><name val="Calibri"/><family val="2"/><scheme val="minor"/></font></fonts>"#,
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
    crate::save::new_archive(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", PACKAGE_RELATIONSHIPS.as_bytes()),
        ("xl/workbook.xml", WORKBOOK.as_bytes()),
        (
            "xl/_rels/workbook.xml.rels",
            WORKBOOK_RELATIONSHIPS.as_bytes(),
        ),
        ("xl/worksheets/sheet1.xml", WORKSHEET.as_bytes()),
        ("xl/styles.xml", STYLES.as_bytes()),
        ("xl/sharedStrings.xml", SHARED_STRINGS.as_bytes()),
    ])
    .expect("seven small parts fit a ZIP archive")
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
