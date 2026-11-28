//! Isolated assumptions for a page being tried on.
use calc_skin::{
    AnchorId, Bound, Budget, Change, Coverage, ModelChange, Page, Preview, PreviewDepth,
    RawValue, Refusal, SourceRef, ViewId, query,
};
use dna_kit::{Live, Session, ViewCommand};
use leptos::prelude::*;
use std::collections::BTreeMap;

const TRIAL: &str = "app.preview-inputs";

#[derive(Default)]
struct Values {
    serial: u64,
    inputs: BTreeMap<AnchorId, (Bound, String)>,
    waiting: bool,
    result: Option<Result<Preview, Refusal>>,
}

#[derive(Clone, Copy, Default)]
struct Trial(RwSignal<Values>);

fn trial(session: Session) -> RwSignal<Values> {
    session.local::<Trial>().0
}

pub(super) fn reset(session: Session) {
    trial(session).update(|values| {
        *values = Values { serial: values.serial.wrapping_add(1), ..Values::default() };
    });
}

pub(super) fn command(session: Session, view: &ViewId) -> ViewCommand {
    let trial = trial(session);
    ViewCommand::new(view, TRIAL, "Calculate trial inputs", &[], move |_| {
        let preview = untrack(|| super::preview_page(session)).ok_or("Try on an App page first")?;
        let stamp = session.stamp().ok_or("Open the workbook first")?;
        let changes = trial.with_untracked(|values| {
            if values.waiting {
                return Err("The trial is still calculating".to_owned());
            }
            let mut changes = Vec::new();
            for (bound, text) in values.inputs.values() {
                if !super::permits(&preview.page, bound) {
                    return Err("The trial binding changed. Reopen the page preview".into());
                }
                changes.push(Change::Model(ModelChange::EnterBound {
                    bound: bound.clone(), text: text.clone(),
                }));
            }
            if changes.is_empty() {
                return Err("Enter a trial value first".into());
            }
            Ok(changes)
        })?;
        trial.update(|values| {
            values.waiting = true;
            values.result = None;
        });
        let serial = trial.with_untracked(|values| values.serial);
        session.preview_batch(
            super::batch(stamp, "Trial App inputs", changes),
            PreviewDepth::WithEffects { budget: Budget {
                cells: 65_536,
                nodes: 65_536,
                edges: 65_536,
                bytes: 4 * 1024 * 1024,
                millis: 2_000,
            }},
            move |result| {
                if untrack(|| super::preview_page(session)).is_none() {
                    return;
                }
                trial.update(|values| {
                    if values.serial == serial {
                        values.waiting = false;
                        values.result = Some(result);
                    }
                });
            },
        );
        Ok(())
    })
    .when("!editing")
}

fn reads(page: &Page) -> Vec<(&str, &Bound)> {
    let mut bindings = Vec::new();
    let _: Result<(), std::convert::Infallible> = page.root.try_for_each_part(&mut |_, component, slots| {
        for (slot, bound) in slots {
            let role = match (component, slot.as_str()) {
                ("input", "value") => "Write",
                ("value", "value") => "Output",
                _ => "Read",
            };
            bindings.push((role, bound));
        }
        Ok(())
    });
    bindings
}

pub(super) fn mount(session: Session, page: &Page) -> impl IntoView + use<> {
    let state = trial(session);
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut disclosed = Vec::new();
    for (role, bound) in reads(page) {
        let address = bound.address.clone();
        let anchor = bound.anchor.to_string();
        if role == "Write" {
            inputs.push(input(state, bound.clone()));
        }
        if role == "Output" {
            outputs.push(output(session, state, bound.clone()));
        }
        disclosed.push(view! {
            <li>{if role == "Write" { "Write" } else { "Read" }} " " <code>{address}</code> " · " <code>{anchor}</code></li>
        });
    }
    let can_run = move || state.with(|trial| !trial.waiting && !trial.inputs.is_empty());
    view! {
        <details class="dna-page-card" data-dnacalc="app-trial-inputs" open>
            <summary>"Try inputs in isolation"</summary>
            <div class="dna-page-tools">
                {inputs}
                <button type="button" data-command=TRIAL disabled=move || !can_run()
                    on:click=move |_| { let _ = session.command(TRIAL); }>"Calculate trial"</button>
            </div>
            <Show when=move || state.with(|trial| trial.waiting)><p role="status">"Calculating trial"</p></Show>
            {move || {
                let reason = state.with(|trial| match &trial.result {
                    Some(Ok(result)) => result.refusal.clone(),
                    Some(Err(reason)) => Some(reason.clone()),
                    None => None,
                });
                reason.map(|reason| view! { <p role="alert">{dna_kit::refusal_text(reason)}</p> })
            }}
            <ul data-dnacalc="app-trial-effects">{outputs}</ul>
            <details><summary>"Workbook bindings"</summary><ul>{disclosed}</ul></details>
        </details>
    }
}

fn input(state: RwSignal<Values>, bound: Bound) -> impl IntoView {
    let id = bound.anchor;
    view! {
        <label>{bound.address.clone()}
            <input type="text" data-trial-address=bound.address.clone()
                disabled=move || state.with(|trial| trial.waiting)
                prop:value=move || state.with(|trial| trial.inputs.get(&id).map(|(_, text)| text.clone()).unwrap_or_default())
                on:input=move |event| {
                    let input = event_target_value(&event);
                    state.update(|trial| {
                        trial.result = None;
                        if input.is_empty() {
                            trial.inputs.remove(&id);
                        } else {
                            trial.inputs.insert(id, (bound.clone(), input));
                        }
                    });
                }/>
        </label>
    }
}

fn output(session: Session, state: RwSignal<Values>, bound: Bound) -> impl IntoView {
    let source = session.following::<query::ResolveBound>();
    move || {
        let ready = state.with(|trial| matches!(&trial.result, Some(Ok(result)) if result.refusal.is_none()));
        let query = ready.then(|| query::ResolveBound { bound: bound.clone() });
        let Live::Ready(answer) = source.at(query) else { return None };
        let SourceRef::Cells { areas, .. } = answer.body else { return None };
        let [area] = areas.as_slice() else { return None };
        if answer.coverage != Coverage::Complete || area.0 != area.1 {
            return None;
        }
        let effect = state.with(|trial| {
            let Some(Ok(result)) = &trial.result else { return None };
            result.effects.iter().find(|effect| effect.at == area.0).cloned()
        })?;
        let display = |value: &RawValue| match value {
            RawValue::Number { bits } => format!("{:.2}", f64::from_bits(*bits)),
            other => other.display(),
        };
        let from = display(&effect.before.value);
        let to = display(&effect.after.value);
        Some(view! {
            <li data-trial-address=bound.address.clone()>
                {bound.address.clone()} " " <output title=effect.before.value.display()>{from}</output>
                " → " <output title=effect.after.value.display()>{to}</output>
            </li>
        })
    }
}
