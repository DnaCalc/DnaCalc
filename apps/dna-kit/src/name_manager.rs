//! Defined names over the workbook's inventory and the shared preview seam.

use crate::commands::{CommandId, CommandSpec, Layer, When};
use crate::{Invocation, Live, Seam, Session};
use calc_skin::{
    Batch, Change, InventoryItem, InventoryKind, ModelChange, ModelPropertyDiff, NameChange,
    NameScope, Revision, SourceRef, WorkbookId, query,
};
use leptos::prelude::*;

pub const OPEN: &str = "classic.name-manager";
const CHANGE: &str = "classic.change-name";

#[derive(Clone)]
struct ExistingName {
    name: String,
    formula: String,
    revision: Revision,
}

#[derive(Clone)]
struct Draft {
    name: String,
    formula: String,
    scope: NameScope,
    existing: Option<ExistingName>,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            name: String::new(),
            formula: String::new(),
            scope: NameScope::Workbook,
            existing: None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct NameManager {
    workbook: RwSignal<Option<WorkbookId>>,
    draft: RwSignal<Draft>,
    element: NodeRef<leptos::html::Dialog>,
}

impl NameManager {
    pub fn install(session: Session) -> Self {
        let manager = Self {
            workbook: RwSignal::new(None),
            draft: RwSignal::new(Draft::default()),
            element: NodeRef::new(),
        };
        let ready = When::parse("view:classic && mode:ready").expect("a built-in condition parses");
        session.register(
            CommandSpec {
                id: CommandId::new(OPEN),
                label: "Name Manager".into(),
                enabled: ready.clone(),
            },
            move |_| {
                let workbook = session
                    .stamp()
                    .ok_or("The workbook is unavailable.")?
                    .workbook;
                manager.draft.set(Draft::default());
                manager.workbook.set(Some(workbook));
                Ok(())
            },
        );
        session.bind(Layer::Shell, ready.clone(), "Ctrl+F3", CommandId::new(OPEN));
        session.register(
            CommandSpec {
                id: CommandId::new(CHANGE),
                label: "Change a defined name".into(),
                enabled: ready.and(When::parse("writable").expect("a built-in condition parses")),
            },
            move |invocation| {
                let (base, scope, change): (Revision, NameScope, NameChange) =
                    serde_json::from_value(invocation.args.clone())
                        .map_err(|_| "The name change is incomplete.")?;
                let workbook = session
                    .stamp()
                    .map(|stamp| stamp.workbook)
                    .filter(|workbook| Some(*workbook) == manager.workbook.get_untracked())
                    .ok_or("Open Name Manager for the current workbook.")?;
                let label = match change {
                    NameChange::Define { .. } | NameChange::DefineFunction { .. } => "Define name",
                    NameChange::Rename { .. } | NameChange::SetFormula { .. } => "Edit name",
                    NameChange::Remove { .. } => "Delete name",
                };
                session.submit_then(
                    Batch {
                        id: String::new(),
                        workbook: Some(workbook),
                        base: Some(base),
                        expects: vec![SourceRef::Workbook],
                        selection_seq: None,
                        label: Some(label.into()),
                        changes: vec![Change::Model(ModelChange::Name { scope, change })],
                    },
                    move || manager.draft.set(Draft::default()),
                );
                Ok(())
            },
        );
        manager
    }

    /// Close the dialog if it is open. The shell cancels an active preview first.
    pub fn close(self) -> bool {
        let open = self.workbook.get_untracked().is_some();
        if open {
            if let Some(dialog) = self.element.get_untracked() {
                dialog.close();
            }
            self.workbook.set(None);
        }
        open
    }

    pub fn mount(self, session: Session) -> impl IntoView {
        view! {
            <Show when=move || session.view().0 == "classic" && self.workbook.get()
                .is_some_and(|workbook| session.stamp().is_some_and(|stamp| stamp.workbook == workbook))>
                {self.dialog(session)}
            </Show>
        }
    }

    fn change(self, session: Session, remove: bool) {
        let draft = self.draft.get_untracked();
        let Some(stamp) = session.stamp() else {
            return;
        };
        let (base, change) = match draft.existing {
            Some(existing) if remove => (
                existing.revision,
                NameChange::Remove {
                    name: existing.name,
                },
            ),
            Some(existing) if draft.name != existing.name => {
                if draft.formula != existing.formula {
                    session.refuse(calc_skin::Refusal::NotModelled {
                        owner: "dc-s2xup".into(),
                        what: "Renaming a name and changing its formula in one edit. Save the changes separately.".into(),
                    });
                    return;
                }
                (
                    existing.revision,
                    NameChange::Rename {
                        name: existing.name,
                        to: draft.name,
                    },
                )
            }
            Some(existing) => (
                existing.revision,
                NameChange::SetFormula {
                    name: existing.name,
                    formula: draft.formula,
                },
            ),
            None if remove => return,
            None => (
                stamp.model,
                NameChange::Define {
                    name: draft.name,
                    formula: draft.formula,
                },
            ),
        };
        let _ = session.run(&Invocation {
            command: CommandId::new(CHANGE),
            args: serde_json::json!((base, draft.scope, change)),
        });
    }

    fn dialog(self, session: Session) -> impl IntoView {
        Effect::new(move |_| {
            if let Some(dialog) = self.element.get() {
                if !dialog.open() {
                    let _ = dialog.show_modal();
                }
            }
        });
        let filter = RwSignal::new(String::new());
        let inventory = session.watch(query::Inventory {
            kinds: vec![InventoryKind::Names],
        });
        on_cleanup(move || session.unwatch(inventory));
        let disabled = move || {
            !matches!(session.seam(), Seam::Idle)
                || session
                    .summary()
                    .is_some_and(|book| book.capabilities.read_only)
        };
        let rows = move || {
            let answer = match session.read(inventory) {
                Live::Ready(answer) => answer,
                Live::Refused(_) => {
                    return view! { <tr><td colspan="3">"Names are unavailable."</td></tr> }
                        .into_any();
                }
                _ => return view! { <tr><td colspan="3">"Loading names…"</td></tr> }.into_any(),
            };
            let query = filter.get().to_lowercase();
            let revision = answer.stamp.model;
            let mut items: Vec<_> = answer
                .body
                .into_iter()
                .filter_map(|item| {
                    let InventoryItem::Name {
                        scope,
                        name,
                        formula,
                        hidden: false,
                    } = item
                    else {
                        return None;
                    };
                    name.to_lowercase()
                        .contains(&query)
                        .then_some((scope, name, formula))
                })
                .collect();
            items.sort_by_cached_key(|(_, name, _)| name.to_lowercase());
            if items.is_empty() {
                return view! { <tr><td colspan="3">"No matching names."</td></tr> }.into_any();
            }
            items.into_iter().map(|(scope, name, formula)| {
                let scope_label = match &scope {
                    NameScope::Workbook => "Workbook".into(),
                    NameScope::Sheet(sheet) => session.summary().into_iter().flat_map(|book| book.sheets)
                        .find(|item| item.sheet == *sheet).map_or_else(|| "Worksheet".into(), |item| item.name),
                };
                let selected = Draft {
                    name: name.clone(), formula: formula.clone(), scope: scope.clone(),
                    existing: Some(ExistingName {
                        name: name.clone(), formula: formula.clone(), revision,
                    }),
                };
                view! {
                    <tr>
                        <td><button type="button"
                            on:click=move |_| self.draft.set(selected.clone())>{name}</button></td>
                        <td>{scope_label}</td>
                        <td><code>{formula}</code></td>
                    </tr>
                }
            }).collect_view().into_any()
        };
        view! {
            <dialog class="dna-name-manager" data-dnacalc="name-manager" data-dna-action-field
                aria-labelledby="dna-name-manager-title" node_ref=self.element
                on:cancel=move |event: leptos::web_sys::Event| {
                    event.prevent_default();
                    let _ = session.command("edit.cancel");
                }>
                <h2 id="dna-name-manager-title">"Name Manager"</h2>
                <div class="dna-name-manager-tools">
                    <label>"Filter names" <input type="search"
                        on:input=move |event| filter.set(event_target_value(&event))/></label>
                    <button type="button" disabled=disabled
                        on:click=move |_| self.draft.set(Draft::default())>"New"</button>
                    <button type="button" disabled=disabled data-command="edit.undo"
                        on:click=move |_| { let _ = session.command("edit.undo"); }>"Undo"</button>
                    <button type="button" disabled=disabled data-command="edit.redo"
                        on:click=move |_| { let _ = session.command("edit.redo"); }>"Redo"</button>
                </div>
                <div class="dna-name-manager-list">
                    <table data-dnacalc="name-manager-list">
                        <thead><tr><th scope="col">"Name"</th><th scope="col">"Scope"</th>
                            <th scope="col">"Refers to"</th></tr></thead>
                        <tbody>{rows}</tbody>
                    </table>
                </div>
                <form on:submit=move |event| {
                    event.prevent_default();
                    self.change(session, false);
                }>
                    <fieldset disabled=disabled>
                        <legend>{move || if self.draft.with(|draft| draft.existing.is_some()) {
                            "Edit name"
                        } else {
                            "New name"
                        }}</legend>
                        <label>"Name" <input type="text" required data-dnacalc="name-manager-name"
                            prop:value=move || self.draft.with(|draft| draft.name.clone())
                            on:input=move |event| self.draft.update(|draft| draft.name = event_target_value(&event))/>
                        </label>
                        <label>"Scope" <select
                            prop:value=move || serde_json::to_string(&self.draft.get().scope).unwrap()
                            disabled=move || self.draft.with(|draft| draft.existing.is_some())
                            on:change=move |event| {
                                if let Ok(scope) = serde_json::from_str(&event_target_value(&event)) {
                                    self.draft.update(|draft| draft.scope = scope);
                                }
                            }>
                            <option value=serde_json::to_string(&NameScope::Workbook).unwrap()>"Workbook"</option>
                            {move || session.summary().into_iter().flat_map(|book| book.sheets)
                                .filter(|item| item.worksheet)
                                .map(|item| view! {
                                    <option value=serde_json::to_string(&NameScope::Sheet(item.sheet)).unwrap()>
                                        {item.name}
                                    </option>
                                }).collect_view()}
                        </select></label>
                        <label>"Refers to" <input type="text" required data-dnacalc="name-manager-formula"
                            prop:value=move || self.draft.with(|draft| draft.formula.clone())
                            on:input=move |event| self.draft.update(|draft| draft.formula = event_target_value(&event))/>
                        </label>
                        <div class="dna-name-manager-tools">
                            <button type="submit" data-dnacalc="name-manager-save" data-command=CHANGE>"Save"</button>
                            <button type="button" data-command=CHANGE
                                disabled=move || self.draft.with(|draft| draft.existing.is_none())
                                on:click=move |_| self.change(session, true)>"Delete"</button>
                        </div>
                    </fieldset>
                </form>
                {move || match session.seam() {
                    Seam::Review(preview) => Some(view! {
                        <section role="alert" aria-label="Review name change">
                            {preview.properties.into_iter().filter_map(|property| match property {
                                ModelPropertyDiff::DefinedName { before, after, .. } => {
                                    let text = |name: Option<calc_skin::DefinedNameText>| {
                                        name.map_or_else(|| "—".into(), |name| format!("{} {}", name.name, name.formula))
                                    };
                                    Some(view! { <p>{text(before)}" → "{text(after)}</p> })
                                }
                                _ => None,
                            }).collect_view()}
                            {preview.warnings.join(" · ")}
                            <button type="button" data-command="edit.apply-review"
                                on:click=move |_| { let _ = session.command("edit.apply-review"); }>"Apply"</button>
                            <button type="button" data-command="edit.cancel"
                                on:click=move |_| { let _ = session.command("edit.cancel"); }>"Cancel"</button>
                        </section>
                    }),
                    _ => None,
                }}
                <p role="status">{move || session.notice()}</p>
                <button type="button" data-command="edit.cancel"
                    on:click=move |_| { let _ = session.command("edit.cancel"); }>"Close"</button>
            </dialog>
        }
    }
}
