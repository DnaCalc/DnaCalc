//! Reads cell entry text into a neutral authored input or a typed refusal.
//!
//! Ported from OxFml (commit 15daac5) `crates/oxfml_core/src/consumer/runtime/mod.rs`
//! (`calc_value_from_cell_entry_text`, `worksheet_error_code_from_text`,
//! `source_text_is_formula_entry`), the entry verb DnaTreeCalc's grid skin used. The legacy reader
//! read every text Rust's `f64` parser did not as text; Excel reads many such texts as numbers,
//! dates, times, percentages or currency under the locale's number formats, so those are refused
//! here ([`EntryRefusal::NumberShaped`]) instead of being stored as text Excel would not hold.
//! The newer errors follow what Excel was measured to store for each typed error literal
//! (calc-doc's `tests/fixtures/d1/excel-errors.json`).

use calc_fml::{CalcEnvironment, read_number_text};
use calc_func::value::{EXCEL_TEXT_MAX_UTF16_CODE_UNITS, ExcelText, WorksheetErrorCode};
use std::fmt;

/// A typed authored entry, independent of the workbook document model.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Number(f64),
    Text(ExcelText),
    Logical(bool),
    Error(WorksheetErrorCode),
    Formula(ExcelText),
}

/// Why [`interpret_entry`] gives no input for a text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryRefusal {
    /// The text is `=` alone: a formula entry with no formula.
    EmptyFormula,
    /// The text is longer than a cell's text can be.
    TooLong {
        /// Length of the entry in UTF-16 code units.
        units: usize,
    },
    /// Excel may read the text as a number, date, time, percentage or currency under the locale's
    /// number formats, or as a number rounded to 15 significant digits, or (a leading `+` or `-`)
    /// as a formula. That reading is not modelled (dc-b1l): with no locale profile, or where the
    /// profile's reading is not modelled ([`interpret_entry_in`]).
    NumberShaped {
        /// Original entry text whose numeric interpretation is unavailable.
        text: String,
    },
    /// Excel's formula entry refuses the formula by `rule` ([`calc_fml::check_entry`]), at `span`
    /// of the entered text: the grid shows Excel's "There's a problem with this
    /// formula" and stores nothing (dc-mx3x). The file loader and the corpus verdict ask the same
    /// check.
    Rejected {
        /// Span of the input identified by the formula entry check.
        span: calc_fml::syntax::TextSpan,
        /// Rule that rejected the formula entry.
        rule: calc_fml::Rejection,
    },
}

impl fmt::Display for EntryRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntryRefusal::EmptyFormula => f.write_str("the entry is a formula with no text"),
            EntryRefusal::TooLong { units } => {
                write!(
                    f,
                    "the entry is {units} UTF-16 code units, longer than a cell holds"
                )
            }
            EntryRefusal::NumberShaped { text } => write!(
                f,
                "Excel may read {text:?} as a number, date, time or formula under its locale; that reading is not modelled (dc-b1l)"
            ),
            EntryRefusal::Rejected { rule, .. } => {
                write!(f, "there's a problem with this formula: {rule}")
            }
        }
    }
}

impl std::error::Error for EntryRefusal {}

/// What a cell holds after `text` is entered into it: `None` for an empty text, which clears the
/// cell (Excel's empty commit), or the authored input.
///
/// - A text starting with `=` is a formula, its text what follows the `=`. A formula Excel's
///   entry refuses by a measured rule ([`calc_fml::check_entry`]) is refused
///   ([`EntryRefusal::Rejected`]); the host determines the validity of other formulas.
/// - A text starting with `'` is the text after it.
/// - `TRUE` and `FALSE`, in any case, are logicals.
/// - A classic error literal (`#DIV/0!`, `#N/A` and the others, in any case) is that error;
///   `#GETTING_DATA` is `#N/A`, and the newer errors (`#SPILL!`, `#CALC!` and the others) are
///   text, as Excel stores them when typed.
/// - A plain decimal number (optional sign, digits, an optional point and exponent) of at most 15
///   significant digits is that number.
/// - A text that Excel may read as a number in another form is refused
///   ([`EntryRefusal::NumberShaped`]).
/// - Every other text is text.
pub fn interpret_entry(text: &str) -> Result<Option<Entry>, EntryRefusal> {
    interpret_entry_in(text, None)
}

/// [`interpret_entry`] under the host's calculation environment: with a locale profile, a text
/// that is not a formula, a forced text, a logical or an error literal is read as VALUE reads it
/// under that profile, date system and clock ([`calc_fml::read_number_text`]), a number when it
/// reads as one and text when it does not (dc-10y). Excel 16.0.20430 (2026-10-25, en-ZA, each of 73
/// texts entered through `Range.FormulaLocal`, which reads an entry as the user's locale does) held
/// a number exactly where `VALUE` of the same text gave one, the same number, and the text where
/// `VALUE` gave #VALUE!: `1 000` 1000, `R5` 5, `5%` 0.05, `(5)` -5, `1 1/2` 1.5, `1/2/20` 36942
/// (2001-02-20), `12:30 PM` 0.5208..., while `1,000`, `$5` and `3/4/2020` stayed text. A reading
/// that is not modelled (a date without a year when the host has no clock, say) is refused, as is
/// a text starting with `+` or `-` that reads as no number, which Excel's entry may take for a
/// formula, and a plain number of more than 15 significant digits, which Excel rounds. The number
/// format Excel gives such a cell (a date's, a percentage's) is not set.
pub fn interpret_entry_in(
    text: &str,
    environment: Option<&CalcEnvironment>,
) -> Result<Option<Entry>, EntryRefusal> {
    if text.is_empty() {
        return Ok(None);
    }
    if let Some(formula) = text.strip_prefix('=') {
        if formula.is_empty() {
            return Err(EntryRefusal::EmptyFormula);
        }
        let formula = ExcelText::from_utf16_code_units(formula.encode_utf16().collect());
        // Apply Excel's formula-entry rules to the typed text. A parser diagnostic may be
        // syntax this parser does not hold. The compiler reports it.
        if let Err(rejection) = calc_fml::check_entry(&calc_fml::parse_formula(text), false)
            && rejection.rule.is_measured()
        {
            return Err(EntryRefusal::Rejected {
                span: rejection.span,
                rule: rejection.rule,
            });
        }
        return Ok(Some(Entry::Formula(formula)));
    }
    if let Some(forced) = text.strip_prefix('\'') {
        return cell_text(forced).map(Some);
    }
    if text.eq_ignore_ascii_case("TRUE") {
        return Ok(Some(Entry::Logical(true)));
    }
    if text.eq_ignore_ascii_case("FALSE") {
        return Ok(Some(Entry::Logical(false)));
    }
    if let Some(error) = error_literal(text) {
        return Ok(Some(error));
    }
    if is_plain_number(text) {
        if significant_digits(text) <= 15
            && let Some(number) = text.parse::<f64>().ok().filter(|number| number.is_finite())
        {
            return Ok(Some(Entry::Number(number)));
        }
        return Err(EntryRefusal::NumberShaped {
            text: text.to_owned(),
        });
    }
    if let Some(environment) = environment.filter(|environment| environment.profile.is_some()) {
        let units: Vec<u16> = text.encode_utf16().collect();
        let refused = || EntryRefusal::NumberShaped {
            text: text.to_owned(),
        };
        return match read_number_text(&units, environment) {
            Ok(Some(number)) if number.is_finite() => Ok(Some(Entry::Number(number))),
            Ok(Some(_)) => Err(refused()),
            Ok(None) if text.starts_with(['+', '-']) => Err(refused()),
            Ok(None) => cell_text(text).map(Some),
            Err(()) => Err(refused()),
        };
    }
    if number_shaped(text) {
        return Err(EntryRefusal::NumberShaped {
            text: text.to_owned(),
        });
    }
    cell_text(text).map(Some)
}

fn cell_text(text: &str) -> Result<Entry, EntryRefusal> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let count = units.len();
    if count > EXCEL_TEXT_MAX_UTF16_CODE_UNITS {
        return Err(EntryRefusal::TooLong { units: count });
    }
    Ok(Entry::Text(ExcelText::from_utf16_code_units(units)))
}

/// The input an error literal is stored as when typed into a cell (measured,
/// `tests/fixtures/d1/excel-errors.json` of calc-doc).
fn error_literal(text: &str) -> Option<Entry> {
    use WorksheetErrorCode::*;
    let code = match text.to_ascii_uppercase().as_str() {
        "#NULL!" => Null,
        "#DIV/0!" => Div0,
        "#VALUE!" => Value,
        "#REF!" => Ref,
        "#NAME?" => Name,
        "#NUM!" => Num,
        "#N/A" | "#GETTING_DATA" => NA,
        _ => return None,
    };
    Some(Entry::Error(code))
}

/// Whether `text` is a plain decimal number: an optional sign, digits with at most one point
/// (at least one digit), and an optional exponent `e` or `E` with an optional sign and digits.
pub fn is_plain_number(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut at = 0;
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        at += 1;
    }
    let mut digits = 0;
    let mut point = false;
    while let Some(&b) = bytes.get(at) {
        match b {
            b'0'..=b'9' => digits += 1,
            b'.' if !point => point = true,
            _ => break,
        }
        at += 1;
    }
    if digits == 0 {
        return false;
    }
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let start = at;
        while matches!(bytes.get(at), Some(b'0'..=b'9')) {
            at += 1;
        }
        if at == start {
            return false;
        }
    }
    at == bytes.len()
}

/// The significant digits of a plain number's mantissa: its digits without leading zeros and,
/// after a point, without trailing zeros.
fn significant_digits(text: &str) -> usize {
    let mantissa = text
        .split(['e', 'E'])
        .next()
        .unwrap_or("")
        .trim_start_matches(['+', '-']);
    let mut digits: String = mantissa.chars().filter(|c| c.is_ascii_digit()).collect();
    if mantissa.contains('.') {
        digits = digits.trim_end_matches('0').to_owned();
    }
    digits.trim_start_matches('0').len()
}

/// Whether Excel may read `text` other than as text: it holds a digit and only characters that
/// Excel's number, date, time, percentage and currency readings use, or it starts with `+` or `-`
/// (which Excel reads as the start of a formula).
fn number_shaped(text: &str) -> bool {
    if text.len() > 1 && text.starts_with(['+', '-']) {
        return true;
    }
    text.chars().any(|c| c.is_ascii_digit())
        && text.chars().all(|c| {
            c.is_ascii_digit()
                || matches!(
                    c,
                    ' ' | '.'
                        | ','
                        | '%'
                        | '$'
                        | '/'
                        | ':'
                        | '('
                        | ')'
                        | '-'
                        | '+'
                        | 'e'
                        | 'E'
                        | 'a'
                        | 'A'
                        | 'p'
                        | 'P'
                        | 'm'
                        | 'M'
                        | '\u{a3}'
                        | '\u{20ac}'
                        | '\u{a5}'
                )
        })
}
