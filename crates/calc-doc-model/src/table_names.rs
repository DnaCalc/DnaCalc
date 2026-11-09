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
            assert_eq!(added_column_name(&names(list), at), want, "{list:?} at {at}");
        };
        // tbl-col-add-end, -pos1, -pos3, -twice
        case(&base, 4, "Column1");
        case(&base, 0, "Column1");
        case(&base, 2, "Column1");
        case(&["Region", "Product", "Amount", "Units", "Column1"], 5, "Column2");
        // tbl-col-add-three-at-1: each new first column takes the next free number
        case(&["Column1", "Region", "Product", "Amount", "Units"], 0, "Column2");
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
        assert_eq!(free_column_name(&names(&["Region", "Product", "Amount"])), "Column1");
        assert_eq!(free_column_name(&names(&["Region", "Product", "Amount", "Column1"])), "Column2");
        assert_eq!(free_column_name(&names(&["Region", "Column5", "Amount", "Column1", "Column2"])), "Column3");
        assert_eq!(free_column_name(&names(&["Region", "Column1", "Column2", "Column5"])), "Column3");
        assert_eq!(free_column_name(&names(&["COLUMN1"])), "Column2");
    }

    /// tbln-typed-duplicate, tbln-typed-duplicate-three
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_repeated_header_takes_the_smallest_number_from_two() {
        assert_eq!(numbered_duplicate("Region", &names(&["Region", "Product"])), "Region2");
        assert_eq!(numbered_duplicate("Region", &names(&["Region", "Product", "Region2"])), "Region3");
    }
}
