//! A worksheet substream of a BIFF8 file (MS-XLS 2.1.7.20.5), read into the SpreadsheetML
//! worksheet part it stands for: rows (ROW), columns (COLINFO) and cells (NUMBER, RK, MULRK,
//! LABELSST, LABEL, BOOLERR, BLANK, MULBLANK, FORMULA with its STRING). Records this reader does
//! not read are counted by type in [`SheetPart::unknown`].

use super::records::Record;
use super::strings::Pieces;
use crate::xlsb::{Cursor, Xml, formula};
use std::collections::BTreeMap;

const DIMENSIONS: u16 = 0x0200;
const ROW: u16 = 0x0208;
const COLINFO: u16 = 0x007D;
const DEFAULT_ROW_HEIGHT: u16 = 0x0225;
const BLANK: u16 = 0x0201;
const NUMBER: u16 = 0x0203;
const LABEL: u16 = 0x0204;
const BOOLERR: u16 = 0x0205;
const FORMULA: u16 = 0x0006;
const STRING: u16 = 0x0207;
const RK: u16 = 0x027E;
const MULRK: u16 = 0x00BD;
const MULBLANK: u16 = 0x00BE;
const LABELSST: u16 = 0x00FD;

/// Where a sheet's cells are read from and what they need from the workbook.
pub(crate) struct Context<'a> {
    pub(crate) strings: &'a [super::strings::RichString],
}

/// A cell's content as written to SpreadsheetML.
#[derive(Clone, Debug)]
enum Content {
    Number(f64),
    SharedString(u32),
    Text(Vec<u16>),
    Bool(bool),
    Error(u8),
}

#[derive(Default)]
struct RowData {
    attributes: Vec<(&'static str, String)>,
    cells: BTreeMap<u16, Content>,
}

/// The worksheet part of one sheet substream, and what in it was not read.
pub(crate) struct SheetPart {
    pub(crate) xml: Vec<u8>,
    pub(crate) unknown: BTreeMap<u16, u32>,
}

/// The A1 name of a column (0-based).
pub(crate) fn column_name(column: u32) -> String {
    let mut name = Vec::new();
    let mut n = column + 1;
    while n > 0 {
        n -= 1;
        name.push(b'A' + (n % 26) as u8);
        n /= 26;
    }
    name.reverse();
    String::from_utf8(name).expect("ASCII")
}

fn cell_head(c: &mut Cursor<'_>) -> Result<(u16, u16, u16), String> {
    let row = c.u16("a cell").map_err(|e| e.0)?;
    let column = c.u16("a cell").map_err(|e| e.0)?;
    let style = c.u16("a cell").map_err(|e| e.0)?;
    Ok((row, column, style))
}

fn e(m: crate::xlsb::Malformed) -> String {
    m.0
}

/// Reads the substream `records` (BOF to its EOF) of a worksheet or macro sheet. `macro_sheet`
/// selects the document element.
pub(crate) fn read(
    records: &[Record<'_>],
    context: &Context<'_>,
    macro_sheet: bool,
    sheet_name: &str,
) -> Result<SheetPart, String> {
    let mut rows: BTreeMap<u16, RowData> = BTreeMap::new();
    let mut columns: Vec<Vec<(&'static str, String)>> = Vec::new();
    let mut unknown: BTreeMap<u16, u32> = BTreeMap::new();
    let mut default_height: u16 = 255;
    let mut default_row_attributes: Vec<(&'static str, String)> = Vec::new();
    let mut depth = 0usize;
    let put = |rows: &mut BTreeMap<u16, RowData>, row: u16, column: u16, content: Content| {
        rows.entry(row).or_default().cells.insert(column, content);
    };
    // Rows are known before the cells that fill them: ROW records come first, but a cell may
    // lie in a row with no ROW record, so cells create theirs.
    for record in records {
        if depth > 1 {
            // Inside a nested substream (an embedded chart): skipped, counted.
            match record.id {
                super::workbook::BOF => depth += 1,
                super::workbook::EOF => depth -= 1,
                id => *unknown.entry(id).or_insert(0) += 1,
            }
            continue;
        }
        match record.id {
            super::workbook::BOF => {
                depth += 1;
            }
            super::workbook::EOF => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    break;
                }
            }
            DIMENSIONS => {}
            DEFAULT_ROW_HEIGHT => {
                let mut c = Cursor::new(record.body);
                let flags = c.u16("DefaultRowHeight").map_err(e)?;
                default_height = c.u16("DefaultRowHeight").map_err(e)?;
                if flags & 0x01 != 0 {
                    default_row_attributes.push(("customHeight", "1".to_owned()));
                }
                if flags & 0x02 != 0 {
                    default_row_attributes.push(("zeroHeight", "1".to_owned()));
                }
            }
            ROW => {
                let mut c = Cursor::new(record.body);
                let row = c.u16("Row").map_err(e)?;
                c.skip(4, "Row").map_err(e)?;
                let height = c.u16("Row").map_err(e)? & 0x7FFF;
                c.skip(4, "Row").map_err(e)?;
                let flags = c.u32("Row").map_err(e)?;
                let mut attributes = Vec::new();
                let custom_height = flags & 0x40 != 0;
                if custom_height || height != default_height {
                    attributes.push(("ht", formula::number_text(f64::from(height) / 20.0)));
                }
                if flags & 0x20 != 0 {
                    attributes.push(("hidden", "1".to_owned()));
                }
                if custom_height {
                    attributes.push(("customHeight", "1".to_owned()));
                }
                let level = flags & 0x7;
                if level != 0 {
                    attributes.push(("outlineLevel", level.to_string()));
                }
                if flags & 0x10 != 0 {
                    attributes.push(("collapsed", "1".to_owned()));
                }
                rows.entry(row).or_default().attributes = attributes;
            }
            COLINFO => {
                let mut c = Cursor::new(record.body);
                let first = c.u16("ColInfo").map_err(e)?;
                let last = c.u16("ColInfo").map_err(e)?;
                let width = c.u16("ColInfo").map_err(e)?;
                let _style = c.u16("ColInfo").map_err(e)?;
                let flags = c.u16("ColInfo").map_err(e)?;
                let mut attributes = vec![
                    ("min", (u32::from(first) + 1).to_string()),
                    ("max", (u32::from(last.min(255)) + 1).to_string()),
                    ("width", formula::number_text(f64::from(width) / 256.0)),
                ];
                if flags & 0x01 != 0 {
                    attributes.push(("hidden", "1".to_owned()));
                }
                if flags & 0x04 != 0 {
                    attributes.push(("bestFit", "1".to_owned()));
                }
                if flags & 0x02 != 0 {
                    attributes.push(("customWidth", "1".to_owned()));
                }
                let level = (flags >> 8) & 0x7;
                if level != 0 {
                    attributes.push(("outlineLevel", level.to_string()));
                }
                if flags & (1 << 12) != 0 {
                    attributes.push(("collapsed", "1".to_owned()));
                }
                columns.push(attributes);
            }
            NUMBER => {
                let mut c = Cursor::new(record.body);
                let (row, column, _) = cell_head(&mut c)?;
                put(&mut rows, row, column, Content::Number(c.f64("Number").map_err(e)?));
            }
            RK => {
                let mut c = Cursor::new(record.body);
                let (row, column, _) = cell_head(&mut c)?;
                put(&mut rows, row, column, Content::Number(c.rk("RK").map_err(e)?));
            }
            MULRK => {
                let mut c = Cursor::new(record.body);
                let row = c.u16("MulRk").map_err(e)?;
                let first = c.u16("MulRk").map_err(e)?;
                // (ixfe, rk) pairs, then the last column.
                let pairs = c.remaining().saturating_sub(2) / 6;
                for k in 0..pairs {
                    c.skip(2, "MulRk").map_err(e)?;
                    let value = c.rk("MulRk").map_err(e)?;
                    put(&mut rows, row, first.saturating_add(k as u16), Content::Number(value));
                }
            }
            LABELSST => {
                let mut c = Cursor::new(record.body);
                let (row, column, _) = cell_head(&mut c)?;
                let index = c.u32("LabelSst").map_err(e)?;
                if index as usize >= context.strings.len() {
                    return Err(format!(
                        "LABELSST names string {index} of {} in the shared string table",
                        context.strings.len()
                    ));
                }
                put(&mut rows, row, column, Content::SharedString(index));
            }
            LABEL => {
                let mut c = Cursor::new(&record.body[..record.body.len().min(6)]);
                let (row, column, _) = cell_head(&mut c)?;
                let mut p = Pieces::new(record);
                p.skip(6, "Label").map_err(|m| m.0)?;
                let units = p.unicode_string("a label").map_err(|m| m.0)?;
                put(&mut rows, row, column, Content::Text(units));
            }
            BOOLERR => {
                let mut c = Cursor::new(record.body);
                let (row, column, _) = cell_head(&mut c)?;
                let value = c.u8("BoolErr").map_err(e)?;
                let is_error = c.u8("BoolErr").map_err(e)? != 0;
                let content = if is_error {
                    Content::Error(value)
                } else {
                    Content::Bool(value != 0)
                };
                put(&mut rows, row, column, content);
            }
            BLANK | MULBLANK => {
                // A formatted blank cell carries no value; its format is read with the styles.
            }
            FORMULA => {
                return Err(format!(
                    "a formula cell on sheet {sheet_name:?}, which dc-4j0e.3 reads"
                ));
            }
            STRING => {}
            other => *unknown.entry(other).or_insert(0) += 1,
        }
    }

    let mut x = Xml::new();
    let root = if macro_sheet { "xm:macrosheet" } else { "worksheet" };
    let mut namespaces = vec![("xmlns", super::MAIN_NAMESPACE.to_owned())];
    if macro_sheet {
        namespaces.push((
            "xmlns:xm",
            "http://schemas.microsoft.com/office/excel/2006/main".to_owned(),
        ));
    }
    x.open(root, &namespaces);
    let mut format = vec![("defaultRowHeight", formula::number_text(f64::from(default_height) / 20.0))];
    format.extend(default_row_attributes);
    x.empty("sheetFormatPr", &format);
    if !columns.is_empty() {
        x.open("cols", &[]);
        for attributes in &columns {
            x.empty("col", attributes);
        }
        x.close("cols");
    }
    x.open("sheetData", &[]);
    for (row, data) in &rows {
        let mut attributes = vec![("r", (u32::from(*row) + 1).to_string())];
        attributes.extend(data.attributes.iter().cloned());
        if data.cells.is_empty() {
            x.empty("row", &attributes);
            continue;
        }
        x.open("row", &attributes);
        for (column, content) in &data.cells {
            let reference = format!("{}{}", column_name(u32::from(*column)), u32::from(*row) + 1);
            match content {
                Content::Number(v) => {
                    x.open("c", &[("r", reference)]);
                    x.element("v", &[], &formula::number_text(*v));
                    x.close("c");
                }
                Content::SharedString(i) => {
                    x.open("c", &[("r", reference), ("t", "s".to_owned())]);
                    x.element("v", &[], &i.to_string());
                    x.close("c");
                }
                Content::Text(units) => {
                    x.open("c", &[("r", reference), ("t", "inlineStr".to_owned())]);
                    x.open("is", &[]);
                    text_element(&mut x, units);
                    x.close("is");
                    x.close("c");
                }
                Content::Bool(b) => {
                    x.open("c", &[("r", reference), ("t", "b".to_owned())]);
                    x.element("v", &[], if *b { "1" } else { "0" });
                    x.close("c");
                }
                Content::Error(code) => {
                    let text = formula::error_text(*code).ok_or_else(|| {
                        format!("the error code 0x{code:02X} on sheet {sheet_name:?}")
                    })?;
                    x.open("c", &[("r", reference), ("t", "e".to_owned())]);
                    x.element("v", &[], text);
                    x.close("c");
                }
            }
        }
        x.close("row");
    }
    x.close("sheetData");
    x.close(root);
    Ok(SheetPart { xml: x.out, unknown })
}

/// `<t>` with `xml:space="preserve"` when the text starts or ends with a space.
pub(crate) fn text_element(x: &mut Xml, units: &[u16]) {
    let padded = units.first() == Some(&0x20) || units.last() == Some(&0x20);
    if padded {
        x.open("t", &[("xml:space", "preserve".to_owned())]);
    } else {
        x.open("t", &[]);
    }
    x.text(units);
    x.close("t");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn column_names() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(255), "IV");
    }
}
