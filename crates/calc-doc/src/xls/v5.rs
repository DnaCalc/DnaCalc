//! BIFF5 and BIFF7 workbooks (Excel 5.0, 95): the link tables (EXTERNCOUNT, EXTERNSHEET,
//! EXTERNNAME) that formulas index. The workbook globals and each sheet substream hold their own
//! table of EXTERNSHEET records; each record is one referenced sheet of this workbook, one
//! sheet of another workbook (its encoded file name in brackets before the sheet name), or a
//! document block (an external workbook's or the add-in functions' name) that the EXTERNNAME
//! records after it belong to. The tables are turned into the supporting-workbook and sheet
//! references the formula printer reads for BIFF8 files.
//!
//! Reference: OpenOffice.org's Documentation of the Microsoft Excel File Format, sections 2.5.9,
//! 4.10.2 and 5.39 to 5.41.

use super::codepage::decode;
use super::records::Record;
use crate::xlsb::formula::{SupBook, Xti};

const EXTERNSHEET: u16 = 0x0017;
const EXTERNNAME: u16 = 0x0023;

/// An external workbook, gathered across every link table of the file, numbered in order of
/// first appearance.
#[derive(Clone, Debug, Default)]
pub(crate) struct Document {
    pub(crate) path: String,
    pub(crate) sheets: Vec<String>,
    pub(crate) names: Vec<String>,
}

/// What an EXTERNSHEET record refers to.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    /// A sheet of this workbook, or the current sheet, or none (the sheet is deleted).
    Own,
    AddIn,
    /// A sheet (or no sheet, for a document block) of the external document `document`.
    External { document: usize, sheet: Option<usize> },
}

/// One link table, as the formula printer reads it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Table {
    targets: Vec<(Target, Vec<String>)>,
}

/// The external documents of the whole file.
#[derive(Debug, Default)]
pub(crate) struct Registry {
    pub(crate) documents: Vec<Document>,
}

/// The characters of an encoded URL (OpenOffice.org 2.5.9) as a file path; control character 1
/// is a drive letter (or `@` and a server name), 2 the root of the drive, 3 a directory
/// separator, 4 the parent directory, 5 an unencoded URL (a length character, then the URL).
pub(crate) fn decode_url(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{01}' => match chars.next() {
                Some('@') => out.push_str("\\\\"),
                Some(drive) => {
                    out.push(drive);
                    out.push_str(":\\");
                }
                None => {}
            },
            '\u{02}' => out.push('\\'),
            '\u{03}' => out.push('\\'),
            '\u{04}' => out.push_str("..\\"),
            '\u{05}' => {
                let length = chars.next().map_or(0, |c| c as usize);
                for _ in 0..length {
                    if let Some(c) = chars.next() {
                        out.push(c);
                    }
                }
            }
            '\u{06}'..='\u{08}' => {}
            other => out.push(other),
        }
    }
    out
}

impl Registry {
    fn document(&mut self, path: &str) -> usize {
        match self.documents.iter().position(|d| d.path == path) {
            Some(at) => at,
            None => {
                self.documents.push(Document {
                    path: path.to_owned(),
                    ..Default::default()
                });
                self.documents.len() - 1
            }
        }
    }

    /// Reads the link table in `records` (the EXTERNSHEET and EXTERNNAME records of a globals
    /// substream or of a sheet substream).
    pub(crate) fn table(&mut self, records: &[Record<'_>], code_page: u16) -> Result<Table, String> {
        let mut table = Table::default();
        for record in records {
            match record.id {
                EXTERNSHEET => {
                    let body = record.body;
                    let length = usize::from(*body.first().ok_or("an empty EXTERNSHEET record")?);
                    // The length field is one less than the byte count for a sheet of this
                    // workbook (first character 3).
                    let raw = &body[1..];
                    let used = if raw.first() == Some(&3) { length + 1 } else { length };
                    let raw = raw.get(..used.min(raw.len())).unwrap_or(raw);
                    let target = match raw.first() {
                        None | Some(0) | Some(2) | Some(3) | Some(4) => Target::Own,
                        Some(1) if raw.get(1) == Some(&0x3A) => Target::AddIn,
                        Some(1) => {
                            let text = decode(code_page, &raw[1..])?;
                            match (text.rfind('['), text.rfind(']')) {
                                (Some(open), Some(close)) if open < close => {
                                    let file = format!("{}{}", &text[..open], &text[open + 1..close]);
                                    let sheet = text[close + 1..].to_owned();
                                    let path = decode_url(&file);
                                    let document = self.document(&path);
                                    let sheets = &mut self.documents[document].sheets;
                                    let at = match sheets.iter().position(|s| *s == sheet) {
                                        Some(at) => at,
                                        None => {
                                            sheets.push(sheet);
                                            sheets.len() - 1
                                        }
                                    };
                                    Target::External {
                                        document,
                                        sheet: Some(at),
                                    }
                                }
                                _ => Target::External {
                                    document: self.document(&decode_url(&text)),
                                    sheet: None,
                                },
                            }
                        }
                        Some(_) => Target::External {
                            document: self.document(&decode(code_page, raw)?),
                            sheet: None,
                        },
                    };
                    table.targets.push((target, Vec::new()));
                }
                EXTERNNAME => {
                    // Option flags (2), sheet index (2), reserved (2), name (8-bit length).
                    let body = record.body;
                    if body.len() < 7 {
                        return Err("a short EXTERNNAME record".into());
                    }
                    let length = usize::from(body[6]);
                    let name = decode(code_page, body.get(7..7 + length).ok_or("an EXTERNNAME name that is cut off")?)?;
                    if let Some((target, names)) = table.targets.last_mut() {
                        if let Target::External { document, .. } = target {
                            let names_of_document = &mut self.documents[*document].names;
                            if !names_of_document.contains(&name) {
                                names_of_document.push(name.clone());
                            }
                        }
                        names.push(name);
                    }
                }
                _ => {}
            }
        }
        Ok(table)
    }
}

impl Table {
    /// The supporting workbooks and sheet references of this table as the formula printer reads
    /// them: entry `k` of the first list is the book of entry `k` of the second. An external
    /// document's number is its position among `documents` plus one, the number of its
    /// external link part.
    pub(crate) fn references(&self, documents: &[Document]) -> (Vec<SupBook>, Vec<Xti>) {
        let mut books = Vec::new();
        let mut xti = Vec::new();
        for (k, (target, names)) in self.targets.iter().enumerate() {
            let (book, first) = match target {
                Target::Own => (SupBook::This, 0),
                Target::AddIn => (SupBook::AddIn { names: names.clone() }, -2),
                Target::External { document, sheet } => (
                    SupBook::External {
                        number: *document as u32 + 1,
                        sheets: documents[*document].sheets.clone(),
                        names: documents[*document].names.clone(),
                    },
                    sheet.map_or(-2, |s| s as i32),
                ),
            };
            books.push(book);
            xti.push(Xti {
                supbook: k as u32,
                first,
                last: first,
            });
        }
        (books, xti)
    }
}
