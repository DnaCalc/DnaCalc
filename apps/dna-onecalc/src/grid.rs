//! The scratch grid: a small block of cells on the first sheet that a person types values into, as
//! in a worksheet, so the formula can refer to them.
//!
//! An entry is read as Excel reads a constant typed into a cell: nothing is a blank cell, a
//! leading `'` makes the rest text, `TRUE` and `FALSE` are logicals, an error literal such as
//! `#N/A` is that error, a number is a number, and anything else is text. A grid cell holds a
//! constant only: a formula typed into one is refused, because calculating several formulas in
//! dependency order is a calculation engine's work (DNA Calc), which OneCalc does not have. Every
//! grid cell exists, so an empty one is a blank cell, not a cell without input.

use crate::session::InputValue;
use calc_fml::parse_formula;
use calc_fml::syntax::SyntaxKind;
use calc_func::value::WorksheetErrorCode;

/// The grid's rows: 1 to 12.
pub const ROWS: u32 = 12;

/// The grid's columns: A to H.
pub const COLUMNS: u32 = 8;

/// What a grid entry gives its cell.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    /// Nothing typed: a blank cell.
    Blank,
    /// A constant.
    Value(InputValue),
    /// An entry the grid does not hold, and why; the cell then has no input.
    Refused(String),
}

/// The constant `text` typed into a grid cell gives.
pub fn entry(text: &str) -> Entry {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Entry::Blank;
    }
    if let Some(rest) = text.strip_prefix('\'') {
        return text_entry(rest);
    }
    if trimmed.starts_with('=') {
        return Entry::Refused(
            "a formula in a grid cell needs a calculation engine (DNA Calc); OneCalc calculates one formula"
                .to_owned(),
        );
    }
    if trimmed.eq_ignore_ascii_case("TRUE") {
        return Entry::Value(InputValue::logical(true));
    }
    if trimmed.eq_ignore_ascii_case("FALSE") {
        return Entry::Value(InputValue::logical(false));
    }
    if let Some(code) = error_literal(trimmed) {
        return Entry::Value(InputValue::error(code));
    }
    if let Some(number) = number(trimmed) {
        return match InputValue::number(number) {
            Ok(value) => Entry::Value(value),
            Err(_) => Entry::Refused(format!("{trimmed} is not a number a cell holds")),
        };
    }
    text_entry(text)
}

fn text_entry(text: &str) -> Entry {
    match InputValue::text(text) {
        Ok(value) => Entry::Value(value),
        Err(_) => Entry::Refused("text longer than Excel's 32,767 UTF-16 code units".to_owned()),
    }
}

/// A decimal number as typed: digits with an optional sign, decimal point and exponent, or such
/// a number followed by `%` (a hundredth of it). Rust's float syntax is wider (`inf`, `NaN`), so
/// the text is checked to hold only the characters of a decimal number first.
fn number(text: &str) -> Option<f64> {
    let (digits, scale) = match text.strip_suffix('%') {
        Some(rest) => (rest.trim_end(), 100.0),
        None => (text, 1.0),
    };
    let decimal = !digits.is_empty()
        && digits.chars().any(|c| c.is_ascii_digit())
        && digits
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | '+' | '-' | 'e' | 'E'));
    if !decimal {
        return None;
    }
    digits.parse::<f64>().ok().map(|n| n / scale)
}

/// The worksheet error `text` spells, read by calc-fml's lexer as an error literal of a formula.
pub fn error_literal(text: &str) -> Option<WorksheetErrorCode> {
    let tree = parse_formula(text);
    if !tree.diagnostics().is_empty() {
        return None;
    }
    let expression = tree.expression()?;
    if expression.kind() != SyntaxKind::ErrorLiteralExpr {
        return None;
    }
    expression.tokens().first()?.error_code()
}

/// A grid cell's A1 name: `A1` for row 1, column 1.
pub fn address(row: u32, column: u32) -> String {
    format!("{}{row}", column_letter(column))
}

/// The letter of a grid column, 1 to 26.
pub fn column_letter(column: u32) -> char {
    char::from(b'A' + (column - 1) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_func::value::CalcValue;

    fn value(text: &str) -> CalcValue {
        match entry(text) {
            Entry::Value(value) => value.value().clone(),
            other => panic!("{text:?} gave {other:?}"),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn entries_read_as_excel_reads_typed_constants() {
        assert_eq!(entry(""), Entry::Blank);
        assert_eq!(entry("   "), Entry::Blank);
        assert_eq!(value("1.5"), CalcValue::number(1.5));
        assert_eq!(value(" -2e3 "), CalcValue::number(-2000.0));
        assert_eq!(value("50%"), CalcValue::number(0.5));
        assert_eq!(value("true"), CalcValue::logical(true));
        assert_eq!(value("FALSE"), CalcValue::logical(false));
        assert_eq!(value("#n/a"), CalcValue::error(WorksheetErrorCode::NA));
        assert_eq!(value("#DIV/0!"), CalcValue::error(WorksheetErrorCode::Div0));
        assert_eq!(
            value("'12"),
            InputValue::text("12").unwrap().value().clone()
        );
        assert_eq!(
            value("abc"),
            InputValue::text("abc").unwrap().value().clone()
        );
        assert_eq!(
            value("inf"),
            InputValue::text("inf").unwrap().value().clone()
        );
        assert_eq!(
            value("NaN"),
            InputValue::text("NaN").unwrap().value().clone()
        );
        assert_eq!(
            value("1.2.3"),
            InputValue::text("1.2.3").unwrap().value().clone()
        );
        assert!(matches!(entry("=1+1"), Entry::Refused(_)));
        assert!(matches!(entry("1e400"), Entry::Refused(_)));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn addresses() {
        assert_eq!(address(1, 1), "A1");
        assert_eq!(address(12, 8), "H12");
    }
}
