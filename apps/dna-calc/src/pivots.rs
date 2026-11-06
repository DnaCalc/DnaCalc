//! Read-only saved pivot definitions over the worksheet's existing grid presentation.
use crate::{controller::Controller, wire};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

pub fn controls(controller: Controller) -> AnyView {
    let open = RwSignal::new(false);
    let saved = RwSignal::new(Vec::<wire::SavedPivot>::new());
    let issues = RwSignal::new(Vec::<String>::new());
    let ready = RwSignal::new(false);
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        let generation = controller.generation.get();
        let revision = controller.summary.get().revision;
        let sheet = controller.sheet.get();
        ready.set(false);
        saved.set(Vec::new());
        issues.set(Vec::new());
        let Some(sheet) = sheet else {
            return;
        };
        controller.ask(
            wire::Request::PivotTables(wire::Sheet::of(sheet)),
            move |reply| {
                if open.try_get_untracked() != Some(true)
                    || controller.generation.get_untracked() != generation
                    || controller.summary.get_untracked().revision != revision
                    || controller.sheet.get_untracked() != Some(sheet)
                {
                    return;
                }
                if let wire::Reply::PivotTables {
                    tables,
                    issues: messages,
                } = reply
                {
                    saved.set(tables);
                    issues.set(messages);
                    ready.set(true);
                }
            },
        );
    });
    view!{
        <details class="dc-clipboard-tools dc-pivot-tools" name="worksheet-tools" data-dnacalc="pivot-tools" on:toggle=move |ev:web_sys::Event|{
            if let Some(node)=ev.target().and_then(|n|n.dyn_into::<web_sys::HtmlDetailsElement>().ok()){open.set(node.open());}
        } on:keydown=move |ev:leptos::ev::KeyboardEvent|{
            if ev.key()=="Escape"{ev.prevent_default();ev.stop_propagation();
                if let Some(details)=document().query_selector("[data-dnacalc=pivot-tools]").ok().flatten().and_then(|e|e.dyn_into::<web_sys::HtmlDetailsElement>().ok()){details.set_open(false);}
                if let Some(summary)=document().query_selector("[data-dnacalc=pivot-tools-toggle]").ok().flatten().and_then(|e|e.dyn_into::<web_sys::HtmlElement>().ok()){let _=summary.focus();}
            }
        }>
            <summary data-dnacalc="pivot-tools-toggle">"Pivot tables"</summary>
            <Show when=move ||open.get()>
                <p class="dc-note" data-dnacalc="pivot-state">{move ||if ready.get(){"Saved pivot definitions. No pivot refresh has run; displayed cells use the worksheet's current contents."}else{"Reading saved pivot definitions…"}}</p>
                <Show when=move ||ready.get()&&saved.get().is_empty()><p class="dc-note">"No saved pivot tables on this worksheet."</p></Show>
                <For each=move ||saved.get() key=|p|p.part.clone() children=move |pivot|{
                    let area=pivot.area;let generation=controller.generation.get_untracked();let revision=controller.summary.get_untracked().revision;let sheet=controller.sheet.get_untracked();let label=pivot.location.clone();let name=pivot.name.clone();
                    view!{<section class="dc-pivot-summary" data-dnacalc="saved-pivot" data-pivot=name>
                        <h3>{pivot.name}" · "{label}</h3>
                        <p data-dnacalc="pivot-source">{pivot.source}</p>
                        <p data-dnacalc="pivot-fields">{pivot.fields}</p>
                        <p class="dc-note">{pivot.cache}</p>
                        <button type="button" data-dnacalc="pivot-reveal" on:click=move |_|{if controller.generation.get_untracked()!=generation||controller.summary.get_untracked().revision!=revision||controller.sheet.get_untracked()!=sheet{return;}if let Some(area)=area.area(){controller.navigate_to(area.first(),move |arrived|{if arrived&&controller.generation.get_untracked()==generation&&controller.sheet.get_untracked()==sheet{controller.select_areas(area.first(),vec![area]);}});}}>"Show saved range"</button>
                    </section>}
                }/>
                <For each=move ||issues.get() key=|s|s.clone() children=move |message|view!{<p class="dc-note" data-dnacalc="pivot-reading-issue">{message}</p>}/>
            </Show>
        </details>
    }.into_any()
}
