//! Workbook ledger projection.
use crate::cells::Provenance;
use crate::{cells, wire, workbook::Workbook};
use calc_host::{ProjectedCell, ProjectedValue};
use calc_value::{CellKey, CoreValue};
pub use dna_calc_model::ledger::*;
use std::collections::BTreeSet;
/// A projected formula cell as a ledger entry.
pub fn entry(cell: &ProjectedCell, label: String) -> Entry {
    let shown = cells::shown(cell, None, None);
    let core = match cell.value() {
        ProjectedValue::Calculated { value, .. } => Some(value.core().clone()),
        ProjectedValue::FileCached(cache) => Some(cache.value().to_value().core().clone()),
        _ => None,
    };
    Entry {
        at: cell.at(),
        label,
        formula: cells::input_text(cell.input()),
        value: if shown.provenance == Provenance::Refused {
            "refused".to_owned()
        } else {
            shown.text
        },
        number: match core {
            Some(CoreValue::Number(number)) => Some(number),
            _ => None,
        },
        error: matches!(core, Some(CoreValue::Error(_))),
        provenance: shown.provenance,
    }
}

/// One page of the ledger as the page shows it (the workbook's server works it out, dc-p96): the
/// formula cells of the scope ([`calc_host::HostState::formula_cells`]), kept and ordered by the
/// filter and the sort (a filter or a sort that reads values projects every formula cell of the
/// scope), and the page's rows projected with their format and their read counts.
pub fn view_model(
    book: &Workbook,
    sheet: Option<calc_host::value::SheetId>,
    filter_choice: &str,
    text: &str,
    sort_choice: &str,
    page: usize,
    active: Option<CellKey>,
) -> wire::Ledger {
    let host = book.host();
    let mut keys: Vec<CellKey> = Vec::new();
    let mut whole = 0u64;
    for tab in book.sheets() {
        let sheet_keys = host.formula_cells(tab.id);
        whole += sheet_keys.len() as u64;
        if sheet.is_none_or(|only| only == tab.id) {
            keys.extend(sheet_keys);
        }
    }
    let label = |at| cells::cell_label(host, at);
    let filter = filter_for_choice(filter_choice, text);
    let sort = sort_for_choice(sort_choice);
    let readers: BTreeSet<CellKey> = match (&filter, active) {
        (Filter::ReadsActive, Some(at)) => host.dependents(at).into_iter().collect(),
        _ => BTreeSet::new(),
    };
    let rows = if reads_values(&filter, sort) {
        let entries: Vec<Entry> = book
            .project(keys.clone())
            .iter()
            .map(|cell| entry(cell, label(cell.at())))
            .collect();
        ledger_rows(&entries, &filter, sort, &readers)
    } else {
        key_rows(&keys, &filter, sort, &readers)
    };
    let active_page = active
        .and_then(|active| rows.iter().position(|i| keys[*i] == active))
        .map(|i| (i / PAGE) as u64);
    let page = page.min(rows.len().saturating_sub(1) / PAGE);
    let chosen: Vec<CellKey> = rows
        .iter()
        .skip(page * PAGE)
        .take(PAGE)
        .map(|i| keys[*i])
        .collect();
    let shown = book
        .project(chosen)
        .iter()
        .map(|cell| {
            let entry = entry(cell, label(cell.at()));
            let at = entry.at;
            let reason = match cells::error_meaning(cell) {
                Some(meaning) if entry.provenance == Provenance::Calculated => {
                    format!("Calculated worksheet error: {meaning}")
                }
                _ => cells::provenance_text(host, cell),
            };
            wire::LedgerRow {
                reason,
                error: entry.error,
                at: wire::Key::of(at),
                label: entry.label,
                formula: entry.formula,
                value: entry.value,
                provenance: entry.provenance,
                format: host.number_format(at).unwrap_or("General").to_owned(),
                reads: host.precedents(at).map_or(0, |areas| areas.len()) as u64,
                read_by: host.dependents(at).len() as u64,
            }
        })
        .collect();
    wire::Ledger {
        whole,
        page_index: page as u64,
        active_page,
        keys: keys.len() as u64,
        rows: rows.len() as u64,
        page: shown,
    }
}
