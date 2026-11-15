//! The names Excel gives table columns it names itself, measured on Excel 16.0.20527 (2026-11-09,
//! `crates/calc-host/tests/fixtures/pivot/m7-names`, bead dc-8wp.16): a column added to a table, a
//! blank header cell, and a header typed over another's name.
//!
//! - A column added to a table (`ListColumns.Add`, a column inserted inside the table, a value typed
//!   in the cell right of the table) is named after the column on its left. When that name ends in
//!   a number, the new name has the next number (`Column5` gives `Column6`, `Col9` gives `Col10`,
//!   `column5` gives `column6`, `Column0` gives `Column1`); if that name is taken, it is the left
//!   name with `2` (the smallest number from 2 that is free) appended (`Column52`). Otherwise
//!   (the left name has no number at its end, or there is no column on the left) it is `Column`
//!   and the smallest number from 1 that no header has. Leading zeros in the number were not
//!   measured.
//! - A header cell cleared or emptied is `Column` and the smallest number from 1 that no other
//!   header has, whatever the columns around it are called; two cleared headers take 1 and 2 in
//!   the order of the columns.
//! - A header that repeats a name is that name with the smallest number from 2 that is free
//!   (`Region2`, then `Region3`).
//!
//! Names are compared without regard to case, as Excel compares headers.

/// Whether `name` is among `names`, without regard to case.
fn taken(names: &[String], name: &str) -> bool {
    let name = name.to_lowercase();
    names.iter().any(|other| other.to_lowercase() == name)
}

/// `Column` and the smallest number from 1 that no name of `names` has.
pub fn free_column_name(names: &[String]) -> String {
    (1u64..)
        .map(|n| format!("Column{n}"))
        .find(|candidate| !taken(names, candidate))
        .expect("a free column name")
}

/// `name` with the smallest number from 2 appended that no name of `names` has: how Excel names a
/// header that repeats another.
pub fn numbered_duplicate(name: &str, names: &[String]) -> String {
    (2u64..)
        .map(|n| format!("{name}{n}"))
        .find(|candidate| !taken(names, candidate))
        .expect("a free header")
}

/// A1-style cell reference (`A1`, `xfd1048576`): one to three letters up to column XFD, then a row.
fn is_a1_reference(name: &str) -> bool {
    let letters = name.chars().take_while(char::is_ascii_alphabetic).count();
    let digits = &name[letters..];
    if letters == 0
        || letters > 3
        || digits.is_empty()
        || !digits.chars().all(|c| c.is_ascii_digit())
    {
        return false;
    }
    let column = name[..letters]
        .bytes()
        .fold(0u32, |n, c| n * 26 + u32::from(c.to_ascii_uppercase() - b'A' + 1));
    let row: u64 = digits.parse().unwrap_or(u64::MAX);
    column <= crate::value::MAX_COLUMNS && (1..=u64::from(crate::value::MAX_ROWS)).contains(&row)
}

/// R1C1-style cell reference with numbers (`R1`, `C2`, `r1c1`).
fn is_r1c1_reference(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    let mut rest = upper.as_str();
    let mut parts = 0;
    for letter in ['R', 'C'] {
        if let Some(after) = rest.strip_prefix(letter) {
            let digits = after.chars().take_while(char::is_ascii_digit).count();
            if digits > 0 {
                rest = &after[digits..];
                parts += 1;
            }
        }
    }
    parts > 0 && rest.is_empty()
}

/// The `displayName` Excel derives from a table's `Name` (measured, Excel 16.0.20527, dc-piw0.10 Q11):
/// surrounding spaces are dropped; a space, hyphen, `!`, `[` or tab becomes `_` and a `]` is dropped; a name that does not
/// start with a letter, `_` or `\`, or that is an R1C1 reference with numbers (`R1C1`, `r1`), gets a
/// leading `_`; a name that is an A1 reference, `R`, `C` or `TRUE`/`FALSE` gets a trailing `_`. When
/// `taken` says the result is already used (another table, a defined name) `_1`, `_2`, ... is appended.
pub fn table_display_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut display: String = name
        .trim_matches(' ')
        .chars()
        .filter(|c| *c != ']')
        .map(|c| if matches!(c, ' ' | '-' | '!' | '[' | '\t') { '_' } else { c })
        .collect();
    let first = display.chars().next();
    if first.is_none() {
        display.push('_');
    } else if !first.is_some_and(|c| c.is_alphabetic() || c == '_' || c == '\\')
        || is_r1c1_reference(&display)
    {
        display.insert(0, '_');
    } else if is_a1_reference(&display)
        || matches!(display.to_ascii_uppercase().as_str(), "R" | "C" | "TRUE" | "FALSE")
    {
        display.push('_');
    }
    if !taken(&display) {
        return display;
    }
    (1u64..)
        .map(|n| format!("{display}_{n}"))
        .find(|candidate| !taken(candidate))
        .expect("a free display name")
}

/// The name Excel gives a column added at `at` (the index it will have) to a table whose columns
/// are named `names` now.
pub fn added_column_name(names: &[String], at: usize) -> String {
    let left = at.checked_sub(1).and_then(|index| names.get(index));
    let Some(left) = left else {
        return free_column_name(names);
    };
    let digits = left.chars().rev().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 18 {
        return free_column_name(names);
    }
    let split = left.len() - digits;
    let number: u64 = left[split..].parse().expect("ascii digits");
    let candidate = format!("{}{}", &left[..split], number + 1);
    if taken(names, &candidate) {
        numbered_duplicate(left, names)
    } else {
        candidate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    /// Each row is a scenario of the m7 capture (tbl-col-*, tbln-*): the columns before the column
    /// was added, where it went, and the name Excel gave it.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn an_added_column_is_named_as_excel_names_it() {
        let base = ["Region", "Product", "Amount", "Units"];
        let case = |list: &[&str], at: usize, want: &str| {
            assert_eq!(
                added_column_name(&names(list), at),
                want,
                "{list:?} at {at}"
            );
        };
        // tbl-col-add-end, -pos1, -pos3, -twice
        case(&base, 4, "Column1");
        case(&base, 0, "Column1");
        case(&base, 2, "Column1");
        case(
            &["Region", "Product", "Amount", "Units", "Column1"],
            5,
            "Column2",
        );
        // tbl-col-add-three-at-1: each new first column takes the next free number
        case(
            &["Column1", "Region", "Product", "Amount", "Units"],
            0,
            "Column2",
        );
        case(&["Column2", "Column1", "Region"], 0, "Column3");
        // tbl-col-sheet-insert-first: a second column inserted after Region, Column1 being there
        case(&["Region", "Column1", "Product"], 1, "Column2");
        // tbl-col-collide-end, tbl-col-collide-pos, tbln-sparse, tbln-six-only, tbln-asc-pair
        case(&["Region", "Product", "Amount", "Column5"], 4, "Column6");
        case(&["Region", "Column3", "Amount", "Units"], 2, "Column4");
        case(&["Region", "Product", "Column7", "Column2"], 4, "Column3");
        case(&["Region", "Product", "Amount", "Column6"], 4, "Column7");
        case(&["Region", "Product", "Column5", "Column6"], 4, "Column7");
        // the number taken: the left name and 2 (tbl-col-collide-two, tbln-desc-pair, tbln-nonadjacent)
        case(&["Region", "Product", "Column6", "Column5"], 4, "Column52");
        case(&["Region", "Column6", "Amount", "Column5"], 4, "Column52");
        // no left column: the smallest free number (tbln-desc-pair-at1)
        case(&["Region", "Product", "Column6", "Column5"], 0, "Column1");
        // tbln-three-times-over-three, -lower-case, -ten, -zero, -other-prefix
        case(&["Region", "Product", "Amount", "Column3"], 4, "Column4");
        case(&["Region", "Product", "Amount", "column5"], 4, "column6");
        case(&["Region", "Product", "Amount", "Column10"], 4, "Column11");
        case(&["Region", "Product", "Amount", "Column0"], 4, "Column1");
        case(&["Region", "Product", "Amount", "Col9"], 4, "Col10");
        // tbl-col-add-fresh-table: a, b gives Column1, then a first column gives Column2
        case(&["a", "b"], 2, "Column1");
        case(&["a", "b", "Column1"], 0, "Column2");
    }

    /// hdr-table-* and tbln-blank-*: a blank header is the smallest free number, in column order.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_blank_header_is_the_smallest_free_column_number() {
        assert_eq!(
            free_column_name(&names(&["Region", "Product", "Amount"])),
            "Column1"
        );
        assert_eq!(
            free_column_name(&names(&["Region", "Product", "Amount", "Column1"])),
            "Column2"
        );
        assert_eq!(
            free_column_name(&names(&[
                "Region", "Column5", "Amount", "Column1", "Column2"
            ])),
            "Column3"
        );
        assert_eq!(
            free_column_name(&names(&["Region", "Column1", "Column2", "Column5"])),
            "Column3"
        );
        assert_eq!(free_column_name(&names(&["COLUMN1"])), "Column2");
    }

    /// tbln-typed-duplicate, tbln-typed-duplicate-three
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_repeated_header_takes_the_smallest_number_from_two() {
        assert_eq!(
            numbered_duplicate("Region", &names(&["Region", "Product"])),
            "Region2"
        );
        assert_eq!(
            numbered_duplicate("Region", &names(&["Region", "Product", "Region2"])),
            "Region3"
        );
    }

    /// dc-piw0.10 Q11: every row is a line of the recorded `set [Name] ... now Name DisplayName`.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_display_name_is_derived_as_excel_derives_it() {
        let none = |_: &str| false;
        for (name, want) in [
            ("My Table", "My_Table"),
            ("My-Table", "My_Table"),
            ("1Table", "_1Table"),
            ("A1", "A1_"),
            ("R1C1", "_R1C1"),
            ("R", "R_"),
            ("C", "C_"),
            ("r1", "_r1"),
            ("Table.1", "Table.1"),
            ("Sales2024", "Sales2024"),
            ("_x", "_x"),
            ("\\x", "\\x"),
            ("a b c", "a_b_c"),
            (" lead", "lead"),
            ("trail ", "trail"),
            ("T\u{e9}", "T\u{e9}"),
            ("Tab\tle", "Tab_le"),
            ("?q", "_?q"),
            ("a!b", "a_b"),
            ("True", "True_"),
            ("Print_Area", "Print_Area"),
            ("x[1]", "x_1"),
        ] {
            assert_eq!(table_display_name(name, none), want, "{name:?}");
        }
        assert_eq!(table_display_name("Taken", |n| n == "Taken"), "Taken_1");
        assert_eq!(table_display_name("Taken", |n| n == "Taken" || n == "Taken_1"), "Taken_2");
    }
}
