//! What one token's text spells as a reference, in each notation. A1: a cell `[$]letters[$]digits`
//! (`B3`, `$B$3`), a row `[$]digits` (`3`, `$3`) and a column `[$]letters` (`B`, `$B`), letters
//! ASCII in either case. R1C1: `R` and an axis, `C` and an axis, or both in that order, either
//! case, where an axis is nothing (the anchor's own row or column), a 1-based number, or a
//! bracketed signed offset (`R[-2]C[1]`, `RC`, `R3`, `C`).

use super::{AxisRef, CellRef};
use calc_func::value::{MAX_COLUMNS, MAX_ROWS};

/// The three forms a reference endpoint takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Form {
    Cell,
    Row,
    Column,
}

/// A token's text read as a reference endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Spelled {
    Cell(CellRef),
    Row(AxisRef),
    Column(AxisRef),
    /// Text of the form, which no anchor makes a cell, row or column of the grid: a fixed
    /// coordinate outside it, or an offset longer than the grid.
    OutsideGrid(Form),
}

impl Spelled {
    pub(super) fn form(self) -> Form {
        match self {
            Spelled::Cell(_) => Form::Cell,
            Spelled::Row(_) => Form::Row,
            Spelled::Column(_) => Form::Column,
            Spelled::OutsideGrid(form) => form,
        }
    }
}

/// `text` read in A1 notation for a formula anchored at (`anchor_row`, `anchor_column`): a
/// relative coordinate becomes its offset from the anchor's. `None` for text of no A1 form, and
/// for a cell or column without a `$` that lies outside the grid, which is a name (`Sales2023`,
/// `XFE1`) rather than a reference; a row (digits only) or any form with a `$` outside the grid
/// is [`Spelled::OutsideGrid`].
pub(super) fn a1(text: &str, anchor_row: u32, anchor_column: u32) -> Option<Spelled> {
    a1_with_anchor(text, Some((anchor_row, anchor_column)))
}

/// Spell the same token for a syntax check without placing any reference. The axes in the
/// returned value are temporary spelling data and must not be used as a worksheet position.
pub(super) fn a1_syntax(text: &str) -> Option<Spelled> {
    a1_with_anchor(text, None)
}

fn a1_with_anchor(text: &str, anchor: Option<(u32, u32)>) -> Option<Spelled> {
    let bytes = text.as_bytes();
    let mut at = 0;
    let first_dollar = take(bytes, &mut at, b'$');
    let letters = run(bytes, &mut at, u8::is_ascii_alphabetic);
    let second_dollar = !letters.is_empty() && take(bytes, &mut at, b'$');
    let digits = run(bytes, &mut at, u8::is_ascii_digit);
    if at != bytes.len() {
        return None;
    }
    let column = column_number(letters);
    let row = decimal(digits);
    match (letters.is_empty(), digits.is_empty()) {
        (false, false) => {
            if in_grid(column, MAX_COLUMNS) && in_grid(row, MAX_ROWS) {
                Some(Spelled::Cell(CellRef {
                    row: a1_axis(second_dollar, row, anchor.map(|a| a.0)),
                    column: a1_axis(first_dollar, column, anchor.map(|a| a.1)),
                }))
            } else {
                (first_dollar || second_dollar).then_some(Spelled::OutsideGrid(Form::Cell))
            }
        }
        (false, true) if !second_dollar => {
            if in_grid(column, MAX_COLUMNS) {
                Some(Spelled::Column(a1_axis(
                    first_dollar,
                    column,
                    anchor.map(|a| a.1),
                )))
            } else {
                first_dollar.then_some(Spelled::OutsideGrid(Form::Column))
            }
        }
        (true, false) => Some(if in_grid(row, MAX_ROWS) {
            Spelled::Row(a1_axis(first_dollar, row, anchor.map(|a| a.0)))
        } else {
            Spelled::OutsideGrid(Form::Row)
        }),
        _ => None,
    }
}

/// `text` read in R1C1 notation; `None` for text of no R1C1 form.
pub(super) fn r1c1(text: &str) -> Option<Spelled> {
    let bytes = text.as_bytes();
    let mut at = 0;
    let row = if take(bytes, &mut at, b'R') || take(bytes, &mut at, b'r') {
        Some(r1c1_axis(bytes, &mut at, MAX_ROWS)?)
    } else {
        None
    };
    let column = if take(bytes, &mut at, b'C') || take(bytes, &mut at, b'c') {
        Some(r1c1_axis(bytes, &mut at, MAX_COLUMNS)?)
    } else {
        None
    };
    if at != bytes.len() {
        return None;
    }
    Some(match (row, column) {
        (Some(Some(row)), Some(Some(column))) => Spelled::Cell(CellRef { row, column }),
        (Some(_), Some(_)) => Spelled::OutsideGrid(Form::Cell),
        (Some(Some(row)), None) => Spelled::Row(row),
        (Some(None), None) => Spelled::OutsideGrid(Form::Row),
        (None, Some(Some(column))) => Spelled::Column(column),
        (None, Some(None)) => Spelled::OutsideGrid(Form::Column),
        (None, None) => return None,
    })
}

/// The axis after an `R` or a `C`: outer `None` when the text has no axis form there, inner
/// `None` when it does but no anchor puts it on a grid of `max` rows or columns.
fn r1c1_axis(bytes: &[u8], at: &mut usize, max: u32) -> Option<Option<AxisRef>> {
    if take(bytes, at, b'[') {
        let negative = take(bytes, at, b'-');
        if !negative {
            take(bytes, at, b'+');
        }
        let digits = run(bytes, at, u8::is_ascii_digit);
        if digits.is_empty() || !take(bytes, at, b']') {
            return None;
        }
        let magnitude = decimal(digits);
        // From some anchor the offset lands on the grid only when it is shorter than the grid.
        if magnitude >= u64::from(max) {
            return Some(None);
        }
        let magnitude = i32::try_from(magnitude).ok()?;
        return Some(Some(AxisRef::Relative(if negative {
            -magnitude
        } else {
            magnitude
        })));
    }
    let digits = run(bytes, at, u8::is_ascii_digit);
    if digits.is_empty() {
        return Some(Some(AxisRef::Relative(0)));
    }
    let index = decimal(digits);
    Some(in_grid(index, max).then_some(AxisRef::Absolute(index as u32)))
}

fn a1_axis(absolute: bool, index: u64, anchor: Option<u32>) -> AxisRef {
    // `index` is on the grid (at most MAX_ROWS), so the offset fits an i32.
    match (absolute, anchor) {
        (false, Some(anchor)) => AxisRef::Relative((index as i64 - i64::from(anchor)) as i32),
        _ => AxisRef::Absolute(index as u32),
    }
}

fn in_grid(index: u64, max: u32) -> bool {
    (1..=u64::from(max)).contains(&index)
}

/// The column number of ASCII letters (`A` is 1, `XFD` is 16384), saturating far above the grid.
fn column_number(letters: &[u8]) -> u64 {
    letters.iter().fold(0u64, |number, letter| {
        number
            .saturating_mul(26)
            .saturating_add(u64::from(letter.to_ascii_uppercase() - b'A' + 1))
    })
}

/// The value of ASCII digits, saturating far above the grid.
fn decimal(digits: &[u8]) -> u64 {
    digits.iter().fold(0u64, |number, digit| {
        number
            .saturating_mul(10)
            .saturating_add(u64::from(digit - b'0'))
    })
}

fn take(bytes: &[u8], at: &mut usize, expected: u8) -> bool {
    if bytes.get(*at) == Some(&expected) {
        *at += 1;
        true
    } else {
        false
    }
}

fn run<'b>(bytes: &'b [u8], at: &mut usize, accept: fn(&u8) -> bool) -> &'b [u8] {
    let start = *at;
    while bytes.get(*at).is_some_and(accept) {
        *at += 1;
    }
    &bytes[start..*at]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(offset: i32) -> AxisRef {
        AxisRef::Relative(offset)
    }

    fn abs(index: u32) -> AxisRef {
        AxisRef::Absolute(index)
    }

    fn cell(row: AxisRef, column: AxisRef) -> Option<Spelled> {
        Some(Spelled::Cell(CellRef { row, column }))
    }

    /// A1 forms read at the anchor C5 (row 5, column 3).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a1_forms_at_an_anchor() {
        let at = |text: &str| a1(text, 5, 3);
        assert_eq!(at("B3"), cell(rel(-2), rel(-1)));
        assert_eq!(at("b3"), cell(rel(-2), rel(-1)));
        assert_eq!(at("$B3"), cell(rel(-2), abs(2)));
        assert_eq!(at("B$3"), cell(abs(3), rel(-1)));
        assert_eq!(at("$B$3"), cell(abs(3), abs(2)));
        assert_eq!(at("XFD1048576"), cell(rel(1_048_571), rel(16_381)));
        assert_eq!(at("3"), Some(Spelled::Row(rel(-2))));
        assert_eq!(at("$3"), Some(Spelled::Row(abs(3))));
        assert_eq!(at("B"), Some(Spelled::Column(rel(-1))));
        assert_eq!(at("$B"), Some(Spelled::Column(abs(2))));
        // Outside the grid: without a `$` a cell or column form is a name; a row, or any form
        // with a `$`, is refused.
        assert_eq!(at("XFE1"), None);
        assert_eq!(at("Sales2023"), None);
        assert_eq!(at("A0"), None);
        assert_eq!(at("A1048577"), None);
        assert_eq!(at("XFE"), None);
        assert_eq!(at("$XFE$1"), Some(Spelled::OutsideGrid(Form::Cell)));
        assert_eq!(at("A$1048577"), Some(Spelled::OutsideGrid(Form::Cell)));
        assert_eq!(at("$XFE"), Some(Spelled::OutsideGrid(Form::Column)));
        assert_eq!(at("0"), Some(Spelled::OutsideGrid(Form::Row)));
        assert_eq!(at("1048577"), Some(Spelled::OutsideGrid(Form::Row)));
        assert_eq!(
            at("99999999999999999999999"),
            Some(Spelled::OutsideGrid(Form::Row))
        );
        // No A1 form.
        for text in [
            "", "$", "$$3", "A$", "R1C1", "R[1]C", "A1B", "Ä1", "A_1", "1A", "A1.5", "$A$",
        ] {
            assert_eq!(at(text), None, "{text}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn r1c1_forms() {
        assert_eq!(r1c1("R3C2"), cell(abs(3), abs(2)));
        assert_eq!(r1c1("r3c2"), cell(abs(3), abs(2)));
        assert_eq!(r1c1("R[-2]C[1]"), cell(rel(-2), rel(1)));
        assert_eq!(r1c1("R[+2]C"), cell(rel(2), rel(0)));
        assert_eq!(r1c1("RC"), cell(rel(0), rel(0)));
        assert_eq!(r1c1("RC5"), cell(rel(0), abs(5)));
        assert_eq!(r1c1("R"), Some(Spelled::Row(rel(0))));
        assert_eq!(r1c1("R3"), Some(Spelled::Row(abs(3))));
        assert_eq!(r1c1("R[-1]"), Some(Spelled::Row(rel(-1))));
        assert_eq!(r1c1("C"), Some(Spelled::Column(rel(0))));
        assert_eq!(r1c1("C[2]"), Some(Spelled::Column(rel(2))));
        assert_eq!(r1c1("R1048576C16384"), cell(abs(1_048_576), abs(16_384)));
        assert_eq!(
            r1c1("R[-1048575]C[16383]"),
            cell(rel(-1_048_575), rel(16_383))
        );
        assert_eq!(r1c1("R1048577C1"), Some(Spelled::OutsideGrid(Form::Cell)));
        assert_eq!(r1c1("R1C0"), Some(Spelled::OutsideGrid(Form::Cell)));
        assert_eq!(r1c1("R[1048576]C"), Some(Spelled::OutsideGrid(Form::Cell)));
        assert_eq!(r1c1("R0"), Some(Spelled::OutsideGrid(Form::Row)));
        assert_eq!(r1c1("C16385"), Some(Spelled::OutsideGrid(Form::Column)));
        assert_eq!(r1c1("C[-16384]"), Some(Spelled::OutsideGrid(Form::Column)));
        for text in [
            "", "B3", "Rate", "Cat", "CR", "R[]", "R[1", "R[x]", "R[--1]", "R1C1A", "R2D2", "3",
        ] {
            assert_eq!(r1c1(text), None, "{text}");
        }
    }
}
