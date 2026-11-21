//! The worksheet's Sparklines tools (dc-zjsg.7): insert a sparkline in the active cell over a data range, and for a
//! sparkline in the active cell, its group's type and the points it shows apart, or delete it. Each is one undoable
//! edit through the host's sparkline command; the save writes the worksheet's sparkline extension.

use dna_calc_model::Target;
use crate::workbook_command::{WorkbookCommand, cell_target, projected_cell};
use crate::{controller::Controller, wire};
use leptos::prelude::*;

const KINDS: &[(&str, &str)] = &[("Line", "line"), ("Column", "column"), ("Win/Loss", "stacked")];
const POINTS: &[(&str, &str)] = &[
    ("Markers", "markers"),
    ("High point", "high"),
    ("Low point", "low"),
    ("First point", "first"),
    ("Last point", "last"),
    ("Negative points", "negative"),
];

pub fn controls(controller: Controller, sparklines: RwSignal<Vec<wire::sparklines::DrawnSparkline>>) -> AnyView {
    let data = RwSignal::new(String::new());
    let kind = RwSignal::new("line".to_owned());
    let key = move || controller.active_key();
    let here = move || {
        let at = wire::Key::of(key()?);
        sparklines.get().into_iter().find(|s| s.cell == at)
    };
    let revision = move || controller.summary.get_untracked().revision;
    let sheet = move || controller.sheet.get_untracked();
    let send = move |intent: WorkbookCommand| controller.dispatch(intent, |_| {});
    let insert = move |_| {
        let (Some(sheet), Some(at)) = (sheet(), key()) else { return };
        send(WorkbookCommand::SparklineAdd { sheet: Target::Sheet(sheet), revision: revision(), at: cell_target(at), data: data.get_untracked(), kind: kind.get_untracked() });
    };
    let selected = move || {
        let s = here()?;
        let at = s.cell;
        let shown = [s.markers.is_some(), s.high.is_some(), s.low.is_some(), s.first.is_some(), s.last.is_some(), s.negative.is_some()];
        Some(view! {
            <fieldset data-dnacalc="sparkline-selected">
                <legend>"The sparkline in the active cell"</legend>
                <label>"Type "
                    <select data-dnacalc="sparkline-kind" on:change=move |ev| {
                        let Some(sheet) = sheet() else { return };
                        send(WorkbookCommand::SparklineKind { sheet: Target::Sheet(sheet), revision: revision(), at: projected_cell(at), kind: event_target_value(&ev) });
                    }>
                        {KINDS.iter().map(|(label, value)| view! { <option value=*value selected=s.kind == *value>{*label}</option> }).collect_view()}
                    </select>
                </label>
                <div class="dc-command-row">
                    {POINTS
                        .iter()
                        .zip(shown)
                        .map(|((label, point), on)| view! {
                            <label>
                                <input type="checkbox" data-dnacalc="sparkline-show" data-point=*point prop:checked=on on:change=move |ev| {
                                    let Some(sheet) = sheet() else { return };
                                    send(WorkbookCommand::SparklineShow { sheet: Target::Sheet(sheet), revision: revision(), at: projected_cell(at), point: (*point).to_owned(), on: event_target_checked(&ev) });
                                }/>
                                {*label}
                            </label>
                        })
                        .collect_view()}
                </div>
                <button type="button" data-dnacalc="sparkline-delete" on:click=move |_| {
                    let Some(sheet) = sheet() else { return };
                    send(WorkbookCommand::SparklineDelete { sheet: Target::Sheet(sheet), revision: revision(), at: projected_cell(at) });
                }>"Delete sparkline"</button>
            </fieldset>
        })
    };
    view! {
        <details class="dc-clipboard-tools" name="worksheet-tools" data-dnacalc="sparkline-tools">
            <summary>"Sparklines"</summary>
            <fieldset>
                <legend>"Insert in the active cell"</legend>
                <label>"Data range "
                    <input data-dnacalc="sparkline-data" size="10" placeholder="A2:E2" prop:value=move || data.get() on:input=move |ev| data.set(event_target_value(&ev))/>
                </label>
                <select data-dnacalc="sparkline-new-kind" on:change=move |ev| kind.set(event_target_value(&ev))>
                    {KINDS.iter().map(|(label, value)| view! { <option value=*value>{*label}</option> }).collect_view()}
                </select>
                <button type="button" data-dnacalc="sparkline-insert" on:click=insert>"Insert"</button>
            </fieldset>
            {selected}
        </details>
    }
    .into_any()
}
