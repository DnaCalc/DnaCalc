//! Cell entry: the text a user types into a cell, read as Excel reads it, into the authored input
//! a [`crate::CellEdit`] sets, or a typed refusal where Excel's reading is not modelled.
//!
//! Ported from OxFml (commit 15daac5) `crates/oxfml_core/src/consumer/runtime/mod.rs`
//! (`calc_value_from_cell_entry_text`, `worksheet_error_code_from_text`,
//! `source_text_is_formula_entry`), the entry verb DnaTreeCalc's grid skin used. The legacy reader
//! read every text Rust's `f64` parser did not as text; Excel reads many such texts as numbers,
//! dates, times, percentages or currency under the locale's number formats, so those are refused
//! here ([`EntryRefusal::NumberShaped`]) instead of being stored as text Excel would not hold.
//! The newer errors follow what Excel was measured to store for each typed error literal
//! (calc-doc's `tests/fixtures/d1/excel-errors.json`).

use calc_doc_model::value::{ExcelText, WorksheetErrorCode};
use calc_doc_model::{CellInput, CellText, FiniteNumber, FormulaText};
use std::fmt;

/// Why [`interpret_entry`] gives no input for a text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryRefusal {
    /// The text is `=` alone: a formula entry with no formula.
    EmptyFormula,
    /// The text is longer than a cell's text can be.
    TooLong { units: usize },
    /// Excel may read the text as a number, date, time, percentage or currency under the locale's
    /// number formats, or as a number rounded to 15 significant digits, or (a leading `+` or `-`)
    /// as a formula. That reading is not modelled; it belongs to work package L3 (dc-m3u, locale
    /// parsing).
    NumberShaped { text: String },
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
                "Excel may read {text:?} as a number, date, time or formula under its locale; that reading is not modelled (L3, dc-m3u)"
            ),
        }
    }
}

impl std::error::Error for EntryRefusal {}

/// What a cell holds after `text` is entered into it: `None` for an empty text, which clears the
/// cell (Excel's empty commit), or the authored input.
///
/// - A text starting with `=` is a formula, its text what follows the `=`. Whether the formula is
///   valid is the edit's to decide ([`crate::CommandError::Document`]).
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
pub fn interpret_entry(text: &str) -> Result<Option<CellInput>, EntryRefusal> {
    if text.is_empty() {
        return Ok(None);
    }
    if let Some(formula) = text.strip_prefix('=') {
        let formula = FormulaText::new(ExcelText::from_utf16_code_units(
            formula.encode_utf16().collect(),
        ))
        .map_err(|_| EntryRefusal::EmptyFormula)?;
        return Ok(Some(CellInput::Formula(formula)));
    }
    if let Some(forced) = text.strip_prefix('\'') {
        return cell_text(forced).map(Some);
    }
    if text.eq_ignore_ascii_case("TRUE") {
        return Ok(Some(CellInput::Logical(true)));
    }
    if text.eq_ignore_ascii_case("FALSE") {
        return Ok(Some(CellInput::Logical(false)));
    }
    if let Some(error) = error_literal(text) {
        return Ok(Some(error));
    }
    if is_plain_number(text) {
        if significant_digits(text) <= 15
            && let Some(number) = text.parse::<f64>().ok().and_then(FiniteNumber::new)
        {
            return Ok(Some(CellInput::Number(number)));
        }
        return Err(EntryRefusal::NumberShaped {
            text: text.to_owned(),
        });
    }
    if number_shaped(text) {
        return Err(EntryRefusal::NumberShaped {
            text: text.to_owned(),
        });
    }
    cell_text(text).map(Some)
}

fn cell_text(text: &str) -> Result<CellInput, EntryRefusal> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let count = units.len();
    CellText::new(ExcelText::from_utf16_code_units(units))
        .map(CellInput::Text)
        .ok_or(EntryRefusal::TooLong { units: count })
}

/// The input an error literal is stored as when typed into a cell (measured,
/// `tests/fixtures/d1/excel-errors.json` of calc-doc).
fn error_literal(text: &str) -> Option<CellInput> {
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
    Some(CellInput::Error(code))
}

/// Whether `text` is a plain decimal number: an optional sign, digits with at most one point
/// (at least one digit), and an optional exponent `e` or `E` with an optional sign and digits.
fn is_plain_number(text: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn number(value: f64) -> Option<CellInput> {
        Some(CellInput::Number(FiniteNumber::new(value).unwrap()))
    }

    fn text(value: &str) -> Option<CellInput> {
        Some(CellInput::Text(
            CellText::new(ExcelText::from_interop_assignment(value)).unwrap(),
        ))
    }

    // Ported from OxFml's authored_input_returns_calc_value_for_cell_entry_literals.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn entry_literals() {
        assert_eq!(interpret_entry(""), Ok(None));
        assert_eq!(interpret_entry("123.4"), Ok(number(123.4)));
        assert_eq!(interpret_entry("'123.4"), Ok(text("123.4")));
        assert_eq!(
            interpret_entry("#DIV/0!"),
            Ok(Some(CellInput::Error(WorksheetErrorCode::Div0)))
        );
        assert_eq!(
            interpret_entry("#getting_data"),
            Ok(Some(CellInput::Error(WorksheetErrorCode::NA)))
        );
        assert_eq!(interpret_entry("#SPILL!"), Ok(text("#SPILL!")));
        assert_eq!(interpret_entry("true"), Ok(Some(CellInput::Logical(true))));
        assert_eq!(interpret_entry("-1.5e3"), Ok(number(-1500.0)));
        assert_eq!(interpret_entry(".5"), Ok(number(0.5)));
        assert_eq!(interpret_entry("Q1 2026"), Ok(text("Q1 2026")));
        assert_eq!(interpret_entry("abc"), Ok(text("abc")));
        assert_eq!(
            interpret_entry("123456789012345"),
            Ok(number(123456789012345.0))
        );
        assert_eq!(interpret_entry("1.50000000000000000"), Ok(number(1.5)));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn entry_formulas_and_refusals() {
        let Ok(Some(CellInput::Formula(formula))) = interpret_entry("=SUM(1,2)") else {
            panic!("a formula");
        };
        assert_eq!(formula.text().to_string_lossy(), "SUM(1,2)");
        assert_eq!(interpret_entry("="), Err(EntryRefusal::EmptyFormula));
        for shaped in [
            "12%",
            "1,000",
            "$5",
            "2026-10-23",
            "10:30",
            "1/2",
            " 12",
            "+A1",
            "-x",
            "1234567890123456",
            "1e999",
        ] {
            assert!(
                matches!(
                    interpret_entry(shaped),
                    Err(EntryRefusal::NumberShaped { .. })
                ),
                "{shaped}"
            );
        }
    }
}
