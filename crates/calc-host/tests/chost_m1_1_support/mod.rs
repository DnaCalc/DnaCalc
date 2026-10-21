//! The chost-m1.1 fixture: a two-sheet workbook package generated here, byte for byte, with no
//! calc-doc code. Its formula cells carry deliberately wrong cached results (and one carries
//! none), so a cache that is reported as calculated is caught. The ZIP writer stores every entry
//! uncompressed and computes each entry's CRC-32 here, so the case knows the identity of the part
//! every cache must name without reading it back through calc-doc.

/// The CRC-32 (IEEE 802.3, reflected) of `data`, bit by bit.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn push16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// An archive of `entries` (name, content), each stored uncompressed, in the order given.
pub fn stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    // DOS date 1980-01-01, time 00:00.
    const DATE: u16 = 0x21;
    let mut out = Vec::new();
    let mut offsets = Vec::new();
    for (name, content) in entries {
        offsets.push(out.len() as u32);
        push32(&mut out, 0x0403_4b50);
        for field in [20u16, 0, 0, 0, DATE] {
            push16(&mut out, field);
        }
        push32(&mut out, crc32(content));
        push32(&mut out, content.len() as u32);
        push32(&mut out, content.len() as u32);
        push16(&mut out, name.len() as u16);
        push16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(content);
    }
    let directory = out.len();
    for ((name, content), offset) in entries.iter().zip(offsets) {
        push32(&mut out, 0x0201_4b50);
        for field in [20u16, 20, 0, 0, 0, DATE] {
            push16(&mut out, field);
        }
        push32(&mut out, crc32(content));
        push32(&mut out, content.len() as u32);
        push32(&mut out, content.len() as u32);
        for field in [name.len() as u16, 0, 0, 0, 0] {
            push16(&mut out, field);
        }
        push32(&mut out, 0);
        push32(&mut out, offset);
        out.extend_from_slice(name.as_bytes());
    }
    let size = (out.len() - directory) as u32;
    push32(&mut out, 0x0605_4b50);
    for field in [0u16, 0, entries.len() as u16, entries.len() as u16] {
        push16(&mut out, field);
    }
    push32(&mut out, size);
    push32(&mut out, directory as u32);
    push16(&mut out, 0);
    out
}

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const RELATIONSHIP_ATTRIBUTES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const WORKSHEET: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";

/// The part every cache of the Calc sheet is read from.
pub const CALC_PART: &str = "xl/worksheets/sheet2.xml";

/// The Inputs sheet: A1 = 2, A2 = 3, A3 = the text "rate", B1 = 0.5.
pub const INPUTS_SHEET: &str = r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><v>2</v></c><c r="B1"><v>0.5</v></c></row><row r="2"><c r="A2"><v>3</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>rate</t></is></c></row></sheetData></worksheet>"#;

/// B1 calls a name no catalogue holds (Excel shows #NAME? for it), so it stays not modelled as
/// functions are added to calc-func.
/// The Calc sheet's formulas, each with the result the file caches for it:
///
/// | cell | formula                         | cached  | value from the inputs above |
/// |------|---------------------------------|---------|-----------------------------|
/// | A1   | `Inputs!A1+Inputs!A2`           | 41      | 5                           |
/// | B1   | `NOSUCHFUNCTION(Inputs!B1,0)`   | 1       | not in the catalogue        |
/// | C1   | `IF(Inputs!A1>1,"big","small")` | "small" | "big"                       |
/// | A2   | `A1*10`                         | 999     | 50                          |
/// | B2   | `Inputs!B1*4`                   | none    | 2                           |
/// | A3   | `SUM(Inputs!A1:A2)*Inputs!B1`   | 7       | 2.5                         |
pub const CALC_SHEET: &str = r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><f>Inputs!A1+Inputs!A2</f><v>41</v></c><c r="B1"><f>NOSUCHFUNCTION(Inputs!B1,0)</f><v>1</v></c><c r="C1" t="str"><f>IF(Inputs!A1&gt;1,"big","small")</f><v>small</v></c></row><row r="2"><c r="A2"><f>A1*10</f><v>999</v></c><c r="B2"><f>Inputs!B1*4</f></c></row><row r="3"><c r="A3"><f>SUM(Inputs!A1:A2)*Inputs!B1</f><v>7</v></c></row></sheetData></worksheet>"#;

/// The package: the workbook part names the sheets Inputs (sheetId 1) and Calc (sheetId 2), in
/// that order.
pub fn package() -> Vec<u8> {
    package_of(INPUTS_SHEET)
}

/// The package with `inputs` as the Inputs sheet's part.
pub fn package_of(inputs: &str) -> Vec<u8> {
    let content_types = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/{CALC_PART}" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#
    );
    let relationships = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{OFFICE_DOCUMENT}" Target="xl/workbook.xml"/></Relationships>"#
    );
    let workbook = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="{MAIN}" xmlns:r="{RELATIONSHIP_ATTRIBUTES}"><sheets><sheet name="Inputs" sheetId="1" r:id="rId1"/><sheet name="Calc" sheetId="2" r:id="rId2"/></sheets></workbook>"#
    );
    let workbook_relationships = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{WORKSHEET}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{WORKSHEET}" Target="worksheets/sheet2.xml"/></Relationships>"#
    );
    stored_zip(&[
        ("[Content_Types].xml", content_types.as_bytes()),
        ("_rels/.rels", relationships.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        (
            "xl/_rels/workbook.xml.rels",
            workbook_relationships.as_bytes(),
        ),
        ("xl/worksheets/sheet1.xml", inputs.as_bytes()),
        (CALC_PART, CALC_SHEET.as_bytes()),
    ])
}
