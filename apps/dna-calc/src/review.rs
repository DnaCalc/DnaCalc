//! Conditional rules, review details and explicit validation decisions through worker messages.
use crate::{
    controller::Controller,
    wire::{self, ConditionalChange, Intent, Reply, Request, Sheet},
};
use leptos::prelude::*;

/// Only user-activated web and mail links become navigable anchors.
pub fn external_link(target: &str) -> Option<String> {
    let target = target.trim();
    let lower = target.to_ascii_lowercase();
    (!target.chars().any(|c| c.is_control())
        && ["https://", "http://", "mailto:"]
            .iter()
            .any(|p| lower.starts_with(p)))
    .then(|| target.to_owned())
}

pub fn controls(controller: Controller) -> impl IntoView {
    let validation = NodeRef::<leptos::html::Dialog>::new();
    Effect::new(move |_| {
        if let Some(dialog) = validation.get() {
            if controller.validation_decision.get().is_some() && !dialog.open() {
                let _ = dialog.show_modal();
            } else if controller.validation_decision.get().is_none() && dialog.open() {
                dialog.close();
            }
        }
    });
    let decide = move |choice| {
        if let Some(dialog) = validation.get_untracked() {
            dialog.close();
        }
        controller.validation_answer(choice);
    };
    let dialog = NodeRef::<leptos::html::Dialog>::new();
    let data = RwSignal::new(None::<wire::Review>);
    let selected = RwSignal::new(None::<u32>);
    let applies = RwSignal::new(String::new());
    let formulas = RwSignal::new(String::new());
    let stop = RwSignal::new(false);
    let kind = RwSignal::new("dataBar".to_owned());
    let color = RwSignal::new("327A94".to_owned());
    Effect::new(move |_| {
        controller.generation.track();
        controller.review_open.set(false);
        data.set(None);
        selected.set(None);
    });
    Effect::new(move |_| {
        let open = controller.review_open.get();
        let at = controller.active_key();
        let revision = controller.revision.get();
        let generation = controller.generation.get();
        if !open {
            return;
        }
        data.set(None);
        selected.set(None);
        let Some(at) = at else {
            data.set(None);
            return;
        };
        controller.ask(Request::Review(wire::Key::of(at)), move |reply| {
            if controller.review_open.get_untracked()
                && controller.active_key_untracked() == Some(at)
                && controller.revision.get_untracked() == revision
                && controller.generation.get_untracked() == generation
                && let Reply::Review(found) = reply
            {
                data.set(Some(found));
                selected.set(None);
                applies.set(
                    controller
                        .selected_area()
                        .map(|a| {
                            format!(
                                "{}{}:{}{}",
                                dna_formula_ui::display::column_letters(a.first().column()),
                                a.first().row(),
                                dna_formula_ui::display::column_letters(a.last().column()),
                                a.last().row()
                            )
                        })
                        .unwrap_or_default(),
                );
            }
        });
    });
    Effect::new(move |_| {
        if let Some(dialog) = dialog.get() {
            if controller.review_open.get() && !dialog.open() {
                let _ = dialog.show_modal();
            } else if !controller.review_open.get() && dialog.open() {
                dialog.close();
            }
        }
    });
    let close = move || {
        if let Some(dialog) = dialog.get_untracked() {
            dialog.close();
        }
        controller.review_open.set(false);
        data.set(None);
        selected.set(None);
        crate::grid::focus_grid();
    };
    let submit = move |change: ConditionalChange| {
        let (Some(sheet), Some(data)) = (controller.sheet.get_untracked(), data.get_untracked())
        else {
            return;
        };
        if data.at.and_then(wire::Key::cell) != controller.active_key_untracked()
            || data
                .at
                .is_none_or(|at| at.cell().is_none_or(|at| at.sheet() != sheet))
        {
            controller.say("Wait for the current cell's rules to load", true);
            return;
        }
        controller.dispatch(
            Intent::EditConditional {
                sheet: Sheet::of(sheet),
                revision: data.revision,
                index: selected.get_untracked(),
                change,
            },
            move |outcome| {
                if outcome.accepted {
                    selected.set(None);
                }
            },
        );
    };
    let choose = move |index: u32| {
        if let Some(rule) = data
            .get_untracked()
            .and_then(|d| d.rules.into_iter().find(|r| r.index == index))
        {
            selected.set(Some(index));
            applies.set(rule.applies);
            formulas.set(rule.formulas.join("\n"));
            stop.set(rule.stop);
        }
    };
    let current = Memo::new(move |_| {
        data.get().and_then(|d| {
            d.rules
                .into_iter()
                .find(|r| Some(r.index) == selected.get())
        })
    });
    let rules = move || {
        data.get().map(|d|d.rules.into_iter().map(|r|{
        let index=r.index;let title=format!("{} · {}",r.priority,r.kind);let state=r.state;let area=r.applies;
        view!{<li><button type="button" class="dc-rule-row" data-dnacalc="rule-row" data-index=index aria-pressed=move ||(selected.get()==Some(index)).to_string() on:click=move |_|choose(index)><strong>{title}</strong><span>{area}</span><small>{state}</small></button></li>}
    }).collect_view())
    };
    let notes = move || {
        data.get().map(|d|{
        if d.notes.is_empty(){view!{<p class="dc-muted">"No note on this cell."</p>}.into_any()}
        else {d.notes.into_iter().map(|(author,text)|view!{<article class="dc-review-note" data-dnacalc="review-note"><h4>{author}</h4><p>{text}</p></article>}).collect_view().into_any()}
    })
    };
    let links = move || {
        data.get().map(|d|d.links.into_iter().map(|link|{
        let external=link.target.as_deref().and_then(external_link);
        let location=link.location.clone();let description=link.tooltip;let display=link.display;
        let target=link.target.clone();
        if let Some(href)=external {
            let href=if let Some(location)=location {format!("{href}#{location}")}else{href};
            view!{<div class="dc-review-link" data-dnacalc="review-link"><a href=href target="_blank" rel="noopener noreferrer" title=description>{display}</a><small>{target}</small></div>}.into_any()
        } else if target.is_none() && let Some(location)=location {
            view!{<div class="dc-review-link" data-dnacalc="review-link"><button title=description on:click=move |_|{close();controller.go_to_text(location.clone(),|_|{});}>{display}</button></div>}.into_any()
        } else {view!{<p class="dc-review-link" data-dnacalc="review-link">{display}" · Opening this link type is unavailable. "<code>{target}</code></p>}.into_any()}
    }).collect_view())
    };
    view! {

        <dialog class="dc-replace dc-validation-dialog" node_ref=validation data-dnacalc="validation-dialog" aria-labelledby="dc-validation-title" aria-describedby="dc-validation-message" on:cancel=move |ev:leptos::ev::Event|{ev.prevent_default();decide("cancel");}>
            <h2 id="dc-validation-title">{move ||controller.validation_decision.get().map(|d|format!("{} · {}",d.style,d.title)).unwrap_or_default()}</h2>
            <p id="dc-validation-message" data-dnacalc="validation-message">{move ||controller.validation_decision.get().map(|d|d.message).unwrap_or_default()}</p>
            <p class="dc-muted">"The cell has not changed."</p>
            <div class="dc-dialog-actions">
                {move ||controller.validation_decision.get().filter(|d|d.style!="Stop").map(|_|view!{<button data-dnacalc="validation-keep" on:click=move |_|decide("keep")>"Keep entry"</button>})}
                {move ||controller.validation_decision.get().filter(|d|d.style!="Information").map(|d|view!{<button data-dnacalc="validation-retry" on:click=move |_|decide("retry")>{if d.style=="Stop" {"Retry"} else {"Edit entry"}}</button>})}
                <button data-dnacalc="validation-cancel" on:click=move |_|decide("cancel")>"Cancel entry"</button>
            </div>
        </dialog>
        <dialog class="dc-replace dc-review-dialog" node_ref=dialog data-dnacalc="review-dialog" aria-labelledby="dc-review-title" on:cancel=move |ev:leptos::ev::Event|{ev.prevent_default();close();}>
            <div class="dc-review-heading"><div><h2 id="dc-review-title">"Rules & review"</h2><p>{move ||controller.active_key().map(|at|controller.cell_label(at)).unwrap_or_default()}</p></div><button data-dnacalc="review-close" on:click=move |_|close()>"Close"</button></div>
            {move ||data.get().is_none().then(||view!{<p role="status">"Loading this cell’s rules and review details…"</p>})}
            <div class="dc-review-layout" aria-busy=move ||data.get().is_none().to_string()>
                <section aria-labelledby="dc-rules-title"><h3 id="dc-rules-title">"Conditional formats"</h3><p class="dc-muted">"Supported rules appear below; other imported rules remain preserved. Lowest priority number runs first."</p>
                    <ol class="dc-rule-list" data-dnacalc="rule-list">{rules}</ol>
                    {move ||data.get().filter(|d|d.rules.is_empty()).map(|_|view!{<p>"No supported rules to display on this worksheet."</p>})}
                    <div class="dc-dialog-actions"><button data-dnacalc="rule-up" disabled=move ||selected.get().is_none() on:click=move |_|submit(ConditionalChange::Move {up:true})>"Move up"</button><button data-dnacalc="rule-down" disabled=move ||selected.get().is_none() on:click=move |_|submit(ConditionalChange::Move {up:false})>"Move down"</button><button data-dnacalc="rule-remove" disabled=move ||current.get().is_none_or(|r|!r.editable) on:click=move |_|submit(ConditionalChange::Remove)>"Remove rule"</button></div>
                    <label>"Applies to"<input data-dnacalc="rule-applies" prop:value=move ||applies.get() on:input=move |ev|applies.set(event_target_value(&ev))/></label>
                    <p class="dc-muted">"Space-separated ranges on this worksheet, such as A1:A10 C1:C10."</p>
                    {move ||current.get().filter(|r|r.formula_editable).map(|_|view!{<label>"Condition formulas, one per line"<textarea data-dnacalc="rule-formulas" prop:value=move ||formulas.get() on:input=move |ev|formulas.set(event_target_value(&ev))></textarea></label>})}
                    {move ||current.get().filter(|r|!r.visual).map(|_|view!{<label class="dc-inline-label"><input type="checkbox" data-dnacalc="rule-stop" prop:checked=move ||stop.get() on:change=move |ev|stop.set(event_target_checked(&ev))/ >"Stop if true"</label>})}
                    {move ||current.get().filter(|r|!r.editable).map(|_|view!{<p>"This extension-backed rule is preserved. Its range, condition and removal cannot be edited yet."</p>})}
                    <button data-dnacalc="rule-apply" disabled=move ||current.get().is_none_or(|r|!r.editable) on:click=move |_|submit(ConditionalChange::Update {applies:applies.get_untracked(),formulas:formulas.get_untracked().lines().map(str::to_owned).collect(),stop:stop.get_untracked()})>"Apply rule changes"</button>
                    <fieldset><legend>"Add a visual rule"</legend><label>"Rule type"<select data-dnacalc="rule-kind" prop:value=move ||kind.get() on:change=move |ev|kind.set(event_target_value(&ev))><option value="dataBar">"Data bar"</option><option value="colorScale">"Colour scale"</option><option value="iconSet">"Three arrows"</option></select></label><label>"RGB colour"<input data-dnacalc="rule-color" maxlength="7" prop:value=move ||color.get() on:input=move |ev|color.set(event_target_value(&ev))/></label><button data-dnacalc="rule-add" disabled=move ||data.get().is_none() on:click=move |_|submit(ConditionalChange::Add {kind:kind.get_untracked(),applies:applies.get_untracked(),color:color.get_untracked()})>"Add rule"</button></fieldset>
                    <div class="dc-dialog-actions"><button data-dnacalc="review-undo" disabled=move ||controller.summary.get().undo==0 on:click=move |_|controller.undo()>"Undo"</button><button data-dnacalc="review-redo" disabled=move ||controller.summary.get().redo==0 on:click=move |_|controller.redo()>"Redo"</button></div>
                </section>
                <section aria-labelledby="dc-notes-title"><h3 id="dc-notes-title">"Notes & links"</h3>{notes}{links}<p class="dc-muted">"Imported notes and links are preserved on save. Editing notes and replying to comments are unavailable."</p>{move ||data.get().and_then(|d|d.validation).map(|v|view!{<h3>"Data validation"</h3><p data-dnacalc="review-validation">{v}</p>})}{move ||data.get().map(|d|d.unsupported.into_iter().map(|text|view!{<p>{text}</p>}).collect_view())}</section>
            </div>
            <p class="dc-review-status" role="status" data-dnacalc="review-status">{move ||controller.status.get().text}</p>
        </dialog>
    }
}

#[cfg(test)]
mod tests {
    use super::external_link;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn only_explicit_web_or_mail_targets_are_links() {
        assert!(external_link(" https://example.test/a?q=b ").is_some());
        assert!(external_link("HTTP://example.test").is_some());
        assert!(external_link("mailto:a@example.test").is_some());
        for target in [
            "javascript:alert(1)",
            "data:text/html,hello",
            "file:///tmp/a",
            "https:\n//example.test",
            "//example.test",
            "tel:123",
        ] {
            assert!(external_link(target).is_none(), "{target}");
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn validation_list_stops_at_edges_and_handles_empty_lists() {
        let next = crate::grid::validation_list_index;
        assert_eq!(next(0, "ArrowUp", 3), Some(0));
        assert_eq!(next(2, "ArrowDown", 3), Some(2));
        assert_eq!(next(8, "Home", 3), Some(0));
        assert_eq!(next(0, "End", 3), Some(2));
        assert_eq!(next(0, "ArrowDown", 0), None);
        assert_eq!(next(0, "Enter", 3), None);
    }
}
