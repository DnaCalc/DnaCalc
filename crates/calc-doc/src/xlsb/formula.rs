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
use super::records::{Cursor, Malformed, Read, text};
use calc_catalogue::{binary_function, binary_named_call_spelling};
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

/// Which file form's tokens are read: the widths of references, strings and array data differ
/// (MS-XLSB 2.5.98 and MS-XLS 2.5.198); the token numbers, function numbers and printed forms
/// do not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layout {
    Xlsb,
    Biff8,
    /// BIFF5 and BIFF7: 14-bit rows, byte strings in the workbook's code page, local link tables.
    Biff5,
    /// BIFF4: the same reference widths and function IDs, but shorter name tokens.
    Biff4,
    /// BIFF3: BIFF4 references/names, with one-byte function IDs.
    Biff3,
}

/// What a formula's tokens are printed against.
pub(crate) struct Context<'a> {
    pub(crate) layout: Layout,
    /// The code page of byte strings (BIFF5 only).
    pub(crate) code_page: u16,
    pub(crate) sheets: &'a [String],
    /// Source tab indices omitted from the converted workbook.
    pub(crate) unavailable_sheets: &'a [u32],
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
    let layout = context.layout;
    let biff5 = matches!(layout, Layout::Biff3 | Layout::Biff4 | Layout::Biff5);
    // Widths shared by BIFF5 and BIFF8: the tExp and tTbl row and column are 16 bits each.
    let biff8 = layout != Layout::Xlsb;
    // A raw worksheet has one sheet. Its workbook-level names' unqualified references
    // belong to that sheet; make that binding explicit in the resulting workbook.
    let raw_name_prefix = if context.sheet.is_none() && matches!(layout, Layout::Biff3 | Layout::Biff4)
        && context.sheets.len() == 1 { format!("{}!", sheet_prefix(&context.sheets[0])) } else { String::new() };
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
                let (row, column) = if biff8 {
                    (
                        u32::from(cursor.u16("PtgExp")?),
                        u32::from(cursor.u16("PtgExp")?),
                    )
                } else {
                    (cursor.u32("PtgExp")?, extra.u32("PtgExtraCol")?)
                };
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
                let (count, wide) = if biff5 {
                    (usize::from(cursor.u8("PtgStr")?), false)
                } else if biff8 {
                    let count = usize::from(cursor.u8("PtgStr")?);
                    (count, cursor.u8("PtgStr")? & 1 != 0)
                } else {
                    (cursor.u16("PtgStr")? as usize, true)
                };
                let units = if wide {
                    cursor.units(count, "PtgStr")?
                } else if biff5 {
                    byte_units(context.code_page, cursor.bytes(count, "PtgStr")?)?
                } else {
                    cursor
                        .bytes(count, "PtgStr")?
                        .iter()
                        .map(|b| u16::from(*b))
                        .collect()
                };
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}{}", quoted(&text(&units))),
                });
            }
            0x18 if biff8 => return Err(bad("a natural-language formula token in a BIFF8 formula (dc-4j0e.3)")),
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
                    // BIFF can retain an attribute with no flags set; its data word carries
                    // no text or operation when none of the attribute flags is enabled.
                    0x00 if biff8 => {}
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
                            text: format!("{space}{}({})", attribute_sum_name(), operand.text),
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
                let array = if biff5 {
                    cursor.skip(7, "PtgArray")?;
                    biff5_array_constant(&mut extra, context.code_page)?
                } else if biff8 {
                    cursor.skip(7, "PtgArray")?;
                    biff8_array_constant(&mut extra)?
                } else {
                    cursor.skip(14, "PtgArray")?;
                    array_constant(&mut extra)?
                };
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}{array}"),
                });
            }
            0x21 | 0x22 => {
                // A macro sheet's command (fCeFunc), and its dialog form (fPrompt, `OPEN?`).
                let (mut command, mut prompt) = (false, false);
                let (count, number) = if base == 0x21 {
                    let number = if layout == Layout::Biff3 { u16::from(cursor.u8("PtgFunc")?) } else { cursor.u16("PtgFunc")? };
                    let fixed = match (layout, number) {
                        // Excel opens fixed BIFF8 GETPIVOTDATA with two arguments.
                        (Layout::Biff8, 358) => 2,
                        _ => binary_function(number)
                            .map(|(_, arity)| arity)
                            .filter(|arity| *arity >= 0)
                            .ok_or_else(|| bad(format!("the fixed-arity function number {number}")))?,
                    };
                    (fixed as usize, number)
                } else {
                    let count = cursor.u8("PtgFuncVar")?;
                    let raw = if layout == Layout::Biff3 { u16::from(cursor.u8("PtgFuncVar")?) } else { cursor.u16("PtgFuncVar")? };
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
                    binary_named_call_spelling(arguments.remove(0).trim_start())
                } else {
                    let name = binary_function(number)
                        .map(|(name, _)| name)
                        .ok_or_else(|| bad(format!("the function number {number}")))?;
                    name.to_owned()
                };
                stack.push(Item {
                    text: format!("{space}{name}({})", arguments.join(",")),
                });
            }
            0x23 => {
                let index = if biff5 {
                    let index = u32::from(cursor.u16("PtgName")?);
                    cursor.skip(if matches!(layout, Layout::Biff3 | Layout::Biff4) { 8 } else { 12 }, "PtgName")?;
                    index
                } else {
                    cursor.u32("PtgName")?
                };
                let name = defined_name(index, context)?;
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}{name}"),
                });
            }
            0x24 | 0x2C => {
                let (row, column) = read_cell(&mut cursor, layout)?;
                let cell = cell_text(row, column, base == 0x2C, origin, layout);
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}{raw_name_prefix}{cell}"),
                });
            }
            0x25 | 0x2D => {
                let (rows, columns) = read_area(&mut cursor, layout)?;
                let area = area_text(rows, columns, base == 0x2D, origin, layout);
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}{raw_name_prefix}{area}"),
                });
            }
            // PtgMemArea, PtgMemErr, PtgMemNoMem, PtgMemFunc: the reference expression that
            // follows prints itself; their extra data is skipped.
            0x26 => {
                cursor.skip(6, "PtgMemArea")?;
                if biff5 {
                    let count = usize::from(extra.u16("PtgExtraMem")?);
                    extra.skip(count * 6, "PtgExtraMem")?;
                } else if biff8 {
                    let count = usize::from(extra.u16("PtgExtraMem")?);
                    extra.skip(count * 8, "PtgExtraMem")?;
                } else {
                    let count = extra.u32("PtgExtraMem")? as usize;
                    extra.skip(count * 16, "PtgExtraMem")?;
                }
            }
            0x27 | 0x28 => cursor.skip(6, "PtgMemErr")?,
            0x29 => cursor.skip(2, "PtgMemFunc")?,
            0x2A => {
                cursor.skip(if biff5 { 3 } else if biff8 { 4 } else { 6 }, "PtgRefErr")?;
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}#REF!"),
                });
            }
            0x2B => {
                cursor.skip(if biff5 { 6 } else if biff8 { 8 } else { 12 }, "PtgAreaErr")?;
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}#REF!"),
                });
            }
            0x39 => {
                let name = if biff5 {
                    let ixti = cursor.u16("PtgNameX")? as i16;
                    cursor.skip(8, "PtgNameX")?;
                    let index = u32::from(cursor.u16("PtgNameX")?);
                    cursor.skip(12, "PtgNameX")?;
                    if ixti < 0 {
                        defined_name(index, context)?
                    } else {
                        external_name((ixti - 1) as u16, index, context)?
                    }
                } else {
                    let ixti = cursor.u16("PtgNameX")?;
                    let index = cursor.u32("PtgNameX")?;
                    external_name(ixti, index, context)?
                };
                let space = pending.take();
                stack.push(Item {
                    text: format!("{space}{name}"),
                });
            }
            0x3A | 0x3C => {
                let (prefix, (row, column)) = if biff5 {
                    let prefix = biff5_prefix(&mut cursor, context)?;
                    (prefix, read_cell(&mut cursor, layout)?)
                } else {
                    let ixti = cursor.u16("a 3-D reference")?;
                    let row = if biff8 {
                        u32::from(cursor.u16("a 3-D reference")?)
                    } else {
                        cursor.u32("a 3-D reference")?
                    };
                    let column = cursor.u16("a 3-D reference")?;
                    (sheets_prefix(ixti, context)?, (row, column))
                };
                let cell = if base == 0x3C {
                    "#REF!".to_owned()
                } else {
                    cell_text(row, column, false, origin, layout)
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
                let (prefix, (rows, columns)) = if biff5 {
                    let prefix = biff5_prefix(&mut cursor, context)?;
                    (prefix, read_area(&mut cursor, layout)?)
                } else {
                    let ixti = cursor.u16("a 3-D area")?;
                    let rows = if biff8 {
                        (
                            u32::from(cursor.u16("a 3-D area")?),
                            u32::from(cursor.u16("a 3-D area")?),
                        )
                    } else {
                        (cursor.u32("a 3-D area")?, cursor.u32("a 3-D area")?)
                    };
                    let columns = (cursor.u16("a 3-D area")?, cursor.u16("a 3-D area")?);
                    (sheets_prefix(ixti, context)?, (rows, columns))
                };
                let area = if base == 0x3D {
                    "#REF!".to_owned()
                } else {
                    area_text(rows, columns, false, origin, layout)
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

/// A cell address as a token holds it: the row and the raw column word with its relative flags in
/// bits 14 (column) and 15 (row). BIFF5 stores a 14-bit row with both flags in the row word and a
/// byte for the column.
fn read_cell(cursor: &mut Cursor<'_>, layout: Layout) -> Read<(u32, u16)> {
    Ok(match layout {
        Layout::Xlsb => (cursor.u32("a reference's row")?, cursor.u16("a reference's column")?),
        Layout::Biff8 => (
            u32::from(cursor.u16("a reference's row")?),
            cursor.u16("a reference's column")?,
        ),
        Layout::Biff3 | Layout::Biff4 | Layout::Biff5 => {
            let row = cursor.u16("a reference's row")?;
            let column = cursor.u8("a reference's column")?;
            (u32::from(row & 0x3FFF), u16::from(column) | (row & 0xC000))
        }
    })
}

/// A cell range address as a token holds it: (first row, last row), (first column word, last
/// column word); see [`read_cell`].
fn read_area(cursor: &mut Cursor<'_>, layout: Layout) -> Read<((u32, u32), (u16, u16))> {
    Ok(match layout {
        Layout::Xlsb => (
            (cursor.u32("an area")?, cursor.u32("an area")?),
            (cursor.u16("an area")?, cursor.u16("an area")?),
        ),
        Layout::Biff8 => (
            (u32::from(cursor.u16("an area")?), u32::from(cursor.u16("an area")?)),
            (cursor.u16("an area")?, cursor.u16("an area")?),
        ),
        Layout::Biff3 | Layout::Biff4 | Layout::Biff5 => {
            let (first, last) = (cursor.u16("an area")?, cursor.u16("an area")?);
            let (c1, c2) = (cursor.u8("an area")?, cursor.u8("an area")?);
            (
                (u32::from(first & 0x3FFF), u32::from(last & 0x3FFF)),
                (u16::from(c1) | (first & 0xC000), u16::from(c2) | (last & 0xC000)),
            )
        }
    })
}

/// Code page `page` bytes as UTF-16 units.
fn byte_units(page: u16, bytes: &[u8]) -> Read<Vec<u16>> {
    crate::xls::codepage::decode(page, bytes)
        .map(|text| text.encode_utf16().collect())
        .map_err(bad)
}

/// The qualifier of a BIFF5 3-D token: a negative index to the local link table, then 8 unused
/// bytes and the first and last sheet of this workbook (`0xFFFF` for a deleted one); a positive
/// index names an external workbook's sheet and 12 bytes follow that are not used.
fn biff5_prefix(cursor: &mut Cursor<'_>, context: &Context<'_>) -> Read<Option<String>> {
    let index = cursor.u16("a 3-D reference")? as i16;
    if index >= 0 {
        cursor.skip(12, "a 3-D reference")?;
        return sheets_prefix((index - 1) as u16, context);
    }
    cursor.skip(8, "a 3-D reference")?;
    let (first, last) = (cursor.u16("a 3-D reference")?, cursor.u16("a 3-D reference")?);
    if first == 0xFFFF || last == 0xFFFF {
        return Ok(None);
    }
    let name = |tab: u16| -> Read<&String> {
        if context.unavailable_sheets.contains(&u32::from(tab)) {
            return Err(bad(format!("a reference to omitted module sheet {tab}")));
        }
        context
            .sheets
            .get(usize::from(tab))
            .ok_or_else(|| bad(format!("the sheet {tab} of a reference")))
    };
    let (first, last) = (name(first)?, name(last)?);
    let joined = if first != last { format!("{first}:{last}") } else { first.clone() };
    Ok(Some(format!("{}!", sheet_prefix(&joined))))
}

/// A row and a column of a reference as A1 text: `$` before each absolute coordinate. A relative
/// coordinate of a PtgRefN or PtgAreaN (`offsets`) is an offset from `origin`, wrapping around
/// the grid's edge.
fn coordinate_text(
    row: u32,
    raw_column: u16,
    offsets: bool,
    origin: Origin,
    layout: Layout,
) -> (String, String) {
    let biff8 = layout != Layout::Xlsb;
    let column = u32::from(raw_column & if biff8 { 0x00FF } else { 0x3FFF });
    let column_relative = raw_column & 0x4000 != 0;
    let row_relative = raw_column & 0x8000 != 0;
    let (rows, columns) = match layout {
        Layout::Xlsb => (1_048_576, 16_384),
        Layout::Biff8 => (65_536, 256),
        Layout::Biff3 | Layout::Biff4 | Layout::Biff5 => (16_384, 256),
    };
    let row = if offsets && row_relative {
        // BIFF8 stores a relative row as a signed 16-bit offset, BIFF5 as 14 bits, xlsb as 32.
        let offset = match layout {
            Layout::Xlsb => i64::from(row as i32),
            Layout::Biff8 => i64::from(row as u16 as i16),
            Layout::Biff3 | Layout::Biff4 | Layout::Biff5 => i64::from(((row << 18) as i32) >> 18),
        };
        (i64::from(origin.row) + offset).rem_euclid(rows) as u32
    } else {
        row
    };
    let column = if offsets && column_relative {
        let offset = if biff8 {
            i64::from(column as u8 as i8)
        } else if column & 0x2000 != 0 {
            i64::from(column) - 0x4000
        } else {
            i64::from(column)
        };
        (i64::from(origin.column) + offset).rem_euclid(columns) as u32
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

fn cell_text(row: u32, column: u16, offsets: bool, origin: Origin, layout: Layout) -> String {
    let (c, r) = coordinate_text(row, column, offsets, origin, layout);
    format!("{c}{r}")
}

/// An area as A1 text: whole columns (`A:B`) when it spans every row, whole rows (`1:3`) when it
/// spans every column.
fn area_text(
    rows: (u32, u32),
    columns: (u16, u16),
    offsets: bool,
    origin: Origin,
    layout: Layout,
) -> String {
    let (c1, r1) = coordinate_text(rows.0, columns.0, offsets, origin, layout);
    let (c2, r2) = coordinate_text(rows.1, columns.1, offsets, origin, layout);
    let (last_row, last_column, mask) = match layout {
        Layout::Xlsb => (1_048_575, 16_383, 0x3FFF),
        Layout::Biff8 => (65_535, 255, 0x00FF),
        Layout::Biff3 | Layout::Biff4 | Layout::Biff5 => (16_383, 255, 0x00FF),
    };
    let all_rows = !offsets && rows.0 == 0 && rows.1 == last_row;
    let all_columns = !offsets && columns.0 & mask == 0 && columns.1 & mask == last_column;
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
        if book_prefix.is_empty() && tab >= 0 && context.unavailable_sheets.contains(&(tab as u32)) {
            return Err(bad(format!("a reference to omitted module sheet {tab}")));
        }
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
            if context.unavailable_sheets.contains(&sheet) {
                return Err(bad(format!("a name scoped to omitted module sheet {sheet}")));
            }
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

/// An array constant of a BIFF8 formula (PtgExtraArray, MS-XLS 2.5.198.8): the column count less
/// one (a byte), the row count less one (16 bits), then each value in row order, 8 bytes or a
/// string: 0 empty, 1 number, 2 string, 4 boolean, 16 error.
fn biff8_array_constant(extra: &mut Cursor<'_>) -> Read<String> {
    let columns = usize::from(extra.u8("an array's columns")?) + 1;
    let rows = usize::from(extra.u16("an array's rows")?) + 1;
    let mut out = String::from("{");
    for row in 0..rows {
        if row > 0 {
            out.push(';');
        }
        for column in 0..columns {
            if column > 0 {
                out.push(',');
            }
            match extra.u8("an array value")? {
                0x00 => {
                    extra.skip(8, "an empty array value")?;
                    out.push_str("\"\"");
                }
                0x01 => out.push_str(&formula_number(extra.f64("an array number")?)),
                0x02 => {
                    let count = usize::from(extra.u16("an array string")?);
                    let wide = extra.u8("an array string")? & 1 != 0;
                    let units: Vec<u16> = if wide {
                        extra.units(count, "an array string")?
                    } else {
                        extra
                            .bytes(count, "an array string")?
                            .iter()
                            .map(|b| u16::from(*b))
                            .collect()
                    };
                    out.push_str(&quoted(&text(&units)));
                }
                0x04 => {
                    let value = extra.u8("an array boolean")?;
                    extra.skip(7, "an array boolean")?;
                    out.push_str(if value != 0 { "TRUE" } else { "FALSE" });
                }
                0x10 => {
                    let code = extra.u8("an array error")?;
                    extra.skip(7, "an array error")?;
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

/// An array constant of a BIFF5 formula (MS-XLS-free: OpenOffice.org's Excel file format 2.5.8):
/// the column count (a byte, 0 for 256) and the row count (16 bits), then each value, 9 bytes
/// or a byte string: 0 empty, 1 number, 2 string, 4 boolean, 16 error.
fn biff5_array_constant(extra: &mut Cursor<'_>, code_page: u16) -> Read<String> {
    let columns = match extra.u8("an array's columns")? {
        0 => 256,
        n => usize::from(n),
    };
    let rows = usize::from(extra.u16("an array's rows")?);
    let mut out = String::from("{");
    for row in 0..rows {
        if row > 0 {
            out.push(';');
        }
        for column in 0..columns {
            if column > 0 {
                out.push(',');
            }
            match extra.u8("an array value")? {
                0x00 => {
                    extra.skip(8, "an empty array value")?;
                    out.push_str("\"\"");
                }
                0x01 => out.push_str(&formula_number(extra.f64("an array number")?)),
                0x02 => {
                    let count = usize::from(extra.u8("an array string")?);
                    let units = byte_units(code_page, extra.bytes(count, "an array string")?)?;
                    out.push_str(&quoted(&text(&units)));
                }
                0x04 => {
                    let value = extra.u8("an array boolean")?;
                    extra.skip(7, "an array boolean")?;
                    out.push_str(if value != 0 { "TRUE" } else { "FALSE" });
                }
                0x10 => {
                    let code = extra.u8("an array error")?;
                    extra.skip(7, "an array error")?;
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

/// The function a PtgAttrSum calls (MS-XLSB PtgAttrSum: SUM of one argument), from the catalogue.
fn attribute_sum_name() -> &'static str {
    calc_catalogue::declaration(calc_catalogue::aggregate::SUM)
        .expect("SUM is declared")
        .spelling()
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
