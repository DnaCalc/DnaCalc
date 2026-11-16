//! A worksheet substream of a BIFF8 file (MS-XLS 2.1.7.20.5), read into the SpreadsheetML
//! worksheet part it stands for: rows (ROW), columns (COLINFO) and cells (NUMBER, RK, MULRK,
//! LABELSST, LABEL, BOOLERR, BLANK, MULBLANK, FORMULA with its STRING). Records this reader does
//! not read are counted by type in [`SheetPart::unknown`].

use super::records::Record;
use super::strings::Pieces;
use super::formula::Links;
use crate::xlsb::formula::{Origin, print};
use crate::xlsb::{Cursor, Xml, formula};
use std::collections::BTreeMap;

const DIMENSIONS: u16 = 0x0200;
const ROW: u16 = 0x0208;
const COLINFO: u16 = 0x007D;
const GCW: u16 = 0x00AB;
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
const RSTRING: u16 = 0x00D6;
const SHRFMLA: u16 = 0x04BC;
const ARRAY: u16 = 0x0221;
const TABLE: u16 = 0x0236;

/// Where a sheet's cells are read from and what they need from the workbook.
pub(crate) struct Context<'a> {
    pub(crate) strings: &'a [super::strings::RichString],
    pub(crate) links: &'a Links,
    pub(crate) styles: &'a super::styles::Styles,
    /// The tab index of the sheet being read.
    pub(crate) tab: u32,
    pub(crate) number: usize,
    /// The code page of a BIFF5 or BIFF7 workbook, whose strings are bytes in it.
    pub(crate) v5: Option<u16>,
    /// The pictures of the workbook's blip store.
    pub(crate) images: &'a [Option<super::drawing::Image>],
    /// The range of the sheet's `_FilterDatabase` name (its automatic filter).
    pub(crate) filter_range: Option<String>,
}

/// What a formula cell's `<f>` element holds.
#[derive(Clone, Debug)]
enum Formula {
    Plain { text: String, needs_calc: bool },
    /// The anchor of an array formula over `range`.
    Array { range: String, text: String, needs_calc: bool },
    /// The first cell of a data table: the attributes of its `<f>`.
    Table(Vec<(&'static str, String)>),
    /// A cell an array formula or a data table fills: its value only.
    Member,
}

#[derive(Clone, Debug)]
enum Cached {
    Number(f64),
    Text(Vec<u16>),
    Bool(bool),
    Error(u8),
}

/// A shared or array formula record: its range, tokens and extra data.
struct Group {
    rows: (u16, u16),
    columns: (u8, u8),
    rgce: Vec<u8>,
    rgcb: Vec<u8>,
    needs_calc: bool,
}

fn group(body: &[u8], array: bool) -> Result<Group, String> {
    let mut c = Cursor::new(body);
    let rows = (c.u16("a formula group").map_err(e)?, c.u16("a formula group").map_err(e)?);
    let columns = (c.u8("a formula group").map_err(e)?, c.u8("a formula group").map_err(e)?);
    // ARRAY: calculation flags and 4 unused bytes; SHRFMLA: unused byte and use count.
    let needs_calc = if array {
        let flags = c.u16("a formula group").map_err(e)?;
        c.skip(4, "a formula group").map_err(e)?;
        flags & 1 != 0
    } else {
        c.skip(2, "a formula group").map_err(e)?;
        false
    };
    let count = usize::from(c.u16("a formula group").map_err(e)?);
    let rgce = c.bytes(count, "a formula group").map_err(e)?.to_vec();
    let rgcb = c.bytes(c.remaining(), "a formula group").map_err(e)?.to_vec();
    Ok(Group {
        rows,
        columns,
        rgce,
        rgcb,
        needs_calc,
    })
}

/// A cell's content as written to SpreadsheetML.
#[derive(Clone, Debug)]
enum Content {
    Number(f64),
    SharedString(u32),
    Text(Vec<u16>),
    Bool(bool),
    Error(u8),
    Formula(Formula, Cached),
    /// A cell with a format and no value.
    Blank,
}

#[derive(Default)]
struct RowData {
    attributes: Vec<(&'static str, String)>,
    cells: BTreeMap<u16, (u16, Content)>,
}

/// The worksheet part of one sheet substream, and what in it was not read.
pub(crate) struct SheetPart {
    pub(crate) xml: Vec<u8>,
    pub(crate) unknown: BTreeMap<u16, u32>,
    /// The relationships of the sheet part (hyperlink targets).
    pub(crate) rels: Vec<super::features::Rel>,
    /// Parts the sheet owns: its comments and their VML drawing.
    pub(crate) parts: Vec<(String, &'static str, Vec<u8>)>,
    /// Relationship parts of those (the drawing's).
    pub(crate) entries: Vec<(String, Vec<u8>)>,
    /// Drawing objects not written, by OBJ type.
    pub(crate) unread_objects: BTreeMap<u16, u32>,
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
    let mut default_column_width: Option<u16> = None;
    let mut column_widths: Vec<(u16, u16, u16)> = Vec::new();
    let mut global_column_widths: Vec<[u8; 32]> = Vec::new();
    let mut row_heights: BTreeMap<u16, f64> = BTreeMap::new();
    let mut default_row_attributes: Vec<(&'static str, String)> = Vec::new();
    let mut depth = 0usize;
    let mut in_custom_view = false;
    let mut features = super::features::Features::default();
    let mut drawing = super::drawing::Drawing::default();
    let mut rules = super::rules::Rules::default();
    features.number = context.number;
    features.v5 = context.v5;
    // Shared and array formulas and data tables, by the cell of the FORMULA record they follow
    // (where a PtgExp finds them) and, for shared formulas, by the first cell of their range.
    let mut shared: BTreeMap<(u32, u32), Group> = BTreeMap::new();
    let mut arrays: BTreeMap<(u32, u32), Group> = BTreeMap::new();
    let mut tables: BTreeMap<(u32, u32), Vec<(&'static str, String)>> = BTreeMap::new();
    for (i, record) in records.iter().enumerate() {
        if !matches!(record.id, SHRFMLA | ARRAY | TABLE) {
            continue;
        }
        let mut j = i;
        while j > 0 && matches!(records[j - 1].id, SHRFMLA | ARRAY | TABLE) {
            j -= 1;
        }
        let Some(previous) = j.checked_sub(1).map(|j| &records[j]).filter(|r| r.id == FORMULA) else {
            return Err(format!("a formula group record on sheet {sheet_name:?} that follows no formula"));
        };
        let mut c = Cursor::new(previous.body);
        let at = (
            u32::from(c.u16("Formula").map_err(e)?),
            u32::from(c.u16("Formula").map_err(e)?),
        );
        let body = record.joined();
        match record.id {
            SHRFMLA => {
                let g = group(&body, false)?;
                shared.insert((u32::from(g.rows.0), u32::from(g.columns.0)), group(&body, false)?);
                shared.insert(at, g);
            }
            ARRAY => {
                arrays.insert(at, group(&body, true)?);
            }
            _ => {
                let mut c = Cursor::new(&body);
                let first_row = c.u16("Table").map_err(e)?;
                let last_row = c.u16("Table").map_err(e)?;
                let first_column = c.u8("Table").map_err(e)?;
                let last_column = c.u8("Table").map_err(e)?;
                let flags = c.u16("Table").map_err(e)?;
                let row_input = (c.u16("Table").map_err(e)?, c.u16("Table").map_err(e)?);
                let column_input = (c.u16("Table").map_err(e)?, c.u16("Table").map_err(e)?);
                let two = flags & 0x08 != 0;
                let mut attributes = vec![
                    ("t", "dataTable".to_owned()),
                    (
                        "ref",
                        format!(
                            "{}{}:{}{}",
                            column_name(u32::from(first_column)),
                            u32::from(first_row) + 1,
                            column_name(u32::from(last_column)),
                            u32::from(last_row) + 1
                        ),
                    ),
                ];
                if two {
                    attributes.push(("dt2D", "1".to_owned()));
                }
                if flags & 0x04 != 0 {
                    attributes.push(("dtr", "1".to_owned()));
                }
                if flags & 0x10 != 0 {
                    attributes.push(("del1", "1".to_owned()));
                }
                if flags & 0x20 != 0 {
                    attributes.push(("del2", "1".to_owned()));
                }
                let cell = |p: (u16, u16)| format!("{}{}", column_name(u32::from(p.1)), u32::from(p.0) + 1);
                attributes.push(("r1", cell(row_input)));
                if two {
                    attributes.push(("r2", cell(column_input)));
                }
                tables.insert(at, attributes);
            }
        }
    }
    let put = |rows: &mut BTreeMap<u16, RowData>, row: u16, column: u16, ixfe: u16, content: Content| {
        rows.entry(row).or_default().cells.insert(column, (ixfe, content));
    };
    // Rows are known before the cells that fill them: ROW records come first, but a cell may
    // lie in a row with no ROW record, so cells create theirs.
    for (index, record) in records.iter().enumerate() {
        if depth > 1 {
            // Inside a nested substream (an embedded chart): read by the chart's object.
            match record.id {
                super::workbook::BOF => depth += 1,
                super::workbook::EOF => depth -= 1,
                _ => {
                    let print = |rgce: &[u8]| -> Option<String> {
                        let context = context.links.context(Some(context.tab));
                        match print(rgce, &[], &context, Origin::default()) {
                            Ok(Ok(text)) => Some(text),
                            _ => None,
                        }
                    };
                    drawing.chart_record(record, &print);
                }
            }
            continue;
        }
        if depth == 1 && record.id == 0x01AA {
            if in_custom_view { return Err(format!("a nested custom sheet view on {sheet_name:?}")); }
            in_custom_view = true;
            *unknown.entry(record.id).or_insert(0) += 1;
            continue;
        }
        if depth == 1 && record.id == 0x01AB && in_custom_view {
            in_custom_view = false;
            *unknown.entry(record.id).or_insert(0) += 1;
            continue;
        }
        if depth == 1 && in_custom_view {
            if record.id == super::workbook::EOF {
                return Err(format!("an unterminated custom sheet view on {sheet_name:?}"));
            }
            *unknown.entry(record.id).or_insert(0) += 1;
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
            // Shared formula definitions were parsed in the group pass above and expanded into
            // each member's formula. They are not unread records.
            SHRFMLA => {}
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
                if flags & 0x04 != 0 {
                    default_row_attributes.push(("thickTop", "1".to_owned()));
                }
                if flags & 0x08 != 0 {
                    default_row_attributes.push(("thickBottom", "1".to_owned()));
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
                if flags & (1 << 28) != 0 {
                    attributes.push(("thickTop", "1".to_owned()));
                }
                if flags & (1 << 29) != 0 {
                    attributes.push(("thickBot", "1".to_owned()));
                }
                if flags & (1 << 30) != 0 {
                    attributes.push(("ph", "1".to_owned()));
                }
                if flags & 0x80 != 0 {
                    let style = context.styles.cell_format(((flags >> 16) & 0x0FFF) as u16);
                    if style != 0 {
                        attributes.push(("s", style.to_string()));
                    }
                    attributes.push(("customFormat", "1".to_owned()));
                }
                row_heights.insert(row, f64::from(height) / 20.0);
                rows.entry(row).or_default().attributes = attributes;
            }
            COLINFO => {
                let mut c = Cursor::new(record.body);
                let first = c.u16("ColInfo").map_err(e)?;
                let last = c.u16("ColInfo").map_err(e)?;
                let width = c.u16("ColInfo").map_err(e)?;
                let ixfe = c.u16("ColInfo").map_err(e)?;
                let flags = c.u16("ColInfo").map_err(e)?;
                let mut attributes = vec![
                    ("min", (u32::from(first) + 1).to_string()),
                    ("max", (u32::from(last.min(255)) + 1).to_string()),
                    ("width", formula::number_text(f64::from(width) / 256.0)),
                ];
                let style = context.styles.cell_format(ixfe);
                if style != 0 {
                    attributes.push(("style", style.to_string()));
                }
                if flags & 0x01 != 0 {
                    attributes.push(("hidden", "1".to_owned()));
                }
                if flags & 0x04 != 0 {
                    attributes.push(("bestFit", "1".to_owned()));
                }
                if flags & 0x08 != 0 {
                    attributes.push(("phonetic", "1".to_owned()));
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
                column_widths.push((first, last.min(255), width));
                columns.push(attributes);
            }
            GCW if context.v5.is_some() && record.body.len() == 34 && record.body[..2] == [32, 0] => {
                let mut bits = [0u8; 32];
                bits.copy_from_slice(&record.body[2..]);
                global_column_widths.push(bits);
            }
            NUMBER => {
                let mut c = Cursor::new(record.body);
                let (row, column, ixfe) = cell_head(&mut c)?;
                put(&mut rows, row, column, ixfe, Content::Number(c.f64("Number").map_err(e)?));
            }
            RK => {
                let mut c = Cursor::new(record.body);
                let (row, column, ixfe) = cell_head(&mut c)?;
                put(&mut rows, row, column, ixfe, Content::Number(c.rk("RK").map_err(e)?));
            }
            MULRK => {
                let mut c = Cursor::new(record.body);
                let row = c.u16("MulRk").map_err(e)?;
                let first = c.u16("MulRk").map_err(e)?;
                // (ixfe, rk) pairs, then the last column.
                let pairs = c.remaining().saturating_sub(2) / 6;
                for k in 0..pairs {
                    let ixfe = c.u16("MulRk").map_err(e)?;
                    let value = c.rk("MulRk").map_err(e)?;
                    put(&mut rows, row, first.saturating_add(k as u16), ixfe, Content::Number(value));
                }
            }
            LABELSST => {
                let mut c = Cursor::new(record.body);
                let (row, column, ixfe) = cell_head(&mut c)?;
                let index = c.u32("LabelSst").map_err(e)?;
                if index as usize >= context.strings.len() {
                    return Err(format!(
                        "LABELSST names string {index} of {} in the shared string table",
                        context.strings.len()
                    ));
                }
                put(&mut rows, row, column, ixfe, Content::SharedString(index));
            }
            LABEL | RSTRING if context.v5.is_some() => {
                // BIFF5: a 16-bit length and the bytes in the workbook's code page; an RSTRING's
                // formatting runs after them are not read.
                let mut c = Cursor::new(record.body);
                let (row, column, ixfe) = cell_head(&mut c)?;
                let count = usize::from(c.u16("Label").map_err(e)?);
                let bytes = c.bytes(count, "Label").map_err(e)?;
                let text = super::codepage::decode(context.v5.unwrap_or(1252), bytes)?;
                put(&mut rows, row, column, ixfe, Content::Text(text.encode_utf16().collect()));
            }
            LABEL => {
                let mut c = Cursor::new(&record.body[..record.body.len().min(6)]);
                let (row, column, ixfe) = cell_head(&mut c)?;
                let mut p = Pieces::new(record);
                p.skip(6, "Label").map_err(|m| m.0)?;
                let units = p.unicode_string("a label").map_err(|m| m.0)?;
                put(&mut rows, row, column, ixfe, Content::Text(units));
            }
            BOOLERR => {
                let mut c = Cursor::new(record.body);
                let (row, column, ixfe) = cell_head(&mut c)?;
                let value = c.u8("BoolErr").map_err(e)?;
                let is_error = c.u8("BoolErr").map_err(e)? != 0;
                let content = if is_error {
                    Content::Error(value)
                } else {
                    Content::Bool(value != 0)
                };
                put(&mut rows, row, column, ixfe, content);
            }
            BLANK => {
                let mut c = Cursor::new(record.body);
                let (row, column, ixfe) = cell_head(&mut c)?;
                put(&mut rows, row, column, ixfe, Content::Blank);
            }
            MULBLANK => {
                let mut c = Cursor::new(record.body);
                let row = c.u16("MulBlank").map_err(e)?;
                let first = c.u16("MulBlank").map_err(e)?;
                for k in 0..c.remaining().saturating_sub(2) / 2 {
                    let ixfe = c.u16("MulBlank").map_err(e)?;
                    put(&mut rows, row, first.saturating_add(k as u16), ixfe, Content::Blank);
                }
            }
            FORMULA => {
                let body = record.joined();
                let mut c = Cursor::new(&body);
                let (row, column, ixfe) = cell_head(&mut c)?;
                let value = c.bytes(8, "Formula").map_err(e)?.to_vec();
                let flags = c.u16("Formula").map_err(e)?;
                c.skip(4, "Formula").map_err(e)?;
                let needs_calc = flags & 1 != 0;
                // Calculation scheduling and the error-check exclusion are not yet applied by
                // the product, even where the converted formula retains the former flag.
                if flags & 0x21 != 0 { *unknown.entry(FORMULA).or_insert(0) += 1; }
                let count = usize::from(c.u16("Formula").map_err(e)?);
                let rgce = c.bytes(count, "Formula").map_err(e)?;
                let rgcb = c.bytes(c.remaining(), "Formula").map_err(e)?;
                let cached = if value[6] == 0xFF && value[7] == 0xFF {
                    match value[0] {
                        0 | 3 => {
                            // The text is in the STRING record after this formula (and its
                            // shared, array or table record); a type 3 value is the empty text.
                            let mut units = Vec::new();
                            if value[0] == 0 {
                                let next = records[index + 1..]
                                    .iter()
                                    .find(|r| !matches!(r.id, SHRFMLA | ARRAY | TABLE));
                                match next {
                                    Some(r) if r.id == STRING && context.v5.is_some() => {
                                        let mut c = Cursor::new(r.body);
                                        let count = usize::from(c.u16("String").map_err(e)?);
                                        let bytes = c.bytes(count, "String").map_err(e)?;
                                        units = super::codepage::decode(context.v5.unwrap_or(1252), bytes)?
                                            .encode_utf16()
                                            .collect();
                                    }
                                    Some(r) if r.id == STRING => {
                                        let mut p = Pieces::new(r);
                                        units = p.unicode_string("a formula's text").map_err(|m| m.0)?;
                                    }
                                    _ => {
                                        return Err(format!(
                                            "a formula cell on sheet {sheet_name:?} with a text result and no STRING record"
                                        ));
                                    }
                                }
                            }
                            Cached::Text(units)
                        }
                        1 => Cached::Bool(value[2] != 0),
                        2 => Cached::Error(value[2]),
                        other => {
                            return Err(format!("a formula result of type {other} on sheet {sheet_name:?}"));
                        }
                    }
                } else {
                    let mut bytes = [0u8; 8];
                    bytes.copy_from_slice(&value);
                    Cached::Number(f64::from_le_bytes(bytes))
                };
                let at = (u32::from(row), u32::from(column));
                let here = format!("{}{}", column_name(at.1), at.0 + 1);
                let links = context.links.context(Some(context.tab));
                let origin = Origin {
                    row: at.0,
                    column: at.1,
                };
                let fail = |what: String| format!("{what} in the formula of {here} on sheet {sheet_name:?}");
                let formula = if rgce.first() == Some(&0x02) {
                    // PtgTbl: a cell inside a data table; the first one holds the table.
                    match tables.get(&at) {
                        Some(attributes) => {
                            let mut attributes = attributes.clone();
                            if needs_calc { attributes.push(("ca", "1".to_owned())); }
                            Formula::Table(attributes)
                        }
                        None => Formula::Member,
                    }
                } else {
                    match print(rgce, rgcb, &links, origin).map_err(|m| fail(m.0))? {
                        Ok(text) => Formula::Plain { text, needs_calc },
                        Err(expanded) => {
                            let key = (expanded.row, expanded.column);
                            if let Some(array) = arrays.get(&key) {
                                if key == at {
                                    let text = print(&array.rgce, &array.rgcb, &links, origin)
                                        .map_err(|m| fail(m.0))?
                                        .map_err(|_| fail("a PtgExp in an array formula".into()))?;
                                    let range = format!(
                                        "{}{}:{}{}",
                                        column_name(u32::from(array.columns.0)),
                                        u32::from(array.rows.0) + 1,
                                        column_name(u32::from(array.columns.1)),
                                        u32::from(array.rows.1) + 1
                                    );
                                    Formula::Array { range, text, needs_calc: array.needs_calc || needs_calc }
                                } else {
                                    Formula::Member
                                }
                            } else if let Some(group) = shared.get(&key) {
                                let text = print(&group.rgce, &group.rgcb, &links, origin)
                                    .map_err(|m| fail(m.0))?
                                    .map_err(|_| fail("a PtgExp in a shared formula".into()))?;
                                Formula::Plain { text, needs_calc }
                            } else {
                                return Err(fail(format!(
                                    "a PtgExp naming row {} column {}, where no shared or array formula is",
                                    key.0 + 1,
                                    key.1 + 1
                                )));
                            }
                        }
                    }
                };
                put(&mut rows, row, column, ixfe, Content::Formula(formula, cached));
            }
            STRING => {}
            // Workbook calculation settings are collected across sheet substreams.
            0x000C | 0x000D | 0x000F | 0x0010 | 0x0011 | 0x005F => {}
            // BIFF5's local link table: read with the workbook, before the formulas.
            0x0016 | 0x0017 | 0x0023 if context.v5.is_some() => {}
            other => {
                // The drawing sees OBJ before the features do (both read it), and a TXO first.
                let taken = drawing.read(record);
                let print_at = |rgce: &[u8], at: (u16, u16)| -> Option<String> {
                    let links = context.links.context(Some(context.tab));
                    let origin = Origin { row: u32::from(at.0), column: u32::from(at.1) };
                    match print(rgce, &[], &links, origin) {
                        Ok(Ok(text)) => Some(text),
                        _ => None,
                    }
                };
                if !taken && !rules.read(record, context.styles, &print_at, sheet_name, context.v5)? && !features.read(record, context.styles)? {
                    *unknown.entry(other).or_insert(0) += 1;
                }
            }
        }
    }

    if features.printer_part().is_some() {
        if let Some(count) = unknown.get_mut(&0x004D) {
            *count -= 1;
            if *count == 0 { unknown.remove(&0x004D); }
        }
    }
    // In BIFF5 GCW chooses the standard width even where COLINFO provides an explicit width.
    // Emit non-overlapping columns so the selection does not erase their other formatting.
    if let [bits] = global_column_widths.as_slice() {
        let standard = features.standard_column_width;
        let fallback = features.default_column_width.unwrap_or(8).min(255) * 256;
        let mut expanded = BTreeMap::new();
        for attrs in &columns {
            let first = attrs.iter().find(|(key, _)| *key == "min").and_then(|(_, v)| v.parse::<u16>().ok()).unwrap_or(1);
            let last = attrs.iter().find(|(key, _)| *key == "max").and_then(|(_, v)| v.parse::<u16>().ok()).unwrap_or(0).min(256);
            for column in first..=last {
                let mut a = attrs.clone();
                for (key, value) in &mut a {
                    if *key == "min" || *key == "max" { *value = column.to_string(); }
                }
                expanded.insert(column - 1, a);
            }
        }
        for column in 0..256u16 {
            if bits[usize::from(column / 8)] & (1 << (column % 8)) == 0 { continue; }
            if standard.is_none() && !expanded.contains_key(&column) { continue; }
            let width = standard.unwrap_or(fallback);
            let a = expanded.entry(column).or_insert_with(|| vec![
                ("min", (column + 1).to_string()), ("max", (column + 1).to_string()),
            ]);
            a.retain(|(key, _)| !matches!(*key, "width" | "customWidth"));
            a.push(("width", formula::number_text(f64::from(width) / 256.0)));
            a.push(("customWidth", "1".to_owned()));
        }
        columns = expanded.into_values().collect();
        column_widths = columns.iter().filter_map(|a| {
            let column = a.iter().find(|(k, _)| *k == "min")?.1.parse::<u16>().ok()?.checked_sub(1)?;
            let width = a.iter().find(|(k, _)| *k == "width")?.1.parse::<f64>().ok()?;
            Some((column, column, (width * 256.0) as u16))
        }).collect();
    } else if !global_column_widths.is_empty() {
        *unknown.entry(GCW).or_insert(0) += global_column_widths.len() as u32;
    }
    let mut x = Xml::new();
    let root = if macro_sheet { "xm:macrosheet" } else { "worksheet" };
    let mut namespaces = vec![
        ("xmlns", super::MAIN_NAMESPACE.to_owned()),
        ("xmlns:r", super::RELATIONSHIPS_NAMESPACE.to_owned()),
    ];
    if macro_sheet {
        namespaces.push((
            "xmlns:xm",
            "http://schemas.microsoft.com/office/excel/2006/main".to_owned(),
        ));
    }
    x.open(root, &namespaces);
    features.write_properties(&mut x);
    features.write_views(&mut x);
    if let Some(width) = features.default_column_width.filter(|w| *w != 8) {
        default_column_width = Some(width);
    }
    let mut format = vec![("defaultRowHeight", formula::number_text(f64::from(default_height) / 20.0))];
    format.extend(default_row_attributes);
    if let Some(width) = features.standard_column_width.filter(|_| context.v5.is_none()) {
        format.push(("defaultColWidth", formula::number_text(f64::from(width) / 256.0)));
    } else if let Some(width) = default_column_width {
        format.push(("defaultColWidth", width.to_string()));
    }
    if let Some((row, column)) = features.outline_levels {
        if row != 0 { format.push(("outlineLevelRow", row.to_string())); }
        if column != 0 { format.push(("outlineLevelCol", column.to_string())); }
    }
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
        for (column, (ixfe, content)) in &data.cells {
            let style = context.styles.cell_format(*ixfe);
            let phonetic = features.show_phonetic(*row, *column);
            if matches!(content, Content::Blank) && style == 0 && !phonetic {
                continue;
            }
            let reference = format!("{}{}", column_name(u32::from(*column)), u32::from(*row) + 1);
            let attrs = |kind: Option<&str>| -> Vec<(&'static str, String)> {
                let mut a = vec![("r", reference.clone())];
                if style != 0 {
                    a.push(("s", style.to_string()));
                }
                if phonetic { a.push(("ph", "1".to_owned())); }
                if let Some(kind) = kind {
                    a.push(("t", kind.to_owned()));
                }
                a
            };
            match content {
                Content::Blank => x.empty("c", &attrs(None)),
                Content::Number(v) => {
                    x.open("c", &attrs(None));
                    x.element("v", &[], &formula::number_text(*v));
                    x.close("c");
                }
                Content::SharedString(i) => {
                    x.open("c", &attrs(Some("s")));
                    x.element("v", &[], &i.to_string());
                    x.close("c");
                }
                Content::Text(units) => {
                    x.open("c", &attrs(Some("inlineStr")));
                    x.open("is", &[]);
                    text_element(&mut x, units);
                    x.close("is");
                    x.close("c");
                }
                Content::Bool(b) => {
                    x.open("c", &attrs(Some("b")));
                    x.element("v", &[], if *b { "1" } else { "0" });
                    x.close("c");
                }
                Content::Formula(f, cached) => {
                    let mut attributes = attrs(None);
                    let (kind, value): (Option<&str>, String) = match cached {
                        Cached::Number(v) => (None, formula::number_text(*v)),
                        Cached::Text(units) => (Some("str"), String::from_utf16_lossy(units)),
                        Cached::Bool(b) => (Some("b"), if *b { "1" } else { "0" }.to_owned()),
                        Cached::Error(code) => (
                            Some("e"),
                            formula::error_text(*code)
                                .ok_or_else(|| format!("the error code 0x{code:02X} on sheet {sheet_name:?}"))?
                                .to_owned(),
                        ),
                    };
                    if let Some(kind) = kind {
                        attributes.push(("t", kind.to_owned()));
                    }
                    x.open("c", &attributes);
                    match f {
                        Formula::Plain { text, needs_calc } => {
                            let attributes = if *needs_calc { vec![("ca", "1".to_owned())] } else { Vec::new() };
                            x.element("f", &attributes, text)
                        }
                        Formula::Array { range, text, needs_calc } => {
                            let mut a = vec![("t", "array".to_owned()), ("ref", range.clone())];
                            if *needs_calc { a.push(("ca", "1".to_owned())); }
                            x.element("f", &a, text);
                        }
                        Formula::Table(attributes) => x.empty("f", attributes),
                        Formula::Member => {}
                    }
                    x.element("v", &[], &value);
                    x.close("c");
                }
                Content::Error(code) => {
                    let text = formula::error_text(*code).ok_or_else(|| {
                        format!("the error code 0x{code:02X} on sheet {sheet_name:?}")
                    })?;
                    x.open("c", &attrs(Some("e")));
                    x.element("v", &[], text);
                    x.close("c");
                }
            }
        }
        x.close("row");
    }
    x.close("sheetData");
    let mut rels = Vec::new();
    let mut parts = Vec::new();
    let mut entries = Vec::new();
    let mut unread_objects = BTreeMap::new();
    let mut drawing_id = None;
    if !drawing.is_empty() && context.v5.is_none() {
        let width_px = |column: u16| -> f64 {
            let found = column_widths
                .iter()
                .find(|(first, last, _)| (*first..=*last).contains(&column));
            found.map_or(64.0, |(_, _, w)| f64::from(*w) / 256.0 * 7.0 + 0.0)
        };
        let height_pt = |row: u16| -> f64 {
            row_heights.get(&row).copied().unwrap_or(f64::from(default_height) / 20.0)
        };
        let geometry = |column: u16, dx: u16, row: u16, dy: u16| -> (u16, i64, u16, i64) {
            (
                column,
                (f64::from(dx) / 1024.0 * width_px(column) * 9525.0) as i64,
                row,
                (f64::from(dy) / 256.0 * height_pt(row) * 12700.0) as i64,
            )
        };
        if let Some(written) = drawing.write(context.number, context.images, &geometry, sheet_name) {
            unread_objects = written.unread.clone();
            if !written.xml.is_empty() {
                let drawing_part = format!("xl/drawings/drawing{}.xml", context.number);
                let mut drels = Xml::new();
                drels.open(
                    "Relationships",
                    &[("xmlns", "http://schemas.openxmlformats.org/package/2006/relationships".to_owned())],
                );
                for rel in &written.rels {
                    drels.empty(
                        "Relationship",
                        &[
                            ("Id", rel.id.clone()),
                            ("Type", format!("{}/{}", super::RELATIONSHIPS_NAMESPACE, rel.kind)),
                            ("Target", rel.target.clone()),
                        ],
                    );
                }
                drels.close("Relationships");
                entries.push((
                    format!("xl/drawings/_rels/drawing{}.xml.rels", context.number),
                    drels.out,
                ));
                parts.push((
                    drawing_part,
                    "application/vnd.openxmlformats-officedocument.drawing+xml",
                    written.xml,
                ));
                for (k, image) in written.images.iter().enumerate() {
                    parts.push((
                        format!("xl/media/image{}_{}.{}", context.number, k + 1, image.extension),
                        image.content_type,
                        image.bytes.clone(),
                    ));
                }
                for (k, chart) in written.charts.into_iter().enumerate() {
                    parts.push((
                        format!("xl/charts/chart{}_{}.xml", context.number, k + 1),
                        "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
                        chart,
                    ));
                }
                rels.push(super::features::Rel {
                    id: "rIdDrawing".to_owned(),
                    kind: "drawing",
                    target: format!("../drawings/drawing{}.xml", context.number),
                    external: false,
                });
                drawing_id = Some("rIdDrawing");
            }
        }
    }
    features.write_after_data(&mut x, &mut rels, drawing_id, &rules, context.filter_range.as_deref());
    x.close(root);
    Ok(SheetPart {
        xml: x.out,
        unknown,
        rels,
        parts: {
            parts.extend(features.comment_parts());
            if let Some(printer) = features.printer_part() { parts.push(printer); }
            parts
        },
        entries,
        unread_objects,
    })
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
