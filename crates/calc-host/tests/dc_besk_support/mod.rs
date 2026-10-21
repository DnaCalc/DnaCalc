//! The dc-besk fixture: a two-sheet workbook package generated here, byte for byte, with no
//! calc-doc code (the ZIP writer is the chost-m1.1 support module's), whose formulas spell
//! functions as a file stores them: with Excel's `_xlfn.` prefix, and in the other spellings whose
//! reading on load the case shows.

use crate::chost_m1_1_support::stored_zip;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const RELATIONSHIP_ATTRIBUTES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const WORKSHEET: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";

/// The part the Calc sheet's formulas and their caches are read from.
pub const CALC_PART: &str = "xl/worksheets/sheet2.xml";

/// The Data sheet: A1:A3 = 1, 5, 3; B1:B3 = the texts "x", "y", "x"; C1 = the error #N/A.
pub const DATA_SHEET: &str = r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1" t="inlineStr"><is><t>x</t></is></c><c r="C1" t="e"><v>#N/A</v></c></row><row r="2"><c r="A2"><v>5</v></c><c r="B2" t="inlineStr"><is><t>y</t></is></c></row><row r="3"><c r="A3"><v>3</v></c><c r="B3" t="inlineStr"><is><t>x</t></is></c></row></sheetData></worksheet>"#;

/// The Calc sheet's formulas, one per row of column A from row 1, each as the `<f>` element holds
/// it, with the number the file caches for it. Every cache is deliberately wrong (999, or 1 for
/// the unknown name), so a file cache reported as calculated fails the case.
pub const CALC_ROWS: [(&str, f64); 20] = [
    (r#"_xlfn.MAXIFS(Data!A1:A3,Data!B1:B3,"x")"#, 999.0),
    (r#"_xlfn.MINIFS(Data!A1:A3,Data!B1:B3,"x")"#, 999.0),
    (
        r#"_xlfn.MAXIFS(Data!A1:A3,Data!B1:B3,"x")*10+_xlfn.MINIFS(Data!A1:A3,Data!B1:B3,"x")"#,
        999.0,
    ),
    ("_xlfn.IFNA(Data!C1,7)", 999.0),
    ("_xlfn.NOSUCHFUNCTION(Data!A1)", 1.0),
    (r#"_xlfn._xlws.MAXIFS(Data!A1:A3,Data!B1:B3,"x")"#, 999.0),
    (r#"_XLFN.MAXIFS(Data!A1:A3,Data!B1:B3,"x")"#, 999.0),
    (r#"_xlfn.maxifs(Data!A1:A3,Data!B1:B3,"x")"#, 999.0),
    (r#"_XLFN._XLWS.MAXIFS(Data!A1:A3,Data!B1:B3,"x")"#, 999.0),
    ("_xlfn.SUM(Data!A1:A3)", 999.0),
    (r#"_xlfn.IF(Data!A2,"big","small")"#, 999.0),
    (r#"_xlws.MAXIFS(Data!A1:A3,Data!B1:B3,"x")"#, 999.0),
    (r#"_xlfn._xlfn.MAXIFS(Data!A1:A3,Data!B1:B3,"x")"#, 999.0),
    (r#"_xlws._xlfn.MAXIFS(Data!A1:A3,Data!B1:B3,"x")"#, 999.0),
    (r#"MAXIFS(Data!A1:A3,Data!B1:B3,"x")"#, 999.0),
    ("IFNA(Data!C1,7)", 999.0),
    (r#"_XlFn.MAXIFS(Data!A1:A3,Data!B1:B3,"x")"#, 999.0),
    (r#"_xlfn._XLWS.MAXIFS(Data!A1:A3,Data!B1:B3,"x")"#, 999.0),
    (r#"_XLFN._xlws.MAXIFS(Data!A1:A3,Data!B1:B3,"x")"#, 999.0),
    (r#"_xLfN._xLwS.MAXIFS(Data!A1:A3,Data!B1:B3,"x")"#, 999.0),
];

/// The Calc sheet's part: [`CALC_ROWS`] in column A, none of whose texts holds a character XML
/// escapes in element text.
pub fn calc_sheet() -> String {
    let mut rows = String::new();
    for (index, (formula, cache)) in CALC_ROWS.iter().enumerate() {
        assert!(
            !formula.contains(['&', '<', '>']),
            "{formula}: written without escapes"
        );
        let row = index + 1;
        rows.push_str(&format!(
            r#"<row r="{row}"><c r="A{row}"><f>{formula}</f><v>{cache}</v></c></row>"#
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="{MAIN}"><sheetData>{rows}</sheetData></worksheet>"#
    )
}

/// The package: the workbook part names the sheets Data (sheetId 1) and Calc (sheetId 2), in that
/// order.
pub fn package() -> Vec<u8> {
    let content_types = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/{CALC_PART}" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#
    );
    let relationships = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{OFFICE_DOCUMENT}" Target="xl/workbook.xml"/></Relationships>"#
    );
    let workbook = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="{MAIN}" xmlns:r="{RELATIONSHIP_ATTRIBUTES}"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="Calc" sheetId="2" r:id="rId2"/></sheets></workbook>"#
    );
    let workbook_relationships = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{WORKSHEET}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{WORKSHEET}" Target="worksheets/sheet2.xml"/></Relationships>"#
    );
    let calc = calc_sheet();
    stored_zip(&[
        ("[Content_Types].xml", content_types.as_bytes()),
        ("_rels/.rels", relationships.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        (
            "xl/_rels/workbook.xml.rels",
            workbook_relationships.as_bytes(),
        ),
        ("xl/worksheets/sheet1.xml", DATA_SHEET.as_bytes()),
        (CALC_PART, calc.as_bytes()),
    ])
}
