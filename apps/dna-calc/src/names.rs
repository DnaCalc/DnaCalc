//! Name Manager keeps definitions separate from explicitly requested evaluations.
use crate::{
    controller::Controller,
    wire::{self, Intent, Reply, Request},
};
use dna_calc_model::{Command, names::NameRecord};
use leptos::prelude::*;

fn sheet_key(sheet: wire::Sheet) -> String {
    format!("{}:{}", sheet.0, sheet.1)
}
fn parse_sheet(text: &str) -> Option<wire::Sheet> {
    let (a, b) = text.split_once(':')?;
    Some(wire::Sheet(a.parse().ok()?, b.parse().ok()?))
}
fn scope_label(controller: Controller, scope: Option<wire::Sheet>) -> String {
    scope
        .map(|sheet| {
            controller
                .sheets()
                .iter()
                .find(|s| s.id == sheet.id())
                .map(|s| s.name.clone())
                .unwrap_or("Unavailable worksheet".into())
        })
        .unwrap_or("Workbook".into())
}
fn choose(controller: Controller, row: NameRecord) {
    let state = controller.name_field(|s| s, |s| s);
    let revision = controller.summary.get_untracked().revision;
    state.update(|s| {
        s.creating = false;
        s.name = row.name.clone();
        s.scope = row.scope.map(sheet_key).unwrap_or_default();
        s.formula = format!("={}", row.formula);
        s.selected = Some(row);
        s.evaluation = None;
        s.message.clear();
        s.edit_revision = revision;
    });
}
#[inline(never)]
pub fn controls(controller: Controller) -> impl IntoView {
    let dialog = NodeRef::<leptos::html::Dialog>::new();
    let name_field = NodeRef::<leptos::html::Input>::new();
    let open = controller.name_field(|s| &s.open, |s| &mut s.open);
    let listing = controller.name_field(|s| &s.listing, |s| &mut s.listing);
    let query = controller.name_field(|s| &s.query, |s| &mut s.query);
    let page = controller.name_field(|s| &s.page, |s| &mut s.page);
    let selected = controller.name_field(|s| &s.selected, |s| &mut s.selected);
    let creating = controller.name_field(|s| &s.creating, |s| &mut s.creating);
    let name = controller.name_field(|s| &s.name, |s| &mut s.name);
    let scope = controller.name_field(|s| &s.scope, |s| &mut s.scope);
    let formula = controller.name_field(|s| &s.formula, |s| &mut s.formula);
    let anchor = controller.name_field(|s| &s.anchor, |s| &mut s.anchor);
    let anchor_sheet = controller.name_field(|s| &s.anchor_sheet, |s| &mut s.anchor_sheet);
    let evaluation = controller.name_field(|s| &s.evaluation, |s| &mut s.evaluation);
    let evaluation_observation = controller.name_field(
        |s| &s.evaluation_observation,
        |s| &mut s.evaluation_observation,
    );
    let message = controller.name_field(|s| &s.message, |s| &mut s.message);
    let epoch = controller.name_field(|s| &s.epoch, |s| &mut s.epoch);
    let refresh = controller.name_field(|s| &s.refresh, |s| &mut s.refresh);
    let edit_revision = controller.name_field(|s| &s.edit_revision, |s| &mut s.edit_revision);
    let close = move || {
        open.set(false);
        if let Some(d) = dialog.get_untracked() {
            d.close();
        }
        controller.focus_active_skin();
    };
    Effect::new(move |_| {
        if let Some(d) = dialog.get() {
            if open.get() && !d.open() {
                if let Some(at) = controller.active_key_untracked() {
                    anchor.set(crate::cells::address(at.row(), at.column()));
                    anchor_sheet.set(sheet_key(wire::Sheet::of(at.sheet())));
                }
                let _ = d.show_modal();
            } else if !open.get() && d.open() {
                d.close();
            }
        }
    });
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        controller.revision.track();
        refresh.track();
        let query = query.get();
        let page = page.get();
        epoch.update_value(|e| *e += 1);
        let request = epoch.get_value();
        let generation = controller.generation.get_untracked();
        controller.ask_result(Request::Names { query, page }, move |reply| {
            if !open.get_untracked()
                || epoch.get_value() != request
                || controller.generation.get_untracked() != generation
            {
                return;
            }
            match reply {
                Ok(Reply::Names(value)) => listing.set(value),
                Err(e) => message.set(e),
                _ => message.set("The names list did not answer".into()),
            }
        });
    });
    let new_name = move || {
        creating.set(true);
        selected.set(None);
        name.set(String::new());
        scope.set(String::new());
        formula.set("=".into());
        evaluation.set(None);
        message.set(String::new());
        edit_revision.set(controller.summary.get_untracked().revision);
        if let Some(field) = name_field.get_untracked() {
            let _ = field.focus();
        }
    };
    let save = move |delete: bool| {
        let row = selected.get_untracked();
        if !creating.get_untracked() && row.is_none() {
            return;
        }
        let generation = controller.generation.get_untracked();
        controller.dispatch(
            Intent::EditName {
                id: row.as_ref().map(|r| r.id),
                version: row.as_ref().map(|r| r.version),
                name: name.get_untracked(),
                scope: parse_sheet(&scope.get_untracked()),
                formula: (!delete).then(|| formula.get_untracked()),
            },
            move |outcome| {
                if controller.generation.get_untracked() != generation || !open.get_untracked() {
                    return;
                }
                if outcome.accepted {
                    selected.set(None);
                    creating.set(false);
                    name.set(String::new());
                    formula.set(String::new());
                    evaluation.set(None);
                    message.set(
                        if delete {
                            "Name deleted; Undo restores it"
                        } else {
                            "Definition saved; values are evaluated only when requested"
                        }
                        .into(),
                    );
                    refresh.update(|r| *r += 1);
                } else {
                    message.set(
                        outcome
                            .error
                            .clone()
                            .unwrap_or("The name did not change".into()),
                    );
                }
            },
        );
    };
    let evaluate = move || {
        let Some(row) = selected.get_untracked() else {
            return;
        };
        let Some((r, c)) = crate::cells::parse_address(&anchor.get_untracked()) else {
            message.set("Choose an A1 cell as the evaluation anchor".into());
            return;
        };
        let Some(sheet) = parse_sheet(&anchor_sheet.get_untracked()) else {
            message.set("Choose the evaluation worksheet".into());
            return;
        };
        let Ok(at) = calc_value::CellKey::new(sheet.id(), r, c) else {
            message.set("The evaluation anchor is outside the worksheet".into());
            return;
        };
        let generation = controller.generation.get_untracked();
        let id = row.id;
        controller.evaluate_name(
            Request::EvaluateName {
                id,
                version: row.version,
                anchor: wire::Key::of(at),
            },
            move |reply| {
                if !open.get_untracked()
                    || controller.generation.get_untracked() != generation
                    || selected.with_untracked(|s| s.as_ref().map(|r| r.id)) != Some(id)
                {
                    return;
                }
                match reply {
                    Ok(Reply::NameEvaluation(value)) => {
                        evaluation_observation.set(controller.revision.get_untracked());
                        evaluation.set(Some(value));
                        message.set(String::new());
                    }
                    Err(e) => message.set(e),
                    _ => message.set("The name evaluation did not answer".into()),
                }
            },
        );
    };
    let rows = move || {
        listing.get().rows.into_iter().map(|row|{
        let label=row.name.clone();let scope=scope_label(controller,row.scope);let definition=format!("={}",row.formula);let id=row.id;let click=row.clone();let hidden=row.hidden;
        view!{<tr data-dnacalc="name-row" data-name=label.clone() data-name-scope=scope.clone()><th scope="row"><button type="button" data-dnacalc="name-select" aria-pressed=move ||selected.with(|s|s.as_ref().is_some_and(|r|r.id==id)).to_string() on:click=move |_|choose(controller,click.clone())>{label.clone()}</button></th><td>{scope.clone()}</td><td class="dc-name-formula">{definition}</td><td>{if hidden {"Hidden"}else{""}}</td></tr>}
    }).collect_view()
    };
    view!{
        <dialog node_ref=dialog class="dc-names-dialog" data-dnacalc="names-dialog" aria-labelledby="dc-names-title" on:cancel=move |_:web_sys::Event|close() on:close=move |_:web_sys::Event|open.set(false)>
            <h2 id="dc-names-title">"Name Manager"</h2>
            <p class="dc-note">"Definitions are listed without evaluation. Evaluate reads the published workbook for the selected name at the caller cell you choose."</p>
            <div class="dc-name-toolbar"><label>"Filter names"<input data-dnacalc="names-filter" prop:value=move ||query.get() on:input=move |ev|{query.set(event_target_value(&ev));page.set(0);}/></label><button type="button" data-dnacalc="name-new" on:click=move |_|new_name()>"New…"</button><button type="button" data-dnacalc="names-undo" disabled=move ||controller.command_availability(dna_calc_model::CommandId::Undo).is_err() on:click=move |_|controller.command(Command::Undo)>"Undo"</button><button type="button" data-dnacalc="names-redo" disabled=move ||controller.command_availability(dna_calc_model::CommandId::Redo).is_err() on:click=move |_|controller.command(Command::Redo)>"Redo"</button></div>
            <div class="dc-name-list"><table><thead><tr><th>"Name"</th><th>"Scope"</th><th>"Refers to"</th><th>"Visibility"</th></tr></thead><tbody>{rows}</tbody></table></div>
            <div class="dc-name-toolbar"><button type="button" data-dnacalc="names-previous" disabled=move ||page.get()==0 on:click=move |_|page.update(|p|*p=p.saturating_sub(1))>"Previous"</button><span data-dnacalc="names-count">{move ||listing.with(|l|format!("{} names · page {}",l.total,l.page+1))}</span><button type="button" data-dnacalc="names-next" disabled=move ||listing.with(|l|(l.page+1).saturating_mul(200)>=l.total as u64) on:click=move |_|page.update(|p|*p+=1)>"Next"</button></div>
            <fieldset class="dc-name-editor"><legend>{move ||if creating.get(){"New name"}else{"Selected definition"}}</legend>
                <label>"Name"<input node_ref=name_field data-dnacalc="name-name" prop:value=move ||name.get() readonly=move ||!creating.get() disabled=move ||!creating.get()&&selected.get().is_none() on:input=move |ev|name.set(event_target_value(&ev))/></label>
                <label>"Scope"<select data-dnacalc="name-scope" disabled=move ||!creating.get() prop:value=move ||scope.get() on:change=move |ev|scope.set(event_target_value(&ev))><option value="">"Workbook"</option>{move ||controller.sheets().into_iter().map(|s|view!{<option value=sheet_key(wire::Sheet::of(s.id))>{s.name}</option>}).collect_view()}</select></label>
                <label class="dc-name-definition">"Refers to"<textarea data-dnacalc="name-formula" rows="2" spellcheck="false" disabled=move ||!creating.get()&&selected.get().is_none() prop:value=move ||formula.get() on:input=move |ev|formula.set(event_target_value(&ev))></textarea></label>
                <p class="dc-note">"Name and scope stay fixed when editing an existing definition. Renaming is not yet modelled."</p>
                <div class="dc-dialog-actions"><button type="button" data-dnacalc="name-save" disabled=move ||(!creating.get()&&selected.get().is_none())||edit_revision.get()!=controller.summary.get().revision on:click=move |_|save(false)>"Save definition"</button><button type="button" data-dnacalc="name-delete" disabled=move ||selected.get().is_none()||edit_revision.get()!=controller.summary.get().revision on:click=move |_|save(true)>"Delete"</button></div>
            </fieldset>
            <fieldset><legend>"Explicit evaluation"</legend>
                <div class="dc-name-toolbar"><label>"Caller worksheet"<select data-dnacalc="name-evaluation-sheet" prop:value=move ||anchor_sheet.get() on:change=move |ev|anchor_sheet.set(event_target_value(&ev))>{move ||controller.sheets().into_iter().map(|s|view!{<option value=sheet_key(wire::Sheet::of(s.id))>{s.name}</option>}).collect_view()}</select></label><label>"Caller cell"<input data-dnacalc="name-anchor" prop:value=move ||anchor.get() on:input=move |ev|anchor.set(event_target_value(&ev))/></label><button type="button" data-dnacalc="name-evaluate" disabled=move ||selected.get().is_none() on:click=move |_|evaluate()>"Evaluate"</button></div>
                <output data-dnacalc="name-value">{move ||evaluation.with(|e|e.as_ref().map(|e|e.text.clone().unwrap_or_else(|e|format!("Refused: {e}"))).unwrap_or("Not evaluated".into()))}</output>
                <p class="dc-note">"The result is retained from this explicit command. Evaluate again to inspect current published values."</p>
                <p class="dc-note" data-dnacalc="name-evaluation-context">{move ||evaluation.with(|e|e.as_ref().map(|e|format!("Explicit evaluation {} at {} · definition version {} · revision {}{}",e.command,e.anchor.cell().map(|at|controller.cell_label(at)).unwrap_or_default(),e.version,e.revision,if e.revision!=controller.summary.get().revision{" · authored workbook has changed"}else if evaluation_observation.get()!=controller.revision.get(){" · published view has changed"}else{""})).unwrap_or_default())}</p>
            </fieldset>
            <p class="dc-note" hidden=move ||(!creating.get()&&selected.get().is_none())||edit_revision.get()==controller.summary.get().revision>"The workbook changed. Select the definition again before editing it."</p>
            <p role="status" data-dnacalc="names-message">{move ||message.get()}</p>
            <div class="dc-dialog-actions"><button type="button" data-dnacalc="names-close" on:click=move |_|close()>"Close"</button></div>
        </dialog>
    }.into_any()
}
