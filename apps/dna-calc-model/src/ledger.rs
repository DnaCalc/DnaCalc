//! Pure ledger filtering, ordering and grouping.
//! Extracted from DNA Calc; retains the DnaTreeCalc bf6dfd7
//! `src/dnatreecalc-skins/src/ledger.rs` helpers ported there.
use crate::projection::Provenance;
use calc_value::CellKey;
use std::collections::BTreeSet;
pub const PAGE: usize = 100;
/// One formula cell as the ledger filters and sorts it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub at: CellKey,
    /// `Sheet1!B3`.
    pub label: String,
    /// The formula as entered, with its `=`.
    pub formula: String,
    /// The value as a cell shows it (General).
    pub value: String,
    /// The value when it is a number.
    pub number: Option<f64>,
    pub error: bool,
    pub provenance: Provenance,
}

/// Which formula cells the ledger keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    /// Those whose value is an error.
    Errors,
    /// Those of one provenance.
    Provenance(Provenance),
    /// Those whose address, formula or value holds this text (any case).
    Text(String),
    /// Those whose formulas read the active cell.
    ReadsActive,
}

/// The order of the ledger's rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    /// The workbook's order: sheet, then row, then column.
    Address,
    AddressDescending,
    /// Numbers first, smallest first; then the rest in address order.
    ValueAscending,
    /// Numbers first, largest first; then the rest in address order.
    ValueDescending,
}

/// The bar's choice for a filter, and back: ported from ledger.rs `cleave_filter_choice` and
/// `cleave_filter_for_choice`.
pub fn filter_for_choice(choice: &str, text: &str) -> Filter {
    match choice {
        "errors" => Filter::Errors,
        "calculated" => Filter::Provenance(Provenance::Calculated),
        "refused" => Filter::Provenance(Provenance::Refused),
        "file-cache" => Filter::Provenance(Provenance::FileCache),
        "stale" => Filter::Provenance(Provenance::Stale),
        "text" => Filter::Text(text.to_owned()),
        "reads-active" => Filter::ReadsActive,
        _ => Filter::All,
    }
}

pub fn sort_for_choice(choice: &str) -> Sort {
    match choice {
        "address-desc" => Sort::AddressDescending,
        "value-asc" => Sort::ValueAscending,
        "value-desc" => Sort::ValueDescending,
        _ => Sort::Address,
    }
}

/// The entries `filter` keeps, in `sort`'s order, as indexes into `entries` (which are in
/// address order). `readers` are the cells whose formulas read the active cell.
pub fn ledger_rows(
    entries: &[Entry],
    filter: &Filter,
    sort: Sort,
    readers: &BTreeSet<CellKey>,
) -> Vec<usize> {
    let needle = match filter {
        Filter::Text(text) => text.to_lowercase(),
        _ => String::new(),
    };
    let mut rows: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| match filter {
            Filter::All => true,
            Filter::Errors => entry.error,
            Filter::Provenance(provenance) => entry.provenance == *provenance,
            Filter::Text(_) => {
                needle.is_empty()
                    || entry.label.to_lowercase().contains(&needle)
                    || entry.formula.to_lowercase().contains(&needle)
                    || entry.value.to_lowercase().contains(&needle)
            }
            Filter::ReadsActive => readers.contains(&entry.at),
        })
        .map(|(index, _)| index)
        .collect();
    match sort {
        Sort::Address => {}
        Sort::AddressDescending => rows.reverse(),
        Sort::ValueAscending | Sort::ValueDescending => {
            let descending = sort == Sort::ValueDescending;
            rows.sort_by(|a, b| {
                match (entries[*a].number, entries[*b].number) {
                    (Some(x), Some(y)) => {
                        let order = x.total_cmp(&y);
                        if descending { order.reverse() } else { order }
                    }
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
                .then(a.cmp(b))
            });
        }
    }
    rows
}

/// The rows in groups by provenance, each group in the rows' order and the groups in the order
/// of their first row: ported from ledger.rs `group_rows_by_calc_class`.
pub fn group_by_provenance(entries: &[Entry], rows: &[usize]) -> Vec<(Provenance, Vec<usize>)> {
    let mut groups: Vec<(Provenance, Vec<usize>)> = Vec::new();
    for &row in rows {
        let provenance = entries[row].provenance;
        match groups.iter_mut().find(|(p, _)| *p == provenance) {
            Some((_, members)) => members.push(row),
            None => groups.push((provenance, vec![row])),
        }
    }
    groups
}

/// The rows of a filter and a sort that read no value, as indexes into `keys` (in address
/// order): every cell, or those whose formulas read the active cell (`readers`).
pub fn key_rows(
    keys: &[CellKey],
    filter: &Filter,
    sort: Sort,
    readers: &BTreeSet<CellKey>,
) -> Vec<usize> {
    let mut rows: Vec<usize> = (0..keys.len())
        .filter(|index| !matches!(filter, Filter::ReadsActive) || readers.contains(&keys[*index]))
        .collect();
    if sort == Sort::AddressDescending {
        rows.reverse();
    }
    rows
}

/// Whether a filter or a sort reads the cells' values, so every formula cell is projected.
pub fn reads_values(filter: &Filter, sort: Sort) -> bool {
    matches!(
        filter,
        Filter::Errors | Filter::Provenance(_) | Filter::Text(_)
    ) || matches!(sort, Sort::ValueAscending | Sort::ValueDescending)
}
