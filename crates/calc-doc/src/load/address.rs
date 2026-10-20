//! Cell addresses as SpreadsheetML writes them in `r` and `ref` attributes: column letters then a
//! row number, without `$` (`A1`, `XFD1048576`), and areas of two of them joined by `:`.

use calc_doc_model::value::{MAX_COLUMNS, MAX_ROWS};

/// The column number (1 is A) of one to three ASCII letters, either case; `None` past
/// [`MAX_COLUMNS`] or for anything else.
pub(super) fn column_number(letters: &str) -> Option<u32> {
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    let mut column = 0u32;
    for letter in letters.bytes() {
        if !letter.is_ascii_alphabetic() {
            return None;
        }
        column = column * 26 + u32::from(letter.to_ascii_uppercase() - b'A') + 1;
    }
    (column <= MAX_COLUMNS).then_some(column)
}

/// The letters of column `column` (1 is A), in upper case.
pub(super) fn column_letters(mut column: u32) -> String {
    let mut letters = Vec::new();
    while column > 0 {
        let digit = (column - 1) % 26;
        letters.push(b'A' + digit as u8);
        column = (column - 1) / 26;
    }
    letters.reverse();
    String::from_utf8(letters).expect("ASCII letters")
}

/// The row number of ASCII digits; `None` for 0, past [`MAX_ROWS`] or for anything else.
pub(super) fn row_number(digits: &str) -> Option<u32> {
    if digits.is_empty() || !digits.bytes().all(|digit| digit.is_ascii_digit()) {
        return None;
    }
    let row: u32 = digits.parse().ok()?;
    (1..=MAX_ROWS).contains(&row).then_some(row)
}

/// The (row, column) of a cell address such as `B7`.
pub(super) fn cell(text: &str) -> Option<(u32, u32)> {
    let split = text.find(|c: char| c.is_ascii_digit())?;
    let column = column_number(&text[..split])?;
    let row = row_number(&text[split..])?;
    Some((row, column))
}

/// The (top, left, bottom, right) of an area such as `C1:C10`, its corners in either order, or of
/// one cell.
pub(super) fn area(text: &str) -> Option<(u32, u32, u32, u32)> {
    let (first, last) = match text.split_once(':') {
        Some((first, last)) => (cell(first)?, cell(last)?),
        None => {
            let only = cell(text)?;
            (only, only)
        }
    };
    Some((
        first.0.min(last.0),
        first.1.min(last.1),
        first.0.max(last.0),
        first.1.max(last.1),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_at_the_grid_edges() {
        assert_eq!(cell("A1"), Some((1, 1)));
        assert_eq!(cell("xfd1048576"), Some((MAX_ROWS, MAX_COLUMNS)));
        assert_eq!(cell("AA10"), Some((10, 27)));
        assert_eq!(cell("XFE1"), None);
        assert_eq!(cell("A0"), None);
        assert_eq!(cell("A1048577"), None);
        assert_eq!(cell("$A$1"), None);
        assert_eq!(cell("A"), None);
        assert_eq!(cell("1"), None);
        assert_eq!(cell("A1B"), None);
        assert_eq!(area("C10:A1"), Some((1, 1, 10, 3)));
        for column in [1, 26, 27, 52, 53, 702, 703, MAX_COLUMNS] {
            assert_eq!(column_number(&column_letters(column)), Some(column));
        }
        assert_eq!(column_letters(MAX_COLUMNS), "XFD");
    }
}
