//! Evidence-backed worksheet sources for Forms maker's candidate rail.

use super::qualified;
use calc_skin::grid::{Key, Sheet};
use calc_skin::query;
use calc_skin::{InputSignal, InventoryItem, SourceRef, ViewId};
use dna_kit::commands::CommandId;
use dna_kit::{Invocation, Live, Session, ViewCommand, Watch};
use leptos::prelude::*;

const PICK: &str = "forms.pick-cell";

#[derive(Clone, Copy)]
struct Picker(RwSignal<bool>);

impl Default for Picker {
    fn default() -> Self {
        Self(RwSignal::new(false))
    }
}

pub(super) fn commands(session: Session, view: &ViewId) -> [ViewCommand; 2] {
    let open = session.local::<Picker>().0;
    [
        ViewCommand::new(view, PICK, "Pick worksheet cell", &[], move |invocation| {
            let at: Key = serde_json::from_value(invocation.args.clone())
                .map_err(|_| "Choose a worksheet cell.".to_owned())?;
            session.select_cell(at);
            Ok(())
        }),
        ViewCommand::new(
            view,
            "forms.toggle-picker",
            "Classic Companion",
            &[],
            move |_| {
                open.update(|open| *open = !*open);
                Ok(())
            },
        ),
    ]
}

pub(super) fn companion(session: Session) -> impl IntoView {
    let open = session.local::<Picker>().0;
    view! {
        <Show when=move || open.get()>
            <aside class="dna-page-card" aria-label="Classic Companion">
                <h2>"Classic Companion"</h2>
                {dna_grid::picker(session)}
            </aside>
        </Show>
    }
}

pub(super) fn rail(session: Session, inventory: Watch<query::Inventory>) -> impl IntoView {
    let inputs = session.following::<query::InputCandidates>();
    let outputs = session.following::<query::OutputCandidates>();
    let sheet = move || session.active().map(|at| Sheet(at.0, at.1));
    view! {
        <aside class="dna-page-card" aria-label="Worksheet sources">
            <h2>"Worksheet sources"</h2>
            <details open>
                <summary>"Inputs"</summary>
                {move || match inputs.at(sheet().map(|sheet| query::InputCandidates { scope: SourceRef::Sheet(sheet) })) {
                    Live::Ready(answer) if !answer.body.is_empty() => view! {
                        <ul>{answer.body.into_iter().map(|candidate| {
                            let reason = match candidate.signal {
                                InputSignal::ValidationList => "Validation list",
                                InputSignal::UnlockedOnProtectedSheet => "Unlocked on protected sheet",
                                InputSignal::LinkedFormControl => "Linked Form Control",
                            };
                            pick_cell(session, candidate.at, Some(reason))
                        }).collect_view()}</ul>
                    }.into_any(),
                    Live::Ready(_) => view! { <p>"No input candidates on this sheet."</p> }.into_any(),
                    Live::Refused(_) => view! { <p>"Input candidates are unavailable for this sheet."</p> }.into_any(),
                    _ => view! { <p aria-busy="true">"Reading inputs…"</p> }.into_any(),
                }}
            </details>
            <details>
                <summary>"Outputs"</summary>
                {move || match outputs.at(sheet().map(|sheet| query::OutputCandidates { scope: SourceRef::Sheet(sheet) })) {
                    Live::Ready(answer) if !answer.body.is_empty() => view! {
                        <ul>{answer.body.into_iter().map(|at| pick_cell(session, at, None)).collect_view()}</ul>
                    }.into_any(),
                    Live::Ready(_) => view! { <p>"No terminal outputs on this sheet."</p> }.into_any(),
                    Live::Refused(_) => view! { <p>"Output candidates are unavailable for this sheet."</p> }.into_any(),
                    _ => view! { <p aria-busy="true">"Reading outputs…"</p> }.into_any(),
                }}
            </details>
            <details>
                <summary>"Tables and charts"</summary>
                {move || match session.read(inventory) {
                    Live::Ready(answer) => view! {
                        <ul>{answer.body.into_iter().filter_map(|item| match item {
                            InventoryItem::Table(table) if Some(Sheet(table.area.0.0, table.area.0.1)) == sheet() => {
                                let args = serde_json::to_value(&table).expect("table facts serialize");
                                Some(add_object(session, "forms.add-table", table.name, args).into_any())
                            }
                            InventoryItem::Chart { object: SourceRef::Object { sheet: owner, name } }
                                if Some(owner) == sheet() => {
                                    let args = serde_json::to_value(SourceRef::Object { sheet: owner, name: name.clone() })
                                        .expect("chart identities serialize");
                                    Some(add_object(session, "forms.add-chart", name, args).into_any())
                                },
                            _ => None,
                        }).collect_view()}</ul>
                    }.into_any(),
                    Live::Refused(_) => view! { <p>"Worksheet objects are unavailable for this sheet."</p> }.into_any(),
                    _ => view! { <p aria-busy="true">"Reading worksheet objects…"</p> }.into_any(),
                }}
            </details>
        </aside>
    }
}

fn add_object(session: Session, command: &'static str, name: String, args: serde_json::Value) -> impl IntoView {
    view! {
        <li><button type="button" data-command=command on:click=move |_| {
            let _ = session.run(&Invocation {
                command: CommandId::new(command),
                args: args.clone(),
            });
        }>{format!("Add {name}")}</button></li>
    }
}

fn pick_cell(session: Session, at: Key, detail: Option<&'static str>) -> impl IntoView {
    view! {
        <li>
            <button type="button" data-command=PICK on:click=move |_| {
                let _ = session.run(&Invocation {
                    command: CommandId::new(PICK),
                    args: serde_json::to_value(at).expect("cell identities serialize"),
                });
            }>{qualified(session, at)}</button>
            <small>{detail}</small>
        </li>
    }
}
