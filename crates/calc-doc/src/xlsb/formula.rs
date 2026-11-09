//! Formulas of an Excel binary workbook (MS-XLSB 2.2.2, 2.5.98): a parsed expression (Rgce: tokens
//! in reverse Polish order, with the extra data some tokens carry in RgbExtra) printed back as the
//! formula text a SpreadsheetML part holds, the text Excel writes in its `<f>` element: function
//! names with their `_xlfn.` prefix, references in A1 notation qualified by sheet (`Data!A1`,
//! `'Q2 Data'!A1:B2`, `Sheet1:Sheet3!A1`) or external workbook (`[1]Rates!$B$2`), structured
//! references with their table and column names, array constants, and the spaces and line
//! breaks the formula was written with (PtgAttrSpace).
//!
//! The text is the input of calc-fml's one parser, which the loader then reads it with, as it
//! reads any formula of a SpreadsheetML package.

use super::commands::COMMANDS;
use calc_fml::EXCEL_FUNCTION_IDS as FUNCTIONS;
use super::records::{Cursor, Malformed, Read, text};
use crate::save::file_form::function_file_name;
use std::collections::BTreeMap;

/// One XTI (2.5.173): a supporting workbook and a range of its sheets.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Xti {
    pub(crate) supbook: u32,
    pub(crate) first: i32,
    pub(crate) last: i32,
}

/// A supporting workbook (2.1.10): this workbook, an external one (the 1-based number of its
/// external link part among the workbook's, its sheet names and its defined names), or an
/// add-in's functions.
#[derive(Clone, Debug)]
pub(crate) enum SupBook {
    This,
    External {
        number: u32,
        sheets: Vec<String>,
        names: Vec<String>,
    },
    AddIn {
        names: Vec<String>,
    },
}

/// A defined name of the workbook, as a formula names it.
#[derive(Clone, Debug)]
pub(crate) struct NameRef {
    pub(crate) name: String,
    /// The sheet (by tab index) a sheet-scoped name belongs to.
    pub(crate) sheet: Option<u32>,
}

/// A table (ListObject), as a structured reference names it.
#[derive(Clone, Debug)]
pub(crate) struct TableRef {
    pub(crate) name: String,
    pub(crate) columns: Vec<String>,
}

/// What a formula's tokens are printed against.
pub(crate) struct Context<'a> {
    pub(crate) sheets: &'a [String],
    pub(crate) xti: &'a [Xti],
    pub(crate) supbooks: &'a [SupBook],
    pub(crate) names: &'a [NameRef],
    pub(crate) tables: &'a BTreeMap<u32, TableRef>,
    /// The tab index of the sheet the formula is on, when it is on one.
    pub(crate) sheet: Option<u32>,
}

/// Where a formula is evaluated: the 0-based cell relative tokens (PtgRefN, PtgAreaN) are offsets
/// from.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Origin {
    pub(crate) row: u32,
    pub(crate) column: u32,
}

/// A PtgExp: the formula is part of the shared or array formula whose top-left cell this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Expanded {
    pub(crate) row: u32,
    pub(crate) column: u32,
}

fn bad(detail: impl Into<String>) -> Malformed {
    Malformed(detail.into())
}

/// The error a BErr (2.5.98.2) names.
pub(crate) fn error_text(code: u8) -> Option<&'static str> {
    Some(match code {
        0x00 => "#NULL!",
        0x07 => "#DIV/0!",
        0x0F => "#VALUE!",
        0x17 => "#REF!",
        0x1D => "#NAME?",
        0x24 => "#NUM!",
        0x2A => "#N/A",
        0x2B => "#GETTING_DATA",
        _ => return None,
    })
}

fn column_letters(mut column: u32) -> String {
    let mut out = Vec::new();
    column += 1;
    while column > 0 {
        out.push(b'A' + ((column - 1) % 26) as u8);
        column = (column - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).expect("ASCII")
}

/// A sheet name as a formula writes it before `!`: quoted, with each apostrophe doubled, when it
/// is not made of letters, digits, `_` and `.` only, starts with a digit or `.`, or could be read
/// as a cell or R1C1 reference.
pub(crate) fn sheet_prefix(name: &str) -> String {
    let plain = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        && !name.starts_with(|c: char| c.is_ascii_digit() || c == '.')
        && !looks_like_reference(name);
    if plain {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// Whether a name could be read as a cell reference (`A1`, `XFD1048576`) or an R1C1 one (`R`,
/// `C`, `R1C1`, `R2`, `C3`).
fn looks_like_reference(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    let letters: String = upper
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect();
    let digits = &upper[letters.len()..];
    if !letters.is_empty()
        && letters.len() <= 3
        && !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit())
    {
        return true;
    }
    let r1c1 = |s: &str| -> bool {
        let mut chars = s.chars().peekable();
        let mut any = false;
        if chars.peek() == Some(&'R') {
            chars.next();
            any = true;
            while chars.peek().is_some_and(char::is_ascii_digit) {
                chars.next();
            }
        }
        if chars.peek() == Some(&'C') {
            chars.next();
            any = true;
            while chars.peek().is_some_and(char::is_ascii_digit) {
                chars.next();
            }
        }
        any && chars.next().is_none()
    };
    r1c1(&upper)
}

/// The text of a formula's number constant, as Excel writes it: the shortest digits that read
/// back as the number, with an exponent past 15 digits of magnitude.
pub(crate) fn number_text(value: f64) -> String {
    crate::save::text::number(value)
}

/// A number constant of a formula as Excel writes it in the formula's text: rounded to 15
/// significant digits, positional when that takes at most 21 characters and otherwise as a
/// mantissa and a signed exponent (measured with Excel 16.0.20430, 2026-10-27: `1E-11` is written
/// `0.00000000001`, `2.4778044E-10` `0.00000000024778044`, `1.2345678901234E-10` and `1E+21` as
/// they are, and `1.23456789012345E+20` `123456789012345000000`).
pub(crate) fn formula_number(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    let scientific = format!("{value:.14e}");
    let (mantissa, exponent) = scientific.split_once('e').expect("an exponent");
    let exponent: i32 = exponent.parse().expect("an exponent");
    let negative = mantissa.starts_with('-');
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let sign = if negative { "-" } else { "" };
    // Positional: the digits with the point placed by the exponent.
    let positional = if exponent >= 0 {
        let whole = exponent as usize + 1;
        if digits.len() <= whole {
            format!("{sign}{digits}{}", "0".repeat(whole - digits.len()))
        } else {
            format!("{sign}{}.{}", &digits[..whole], &digits[whole..])
        }
    } else {
        format!("{sign}0.{}{digits}", "0".repeat((-exponent - 1) as usize))
    };
    if positional.trim_start_matches('-').len() <= 21 {
        return positional;
    }
    let mantissa = if digits.len() == 1 {
        digits.to_owned()
    } else {
        format!("{}.{}", &digits[..1], &digits[1..])
    };
    format!(
        "{sign}{mantissa}E{}{}",
        if exponent < 0 { "-" } else { "+" },
        exponent.abs()
    )
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}

/// An operand on the printer's stack: its text.
struct Item {
    text: String,
}

/// Spaces and line breaks waiting for the next token (PtgAttrSpace, 2.5.98.30-32).
#[derive(Default)]
struct Pending {
    /// Before the next base expression or operator.
    before: String,
    /// Before an open parenthesis.
    open: String,
    /// Before a close parenthesis.
    close: String,
    /// At the start of the formula, before its `=` (PtgAttrSpace type 6).
    lead: String,
}

impl Pending {
    fn take(&mut self) -> String {
        std::mem::take(&mut self.before)
    }
}

/// Prints the formula `rgce` (with its extra data `rgcb`) evaluated at `origin`; `Err` with why
/// when a token is not one this printer knows, or the tokens do not make one expression. A
/// formula that is all of a shared or array formula of another cell (a lone PtgExp) gives
/// `Ok(Err(expanded))`.
pub(crate) fn print(
    rgce: &[u8],
    rgcb: &[u8],
    context: &Context<'_>,
    origin: Origin,
) -> Read<Result<String, Expanded>> {
    let mut cursor = Cursor::new(rgce);
    let mut extra = Cursor::new(rgcb);
    let mut stack: Vec<Item> = Vec::new();
    let mut pending = Pending::default();
    let pop = |stack: &mut Vec<Item>| -> Read<Item> {
        stack
            .pop()
            .ok_or_else(|| bad("a formula whose operators want more operands than it has"))
    };
    while cursor.remaining() > 0 {
        let ptg = cursor.u8("a token")?;
        let base = if ptg >= 0x20 {
            (ptg & 0x1F) | 0x20
        } else {
            ptg
        };
        match base {
            0x01 => {
                let row = cursor.u32("PtgExp")?;
                let column = extra.u32("PtgExtraCol")?;
                if stack.is_empty() && cursor.remaining() == 0 {
                    return Ok(Err(Expanded { row, column }));
                }
                return Err(bad("a PtgExp inside an expression"));
            }
            0x02 => return Err(bad("a PtgTbl (a data table's formula)")),
            0x03..=0x11 => {
                let symbol = match base {
                    0x03 => "+",
                    0x04 => "-",
                    0x05 => "*",
                    0x06 => "/",
                    0x07 => "^",
                    0x08 => "&",
                    0x09 => "<",
                    0x0A => "<=",
                    0x0B => "=",
                    0x0C => ">=",
                    0x0D => ">",
                    0x0E => "<>",
                    0x0F => " ",
                    0x10 => ",",
                    _ => ":",
                };
                let right = pop(&mut stack)?;
                let left = pop(&mut stack)?;
                let space = pending.take();
                stack.push(Item {
                    text: format!("{}{space}{symbol}{}", left.text, right.text),
                });
            }
            0x12 | 0x13 => {
                let operand = pop(&mut stack)?;
                let space = pending.take();
                let sign = if base == 0x12 { "+" } else { "-" };
                stack.push(Item {
                    text: format!("{space}{sign}{}", operand.text),
                });
            }
            0x14 => {
                let operand = pop(&mut stack)?;
                let space = pending.take();
                stack.push(Item {
                    text: format!("{}{space}%", operand.text),
                });
            }
            0x15 => {
                let operand = pop(&mut stack)?;
                let before = pending.take();
                let open = std::mem::take(&mut pending.open);
                let close = std::mem::take(&mut pending.close);
                stack.push(Item {
                    text: format!("{before}{open}({}{close})", operand.text),
                });
            }
            0x16 => {
                let space = pending.take();
                stack.push(Item { text: space });
            }
            0x17 => {
                let count = cursor.u16("PtgStr")? as usize;
                let units = cursor.units(count, "PtgStr")?;
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}{}", quoted(&text(&units))),
                });
            }
            0x18 => {
                let eptg = cursor.u8("an extended token")?;
                if eptg != 0x19 {
                    return Err(bad(format!("the extended token 0x18 0x{eptg:02X}")));
                }
                let reference = list(&mut cursor, &mut extra, context)?;
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}{reference}"),
                });
            }
            0x19 => {
                let kind = cursor.u8("PtgAttr")?;
                let data = cursor.u16("PtgAttr")?;
                match kind {
                    // Space and line breaks (PtgAttrSpace, PtgAttrSpaceSemi).
                    0x40 | 0x41 => {
                        let kind = (data & 0xFF) as u8;
                        let count = (data >> 8) as usize;
                        let (character, slot) = match kind {
                            0x00 => (" ", &mut pending.before),
                            0x01 => ("\n", &mut pending.before),
                            0x02 => (" ", &mut pending.open),
                            0x03 => ("\n", &mut pending.open),
                            0x04 => (" ", &mut pending.close),
                            0x05 => ("\n", &mut pending.close),
                            0x06 => (" ", &mut pending.lead),
                            other => {
                                return Err(bad(format!("the space attribute type {other}")));
                            }
                        };
                        slot.push_str(&character.repeat(count));
                    }
                    // SUM of one argument (PtgAttrSum).
                    0x10 => {
                        let operand = pop(&mut stack)?;
                        let space = pending.take();
                        stack.push(Item {
                            text: format!("{space}SUM({})", operand.text),
                        });
                    }
                    // Choose's jump table.
                    0x04 => cursor.skip((usize::from(data) + 1) * 2, "PtgAttrChoose")?,
                    // Volatile, If, GoTo, Baxcel, IfError: no text.
                    0x01 | 0x02 | 0x08 | 0x20 | 0x21 | 0x80 => {}
                    other => return Err(bad(format!("the attribute 0x{other:02X}"))),
                }
            }
            0x1C => {
                let code = cursor.u8("PtgErr")?;
                let error =
                    error_text(code).ok_or_else(|| bad(format!("the error code 0x{code:02X}")))?;
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}{error}"),
                });
            }
            0x1D => {
                let value = cursor.u8("PtgBool")?;
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}{}", if value != 0 { "TRUE" } else { "FALSE" }),
                });
            }
            0x1E => {
                let value = cursor.u16("PtgInt")?;
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}{value}"),
                });
            }
            0x1F => {
                let value = cursor.f64("PtgNum")?;
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}{}", formula_number(value)),
                });
            }
            0x20 => {
                cursor.skip(14, "PtgArray")?;
                let array = array_constant(&mut extra)?;
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}{array}"),
                });
            }
            0x21 | 0x22 => {
                // A macro sheet's command (fCeFunc), and its dialog form (fPrompt, `OPEN?`).
                let (mut command, mut prompt) = (false, false);
                let (count, number) = if base == 0x21 {
                    let number = cursor.u16("PtgFunc")?;
                    let fixed = FUNCTIONS
                        .iter()
                        .find(|(n, _, _)| *n == number)
                        .map(|(_, _, arity)| *arity)
                        .filter(|arity| *arity >= 0)
                        .ok_or_else(|| bad(format!("the fixed-arity function number {number}")))?;
                    (fixed as usize, number)
                } else {
                    let count = cursor.u8("PtgFuncVar")?;
                    let raw = cursor.u16("PtgFuncVar")?;
                    command = raw & 0x8000 != 0;
                    prompt = count & 0x80 != 0;
                    (usize::from(count & 0x7F), raw & 0x7FFF)
                };
                if stack.len() < count {
                    return Err(bad("a function call with fewer arguments than it names"));
                }
                let mut arguments: Vec<String> = stack
                    .split_off(stack.len() - count)
                    .into_iter()
                    .map(|item| item.text)
                    .collect();
                let space = pending.take();
                let name = if command {
                    let name = COMMANDS
                        .iter()
                        .find(|(n, _)| *n == number)
                        .map(|(_, name)| *name)
                        .ok_or_else(|| bad(format!("the command number {number}")))?;
                    format!("{name}{}", if prompt { "?" } else { "" })
                } else if number == 255 {
                    // A user-defined or future function: its name is the first argument.
                    if arguments.is_empty() {
                        return Err(bad("a call of function 255 without its name"));
                    }
                    call_name(arguments.remove(0).trim_start())
                } else {
                    let name = FUNCTIONS
                        .iter()
                        .find(|(n, _, _)| *n == number)
                        .map(|(_, name, _)| *name)
                        .ok_or_else(|| bad(format!("the function number {number}")))?;
                    name.to_owned()
                };
                stack.push(Item {
                    text: format!("{space}{name}({})", arguments.join(",")),
                });
            }
            0x23 => {
                let index = cursor.u32("PtgName")?;
                let name = defined_name(index, context)?;
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}{name}"),
                });
            }
            0x24 | 0x2C => {
                let row = cursor.u32("a reference's row")?;
                let column = cursor.u16("a reference's column")?;
                let cell = cell_text(row, column, base == 0x2C, origin);
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}{cell}"),
                });
            }
            0x25 | 0x2D => {
                let rows = (cursor.u32("an area")?, cursor.u32("an area")?);
                let columns = (cursor.u16("an area")?, cursor.u16("an area")?);
                let area = area_text(rows, columns, base == 0x2D, origin);
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}{area}"),
                });
            }
            // PtgMemArea, PtgMemErr, PtgMemNoMem, PtgMemFunc: the reference expression that
            // follows prints itself; their extra data is skipped.
            0x26 => {
                cursor.skip(6, "PtgMemArea")?;
                let count = extra.u32("PtgExtraMem")? as usize;
                extra.skip(count * 16, "PtgExtraMem")?;
            }
            0x27 | 0x28 => cursor.skip(6, "PtgMemErr")?,
            0x29 => cursor.skip(2, "PtgMemFunc")?,
            0x2A => {
                cursor.skip(6, "PtgRefErr")?;
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}#REF!"),
                });
            }
            0x2B => {
                cursor.skip(12, "PtgAreaErr")?;
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}#REF!"),
                });
            }
            0x39 => {
                let ixti = cursor.u16("PtgNameX")?;
                let index = cursor.u32("PtgNameX")?;
                let name = external_name(ixti, index, context)?;
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}{name}"),
                });
            }
            0x3A | 0x3C => {
                let ixti = cursor.u16("a 3-D reference")?;
                let row = cursor.u32("a 3-D reference")?;
                let column = cursor.u16("a 3-D reference")?;
                let prefix = sheets_prefix(ixti, context)?;
                let cell = if base == 0x3C {
                    "#REF!".to_owned()
                } else {
                    cell_text(row, column, false, origin)
                };
                let space = pending.take();
                stack.push(Item {
                    text: match prefix {
                        Some(prefix) => format!("{space}{prefix}{cell}"),
                        None => format!("{space}#REF!"),
                    },
                });
            }
            0x3B | 0x3D => {
                let ixti = cursor.u16("a 3-D area")?;
                let rows = (cursor.u32("a 3-D area")?, cursor.u32("a 3-D area")?);
                let columns = (cursor.u16("a 3-D area")?, cursor.u16("a 3-D area")?);
                let prefix = sheets_prefix(ixti, context)?;
                let area = if base == 0x3D {
                    "#REF!".to_owned()
                } else {
                    area_text(rows, columns, false, origin)
                };
                let space = pending.take();
                stack.push(Item {
                    text: match prefix {
                        Some(prefix) => format!("{space}{prefix}{area}"),
                        None => format!("{space}#REF!"),
                    },
                });
            }
            other => return Err(bad(format!("the token 0x{other:02X}"))),
        }
    }
    let item = pop(&mut stack)?;
    if !stack.is_empty() {
        return Err(bad("a formula whose tokens leave more than one operand"));
    }
    Ok(Ok(format!("{}{}", pending.lead, item.text)))
}

/// A row and a column of a reference as A1 text: `$` before each absolute coordinate. A relative
/// coordinate of a PtgRefN or PtgAreaN (`offsets`) is an offset from `origin`, wrapping around
/// the grid's edge.
fn coordinate_text(row: u32, raw_column: u16, offsets: bool, origin: Origin) -> (String, String) {
    let column = u32::from(raw_column & 0x3FFF);
    let column_relative = raw_column & 0x4000 != 0;
    let row_relative = raw_column & 0x8000 != 0;
    let row = if offsets && row_relative {
        (i64::from(origin.row) + i64::from(row as i32)).rem_euclid(1_048_576) as u32
    } else {
        row
    };
    let column = if offsets && column_relative {
        let offset = if column & 0x2000 != 0 {
            i64::from(column) - 0x4000
        } else {
            i64::from(column)
        };
        (i64::from(origin.column) + offset).rem_euclid(16_384) as u32
    } else {
        column
    };
    (
        format!(
            "{}{}",
            if column_relative { "" } else { "$" },
            column_letters(column)
        ),
        format!("{}{}", if row_relative { "" } else { "$" }, row + 1),
    )
}

fn cell_text(row: u32, column: u16, offsets: bool, origin: Origin) -> String {
    let (c, r) = coordinate_text(row, column, offsets, origin);
    format!("{c}{r}")
}

/// An area as A1 text: whole columns (`A:B`) when it spans every row, whole rows (`1:3`) when it
/// spans every column.
fn area_text(rows: (u32, u32), columns: (u16, u16), offsets: bool, origin: Origin) -> String {
    let (c1, r1) = coordinate_text(rows.0, columns.0, offsets, origin);
    let (c2, r2) = coordinate_text(rows.1, columns.1, offsets, origin);
    let all_rows = !offsets && rows.0 == 0 && rows.1 == 1_048_575;
    let all_columns = !offsets && columns.0 & 0x3FFF == 0 && columns.1 & 0x3FFF == 16_383;
    if all_rows && !all_columns {
        format!("{c1}:{c2}")
    } else if all_columns && !all_rows {
        format!("{r1}:{r2}")
    } else {
        format!("{c1}{r1}:{c2}{r2}")
    }
}

/// The qualifier of a 3-D reference, `Data!`, `'Q2 Data'!`, `Sheet1:Sheet3!`, `[1]Rates!`;
/// `None` for a reference to a deleted sheet of this workbook, which is written `#REF!` alone.
fn sheets_prefix(ixti: u16, context: &Context<'_>) -> Read<Option<String>> {
    let xti = context
        .xti
        .get(usize::from(ixti))
        .ok_or_else(|| bad(format!("the XTI {ixti}")))?;
    let book = context
        .supbooks
        .get(xti.supbook as usize)
        .ok_or_else(|| bad(format!("the supporting workbook {}", xti.supbook)))?;
    let (book_prefix, sheets): (String, &[String]) = match book {
        SupBook::This => (String::new(), context.sheets),
        SupBook::External { number, sheets, .. } => (format!("[{number}]"), sheets.as_slice()),
        SupBook::AddIn { .. } => return Err(bad("a 3-D reference into an add-in")),
    };
    let name = |tab: i32| -> Read<Option<String>> {
        match tab {
            -1 | -2 => Ok(None),
            tab if tab >= 0 => sheets
                .get(tab as usize)
                .cloned()
                .map(Some)
                .ok_or_else(|| bad(format!("the sheet {tab} of a reference"))),
            other => Err(bad(format!("the sheet index {other}"))),
        }
    };
    let (first, last) = (name(xti.first)?, name(xti.last)?);
    let joined = match (first, last) {
        (Some(first), Some(last)) if first != last => format!("{first}:{last}"),
        (Some(first), _) => first,
        // A reference to a deleted sheet.
        (None, _) if book_prefix.is_empty() => return Ok(None),
        (None, _) => return Ok(Some(format!("{book_prefix}#REF!"))),
    };
    if !book_prefix.is_empty() {
        // After an external workbook's number Excel quotes a sheet name unless it is letters,
        // digits and `_` only (Excel-saved twins: `[9]UK99!`, `[2]USGC!`, but `'[3]CHGSPD19.FIN'!`
        // and `'[1]Model inputs'!`).
        let plain = joined.chars().all(|c| c.is_alphanumeric() || c == '_')
            && !joined.starts_with(|c: char| c.is_ascii_digit());
        return Ok(Some(if plain {
            format!("{book_prefix}{joined}!")
        } else {
            format!("'{book_prefix}{}'!", joined.replace('\'', "''"))
        }));
    }
    let quoted = sheet_prefix(&joined);
    Ok(Some(if let Some(rest) = quoted.strip_prefix('\'') {
        format!("'{book_prefix}{rest}!")
    } else if book_prefix.is_empty() {
        format!("{quoted}!")
    } else {
        format!("{book_prefix}{quoted}!")
    }))
}

/// A defined name a PtgName (1-based) names, qualified by its sheet when it is scoped to another
/// sheet than the formula's.
fn defined_name(index: u32, context: &Context<'_>) -> Read<String> {
    let name = index
        .checked_sub(1)
        .and_then(|i| context.names.get(i as usize))
        .ok_or_else(|| bad(format!("the defined name {index}")))?;
    Ok(match name.sheet {
        Some(sheet) if Some(sheet) != context.sheet => {
            let sheet_name = context
                .sheets
                .get(sheet as usize)
                .ok_or_else(|| bad(format!("the sheet {sheet} of a name")))?;
            format!("{}!{}", sheet_prefix(sheet_name), name.name)
        }
        _ => name.name.clone(),
    })
}

/// A name a PtgNameX names: an external workbook's (`[1]!Rate`), or an add-in's function.
fn external_name(ixti: u16, index: u32, context: &Context<'_>) -> Read<String> {
    let xti = context
        .xti
        .get(usize::from(ixti))
        .ok_or_else(|| bad(format!("the XTI {ixti}")))?;
    let lookup = |names: &[String]| -> Read<String> {
        index
            .checked_sub(1)
            .and_then(|i| names.get(i as usize))
            .cloned()
            .ok_or_else(|| bad(format!("the external name {index}")))
    };
    match context.supbooks.get(xti.supbook as usize) {
        Some(SupBook::External { number, names, .. }) => {
            let name = lookup(names)?;
            Ok(format!("[{number}]!{name}"))
        }
        // An add-in's function, written as Excel writes an XLL's: `_xll.fArray`.
        Some(SupBook::AddIn { names }) => {
            let name = lookup(names)?;
            Ok(if name.starts_with("_xll.") {
                name
            } else {
                format!("_xll.{name}")
            })
        }
        // A name of this workbook reached through its own supporting link is written as Excel
        // writes it, `[0]!MyName`.
        Some(SupBook::This) => {
            let i = index as usize;
            context
                .names
                .get(i.wrapping_sub(1))
                .map(|name| format!("[0]!{}", name.name))
                .ok_or_else(|| bad(format!("the name {index} of this workbook")))
        }
        None => Err(bad(format!("the supporting workbook {}", xti.supbook))),
    }
}

/// The name a call of function 255 is written with, from the defined name the binary form calls:
/// a future function (`_xlfn.` and its name) with the prefix Excel writes before it in
/// SpreadsheetML, which for some is none (`ISO.CEILING`, and the field accessor `_FV`, measured in
/// Excel-saved twins); a user-defined function whose name is now a built-in function's with
/// `_xludf.` before it; any other name as it is.
fn call_name(name: &str) -> String {
    if let Some(base) = name.strip_prefix(calc_catalogue::FUTURE_FUNCTION_PREFIX) {
        let base = base.strip_prefix(calc_catalogue::WORKSHEET_FUNCTION_PREFIX).unwrap_or(base);
        if base.eq_ignore_ascii_case("_FV") {
            return base.to_owned();
        }
        return function_file_name(base).unwrap_or_else(|| name.to_owned());
    }
    // A function Excel knows: one of the binary function table, or one written with `_xlfn.`
    // (ACOT is written `_xludf.ACOT`; COPILOT, which Excel 16.0.20430 does not know and writes
    // without a prefix, stays `COPILOT`).
    let known = FUNCTIONS
        .iter()
        .any(|(_, known, _)| known.eq_ignore_ascii_case(name))
        || function_file_name(name).is_some_and(|written| written.starts_with(calc_catalogue::FUTURE_FUNCTION_PREFIX));
    if !name.contains(['!', '[']) && known {
        return format!("{}{name}", calc_catalogue::USER_DEFINED_FUNCTION_PREFIX);
    }
    name.to_owned()
}

/// An array constant (PtgExtraArray): `{1,2;"a",TRUE}`.
fn array_constant(extra: &mut Cursor<'_>) -> Read<String> {
    let rows = extra.u32("an array's rows")?;
    let columns = extra.u32("an array's columns")?;
    let mut out = String::from("{");
    for row in 0..rows {
        if row > 0 {
            out.push(';');
        }
        for column in 0..columns {
            if column > 0 {
                out.push(',');
            }
            let kind = extra.u8("an array value")?;
            match kind {
                0x00 => out.push_str(&formula_number(extra.f64("an array number")?)),
                0x01 => {
                    let count = extra.u16("an array string")? as usize;
                    out.push_str(&quoted(&text(&extra.units(count, "an array string")?)));
                }
                0x02 => out.push_str(if extra.u8("an array boolean")? != 0 {
                    "TRUE"
                } else {
                    "FALSE"
                }),
                0x04 => {
                    let code = extra.u8("an array error")?;
                    extra.skip(3, "an array error")?;
                    out.push_str(
                        error_text(code).ok_or_else(|| bad(format!("the error 0x{code:02X}")))?,
                    );
                }
                other => return Err(bad(format!("the array value type {other}"))),
            }
        }
    }
    out.push('}');
    Ok(out)
}

/// A column name in a structured reference: `'` before each `[`, `]`, `#` and `'`.
fn column_escaped(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if matches!(c, '[' | ']' | '#' | '\'') {
            out.push('\'');
        }
        out.push(c);
    }
    out
}

/// A structured reference (PtgList, 2.5.98.52) as the file form writes it: `Sales[Units]`,
/// `Sales[[#Headers],[Units]]`, `Sales[[#This Row],[Units]]`, `Sales[#All]`.
fn list(cursor: &mut Cursor<'_>, extra: &mut Cursor<'_>, context: &Context<'_>) -> Read<String> {
    let _ixti = cursor.u16("PtgList")?;
    let flags = cursor.u16("PtgList")?;
    let list_index = cursor.u32("PtgList")?;
    let first = cursor.u16("PtgList")?;
    let last = cursor.u16("PtgList")?;
    let columns_kind = flags & 0x3;
    let row_type = (flags >> 2) & 0x1F;
    let bracket_space = flags & (1 << 7) != 0;
    let comma_space = flags & (1 << 8) != 0;
    let invalid = flags & (1 << 12) != 0;
    let nonresident = flags & (1 << 13) != 0;
    if invalid {
        return Ok("#REF!".to_owned());
    }
    if nonresident {
        let _ = extra;
        return Err(bad("a structured reference to a table of another workbook"));
    }
    let table = context
        .tables
        .get(&list_index)
        .ok_or_else(|| bad(format!("the table {list_index}")))?;
    let column = |index: u16| -> Read<String> {
        table
            .columns
            .get(usize::from(index))
            .map(|name| column_escaped(name))
            .ok_or_else(|| bad(format!("the column {index} of table {}", table.name)))
    };
    let rows: &[&str] = match row_type {
        0x00 => &[],
        0x01 => &["#All"],
        0x02 => &["#Headers"],
        0x04 => &["#Data"],
        0x06 => &["#Headers", "#Data"],
        0x08 => &["#Totals"],
        0x0C => &["#Data", "#Totals"],
        0x10 => &["#This Row"],
        other => return Err(bad(format!("the table row type {other}"))),
    };
    let columns = match columns_kind {
        0 => None,
        1 => Some(format!("[{}]", column(first)?)),
        2 => Some(format!("[{}]:[{}]", column(first)?, column(last)?)),
        other => return Err(bad(format!("the table columns kind {other}"))),
    };
    let separator = if comma_space { ", " } else { "," };
    let mut items: Vec<String> = rows.iter().map(|row| format!("[{row}]")).collect();
    if let Some(columns) = &columns {
        items.push(columns.clone());
    }
    let inner = match (items.len(), &columns) {
        (0, _) => String::new(),
        // One column alone: `Sales[Units]` (the brackets are the column's).
        (1, Some(columns)) if !columns.contains("]:[") => {
            return Ok(format!("{}{}", table.name, columns));
        }
        (1, None) => {
            return Ok(format!("{}{}", table.name, items[0]));
        }
        _ => items.join(separator),
    };
    Ok(if bracket_space && !inner.is_empty() {
        format!("{}[ {inner} ]", table.name)
    } else {
        format!("{}[{inner}]", table.name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn formula_numbers_are_written_as_excel_writes_them() {
        // Measured with Excel 16.0.20430 (2026-10-27): each input typed as a constant, and the
        // formula text Excel saved.
        for (value, text) in [
            (1e-11, "0.00000000001"),
            (1.5e-11, "0.000000000015"),
            (1.23456789012345e-10, "1.23456789012345E-10"),
            (1.2345678901234e-10, "1.2345678901234E-10"),
            (2.4778044e-10, "0.00000000024778044"),
            (9.09494701772928e-13, "9.09494701772928E-13"),
            (1e21, "1E+21"),
            (1.5e21, "1.5E+21"),
            (1.23456789012345e21, "1.23456789012345E+21"),
            (1.23456789012345e20, "123456789012345000000"),
            (1e19, "10000000000000000000"),
            // Excel reads typed digits past the 15th as zeros: 12345678901234567890 is stored as
            // the double of 12345678901234500000.
            (12345678901234500000.0, "12345678901234500000"),
            (0.1, "0.1"),
            (1.5e-5, "0.000015"),
            (1.23456789012345, "1.23456789012345"),
            (3.14159265358979, "3.14159265358979"),
            (0.000123456789012345, "0.000123456789012345"),
            (1e-9, "0.000000001"),
            (1.2e-9, "0.0000000012"),
            (0.0, "0"),
        ] {
            assert_eq!(formula_number(value), text, "{value:e}");
        }
    }
}
