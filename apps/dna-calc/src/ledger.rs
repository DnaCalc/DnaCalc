//! The ledger skin: the workbook's formula cells as one audit table. Each row is a formula cell
//! with its address, its formula, its value and that value's provenance, its number format, and
//! how many references it reads and how many formulas read it. A filter keeps the cells whose
//! value is an error, those of one provenance (calculated, refused, the file's value, stale),
//! those whose address, formula or value holds a text, or those whose formulas read the active
//! cell; a sort orders them by address or by value (numbers before the rest); the rows can be
//! grouped by provenance. A click on a row makes that cell the active cell.
//!
//! A formula cell is one Excel counts as holding a formula ([`calc_host::HostState::holds_formula`],
//! the atlas's and the corpus comparison's set). The skin keeps every sheet's cells through one
//! calc-host subscription per sheet ([`crate::watch`]), shows [`PAGE`] rows at a time and changes
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
use crate::watch;
use calc_host::value::{CellKey, CoreValue};
use calc_host::{ProjectedCell, ProjectedValue};
use leptos::prelude::*;
use std::collections::BTreeSet;

/// The rows one page shows.
pub const PAGE: usize = 100;

pub const LEDGER_CSS: &str = r#"
.dc-ledger{overflow:auto;flex:1 1 auto;padding:10px 14px;font:13px system-ui,"Segoe UI",sans-serif}
.dc-ledger .dc-cleave{display:flex;flex-wrap:wrap;gap:8px;align-items:center;margin:0 0 10px}
.dc-ledger table{border-collapse:collapse;min-width:900px}
.dc-ledger th{position:sticky;top:0;background:#f6f8fa;text-align:left;font-weight:600;border-bottom:1px solid #d0d7de;padding:3px 8px}
.dc-ledger td{border-bottom:1px solid #eaeef2;padding:3px 8px;white-space:nowrap;max-width:420px;overflow:hidden;text-overflow:ellipsis}
.dc-ledger td.dc-mono{font:12px ui-monospace,Consolas,monospace}
.dc-ledger tr[data-dnacalc=ledger-row]{cursor:pointer}
.dc-ledger tr[data-active=true]{background:#ddf4ff}
.dc-ledger tr.dc-group td{background:#f6f8fa;font-weight:600;color:#424a53}
.dc-ledger .dc-chip{display:inline-block;font-size:11px;border-radius:10px;padding:0 7px;background:#eaeef2;color:#424a53}
.dc-ledger .dc-chip[data-provenance=calculated]{background:#dafbe1;color:#116329}
.dc-ledger .dc-chip[data-provenance=refused]{background:#ffebe9;color:#a40e26}
.dc-ledger .dc-chip[data-provenance=stale]{background:#fff8c5;color:#9a6700}
"#;

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
        ledger_view(ctx.controller).into_any()
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

fn ledger_view(controller: Controller) -> impl IntoView {
    let sheets = watch::watch_sheets(controller);
    let filter_choice = RwSignal::new(String::new());
    let text = RwSignal::new(String::new());
    let sort_choice = RwSignal::new(String::new());
    let grouped = RwSignal::new(false);
    let this_sheet = RwSignal::new(false);
    let page = RwSignal::new(0usize);

    let entries = Memo::new(move |_| {
        controller.revision.track();
        let only = this_sheet.get().then(|| controller.sheet.get()).flatten();
        sheets.with(|sheets| {
            controller.with_untracked(|book| {
                let host = book.host();
                let mut out = Vec::new();
                for sheet in sheets {
                    if only.is_some_and(|only| only != sheet.id) {
                        continue;
                    }
                    let name = sheet.name.get_value();
                    sheet.mirror.with(|mirror| {
                        for cell in mirror.cells() {
                            if host.holds_formula(cell.at()) {
                                let label = format!(
                                    "{name}!{}",
                                    cells::address(cell.at().row(), cell.at().column())
                                );
                                out.push(entry(cell, label));
                            }
                        }
                    });
                }
                out
            })
        })
    });
    let readers = Memo::new(move |_| {
        controller.revision.track();
        if filter_choice.get() != "reads-active" {
            return BTreeSet::new();
        }
        let Some(at) = controller.active_key() else {
            return BTreeSet::new();
        };
        controller.with_untracked(|book| book.host().dependents(at).into_iter().collect())
    });
    let rows = Memo::new(move |_| {
        let filter = filter_for_choice(&filter_choice.get(), &text.get());
        let sort = sort_for_choice(&sort_choice.get());
        entries.with(|entries| readers.with(|readers| ledger_rows(entries, &filter, sort, readers)))
    });
    Effect::new(move |_| {
        rows.track();
        page.set(0);
    });
    let pages = move || rows.with(Vec::len).div_ceil(PAGE).max(1);

    let row_view = move |index: usize| {
        let entry = entries.with_untracked(|entries| entries[index].clone());
        let at = entry.at;
        let (format, reads, read_by) = controller.with_untracked(|book| {
            let host = book.host();
            (
                host.number_format(at).unwrap_or("General").to_owned(),
                host.precedents(at).map_or(0, |areas| areas.len()),
                host.dependents(at).len(),
            )
        });
        let active = move || controller.active_key() == Some(at);
        view! {
            <tr data-dnacalc="ledger-row" data-cell=entry.label.clone() data-provenance=entry.provenance.name()
                data-active=move || active().to_string() on:click=move |_| controller.go_to(at)>
                <td class="dc-mono">{entry.label.clone()}</td>
                <td class="dc-mono">{entry.formula.clone()}</td>
                <td class="dc-mono" data-dnacalc="ledger-value">{entry.value.clone()}</td>
                <td><span class="dc-chip" data-provenance=entry.provenance.name()>{entry.provenance.name()}</span></td>
                <td class="dc-mono">{format}</td>
                <td>{reads}</td>
                <td>{read_by}</td>
            </tr>
        }
        .into_any()
    };
    let body = move || {
        let first = page.get() * PAGE;
        let shown: Vec<usize> =
            rows.with(|rows| rows.iter().skip(first).take(PAGE).copied().collect());
        if !grouped.get() {
            return shown.into_iter().map(row_view).collect_view().into_any();
        }
        let groups = entries.with_untracked(|entries| group_by_provenance(entries, &shown));
        groups
            .into_iter()
            .map(|(provenance, members)| {
                let count = members.len();
                view! {
                    <tr class="dc-group" data-dnacalc="ledger-group" data-provenance=provenance.name()>
                        <td colspan="7">{format!("{} ({count})", provenance.name())}</td>
                    </tr>
                    {members.into_iter().map(row_view).collect_view()}
                }
            })
            .collect_view()
            .into_any()
    };

    view! {
        <section class="dc-ledger" data-skin="ledger" data-dnacalc="ledger">
            <style>{LEDGER_CSS}</style>
            <div class="dc-cleave">
                <select data-dnacalc="ledger-scope" on:change=move |ev| this_sheet.set(event_target_value(&ev) == "sheet")>
                    <option value="workbook">"Workbook"</option>
                    <option value="sheet">"This sheet"</option>
                </select>
                <select data-dnacalc="ledger-filter" on:change=move |ev| filter_choice.set(event_target_value(&ev))>
                    <option value="">"Every formula cell"</option>
                    <option value="errors">"Errors"</option>
                    <option value="calculated">"Calculated"</option>
                    <option value="refused">"Refused"</option>
                    <option value="file-cache">"File cache"</option>
                    <option value="stale">"Stale"</option>
                    <option value="text">"Holding text"</option>
                    <option value="reads-active">"Reading the active cell"</option>
                </select>
                <input type="text" data-dnacalc="ledger-text" placeholder="text" spellcheck="false"
                    prop:value=move || text.get() on:input=move |ev| text.set(event_target_value(&ev)) />
                <select data-dnacalc="ledger-sort" on:change=move |ev| sort_choice.set(event_target_value(&ev))>
                    <option value="">"By address"</option>
                    <option value="address-desc">"By address, last first"</option>
                    <option value="value-asc">"By value, smallest first"</option>
                    <option value="value-desc">"By value, largest first"</option>
                </select>
                <label><input type="checkbox" data-dnacalc="ledger-group-toggle"
                    on:change=move |ev| grouped.set(event_target_checked(&ev)) />" Group by provenance"</label>
                <button type="button" data-dnacalc="ledger-previous"
                    on:click=move |_| page.update(|p| *p = p.saturating_sub(1))>"Previous"</button>
                <span data-dnacalc="ledger-count">
                    {move || format!("{} of {} formula cells; page {} of {}",
                        rows.with(Vec::len), entries.with(Vec::len), page.get() + 1, pages())}
                </span>
                <button type="button" data-dnacalc="ledger-next"
                    on:click=move |_| page.update(|p| if *p + 1 < pages() { *p += 1 })>"Next"</button>
            </div>
            <table>
                <thead>
                    <tr><th>"Cell"</th><th>"Formula"</th><th>"Value"</th><th>"Provenance"</th>
                        <th>"Format"</th><th>"Reads"</th><th>"Read by"</th></tr>
                </thead>
                <tbody>{body}</tbody>
            </table>
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
