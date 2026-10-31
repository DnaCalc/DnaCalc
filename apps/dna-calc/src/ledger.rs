//! The ledger skin: the workbook's formula cells as one audit table. Each row is a formula cell
//! with its address, its formula, its value and that value's provenance, its number format, and
//! how many references it reads and how many formulas read it. A filter keeps the cells whose
//! value is an error, those of one provenance (calculated, refused, the file's value, stale),
//! those whose address, formula or value holds a text, or those whose formulas read the active
//! cell; a sort orders them by address or by value (numbers before the rest); the rows can be
//! grouped by provenance. A click on a row makes that cell the active cell.
//!
//! A formula cell is one Excel counts as holding a formula ([`calc_host::HostState::holds_formula`],
//! the atlas's and the corpus comparison's set). After each change the skin reads which cells
//! those are ([`calc_host::HostState::formula_cells`], from the document, without projecting
//! them) and projects only the [`PAGE`] rows it shows; a filter or a sort that reads values (errors,
//! a provenance, a text, by value) projects every formula cell of its scope, which on a workbook
//! of a hundred thousand formulas takes seconds per change (measured, dc-gf0). The skin changes
//! nothing but the selection.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnatreecalc-skins/src/ledger.rs`: the
//! population table, the cleave bar (the filters `HasError`, a calc state, `TextMatch` and
//! `DependsOn`, and the sorts by name and by value with numbers first: `ledger_rows`,
//! `cleave_filter_for_choice`, `cleave_sort_for_choice`) and `group_rows_by_calc_class`, with
//! their tests. Left behind: the tree path column and the sort by depth in the tree, the shared
//! cleave state across lenses, the multi-select and the bulk content and number-format edits,
//! which calc-host has no intent for yet.

use crate::cells::{self, Provenance};
use crate::controller::Controller;
use crate::skin::{Skin, SkinContext, SkinId};
use crate::wire;
use crate::workbook::Workbook;
use calc_host::value::{CellKey, CoreValue};
use calc_host::{ProjectedCell, ProjectedValue};
use leptos::prelude::*;
use std::collections::BTreeSet;

/// The rows one page shows.
pub const PAGE: usize = 100;

#[derive(Clone, Copy, Debug, Default)]
pub struct LedgerSkin;

impl Skin for LedgerSkin {
    fn id(&self) -> SkinId {
        SkinId::Ledger
    }

    fn title(&self) -> &'static str {
        "Ledger"
    }

    fn mount(&self, ctx: SkinContext) -> AnyView {
        ledger_view(ctx.controller, ctx.active_skin).into_any()
    }
}

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

/// Page-owned controls that survive skin changes; a new workbook resets them.
#[derive(Clone, Copy)]
pub struct ViewState {
    filter: RwSignal<String>,
    text: RwSignal<String>,
    sort: RwSignal<String>,
    grouped: RwSignal<bool>,
    this_sheet: RwSignal<bool>,
    page: RwSignal<usize>,
}
impl ViewState {
    pub fn new() -> Self {
        Self {
            filter: RwSignal::new(String::new()),
            text: RwSignal::new(String::new()),
            sort: RwSignal::new(String::new()),
            grouped: RwSignal::new(false),
            this_sheet: RwSignal::new(false),
            page: RwSignal::new(0),
        }
    }
    pub fn reset(self) {
        self.filter.set(String::new());
        self.text.set(String::new());
        self.sort.set(String::new());
        self.grouped.set(false);
        self.this_sheet.set(false);
        self.page.set(0);
    }
}
fn short_reason(reason: &str) -> String {
    let mut chars = reason.chars();
    let mut short: String = chars.by_ref().take(120).collect();
    if chars.next().is_some() {
        short.push('…');
    }
    short
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

fn ledger_view(controller: Controller, active_skin: RwSignal<Option<String>>) -> impl IntoView {
    let state = controller.ledger;
    let filter_choice = state.filter;
    let text = state.text;
    let sort_choice = state.sort;
    let grouped = state.grouped;
    let this_sheet = state.this_sheet;
    let page = state.page;
    let previous = StoredValue::new((
        filter_choice.get_untracked(),
        text.get_untracked(),
        sort_choice.get_untracked(),
        this_sheet.get_untracked(),
    ));
    Effect::new(move |_| {
        let next = (
            filter_choice.get(),
            text.get(),
            sort_choice.get(),
            this_sheet.get(),
        );
        if previous.get_value() != next {
            previous.set_value(next);
            page.set(0);
        }
    });
    let ledger = RwSignal::new(wire::Ledger::default());
    let failure = RwSignal::new(String::new());
    let loading = RwSignal::new(true);
    let available = RwSignal::new(false);
    let epoch = StoredValue::new(0u64);
    let reveal = StoredValue::new(true);
    let refresh = RwSignal::new(0u64);
    let last_active = StoredValue::new(controller.active_key_untracked());
    Effect::new(move |_| {
        let revision = controller.revision.get();
        let generation = controller.generation.get();
        refresh.track();
        let sheet = this_sheet
            .get()
            .then(|| controller.sheet.get())
            .flatten()
            .map(wire::Sheet::of);
        let active = controller.active_key();
        if last_active.get_value() != active {
            last_active.set_value(active);
            reveal.set_value(true);
        }
        let request = wire::Request::Ledger {
            sheet,
            filter: filter_choice.get(),
            text: text.get(),
            sort: sort_choice.get(),
            page: page.get() as u64,
            active: active.map(wire::Key::of),
        };
        epoch.update_value(|n| *n += 1);
        let asked = epoch.get_value();
        loading.set(true);
        available.set(false);
        failure.set(String::new());
        controller.ask_result(request,move |reply| {
            if epoch.try_get_value()!=Some(asked)||controller.revision.try_get_untracked()!=Some(revision)||controller.generation.try_get_untracked()!=Some(generation){return;}
            match reply {
                Ok(wire::Reply::Ledger(found))=>{
                    if reveal.get_value() {
                        if let Some(active)=found.active_page {if active!=found.page_index {page.set(active as usize);return;}}
                        reveal.set_value(false);
                        if found.active_page.is_none(){failure.set(if found.whole==0 {"No formulas yet. Enter a formula in Grid or open a workbook."}else if found.rows==0 {"No formulas match these filters. Clear filters to see the full list."}else{"The active cell is outside the current formula list or filters."}.into());}
                        #[cfg(target_arch="wasm32")]
                        set_timeout(move ||crate::app::focus("[data-dnacalc=ledger-row][data-active=true] [data-dnacalc=ledger-select]"),std::time::Duration::from_millis(20));
                    }
                    if found.page_index!=page.get_untracked() as u64 {page.set(found.page_index as usize);}
                    ledger.try_set(found);available.try_set(true);loading.try_set(false);
                },
                Err(error)=>{failure.try_set(format!("Formula list unavailable: {error}"));loading.try_set(false);},
                _=>{}
            }
        });
    });
    let pages = move || ledger.with(|l| (l.rows as usize).div_ceil(PAGE).max(1));
    let open_source = move |at, skin: &'static str| {
        controller.navigate_to(at, move |ok| {
            if ok {
                active_skin.set(Some(skin.into()));
                #[cfg(target_arch = "wasm32")]
                set_timeout(
                    move || {
                        crate::app::focus(if skin == "canvas" {
                            "[data-dnacalc=canvas]"
                        } else {
                            "[data-dnacalc=grid]"
                        })
                    },
                    std::time::Duration::from_millis(20),
                );
            }
        })
    };
    let row_view = move |row: wire::LedgerRow| {
        let Some(at) = row.at.cell() else {
            return ().into_any();
        };
        let active = move || controller.active_key() == Some(at);
        let hidden = controller
            .summary
            .with(|s| !s.sheets.iter().any(|(s, _)| s.id() == at.sheet()));
        let note = if hidden {
            format!(
                "{} Hidden sheet: unhide it in Sheet tools to navigate.",
                row.reason
            )
        } else {
            row.reason
        };
        let concise = (hidden || row.provenance != Provenance::Calculated || row.error)
            .then(|| short_reason(&note));
        let select_name = format!("Inspect {}", row.label);
        let grid_name = format!("Open {} in Grid", row.label);
        let canvas_name = format!("Open {} in Canvas", row.label);
        view!{<tr data-dnacalc="ledger-row" data-cell=row.label.clone() data-provenance=row.provenance.name() data-active=move ||active().to_string() on:click=move |_|if !hidden&&!loading.get_untracked(){controller.go_to(at);}>
            <td class="dc-mono"><button data-dnacalc="ledger-select" aria-label=select_name aria-current=move ||active().to_string() disabled=move ||hidden||loading.get()||!available.get() on:click=move |ev|{ev.stop_propagation();controller.navigate_to(at,move |ok|{if ok&&reveal.try_update_value(|v|*v=true).is_some(){refresh.try_update(|n|*n+=1);}});}>{row.label.clone()}</button></td>
            <td class="dc-mono dc-ledger-formula">{row.formula}</td><td class="dc-mono" data-dnacalc="ledger-value">{row.value}</td>
            <td><span class="dc-chip" data-provenance=row.provenance.name()>{if row.error&&row.provenance==Provenance::Calculated{"calculated error"}else{row.provenance.name()}}</span>{concise.map(|note|view!{<p class="dc-ledger-reason" data-dnacalc="ledger-reason">{note}</p>})}</td>
            <td class="dc-ledger-actions"><button data-dnacalc="ledger-open-grid" aria-label=grid_name disabled=move ||hidden||loading.get()||!available.get() on:click=move |ev|{ev.stop_propagation();open_source(at,"grid");}>"Grid"</button><button data-dnacalc="ledger-open-canvas" aria-label=canvas_name disabled=move ||hidden||loading.get()||!available.get() on:click=move |ev|{ev.stop_propagation();open_source(at,"canvas");}>"Canvas"</button>
                <details data-dnacalc="ledger-details" on:click=move |ev|ev.stop_propagation()><summary>"Details"</summary><p>{note}</p><dl><dt>"Number format"</dt><dd>{row.format}</dd><dt>"Read areas"</dt><dd>{row.reads}</dd><dt>"Dependent cells"</dt><dd>{row.read_by}</dd></dl></details>
            </td>
        </tr>}.into_any()
    };
    let body = move || {
        if !available.get() {
            return ().into_any();
        }
        let shown = ledger.with(|l| l.page.clone());
        if !grouped.get() {
            return shown.into_iter().map(row_view).collect_view().into_any();
        }
        // Grouped by provenance in the page's order: the groups in the order of their first row.
        let mut groups: Vec<(Provenance, Vec<wire::LedgerRow>)> = Vec::new();
        for row in shown {
            match groups.iter_mut().find(|(p, _)| *p == row.provenance) {
                Some((_, members)) => members.push(row),
                None => groups.push((row.provenance, vec![row])),
            }
        }
        groups
            .into_iter()
            .map(|(provenance, members)| {
                let count = members.len();
                view! {
                    <tr class="dc-group" data-dnacalc="ledger-group" data-provenance=provenance.name()>
                        <td colspan="5">{format!("{} ({count})", provenance.name())}</td>
                    </tr>
                    {members.into_iter().map(row_view).collect_view()}
                }
            })
            .collect_view()
            .into_any()
    };

    view! {
        <section class="dc-ledger" data-skin="ledger" data-dnacalc="ledger">
            <div class="dc-cleave">
                <select prop:value={move ||if this_sheet.get(){"sheet"}else{"workbook"}} data-dnacalc="ledger-scope" aria-label="Formula scope" on:change=move |ev| this_sheet.set(event_target_value(&ev) == "sheet")>
                    <option value="workbook">"Workbook"</option>
                    <option value="sheet">"This sheet"</option>
                </select>
                <select prop:value=move ||filter_choice.get() data-dnacalc="ledger-filter" aria-label="Formula filter" on:change=move |ev| filter_choice.set(event_target_value(&ev))>
                    <option value="">"Every formula cell"</option>
                    <option value="errors">"Errors"</option>
                    <option value="calculated">"Calculated"</option>
                    <option value="refused">"Refused"</option>
                    <option value="file-cache">"File cache"</option>
                    <option value="stale">"Stale"</option>
                    <option value="text">"Holding text"</option>
                    <option value="reads-active">"Reading the active cell"</option>
                </select>
                <input type="text" data-dnacalc="ledger-text" aria-label="Filter formula text" placeholder="text" spellcheck="false"
                    prop:value=move || text.get() on:input=move |ev| text.set(event_target_value(&ev)) />
                <select prop:value=move ||sort_choice.get() data-dnacalc="ledger-sort" aria-label="Formula sort" on:change=move |ev| sort_choice.set(event_target_value(&ev))>
                    <option value="">"By address"</option>
                    <option value="address-desc">"By address, last first"</option>
                    <option value="value-asc">"By value, smallest first"</option>
                    <option value="value-desc">"By value, largest first"</option>
                </select>
                <label><input type="checkbox" data-dnacalc="ledger-group-toggle" prop:checked=move ||grouped.get()
                    on:change=move |ev| grouped.set(event_target_checked(&ev)) />" Group by provenance"</label>
                <button type="button" data-dnacalc="ledger-previous" disabled=move ||loading.get()||!available.get()||page.get()==0
                    on:click=move |_| page.update(|p| *p = p.saturating_sub(1))>"Previous"</button>
                <span data-dnacalc="ledger-count">
                    {move || if !available.get(){if loading.get(){"Loading formula list…".into()}else{"Formula counts unavailable".into()}}else{format!("{} of {} formula cells; page {} of {}",
                        ledger.with(|l| l.rows), ledger.with(|l| l.keys), page.get() + 1, pages())}}
                </span>
                <button type="button" data-dnacalc="ledger-next" disabled={move ||loading.get()||!available.get()||page.get()+1>=pages()}
                    on:click=move |_| page.update(|p| if *p + 1 < pages() { *p += 1 })>"Next"</button>
            </div>
            <p class="dc-ledger-population" data-dnacalc="ledger-population">{move ||if !available.get(){if loading.get(){"Loading formula population…".into()}else{"Formula counts unavailable".into()}}else{format!("{} formulas in this workbook · {} in scope · {} match the active filters",ledger.with(|l|l.whole),ledger.with(|l|l.keys),ledger.with(|l|l.rows))}}</p>
            <p role="status" data-dnacalc="ledger-status">{move ||failure.get()}</p>
            <div class="dc-ledger-recovery"><button data-dnacalc="ledger-show-active" on:click=move |_|{reveal.set_value(true);refresh.update(|n|*n+=1);}>"Show active cell"</button><button data-dnacalc="ledger-clear-filter" on:click=move |_|batch(||{filter_choice.set(String::new());text.set(String::new());this_sheet.set(false);page.set(0);})>"Clear filters"</button></div>
            <div class="dc-ledger-table" data-dnacalc="ledger-table" tabindex="0" role="region" aria-label="Formula audit rows; scroll horizontally for source and recovery actions"><table>
                <thead>
                    <tr><th>"Cell"</th><th>"Formula"</th><th>"Value"</th><th>"Provenance"</th>
                        <th>"Recovery & details"</th></tr>
                </thead>
                <tbody>{body}</tbody>
            </table></div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_host::value::{IdAllocator, SheetId};

    fn entries() -> Vec<Entry> {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let s = ids.issue().unwrap();
        let make = |column: u32, value: &str, number: Option<f64>, provenance: Provenance| Entry {
            at: CellKey::new(s, 1, column).unwrap(),
            label: format!("Sheet1!{}1", cells::column_letters(column)),
            formula: format!("=X{column}"),
            value: value.to_owned(),
            number,
            error: value.starts_with('#'),
            provenance,
        };
        vec![
            make(1, "5", Some(5.0), Provenance::Calculated),
            make(2, "#DIV/0!", None, Provenance::Calculated),
            make(3, "refused", None, Provenance::Refused),
            make(4, "-2", Some(-2.0), Provenance::FileCache),
            make(5, "apple", None, Provenance::Stale),
        ]
    }

    /// Ported from ledger.rs `ledger_rows_without_cleave_keeps_key_order...` and
    /// `ledger_rows_applies_cleave_filter...`.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_filters_keep_their_cells_in_address_order() {
        let entries = entries();
        let none = BTreeSet::new();
        let rows = |filter: Filter| ledger_rows(&entries, &filter, Sort::Address, &none);
        assert_eq!(rows(Filter::All), vec![0, 1, 2, 3, 4]);
        assert_eq!(rows(Filter::Errors), vec![1]);
        assert_eq!(rows(Filter::Provenance(Provenance::Refused)), vec![2]);
        assert_eq!(rows(Filter::Provenance(Provenance::Calculated)), vec![0, 1]);
        assert_eq!(rows(Filter::Text("APP".to_owned())), vec![4]);
        assert_eq!(rows(Filter::Text("sheet1!c1".to_owned())), vec![2]);
        assert_eq!(rows(Filter::Text("=x4".to_owned())), vec![3]);
        let readers = BTreeSet::from([entries[3].at, entries[0].at]);
        assert_eq!(
            ledger_rows(&entries, &Filter::ReadsActive, Sort::Address, &readers),
            vec![0, 3]
        );
    }

    /// Ported from ledger.rs `ledger_rows_applies_cleave_sort`: numbers first.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_sorts_put_numbers_first() {
        let entries = entries();
        let none = BTreeSet::new();
        let rows = |sort: Sort| ledger_rows(&entries, &Filter::All, sort, &none);
        assert_eq!(rows(Sort::ValueAscending), vec![3, 0, 1, 2, 4]);
        assert_eq!(rows(Sort::ValueDescending), vec![0, 3, 1, 2, 4]);
        assert_eq!(rows(Sort::AddressDescending), vec![4, 3, 2, 1, 0]);
    }

    /// Ported from ledger.rs `cleave_filter_choice_round_trips_authorable_filters` and
    /// `cleave_sort_choice_round_trips_all_variants`.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_bar_choices_name_their_filters_and_sorts() {
        assert_eq!(filter_for_choice("", "x"), Filter::All);
        assert_eq!(filter_for_choice("errors", ""), Filter::Errors);
        assert_eq!(
            filter_for_choice("stale", ""),
            Filter::Provenance(Provenance::Stale)
        );
        assert_eq!(
            filter_for_choice("text", "ab"),
            Filter::Text("ab".to_owned())
        );
        assert_eq!(filter_for_choice("reads-active", ""), Filter::ReadsActive);
        assert_eq!(sort_for_choice("value-desc"), Sort::ValueDescending);
        assert_eq!(sort_for_choice("nonsense"), Sort::Address);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rows_without_values_come_from_the_keys_alone() {
        let entries = entries();
        let keys: Vec<CellKey> = entries.iter().map(|e| e.at).collect();
        let none = BTreeSet::new();
        assert_eq!(
            key_rows(&keys, &Filter::All, Sort::Address, &none),
            vec![0, 1, 2, 3, 4]
        );
        assert_eq!(
            key_rows(&keys, &Filter::All, Sort::AddressDescending, &none),
            vec![4, 3, 2, 1, 0]
        );
        let readers = BTreeSet::from([keys[1]]);
        assert_eq!(
            key_rows(&keys, &Filter::ReadsActive, Sort::Address, &readers),
            vec![1]
        );
        assert!(!reads_values(&Filter::ReadsActive, Sort::Address));
        assert!(reads_values(&Filter::All, Sort::ValueDescending));
        assert!(reads_values(&Filter::Errors, Sort::Address));
    }

    /// Ported from ledger.rs `group_rows_by_calc_class_orders_groups_by_first_appearance`.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn groups_follow_their_first_row() {
        let entries = entries();
        let groups = group_by_provenance(&entries, &[3, 0, 2, 1]);
        assert_eq!(
            groups,
            vec![
                (Provenance::FileCache, vec![3]),
                (Provenance::Calculated, vec![0, 1]),
                (Provenance::Refused, vec![2]),
            ]
        );
    }
}

#[cfg(test)]
mod concise_tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn concise_reasons_keep_unicode_and_leave_the_full_detail_unchanged() {
        let text = "é".repeat(125);
        let short = short_reason(&text);
        assert_eq!(short.chars().count(), 121);
        assert!(short.ends_with('…'));
        assert_eq!(text.chars().count(), 125);
        assert_eq!(
            short_reason("awaiting recalculation"),
            "awaiting recalculation"
        );
    }
}
