//! The dc-hqcv fixtures: one-sheet workbook packages generated here, byte for byte, with no
//! calc-doc code (the ZIP writer is the chost-m1.1 support module's), whose workbook part declares
//! calculation settings other than the ones the host applies.

use crate::chost_m1_1_support::stored_zip;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const RELATIONSHIP_ATTRIBUTES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const WORKSHEET: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
const STYLES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";

/// The `workbookPr` element both packages hold: the 1904 date system.
pub const WORKBOOK_PR: &str = r#"<workbookPr date1904="1"/>"#;

/// The `calcPr` element of the first package: iterative calculation on (at Excel's default count
/// and maximum change) and precision as displayed (`fullPrecision="0"`).
pub const CALC_PR: &str = r#"<calcPr calcId="191029" iterate="1" iterateCount="100" iterateDelta="0.001" fullPrecision="0"/>"#;

/// The sheet, with no cached results:
///
/// | cell | content  | number format         |
/// |------|----------|-----------------------|
/// | A1   | 1.25     | General               |
/// | A2   | `A1*1`   | `0` (built-in 1)      |
/// | A3   | `A2*2`   | General               |
/// | B1   | `B1+1`   | General               |
///
/// B1 reads itself: a circular reference.
pub const SHEET: &str = r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><v>1.25</v></c><c r="B1"><f>B1+1</f></c></row><row r="2"><c r="A2" s="1"><f>A1*1</f></c></row><row r="3"><c r="A3"><f>A2*2</f></c></row></sheetData></worksheet>"#;

/// The styles part: cell format 0 is General, cell format 1 the built-in number format 1, `0`
/// (no decimals).
const STYLES_PART: &str = r#"<?xml version="1.0" encoding="UTF-8"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="1" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/></cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#;

/// The package whose workbook part holds [`WORKBOOK_PR`], the sheet Sheet1 ([`SHEET`]) and, after
/// it, `calc_pr` (none when empty).
pub fn package(calc_pr: &str) -> Vec<u8> {
    let content_types = r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>"#;
    let relationships = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{OFFICE_DOCUMENT}" Target="xl/workbook.xml"/></Relationships>"#
    );
    let workbook = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="{MAIN}" xmlns:r="{RELATIONSHIP_ATTRIBUTES}">{WORKBOOK_PR}<sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets>{calc_pr}</workbook>"#
    );
    let workbook_relationships = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{WORKSHEET}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{STYLES}" Target="styles.xml"/></Relationships>"#
    );
    stored_zip(&[
        ("[Content_Types].xml", content_types.as_bytes()),
        ("_rels/.rels", relationships.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        (
            "xl/_rels/workbook.xml.rels",
            workbook_relationships.as_bytes(),
        ),
        ("xl/worksheets/sheet1.xml", SHEET.as_bytes()),
        ("xl/styles.xml", STYLES_PART.as_bytes()),
    ])
}

/// The content of the stored (uncompressed) entry `name` of the ZIP `package`, found by walking
/// its local file headers from the start, with no calc-doc code; `None` when no entry has that
/// name or an entry is compressed.
pub fn stored_entry<'a>(package: &'a [u8], name: &str) -> Option<&'a [u8]> {
    let u16_at = |at: usize| -> Option<usize> {
        Some(usize::from(u16::from_le_bytes(
            package.get(at..at + 2)?.try_into().ok()?,
        )))
    };
    let u32_at = |at: usize| -> Option<usize> {
        usize::try_from(u32::from_le_bytes(
            package.get(at..at + 4)?.try_into().ok()?,
        ))
        .ok()
    };
    let mut at = 0;
    while u32_at(at)? == 0x0403_4b50 {
        let method = u16_at(at + 8)?;
        let size = u32_at(at + 18)?;
        let name_length = u16_at(at + 26)?;
        let extra_length = u16_at(at + 28)?;
        let entry_name = package.get(at + 30..at + 30 + name_length)?;
        let content_at = at + 30 + name_length + extra_length;
        let content = package.get(content_at..content_at + size)?;
        if entry_name == name.as_bytes() {
            return (method == 0).then_some(content);
        }
        at = content_at + size;
    }
    None
}

/// The first element `<tag .../>` of `part`, as written, or `None` when `part` holds none.
pub fn empty_element<'a>(part: &'a str, tag: &str) -> Option<&'a str> {
    let start = part.find(&format!("<{tag} "))?;
    let end = start + part[start..].find("/>")? + 2;
    Some(&part[start..end])
}
