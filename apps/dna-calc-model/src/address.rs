//! Shared address labels and sheet-name quoting, moved intact from the shared formula UI.
//! These are inspection and pointed-reference labels, with no workbook implementation.
use calc_value::{CellKey, GridArea, SheetId};

/// A column's letters: 1 is `A`, 26 `Z`, 27 `AA`.
pub fn column_letters(mut column: u32) -> String {
    let mut letters = Vec::new();
    while column > 0 {
        let digit = (column - 1) % 26;
        letters.push(char::from(b'A' + digit as u8));
        column = (column - 1) / 26;
    }
    letters.iter().rev().collect()
}

/// One A1 cell address, optionally anchored with `$`, inside Excel's grid.
pub fn parse_address(text: &str) -> Option<(u32, u32)> {
    let text = text.trim();
    let text = text.strip_prefix('$').unwrap_or(text);
    let split = text.find(|c: char| !c.is_ascii_alphabetic())?;
    let (letters, rest) = text.split_at(split);
    let digits = rest.strip_prefix('$').unwrap_or(rest);
    if letters.is_empty()
        || letters.len() > 3
        || digits.is_empty()
        || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let column = letters.bytes().fold(0u32, |n, b| {
        n * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1)
    });
    let row: u32 = digits.parse().ok()?;
    ((1..=1_048_576).contains(&row) && (1..=16_384).contains(&column)).then_some((row, column))
}

/// What names the sheets a label shows: a DNA OneCalc session names its own sheets, and DNA Calc
/// names a workbook's (dc-rif), so both label cells, areas and refusals with these functions.
pub trait SheetNames {
    /// The name of `sheet`, or `None` when there is no such sheet.
    fn sheet_name_of(&self, sheet: SheetId) -> Option<String>;
}

fn sheet_label(names: &(impl SheetNames + ?Sized), cell: CellKey) -> String {
    names
        .sheet_name_of(cell.sheet())
        .map_or_else(|| "(no sheet)".to_owned(), |name| quoted_sheet(&name))
}

/// A sheet name as reference text: as it is when it is a plain name (a letter or `_`, then
/// letters, digits, `_` and `.`) that cannot be read as a cell (`A1`, `R1C1`, `R`, `C`), and
/// otherwise in single quotes with each quote doubled (`'Q2 Data'`, `'It''s'`). Quoting a name
/// that does not need it is still valid reference text, so the test errs toward quoting.
fn quoted_sheet(name: &str) -> String {
    let mut chars = name.chars();
    let plain = chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    let upper = name.to_ascii_uppercase();
    // A cell of the grid: one to three column letters up to XFD, then a row from 1 to 1048576.
    let cell_like = |text: &str| {
        let letters = text.trim_end_matches(|c: char| c.is_ascii_digit());
        let digits = &text[letters.len()..];
        // At most three letters are folded, so the column number cannot overflow.
        let column = (1..=3)
            .contains(&letters.len())
            .then(|| {
                letters.bytes().try_fold(0u32, |n, b| {
                    b.is_ascii_uppercase()
                        .then(|| n * 26 + u32::from(b - b'A' + 1))
                })
            })
            .flatten()
            .filter(|&n| (1..=16_384).contains(&n));
        let row = digits
            .parse::<u32>()
            .ok()
            .filter(|r| (1..=1_048_576).contains(r));
        column.is_some() && row.is_some()
    };
    let r1c1_like = upper == "R"
        || upper == "C"
        || upper
            .strip_prefix('R')
            .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit() || c == 'C'));
    if plain && !cell_like(&upper) && !r1c1_like {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// A cell's label: `Sheet2!B3`, or `'Q2 Data'!B3` for a sheet name that needs quotes.
pub fn cell(names: &(impl SheetNames + ?Sized), cell: CellKey) -> String {
    format!(
        "{}!{}{}",
        sheet_label(names, cell),
        column_letters(cell.column()),
        cell.row()
    )
}

/// An area's label: `Sheet1!A1:A3`, or a cell's label for one cell.
pub fn area(names: &(impl SheetNames + ?Sized), area: GridArea) -> String {
    let first = area.first();
    let last = area.last();
    if first == last {
        return cell(names, first);
    }
    format!(
        "{}:{}{}",
        cell(names, first),
        column_letters(last.column()),
        last.row()
    )
}

#[cfg(test)]
mod tests {
    use super::quoted_sheet;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sheet_names_that_need_quotes_are_quoted() {
        for (name, text) in [
            ("Sheet1", "Sheet1"),
            ("Data_2024", "Data_2024"),
            ("Totals1", "Totals1"),
            ("Summary", "Summary"),
            ("Revenue2024", "Revenue2024"),
            ("Summary.1", "Summary.1"),
            (
                "SSSSSSSSSSSSSSSSSSSSSSSSSSSSSSS",
                "SSSSSSSSSSSSSSSSSSSSSSSSSSSSSSS",
            ),
            ("XFD1048576", "'XFD1048576'"),
            ("XFE1", "XFE1"),
            ("A1048577", "A1048577"),
            ("my.sheet", "my.sheet"),
            ("Q2 Data", "'Q2 Data'"),
            ("It's", "'It''s'"),
            ("2024", "'2024'"),
            ("A1", "'A1'"),
            ("xfd1048576", "'xfd1048576'"),
            ("R1C1", "'R1C1'"),
            ("R", "'R'"),
            ("c", "'c'"),
            ("Résumé", "'Résumé'"),
            ("a-b", "'a-b'"),
        ] {
            assert_eq!(quoted_sheet(name), text, "{name}");
        }
    }
}

impl SheetNames for crate::projection::Summary {
    fn sheet_name_of(&self, sheet: SheetId) -> Option<String> {
        self.sheets
            .iter()
            .find(|(s, _)| s.id() == sheet)
            .map(|(_, name)| name.clone())
            .or_else(|| {
                self.hidden_sheets
                    .iter()
                    .find(|(s, _, _)| s.id() == sheet)
                    .map(|(_, name, _)| name.clone())
            })
    }
}

impl<T: SheetNames> SheetNames for crate::Snapshot<T> {
    fn sheet_name_of(&self, sheet: SheetId) -> Option<String> {
        (**self).sheet_name_of(sheet)
    }
}

/// A cell address without a worksheet qualifier.
pub fn address(row: u32, column: u32) -> String {
    format!("{}{row}", column_letters(column))
}
