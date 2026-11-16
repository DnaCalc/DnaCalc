//! The workbook globals substream of a BIFF8 file (MS-XLS 2.1.7.20.1): the records between its BOF
//! and EOF that say how the file reads (code page, date system), where each sheet's substream is
//! (BOUNDSHEET8), and which sheet shows first (WINDOW1). Records this reader does not read are
//! counted by type in [`Globals::unknown`], never fatal.

use super::records::Record;
use super::strings::{Pieces, RichString};
use crate::xlsb::Cursor;
use std::collections::BTreeMap;

pub(crate) const BOF: u16 = 0x0809;
pub(crate) const EOF: u16 = 0x000A;
const CODEPAGE: u16 = 0x0042;
const DATEMODE: u16 = 0x0022;
const FILEPASS: u16 = 0x002F;
const BOUNDSHEET: u16 = 0x0085;
const WINDOW1: u16 = 0x003D;
const SST: u16 = 0x00FC;

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

/// What the globals substream says.
#[derive(Debug, Default)]
pub(crate) struct Globals {
    pub(crate) code_page: Option<u16>,
    pub(crate) date_1904: bool,
    pub(crate) sheets: Vec<BoundSheet>,
    pub(crate) active_tab: Option<u16>,
    /// The shared string table (SST), in index order.
    pub(crate) strings: Vec<RichString>,
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
pub(crate) fn read(records: &[Record<'_>]) -> Result<Globals, String> {
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
                let name = short_string(&mut c, "the sheet name")?;
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
