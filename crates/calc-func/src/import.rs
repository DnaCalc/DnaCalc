//! IMPORTCSV and IMPORTTEXT: the file a workbook names, read into a dynamic array.
//!
//! calc-func reads no file itself. A host that can read implements [`ImportRoute`] (path in, bytes
//! out) and registers an [`ImportFunction`] for each function ([`IMPORT_FUNCTIONS`]) under
//! [`IMPORT_PROVIDER`]; a host with no route calculates the catalogue entries instead, which give
//! what Excel gives when a file cannot be read. Excel runs the import on its async path (#BUSY!
//! first); the final value is what is modelled, and a route answers at once.
//!
//! Measured in Excel 16.0.20527 (64-bit) on 2026-11-08 over small files written for the purpose
//! (`tests/fixtures/import`, bead dc-z8c1); the build has both functions, which Microsoft lists
//! for the Insider Beta channel. The arguments:
//!
//! - `IMPORTCSV(path, [skip_rows], [take_rows], [text_only], [locale])` and
//!   `IMPORTTEXT(path, [delimiter], [skip_rows], [take_rows], [encoding], [text_only], [locale])`;
//!   one more argument than these is refused at entry. Both spill.
//! - **path.** Text, spaces around it trimmed, an absolute path (`C:\dir\file.csv`) or a
//!   `file:///` URL of one. A relative path, a folder, a missing file, a wildcard, empty text, a
//!   number and a logical are #VALUE!; an error is itself. An array lifts, and each element's
//!   import gives its first cell. A file with no bytes is #CALC!.
//! - **skip_rows, take_rows.** Whole numbers (a fraction truncates, TRUE is 1); text is #VALUE!.
//!   A negative skip drops rows from the end, a negative take keeps the last rows; skip applies
//!   first. No row left (skip past the end, take 0) is #CALC!. A take beyond the end takes all.
//! - **text_only.** 0 or FALSE converts fields to values; 1 or TRUE leaves every field text; any
//!   other number and text are #VALUE!. With a locale given, TRUE is #VALUE!.
//! - **locale.** A BCP 47 tag (`de-DE`, `de`); another number or text is #VALUE!. It sets the
//!   decimal and group separators and the date order of the conversion. The default is the
//!   operating system's.
//! - **delimiter** (IMPORTTEXT). Text: a separator of any length, tab when omitted or empty. A row
//!   of ascending whole numbers (`{1,3}`) gives fixed widths: the columns start at those
//!   positions (counted from 0) and the text before the first is dropped. A scalar number is
//!   `{n}`. Descending, repeated and negative numbers, a column of numbers and an array of texts
//!   are #VALUE!.
//! - **encoding** (IMPORTTEXT). A Windows code page number; 0 is the system's (1252 here). A
//!   byte order mark (UTF-8, UTF-16 either way) decides first, whatever is given. The default is
//!   UTF-8 read leniently: an invalid byte or unfinished sequence is U+FFFD. Text, 1, -1 and TRUE
//!   are #VALUE!.
//!
//! The file: rows end at `\n`, `\r\n` or `\r`, and a final end adds no row. A field starting with
//! `"` runs to the closing quote (`""` is one), may hold line ends, and takes what follows the
//! closing quote up to the delimiter (`"z"w` is `zw`); a quote anywhere else is a character, so
//! a space before it keeps it (` "x"`). An empty line is a row of one empty text. Fields are
//! not trimmed. The array is as wide as the widest row; a short row is padded with empty text,
//! and a field beyond the width of the first row returned is #VALUE!.
//!
//! Conversion (unless text_only): a field is read by the number-text grammar of the locale
//! (numbers with their group and decimal separators, a sign or parentheses, a percent sign,
//! the locale's currency, dates and times, which become serial numbers); TRUE and FALSE in any
//! case, spaces around them allowed, are logicals; a number whose exponent overflows is #NUM!;
//! anything else stays text, including the error names (`#N/A`). Surrounding quotes do not stop
//! the conversion (`"1"` is 1), and empty fields are empty text.

use crate::locale::{CalcEnvironment, DateSystem, FormatProfile};
use crate::udf::{UserFunction, UserFunctionContext};
use crate::value::{ArrayShape, CalcArray, CalcValue, CoreRef, ExcelText, WorksheetErrorCode};
use std::{fmt, rc::Rc};

/// The provider key the import functions are registered under in a callable table.
pub use calc_catalogue::{IMPORT_PROVIDER, ImportKind};
/// The functions that use the route, by catalogue spelling.
pub const IMPORT_FUNCTIONS: [&str; 2] = [
    calc_catalogue::ported::find(
        calc_catalogue::ported::importing::ENTRIES,
        calc_catalogue::FunctionId::new(calc_catalogue::ported::importing::CODE, 1),
    )
    .spelling,
    calc_catalogue::ported::find(
        calc_catalogue::ported::importing::ENTRIES,
        calc_catalogue::FunctionId::new(calc_catalogue::ported::importing::CODE, 2),
    )
    .spelling,
];

/// A host's way to read a file: the whole file as bytes, for an absolute path (the `file:///`
/// form already turned into one), or `None` when it cannot (missing, a folder, no access).
pub trait ImportRoute: fmt::Debug {
    fn read(&self, path: &str) -> Option<Vec<u8>>;
}

/// Why a call ends before it has an array: with the value Excel gives, or refused because the
/// answer is not modelled.
#[derive(Debug)]
enum Stop {
    Answer(CalcValue),
    Refused(&'static str),
}

type Failure = Stop;

fn value_error() -> Failure {
    Stop::Answer(CalcValue::error(WorksheetErrorCode::Value))
}

fn calc_error() -> Failure {
    Stop::Answer(CalcValue::error(WorksheetErrorCode::Calc))
}

/// How a row of the file is cut into fields.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Delimiter {
    Text(Vec<char>),
    Widths(Vec<usize>),
}

/// A text encoding the import reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Encoding {
    /// UTF-8, read leniently (the default).
    Utf8,
    Windows1252,
    Latin1,
    Ascii,
    Cp437,
    Utf16Le,
    Utf16Be,
}

/// One call's arguments, read.
#[derive(Debug)]
struct Request {
    path: String,
    delimiter: Delimiter,
    skip: Option<i64>,
    take: Option<i64>,
    encoding: Encoding,
    text_only: bool,
    locale: Option<String>,
}

/// An error argument is itself.
fn passes(value: &CalcValue) -> Result<(), Failure> {
    match value.core_ref() {
        CoreRef::Error(_) => Err(Stop::Answer(value.clone())),
        _ => Ok(()),
    }
}

/// A whole-number argument: a number truncates, a logical is 1 or 0, blank is none, and text is
/// #VALUE!.
fn whole(value: &CalcValue) -> Result<Option<i64>, Failure> {
    passes(value)?;
    match value.core_ref() {
        CoreRef::Missing | CoreRef::Empty => Ok(None),
        CoreRef::Number(number) if number.is_finite() => Ok(Some(number.trunc() as i64)),
        CoreRef::Logical(logical) => Ok(Some(i64::from(logical))),
        _ => Err(value_error()),
    }
}

fn windows_path(text: &str) -> Option<String> {
    let trimmed = text.trim_matches(' ');
    let path = match trimmed.get(..8) {
        Some(head) if head.eq_ignore_ascii_case("file:///") => &trimmed[8..],
        _ => trimmed,
    };
    let mut chars = path.chars();
    let (drive, colon, slash) = (chars.next(), chars.next(), chars.next());
    let absolute = drive.is_some_and(|c| c.is_ascii_alphabetic())
        && colon == Some(':')
        && matches!(slash, Some('\\' | '/'));
    absolute.then(|| path.to_owned())
}

impl Request {
    fn read(kind: ImportKind, arguments: &[CalcValue]) -> Result<Self, Failure> {
        let missing = CalcValue::missing();
        let at = |index: usize| arguments.get(index).unwrap_or(&missing);
        let (delimiter_at, skip_at, take_at, encoding_at, text_only_at, locale_at) = match kind {
            ImportKind::Csv => (None, 1, 2, None, 3, 4),
            ImportKind::Text => (Some(1), 2, 3, Some(4), 5, 6),
        };
        passes(at(0))?;
        let path = match at(0).core_ref() {
            CoreRef::Text(text) => windows_path(&text.to_string_lossy()).ok_or_else(value_error)?,
            _ => return Err(value_error()),
        };
        let delimiter = match delimiter_at {
            None => Delimiter::Text(vec![',']),
            Some(index) => delimiter(at(index))?,
        };
        let skip = whole(at(skip_at))?;
        let take = whole(at(take_at))?;
        let encoding = match encoding_at {
            None => Encoding::Utf8,
            Some(index) => encoding(at(index))?,
        };
        let text_only = match at(text_only_at).core_ref() {
            CoreRef::Error(_) => return Err(Stop::Answer(at(text_only_at).clone())),
            CoreRef::Missing | CoreRef::Empty => false,
            CoreRef::Logical(flag) => flag,
            CoreRef::Number(number) if number == 0.0 => false,
            CoreRef::Number(number) if number == 1.0 => true,
            _ => return Err(value_error()),
        };
        passes(at(locale_at))?;
        let locale = match at(locale_at).core_ref() {
            CoreRef::Missing | CoreRef::Empty => None,
            CoreRef::Text(text) => Some(text.to_string_lossy()),
            _ => return Err(value_error()),
        };
        if text_only && locale.is_some() {
            return Err(value_error());
        }
        Ok(Self {
            path,
            delimiter,
            skip,
            take,
            encoding,
            text_only,
            locale,
        })
    }
}

fn delimiter(value: &CalcValue) -> Result<Delimiter, Failure> {
    passes(value)?;
    let widths = |numbers: Vec<f64>| -> Result<Delimiter, Failure> {
        let mut widths = Vec::with_capacity(numbers.len());
        for number in numbers {
            if !number.is_finite() || number < 0.0 {
                return Err(value_error());
            }
            let width = number.trunc() as usize;
            if widths.last().is_some_and(|last| *last >= width) {
                return Err(value_error());
            }
            widths.push(width);
        }
        Ok(Delimiter::Widths(widths))
    };
    match value.core_ref() {
        CoreRef::Missing | CoreRef::Empty => Ok(Delimiter::Text(vec!['\t'])),
        CoreRef::Text(text) => {
            let chars: Vec<char> = text.to_string_lossy().chars().collect();
            Ok(Delimiter::Text(if chars.is_empty() {
                vec!['\t']
            } else {
                chars
            }))
        }
        CoreRef::Number(number) => widths(vec![number]),
        CoreRef::Logical(logical) => widths(vec![f64::from(u8::from(logical))]),
        CoreRef::Array(array) => {
            let shape = array.shape();
            if shape.rows != 1 {
                return Err(value_error());
            }
            let mut numbers = Vec::with_capacity(shape.cols);
            for col in 0..shape.cols {
                match array.get(0, col).map(CalcValue::core_ref) {
                    Some(CoreRef::Number(number)) => numbers.push(number),
                    _ => return Err(value_error()),
                }
            }
            widths(numbers)
        }
        _ => Err(value_error()),
    }
}

fn encoding(value: &CalcValue) -> Result<Encoding, Failure> {
    passes(value)?;
    match value.core_ref() {
        CoreRef::Missing | CoreRef::Empty => Ok(Encoding::Utf8),
        CoreRef::Number(number) => match number {
            n if n == 0.0 || n == 1252.0 => Ok(Encoding::Windows1252),
            n if n == 65001.0 => Ok(Encoding::Utf8),
            n if n == 28591.0 => Ok(Encoding::Latin1),
            n if n == 20127.0 => Ok(Encoding::Ascii),
            n if n == 437.0 => Ok(Encoding::Cp437),
            n if n == 1200.0 => Ok(Encoding::Utf16Le),
            n if n == 1201.0 => Ok(Encoding::Utf16Be),
            // Windows takes every installed code page (850, 1250 to 1258, 932, 936, 20105, 65000
            // and 12000 were read); 1, 99999 and the negatives are #VALUE!. Only the pages above
            // are decoded here, so the others are not answered with a guess.
            n if n.fract() == 0.0 && n > 1.0 && n < 99999.0 => Err(Stop::Refused(
                "an import code page this build does not decode",
            )),
            _ => Err(value_error()),
        },
        _ => Err(value_error()),
    }
}

// --- decoding -----------------------------------------------------------------------------------

/// Code page 1252, bytes 80 to 9F (the five unassigned bytes read as their C1 controls).
const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];
/// Code page 437, bytes 80 to FF.
const CP437_HIGH: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}', '\u{00E7}',
    '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}', '\u{00EE}', '\u{00EC}', '\u{00C4}', '\u{00C5}',
    '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}', '\u{00F9}',
    '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00A2}', '\u{00A3}', '\u{00A5}', '\u{20A7}', '\u{0192}',
    '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}', '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}',
    '\u{00BF}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}',
    '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}',
    '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}',
    '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}',
    '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}',
    '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}',
    '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}', '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}',
    '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}', '\u{2248}',
    '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];

fn utf16_text(bytes: &[u8], little: bool) -> String {
    let units = bytes.chunks(2).map(|pair| {
        let (a, b) = (pair[0], pair.get(1).copied().unwrap_or(0));
        if little {
            u16::from_le_bytes([a, b])
        } else {
            u16::from_be_bytes([a, b])
        }
    });
    char::decode_utf16(units)
        .map(|c| c.unwrap_or('\u{FFFD}'))
        .collect()
}

/// The file's text: a byte order mark decides first, whatever encoding was named.
fn decode(bytes: &[u8], encoding: Encoding) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16_text(rest, true);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16_text(rest, false);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    match encoding {
        Encoding::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
        Encoding::Utf16Le => utf16_text(bytes, true),
        Encoding::Utf16Be => utf16_text(bytes, false),
        Encoding::Latin1 => bytes.iter().map(|&b| char::from(b)).collect(),
        Encoding::Ascii => bytes
            .iter()
            .map(|&b| if b < 0x80 { char::from(b) } else { '?' })
            .collect(),
        Encoding::Windows1252 => bytes
            .iter()
            .map(|&b| match b {
                0x80..=0x9F => CP1252_HIGH[usize::from(b - 0x80)],
                _ => char::from(b),
            })
            .collect(),
        Encoding::Cp437 => bytes
            .iter()
            .map(|&b| {
                if b < 0x80 {
                    char::from(b)
                } else {
                    CP437_HIGH[usize::from(b - 0x80)]
                }
            })
            .collect(),
    }
}

// --- rows and fields ----------------------------------------------------------------------------

fn matches_at(chars: &[char], at: usize, delimiter: &[char]) -> bool {
    chars.get(at..at + delimiter.len()) == Some(delimiter)
}

/// The rows of a delimited text, each a list of field texts (see the module notes).
fn delimited_rows(text: &str, delimiter: &[char]) -> Vec<Vec<String>> {
    let chars: Vec<char> = text.chars().collect();
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut at = 0;
    let mut field_start = true;
    while at < chars.len() {
        let c = chars[at];
        if field_start && c == '"' {
            at += 1;
            while at < chars.len() {
                if chars[at] == '"' {
                    if chars.get(at + 1) == Some(&'"') {
                        field.push('"');
                        at += 2;
                        continue;
                    }
                    at += 1;
                    break;
                }
                field.push(chars[at]);
                at += 1;
            }
            field_start = false;
        } else if matches_at(&chars, at, delimiter) {
            row.push(std::mem::take(&mut field));
            at += delimiter.len();
            field_start = true;
        } else if c == '\r' || c == '\n' {
            at += 1;
            if c == '\r' && chars.get(at) == Some(&'\n') {
                at += 1;
            }
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
            field_start = true;
        } else {
            field.push(c);
            at += 1;
            field_start = false;
        }
    }
    if !row.is_empty() || !field.is_empty() || !field_start {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// The rows of a fixed-width text: columns start at the given positions.
fn fixed_rows(text: &str, widths: &[usize]) -> Vec<Vec<String>> {
    let mut lines = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut at = 0;
    let mut line = Vec::new();
    while at < chars.len() {
        let c = chars[at];
        at += 1;
        if c == '\r' || c == '\n' {
            if c == '\r' && chars.get(at) == Some(&'\n') {
                at += 1;
            }
            lines.push(std::mem::take(&mut line));
        } else {
            line.push(c);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
        .into_iter()
        .map(|line| {
            widths
                .iter()
                .enumerate()
                .map(|(index, &start)| {
                    let end = widths.get(index + 1).copied().unwrap_or(line.len());
                    let (start, end) = (start.min(line.len()), end.min(line.len()));
                    line[start..end].iter().collect()
                })
                .collect()
        })
        .collect()
}

/// Skip, then take, by the module notes: a negative count works from the end.
fn window(mut rows: Vec<Vec<String>>, skip: Option<i64>, take: Option<i64>) -> Vec<Vec<String>> {
    if let Some(skip) = skip {
        let count = usize::try_from(skip.unsigned_abs())
            .unwrap_or(usize::MAX)
            .min(rows.len());
        if skip >= 0 {
            rows.drain(..count);
        } else {
            rows.truncate(rows.len() - count);
        }
    }
    if let Some(take) = take {
        let count = usize::try_from(take.unsigned_abs())
            .unwrap_or(usize::MAX)
            .min(rows.len());
        if take >= 0 {
            rows.truncate(count);
        } else {
            rows.drain(..rows.len() - count);
        }
    }
    rows
}

// --- conversion ---------------------------------------------------------------------------------

/// The number-text profile of a call: the locale named, or the operating system's (`en-ZA`
/// here, whose decimal separator is the comma and whose group separator is the space).
fn profile(locale: Option<&str>) -> Result<FormatProfile, Failure> {
    let Some(tag) = locale else {
        return FormatProfile::for_culture("en-ZA")
            .and_then(|profile| profile.with_separators(",", " "))
            .ok_or(Stop::Refused("the default import locale has no profile"));
    };
    let tag = tag.trim();
    if let Some(profile) = FormatProfile::for_culture(tag) {
        return Ok(profile);
    }
    // A language alone names its main region (`de` is `de-DE`).
    if tag.len() == 2 && tag.is_ascii() {
        let region = tag.to_ascii_uppercase();
        if let Some(profile) = FormatProfile::for_culture(&format!("{tag}-{region}")) {
            return Ok(profile);
        }
    }
    Err(value_error())
}

fn text_value(text: &str) -> CalcValue {
    CalcValue::text(ExcelText::from_utf16_code_units(
        text.encode_utf16().collect(),
    ))
}

/// Whether `text` is a number written with an exponent too large for binary64.
fn overflowing(text: &str) -> bool {
    let (mantissa, exponent) = match text.split_once(['e', 'E']) {
        Some(parts) => parts,
        None => return false,
    };
    let digits = mantissa.trim_start_matches(['+', '-']);
    let numeric = !digits.is_empty()
        && digits
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
        && digits.chars().any(|c| c.is_ascii_digit());
    let exponent_digits = exponent.trim_start_matches(['+', '-']);
    numeric
        && !exponent_digits.is_empty()
        && exponent_digits.chars().all(|c| c.is_ascii_digit())
        && format!(
            "{}{}e{exponent}",
            if mantissa.starts_with('-') { "-" } else { "" },
            digits.replace(',', ".")
        )
        .parse::<f64>()
        .is_ok_and(f64::is_infinite)
}

/// A number in the import's own grammar (the same under every locale, with the locale's
/// separators and currency): a sign before, parentheses around or a minus after; a `%` before or
/// after; the locale's currency symbol before or after; digits with the group separator anywhere
/// among them (`1.5` is 15 where the group separator is the point), the decimal separator, an
/// exponent. A space after a sign, a second sign and a decimal point with no digit are not it.
fn own_number(field: &str, profile: &FormatProfile) -> Option<f64> {
    let mut text = field.trim_matches(' ');
    let mut negative = false;
    if let Some(inner) = text
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
    {
        negative = true;
        text = inner;
    } else if let Some(rest) = text.strip_prefix('-') {
        negative = true;
        text = rest;
    } else if let Some(rest) = text.strip_prefix('+') {
        text = rest;
    } else if let Some(rest) = text.strip_suffix('-') {
        negative = true;
        text = rest;
    }
    // A space next to the sign is not part of a number.
    if text.starts_with(' ') || text.ends_with(' ') {
        return None;
    }
    let mut percent = false;
    if let Some(rest) = text.strip_prefix('%') {
        percent = true;
        text = rest.trim_start_matches(' ');
    } else if let Some(rest) = text.strip_suffix('%') {
        percent = true;
        text = rest.trim_end_matches(' ');
    }
    let currency = profile.currency_symbol;
    if !currency.is_empty() {
        if let Some(rest) = text.strip_prefix(currency) {
            text = rest.trim_start_matches(' ');
        } else if let Some(rest) = text.strip_suffix(currency) {
            text = rest.trim_end_matches(' ');
        }
    }
    let (decimal, group) = (profile.decimal_separator, profile.thousands_separator);
    let mut digits = String::new();
    let (mut seen_digit, mut seen_decimal) = (false, false);
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if c.is_ascii_digit() {
            digits.push(c);
            seen_digit = true;
            rest = &rest[1..];
        } else if rest.starts_with(decimal) && !seen_decimal {
            digits.push('.');
            seen_decimal = true;
            rest = &rest[decimal.len()..];
        } else if rest.starts_with(group) && group != decimal && seen_digit && !seen_decimal {
            // A group separator belongs before the decimal separator (`2.000,25` is no number
            // under en-US).
            rest = &rest[group.len()..];
        } else if (c == 'e' || c == 'E') && seen_digit {
            let exponent = &rest[1..];
            let body = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
            if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            digits.push('e');
            digits.push_str(exponent);
            rest = "";
        } else {
            return None;
        }
    }
    if !seen_digit {
        return None;
    }
    if digits.starts_with('.') {
        digits.insert(0, '0');
    }
    if digits.contains(".e") {
        digits = digits.replace(".e", ".0e");
    }
    if digits.ends_with('.') {
        digits.push('0');
    }
    let mut number: f64 = digits.parse().ok()?;
    if !number.is_finite() {
        return None;
    }
    if percent {
        number /= 100.0;
    }
    // Minus zero is zero.
    Some(if negative && number != 0.0 {
        -number
    } else {
        number
    })
}

/// Whether `text` starts as `yyyy-m-d`, a date written year first.
fn iso_date(text: &str) -> bool {
    let mut parts = text.splitn(3, '-');
    let (Some(year), Some(month), Some(rest)) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let day = rest.split([' ', 'T']).next().unwrap_or_default();
    let digits = |part: &str, most: usize| {
        (1..=most).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_digit())
    };
    year.len() == 4 && digits(year, 4) && digits(month, 2) && digits(day, 2)
}

/// A field as the call's conversion reads it.
fn convert(field: &str, environment: &CalcEnvironment) -> Result<CalcValue, Failure> {
    if field.is_empty() {
        return Ok(text_value(field));
    }
    let trimmed = field.trim_matches(' ');
    if trimmed.eq_ignore_ascii_case("TRUE") {
        return Ok(CalcValue::logical(true));
    }
    if trimmed.eq_ignore_ascii_case("FALSE") {
        return Ok(CalcValue::logical(false));
    }
    if overflowing(trimmed) {
        return Ok(CalcValue::error(WorksheetErrorCode::Num));
    }
    let profile = environment
        .profile
        .as_ref()
        .expect("a conversion has a profile");
    if let Some(number) = own_number(field, profile) {
        return Ok(CalcValue::number(number));
    }
    // Dates and times are read by VALUE's grammar under the locale. What it reads as a number is
    // not the import's: `- 1` is text here and -1 there.
    // A symbol beyond ASCII that is no digit or space is in no date or time.
    if field.chars().any(|c| !c.is_ascii())
        && field
            .chars()
            .all(|c| c.is_ascii() || !(c.is_numeric() || c.is_whitespace()))
    {
        return Ok(text_value(field));
    }
    let units: Vec<u16> = field.encode_utf16().collect();
    // A year first and four digits long is read the same under every locale.
    let year_first = FormatProfile {
        short_date_order: crate::locale::DateComponentOrder::Ymd,
        ..*profile
    };
    let context = crate::value_text::ReadContext {
        profile: if iso_date(trimmed) {
            &year_first
        } else {
            profile
        },
        date_system: environment.date_system,
        now: environment.now,
    };
    match crate::value_text::read(&units, &context) {
        Ok(crate::value_text::Reading::DateTime(serial)) => Ok(CalcValue::number(serial)),
        Ok(crate::value_text::Reading::Number(_)) | Err(crate::value_text::Unread::Rejected) => {
            Ok(text_value(field))
        }
        Err(crate::value_text::Unread::NotModelled) => Err(Stop::Refused(
            "an import field that is not modelled as number text in this locale",
        )),
    }
}

/// The imported array of one call's bytes.
fn build(request: &Request, bytes: &[u8]) -> Result<CalcValue, Failure> {
    let text = decode(bytes, request.encoding);
    let rows = match &request.delimiter {
        Delimiter::Text(delimiter) => delimited_rows(&text, delimiter),
        Delimiter::Widths(widths) => fixed_rows(&text, widths),
    };
    let rows = window(rows, request.skip, request.take);
    let Some(first) = rows.first() else {
        return Err(calc_error());
    };
    // The first row sets the width; the one column more than it holds a longer row's next field.
    let base = first.len();
    let width = if rows.iter().any(|row| row.len() > base) {
        base + 1
    } else {
        base
    };
    let environment = if request.text_only {
        None
    } else {
        Some(CalcEnvironment {
            date_system: DateSystem::System1900,
            compatibility_version: 1,
            profile: Some(profile(request.locale.as_deref())?),
            now: None,
        })
    };
    let mut cells = Vec::with_capacity(rows.len() * width);
    for row in &rows {
        for column in 0..width {
            cells.push(match row.get(column) {
                Some(_) if column >= base => CalcValue::error(WorksheetErrorCode::Value),
                Some(field) => match &environment {
                    Some(environment) => convert(field, environment)?,
                    None => text_value(field),
                },
                None => text_value(""),
            });
        }
    }
    let shape = ArrayShape {
        rows: rows.len(),
        cols: width,
    };
    match CalcArray::new(shape, cells) {
        Some(array) if shape.rows == 1 && shape.cols == 1 => {
            Ok(array.get(0, 0).cloned().expect("one cell"))
        }
        Some(array) => Ok(CalcValue::array(array)),
        None => Err(Stop::Refused("an import's cells do not fill its shape")),
    }
}

// --- the function -------------------------------------------------------------------------------

/// One of the import functions as a host registers it, bound to the host's route.
#[derive(Debug)]
pub struct ImportFunction {
    kind: ImportKind,
    route: Rc<dyn ImportRoute>,
}

impl ImportFunction {
    /// The function of [`IMPORT_FUNCTIONS`] spelled `spelling`, on `route`.
    pub fn new(spelling: &str, route: Rc<dyn ImportRoute>) -> Option<Self> {
        let target = crate::catalogue()
            .resolve_surface(crate::Notation::Call, spelling)
            .ok()?;
        if target.spelling() != spelling {
            return None;
        }
        let crate::EntryKind::HostRoute {
            route: crate::HostRouteKind::Import(kind),
            ..
        } = target.kind()
        else {
            return None;
        };
        Some(Self { kind, route })
    }

    fn element(&self, arguments: &[CalcValue]) -> Result<CalcValue, Failure> {
        let request = Request::read(self.kind, arguments)?;
        let Some(bytes) = self.route.read(&request.path) else {
            return Err(value_error());
        };
        build(&request, &bytes)
    }

    fn lifted_call(&self, arguments: &[CalcValue]) -> Result<CalcValue, Failure> {
        // IMPORTTEXT's delimiter may be an array of widths: it is an argument, not a lift.
        let held = if self.kind == ImportKind::Text {
            arguments.get(1).cloned()
        } else {
            None
        };
        let mut lifting = arguments.to_vec();
        if held.is_some() {
            lifting[1] = CalcValue::missing();
        }
        let (shape, mut elements) = crate::fetch::lifted(&lifting);
        if let Some(delimiter) = held {
            for element in &mut elements {
                element[1] = delimiter.clone();
            }
        }
        if elements.len() == 1 {
            return self.element(&elements[0]);
        }
        // Each element of a lifted call gives its first cell.
        let mut cells = Vec::with_capacity(elements.len());
        for element in &elements {
            let value = match self.element(element) {
                Ok(value) => value,
                Err(Stop::Answer(value)) => value,
                Err(refused) => return Err(refused),
            };
            cells.push(match value.as_array() {
                Some(array) => array.get(0, 0).cloned().expect("a non-empty array"),
                None => value,
            });
        }
        CalcArray::new(shape, cells)
            .map(CalcValue::array)
            .ok_or(Stop::Refused(
                "a lifted import's cells do not fill its shape",
            ))
    }
}

impl UserFunction for ImportFunction {
    fn call(&self, arguments: &[CalcValue]) -> Result<CalcValue, String> {
        match self.lifted_call(arguments) {
            Ok(value) | Err(Stop::Answer(value)) => Ok(value),
            Err(Stop::Refused(why)) => Err(why.to_owned()),
        }
    }

    fn call_with_context(
        &self,
        arguments: &[CalcValue],
        context: &mut dyn UserFunctionContext,
    ) -> Result<CalcValue, String> {
        let arguments = arguments
            .iter()
            .map(|value| match value.as_reference() {
                Some(reference) => context.argument_values_of(reference),
                None => Ok(value.clone()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        match self.lifted_call(&arguments) {
            Ok(value) | Err(Stop::Answer(value)) => Ok(value),
            Err(Stop::Refused(why)) => Err(context.refuse_by(why, "dc-z8c1")),
        }
    }
}
