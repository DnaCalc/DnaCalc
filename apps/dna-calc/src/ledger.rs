//! The ledger skin: the workbook's formula cells as one audit table. Each row is a formula cell
//! with its address, its formula, its value and that value's provenance, its number format, and
//! how many references it reads and how many formulas read it. A filter keeps the cells whose
//! value is an error, those of one provenance (calculated, refused, the file's value, stale),
//! those whose address, formula or value holds a text, or those whose formulas read the active
//! cell; a sort orders them by address or by value (numbers before the rest); the rows can be
//! grouped by provenance. A click on a row makes that cell the active cell.
//!
//! A formula cell is one Excel counts as holding a formula (the worker's formula catalogue,
//! the atlas's and the corpus comparison's set). After each change the skin reads which cells
//! those are (from the document, without projecting
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

#[cfg(test)]
use crate::cells;
use crate::cells::Provenance;
use crate::controller::Controller;
use crate::skin::{Skin, SkinContext, SkinId};
use crate::wire;
#[cfg(test)]
use calc_value::CellKey;
use leptos::prelude::*;
#[cfg(test)]
use std::collections::BTreeSet;

/// The rows one page shows.
pub use dna_calc_model::ledger::*;

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

/// Page-owned controls that survive skin changes; a new workbook resets them.
#[derive(Clone, Copy)]
pub struct ViewState {
    filter: crate::model_adapter::ModelField<String>,
    text: crate::model_adapter::ModelField<String>,
    sort: crate::model_adapter::ModelField<String>,
    grouped: crate::model_adapter::ModelField<bool>,
    this_sheet: crate::model_adapter::ModelField<bool>,
    page: crate::model_adapter::ModelField<usize>,
}
impl ViewState {
    pub fn new(controller: Controller) -> Self {
        Self {
            filter: controller.view_field::<dna_calc_model::LedgerState, _>(
                SkinId::Ledger,
                |s| &s.filter,
                |s| &mut s.filter,
            ),
            text: controller.view_field::<dna_calc_model::LedgerState, _>(
                SkinId::Ledger,
                |s| &s.text,
                |s| &mut s.text,
            ),
            sort: controller.view_field::<dna_calc_model::LedgerState, _>(
                SkinId::Ledger,
                |s| &s.sort,
                |s| &mut s.sort,
            ),
            grouped: controller.view_field::<dna_calc_model::LedgerState, _>(
                SkinId::Ledger,
                |s| &s.grouped,
                |s| &mut s.grouped,
            ),
            this_sheet: controller.view_field::<dna_calc_model::LedgerState, _>(
                SkinId::Ledger,
                |s| &s.this_sheet,
                |s| &mut s.this_sheet,
            ),
            page: controller.view_field::<dna_calc_model::LedgerState, _>(
                SkinId::Ledger,
                |s| &s.page,
                |s| &mut s.page,
            ),
        }
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

fn ledger_view(controller: Controller) -> impl IntoView {
    let state = ViewState::new(controller);
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
    let reveal_focus = controller.view_field::<dna_calc_model::LedgerState, _>(
        SkinId::Ledger,
        |s| &s.pending_focus,
        |s| &mut s.pending_focus,
    );
    reveal_focus.set(Some(controller.focus_snapshot()));
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
            reveal_focus.set(Some(controller.selection_focus_snapshot()));
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
        let focus_ticket = reveal_focus
            .get_untracked()
            .unwrap_or_else(|| controller.selection_focus_snapshot());
        controller.ask_result(request,move |reply| {
            if epoch.try_get_value()!=Some(asked)||controller.revision.try_get_untracked()!=Some(revision)||controller.generation.try_get_untracked()!=Some(generation){return;}
            match reply {
                Ok(wire::Reply::Ledger(found))=>{
                    if reveal.get_value() {
                        if let Some(active)=found.active_page {if active!=found.page_index {page.set(active as usize);return;}}
                        reveal.set_value(false);
                        if found.active_page.is_none(){failure.set(if found.whole==0 {"No formulas yet. Enter a formula in Grid or open a workbook."}else if found.rows==0 {"No formulas match these filters. Clear filters to see the full list."}else{"The active cell is outside the current formula list or filters."}.into());}
                        #[cfg(target_arch="wasm32")]
                        controller.focus_projection(SkinId::Ledger,"[data-dnacalc=ledger-row][data-active=true] [data-dnacalc=ledger-select]".into(),focus_ticket);
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
    let open_source = move |at, skin| controller.reveal(at, skin);
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
        view!{<tr role="row" data-dnacalc="ledger-row" data-cell=row.label.clone() data-provenance=row.provenance.name() data-active=move ||active().to_string() on:click=move |_|if !hidden&&!loading.get_untracked(){controller.go_to(at);}>
            <td role="cell" class="dc-mono"><button data-dnacalc="ledger-select" aria-label=select_name aria-current=move ||active().to_string() disabled=move ||hidden||loading.get()||!available.get() on:click=move |ev|{ev.stop_propagation();controller.navigate_to(at,move |ok|{if ok&&reveal.try_update_value(|v|*v=true).is_some(){refresh.try_update(|n|*n+=1);}});}>{row.label.clone()}</button></td>
            <td role="cell" data-field="formula" class="dc-mono dc-ledger-formula">{row.formula}</td><td role="cell" data-field="value" class="dc-mono" data-dnacalc="ledger-value">{row.value}</td>
            <td role="cell" data-field="provenance" class="dc-ledger-state"><span class="dc-chip" data-provenance=row.provenance.name()>{if row.error&&row.provenance==Provenance::Calculated{"calculated error"}else{row.provenance.name()}}</span>{concise.map(|note|view!{<p class="dc-ledger-reason" data-dnacalc="ledger-reason">{note}</p>})}</td>
            <td role="cell" data-field="recovery" class="dc-ledger-actions"><button data-dnacalc="ledger-open-grid" aria-label=grid_name disabled=move ||hidden||loading.get()||!available.get() on:click=move |ev|{ev.stop_propagation();open_source(at, SkinId::Grid);}>"Grid"</button><button data-dnacalc="ledger-open-canvas" aria-label=canvas_name disabled=move ||hidden||loading.get()||!available.get() on:click=move |ev|{ev.stop_propagation();open_source(at, SkinId::Canvas);}>"Canvas"</button>
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
            <div class="dc-ledger-table" data-dnacalc="ledger-table" tabindex="0" role="region" aria-label="Formula audit rows with source and recovery actions"><table role="table">
                <thead role="rowgroup">
                    <tr><th>"Cell"</th><th>"Formula"</th><th>"Value"</th><th>"Provenance"</th>
                        <th>"Recovery & details"</th></tr>
                </thead>
                <tbody role="rowgroup">{body}</tbody>
            </table></div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_value::{IdAllocator, SheetId};

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
