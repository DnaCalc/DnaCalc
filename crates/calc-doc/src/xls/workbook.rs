//! The workbook globals substream of a BIFF8 file (MS-XLS 2.1.7.20.1): the records between its BOF
//! and EOF that say how the file reads (code page, date system), where each sheet's substream is
//! (BOUNDSHEET8), and which sheet shows first (WINDOW1). Records this reader does not read are
//! counted by type in [`Globals::unknown`], never fatal.

use super::records::Record;
use super::strings::{Pieces, RichString};
use crate::xlsb::Cursor;
use crate::xlsb::formula::Xti;
use std::collections::BTreeMap;

pub(crate) const BOF: u16 = 0x0809;
pub(crate) const EOF: u16 = 0x000A;
const CODEPAGE: u16 = 0x0042;
const DATEMODE: u16 = 0x0022;
const FILEPASS: u16 = 0x002F;
const BOUNDSHEET: u16 = 0x0085;
const WINDOW1: u16 = 0x003D;
const SST: u16 = 0x00FC;
const NAME: u16 = 0x0018;
const EXTERNSHEET: u16 = 0x0017;
const SUPBOOK: u16 = 0x01AE;
const EXTERNNAME: u16 = 0x0023;

/// What a BOUNDSHEET8 sheet is (its `dt`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SheetKind {
    Worksheet,
    MacroSheet,
    Chart,
    /// A Visual Basic module sheet (BIFF5 only): not a sheet in SpreadsheetML.
    Module,
    Other(u8),
}

/// One BOUNDSHEET8 record.
#[derive(Clone, Debug)]
pub(crate) struct BoundSheet {
    /// The byte offset of the sheet's BOF in the workbook stream.
    pub(crate) offset: u32,
    /// 0 visible, 1 hidden, 2 very hidden.
    pub(crate) state: u8,
    pub(crate) kind: SheetKind,
    pub(crate) name: String,
}

/// A NAME record (Lbl, MS-XLS 2.4.150): a defined name with its formula tokens.
#[derive(Clone, Debug)]
pub(crate) struct NameRecord {
    pub(crate) flags: u16,
    pub(crate) key: u8,
    /// The sheet the name is local to (0-based tab index); `None` for a workbook name.
    pub(crate) scope: Option<u32>,
    /// The name as SpreadsheetML writes it (a built-in name with `_xlnm.` before it).
    pub(crate) name: String,
    pub(crate) rgce: Vec<u8>,
    pub(crate) rgcb: Vec<u8>,
    pub(crate) description: Option<String>,
    pub(crate) help: Option<String>,
}

/// A SUPBOOK record (2.4.271) with the EXTERNNAME records after it.
#[derive(Clone, Debug)]
pub(crate) enum SupBookRecord {
    /// This workbook (virtual path 0x0401).
    This,
    /// An add-in's functions (0x3A01).
    AddIn { names: Vec<String> },
    /// Another workbook: its encoded path, sheet names and defined names.
    External {
        path: String,
        sheets: Vec<String>,
        names: Vec<String>,
    },
}

/// What the globals substream says.
#[derive(Debug, Default)]
pub(crate) struct Globals {
    pub(crate) code_page: Option<u16>,
    pub(crate) date_1904: bool,
    pub(crate) sheets: Vec<BoundSheet>,
    pub(crate) active_tab: Option<u16>,
    pub(crate) protect_structure: bool,
    pub(crate) protect_windows: bool,
    pub(crate) protection_verifier: Option<u16>,
    pub(crate) hide_objects: Option<u16>,
    pub(crate) backup_file: bool,
    /// BookBool flags; mail-envelope state has no SpreadsheetML mapping here.
    pub(crate) book_bool: Option<u16>,
    /// The shared string table (SST), in index order.
    pub(crate) strings: Vec<RichString>,
    pub(crate) names: Vec<NameRecord>,
    pub(crate) supbooks: Vec<SupBookRecord>,
    pub(crate) xti: Vec<Xti>,
    pub(crate) styles: super::styles::Styles,
    /// The MSODRAWINGGROUP record: the drawing group container with the blip store.
    pub(crate) drawing_group: Vec<u8>,
    /// The workbook's VBA code name (CODENAME in the globals).
    pub(crate) code_name: Option<String>,
    /// A FILEPASS record: the stream is encrypted (dc-4j0e.7).
    pub(crate) encrypted: bool,
    /// Record types not read, with how many of each, over the whole workbook stream, sheets included.
    pub(crate) unknown: BTreeMap<u16, u32>,
    /// The index of the record after the globals' EOF.
    pub(crate) end: usize,
}

/// The `dt` of a BOF: what the substream holds.
pub(crate) fn bof_type(record: &Record<'_>) -> Option<u16> {
    (record.id == BOF && record.body.len() >= 4)
        .then(|| u16::from_le_bytes([record.body[2], record.body[3]]))
}

/// The BIFF version of a BOF record (0x0600 for BIFF8, 0x0500 for BIFF5/7).
pub(crate) fn bof_version(record: &Record<'_>) -> Option<u16> {
    (record.id == BOF && record.body.len() >= 2)
        .then(|| u16::from_le_bytes([record.body[0], record.body[1]]))
}

/// A ShortXLUnicodeString (2.5.240): a character count byte, a flag byte (bit 0: UTF-16 units, else
/// one byte each, Latin-1 in the sheet name's case), then the characters.
pub(crate) fn short_string(cursor: &mut Cursor<'_>, what: &str) -> Result<String, String> {
    let count = usize::from(cursor.u8(what).map_err(|e| e.0)?);
    let flags = cursor.u8(what).map_err(|e| e.0)?;
    if flags & 1 != 0 {
        let units = cursor.units(count, what).map_err(|e| e.0)?;
        Ok(String::from_utf16_lossy(&units))
    } else {
        let bytes = cursor.bytes(count, what).map_err(|e| e.0)?;
        Ok(bytes.iter().map(|b| char::from(*b)).collect())
    }
}

/// Reads the globals substream starting at `records[0]` (a BOF).
pub(crate) fn read(records: &[Record<'_>], biff5: bool) -> Result<Globals, String> {
    let mut globals = Globals::default();
    let mut depth = 0usize;
    let mut finished = false;
    for (index, record) in records.iter().enumerate() {
        match record.id {
            BOF => depth += 1,
            EOF => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    globals.end = index + 1;
                    finished = true;
                    break;
                }
            }
            CODEPAGE => {
                let mut c = Cursor::new(record.body);
                globals.code_page = Some(c.u16("CodePage").map_err(|e| e.0)?);
            }
            DATEMODE => {
                let mut c = Cursor::new(record.body);
                globals.date_1904 = c.u16("Date1904").map_err(|e| e.0)? == 1;
            }
            FILEPASS => globals.encrypted = true,
            0x00DA => {
                let mut c = Cursor::new(record.body);
                let flags = c.u16("BookBool").map_err(|e| e.0)?;
                if (flags >> 5) & 3 == 3 { return Err("the external-link update setting 3".to_owned()); }
                globals.book_bool = Some(flags);
                if flags & 0x1C != 0 { *globals.unknown.entry(record.id).or_insert(0) += 1; }
            }
            0x0040 => {
                let mut c = Cursor::new(record.body);
                let value = c.u16("Backup").map_err(|e| e.0)?;
                if value > 1 { return Err(format!("the backup setting {value}")); }
                globals.backup_file = value != 0;
            }
            0x008D => {
                let mut c = Cursor::new(record.body);
                let value = c.u16("HideObj").map_err(|e| e.0)?;
                if value > 2 { return Err(format!("the object display mode {value}")); }
                globals.hide_objects = Some(value);
            }
            0x0012 | 0x0019 => {
                let mut c = Cursor::new(record.body);
                let value = c.u16("workbook protection").map_err(|e| e.0)?;
                if value > 1 {
                    return Err(format!("the workbook protection flag {value}"));
                }
                if record.id == 0x0012 { globals.protect_structure = value != 0; }
                else { globals.protect_windows = value != 0; }
            }
            0x0013 => {
                let mut c = Cursor::new(record.body);
                let verifier = c.u16("workbook protection verifier").map_err(|e| e.0)?;
                globals.protection_verifier = (verifier != 0).then_some(verifier);
            }
            0x0016 if biff5 => {}
            EXTERNSHEET | EXTERNNAME if biff5 => {}
            BOUNDSHEET => {
                let mut c = Cursor::new(record.body);
                let offset = c.u32("BoundSheet8").map_err(|e| e.0)?;
                let state = c.u8("BoundSheet8").map_err(|e| e.0)? & 3;
                let kind = match c.u8("BoundSheet8").map_err(|e| e.0)? {
                    0 => SheetKind::Worksheet,
                    1 => SheetKind::MacroSheet,
                    2 => SheetKind::Chart,
                    6 => SheetKind::Module,
                    other => SheetKind::Other(other),
                };
                let name = if biff5 {
                    byte_string(&mut c, globals.code_page.unwrap_or(1252), "the sheet name")?
                } else {
                    short_string(&mut c, "the sheet name")?
                };
                globals.sheets.push(BoundSheet {
                    offset,
                    state,
                    kind,
                    name,
                });
            }
            SST => {
                let mut p = Pieces::new(record);
                p.skip(4, "the SST").map_err(|e| e.0)?;
                let unique = p.u32("the SST").map_err(|e| e.0)? as usize;
                for _ in 0..unique {
                    if p.done() {
                        break;
                    }
                    globals.strings.push(p.rich_string().map_err(|e| e.0)?);
                }
            }
            super::styles::FONT
            | super::styles::FORMAT
            | super::styles::XF
            | super::styles::STYLE
            | super::styles::PALETTE => {
                globals.styles.read(record, biff5.then(|| globals.code_page.unwrap_or(1252)))?;
            }
            super::drawing::MSODRAWING_GROUP => globals.drawing_group.extend_from_slice(&record.joined()),
            0x01BA => {
                let mut p = Pieces::new(record);
                if let Ok(units) = p.unicode_string("a code name") {
                    globals.code_name = Some(String::from_utf16_lossy(&units));
                }
            }
            NAME => globals.names.push(name_record(
                record,
                biff5.then(|| globals.code_page.unwrap_or(1252)),
            )?),
            SUPBOOK => globals.supbooks.push(supbook(record)?),
            EXTERNNAME => {
                let mut c = Cursor::new(record.body);
                c.skip(6, "ExternName").map_err(|e| e.0)?;
                let name = short_string(&mut c, "an external name")?;
                match globals.supbooks.last_mut() {
                    Some(SupBookRecord::External { names, .. } | SupBookRecord::AddIn { names }) => {
                        names.push(name)
                    }
                    _ => return Err("an EXTERNNAME record outside an external SUPBOOK".into()),
                }
            }
            EXTERNSHEET => {
                let mut c = Cursor::new(record.body);
                let count = c.u16("ExternSheet").map_err(|e| e.0)?;
                for _ in 0..count {
                    globals.xti.push(Xti {
                        supbook: u32::from(c.u16("Xti").map_err(|e| e.0)?),
                        first: i32::from(c.u16("Xti").map_err(|e| e.0)? as i16),
                        last: i32::from(c.u16("Xti").map_err(|e| e.0)? as i16),
                    });
                }
            }
            WINDOW1 => {
                // xWn, yWn, dxWn, dyWn, grbit, itabCur, ...
                let mut c = Cursor::new(record.body);
                c.skip(10, "Window1").map_err(|e| e.0)?;
                globals.active_tab = Some(c.u16("Window1").map_err(|e| e.0)?);
            }
            other => *globals.unknown.entry(other).or_insert(0) += 1,
        }
    }
    if !finished {
        return Err("the workbook globals have no EOF record".into());
    }
    Ok(globals)
}

/// A byte string with an 8-bit length (BIFF5) in code page `page`.
pub(crate) fn byte_string(c: &mut Cursor<'_>, page: u16, what: &str) -> Result<String, String> {
    let count = usize::from(c.u8(what).map_err(|e| e.0)?);
    super::codepage::decode(page, c.bytes(count, what).map_err(|e| e.0)?)
}

/// The text of an XLUnicodeStringNoCch: a flag byte (bit 0: UTF-16), then `count` characters.
fn no_count_string(c: &mut Cursor<'_>, count: usize, what: &str) -> Result<String, String> {
    if count == 0 {
        return Ok(String::new());
    }
    let flags = c.u8(what).map_err(|e| e.0)?;
    if flags & 1 != 0 {
        Ok(String::from_utf16_lossy(&c.units(count, what).map_err(|e| e.0)?))
    } else {
        Ok(c.bytes(count, what).map_err(|e| e.0)?.iter().map(|b| char::from(*b)).collect())
    }
}

/// The name a built-in NAME record's code stands for (2.5.171 and the `_xlnm.` names).
fn built_in_name(code: u16) -> Option<&'static str> {
    Some(match code {
        0x00 => "_xlnm.Consolidate_Area",
        0x01 => "_xlnm.Auto_Open",
        0x02 => "_xlnm.Auto_Close",
        0x03 => "_xlnm.Extract",
        0x04 => "_xlnm.Database",
        0x05 => "_xlnm.Criteria",
        0x06 => "_xlnm.Print_Area",
        0x07 => "_xlnm.Print_Titles",
        0x08 => "_xlnm.Recorder",
        0x09 => "_xlnm.Data_Form",
        0x0A => "_xlnm.Auto_Activate",
        0x0B => "_xlnm.Auto_Deactivate",
        0x0C => "_xlnm.Sheet_Title",
        0x0D => "_xlnm._FilterDatabase",
        _ => return None,
    })
}

fn name_record(record: &Record<'_>, v5: Option<u16>) -> Result<NameRecord, String> {
    let body = record.joined();
    let mut c = Cursor::new(&body);
    let what = "a NAME record";
    let m = |e: crate::xlsb::Malformed| e.0;
    let flags = c.u16(what).map_err(m)?;
    let key = c.u8(what).map_err(m)?;
    let name_len = usize::from(c.u8(what).map_err(m)?);
    let formula_len = usize::from(c.u16(what).map_err(m)?);
    c.skip(2, what).map_err(m)?;
    let tab = c.u16(what).map_err(m)?;
    let menu_len = usize::from(c.u8(what).map_err(m)?);
    let description_len = usize::from(c.u8(what).map_err(m)?);
    let help_len = usize::from(c.u8(what).map_err(m)?);
    let status_len = usize::from(c.u8(what).map_err(m)?);
    let text = |c: &mut Cursor<'_>, count: usize| -> Result<String, String> {
        match v5 {
            Some(page) => {
                let bytes = c.bytes(count, what).map_err(m)?;
                super::codepage::decode(page, bytes)
            }
            None => no_count_string(c, count, what),
        }
    };
    let name = text(&mut c, name_len)?;
    let name = if flags & 0x20 != 0 {
        // A built-in name: one character whose code says which.
        let code = name.chars().next().map_or(0, |ch| ch as u32 as u16);
        built_in_name(code)
            .map(str::to_owned)
            .ok_or_else(|| format!("the built-in name code {code}"))?
    } else {
        name
    };
    let rgce = c.bytes(formula_len, what).map_err(m)?.to_vec();
    // BIFF8 reserves these four lengths as zero. In that common case all remaining bytes
    // belong to NameParsedFormula's RgbExtra, including any array constants.
    let rgcb = if menu_len == 0 && description_len == 0 && help_len == 0 && status_len == 0 {
        c.bytes(c.remaining(), what).map_err(m)?.to_vec()
    } else {
        Vec::new()
    };
    let _menu = text(&mut c, menu_len)?;
    let description = text(&mut c, description_len)?;
    let help = text(&mut c, help_len)?;
    let _status = text(&mut c, status_len)?;
    Ok(NameRecord {
        flags,
        key,
        scope: tab.checked_sub(1).map(u32::from),
        name,
        rgce,
        rgcb,
        description: (!description.is_empty()).then_some(description),
        help: (!help.is_empty()).then_some(help),
    })
}

fn supbook(record: &Record<'_>) -> Result<SupBookRecord, String> {
    let mut c = Cursor::new(record.body);
    let what = "a SUPBOOK record";
    let m = |e: crate::xlsb::Malformed| e.0;
    let sheets = usize::from(c.u16(what).map_err(m)?);
    let marker = c.u16(what).map_err(m)?;
    if marker == 0x0401 {
        return Ok(SupBookRecord::This);
    }
    if marker == 0x3A01 {
        return Ok(SupBookRecord::AddIn { names: Vec::new() });
    }
    let path = no_count_string(&mut c, usize::from(marker), what)?;
    let mut names = Vec::new();
    for _ in 0..sheets {
        let count = usize::from(c.u16(what).map_err(m)?);
        names.push(no_count_string(&mut c, count, what)?);
    }
    Ok(SupBookRecord::External {
        path: super::v5::decode_url(&path),
        sheets: names,
        names: Vec::new(),
    })
}
