//! Worksheet scenarios over the native workbook records and the shared preview seam.

use crate::{Live, Seam, Session, ViewCommand};
use calc_skin::grid::{Area, Sheet};
use calc_skin::{
    Batch, Change, InventoryItem, InventoryKind, ModelChange, ScenarioChange, ScenarioRef,
    SourceRef, ViewId, WorkbookId, address, query,
};
use leptos::prelude::*;

pub const OPEN: &str = "classic.scenarios";
const SAVE: &str = "classic.save-scenario";
const SHOW: &str = "classic.show-scenario";
const RENAME: &str = "classic.rename-scenario";
const DELETE: &str = "classic.delete-scenario";

#[derive(Clone, PartialEq, Eq)]
struct Target {
    workbook: WorkbookId,
    sheet: Sheet,
    areas: Vec<Area>,
}

#[derive(Clone, Copy, Default)]
struct ScenarioManager {
    target: RwSignal<Option<Target>>,
    selected: RwSignal<Option<ScenarioRef>>,
    name: RwSignal<String>,
    element: NodeRef<leptos::html::Dialog>,
}

pub fn commands(view: &ViewId, session: Session) -> [ViewCommand; 5] {
    let manager = session.local::<ScenarioManager>();
    [
        ViewCommand::new(view, OPEN, "Scenario Manager", &[], move |_| {
            let selection = session.selection().ok_or("Choose a worksheet.")?;
            let SourceRef::Cells { sheet, areas } = selection.target else {
                return Err("Choose a worksheet cell.".into());
            };
            let stamp = session.stamp().ok_or("The workbook is unavailable.")?;
            manager.name.set(String::new());
            manager.selected.set(None);
            manager.target.set(Some(Target {
                workbook: stamp.workbook,
                sheet,
                areas,
            }));
            Ok(())
        })
        .when("mode:ready"),
        ViewCommand::new(
            view,
            SAVE,
            "Save current values as a scenario",
            &[],
            move |_| {
                let target = manager
                    .target
                    .get_untracked()
                    .ok_or("Open Scenario Manager first.")?;
                let name = manager.name.get_untracked();
                if name.is_empty() {
                    return Err("Enter a scenario name.".into());
                }
                let label = format!("Save scenario {name}");
                let scenario = ScenarioRef {
                    sheet: target.sheet,
                    name,
                };
                let change = ModelChange::SaveScenarioCells {
                    scenario: scenario.clone(),
                    areas: target.areas,
                };
                ScenarioManager::submit(session, target.workbook, label, change, move || {
                    manager.name.set(String::new());
                    manager.selected.set(Some(scenario));
                })
            },
        )
        .when("mode:ready && writable"),
        ViewCommand::new(view, SHOW, "Show a worksheet scenario", &[], move |_| {
            let (workbook, scenario) = manager.chosen()?;
            let label = format!("Show scenario {}", scenario.name);
            ScenarioManager::submit(
                session,
                workbook,
                label,
                ModelChange::ShowScenario { scenario },
                || {},
            )
        })
        .when("mode:ready && writable"),
        ViewCommand::new(
            view,
            RENAME,
            "Rename a worksheet scenario",
            &[],
            move |_| {
                let (workbook, scenario) = manager.chosen()?;
                let to = manager.name.get_untracked();
                let label = format!("Rename scenario {} to {to}", scenario.name);
                let next = ScenarioRef {
                    sheet: scenario.sheet,
                    name: to.clone(),
                };
                let change = ModelChange::EditScenario {
                    scenario,
                    change: ScenarioChange::Rename { to },
                };
                ScenarioManager::submit(session, workbook, label, change, move || {
                    manager.selected.set(Some(next))
                })
            },
        )
        .when("mode:ready && writable"),
        ViewCommand::new(
            view,
            DELETE,
            "Delete a worksheet scenario",
            &[],
            move |_| {
                let (workbook, scenario) = manager.chosen()?;
                let label = format!("Delete scenario {}", scenario.name);
                let change = ModelChange::EditScenario {
                    scenario,
                    change: ScenarioChange::Remove,
                };
                ScenarioManager::submit(session, workbook, label, change, move || {
                    manager.name.set(String::new());
                    manager.selected.set(None);
                })
            },
        )
        .when("mode:ready && writable"),
    ]
}

pub fn close(session: Session) -> bool {
    let manager = session.local::<ScenarioManager>();
    if manager.target.with_untracked(Option::is_none) {
        return false;
    }
    if let Some(element) = manager.element.get_untracked() {
        element.close();
    }
    manager.target.set(None);
    true
}

pub fn mount(session: Session) -> impl IntoView {
    let manager = session.local::<ScenarioManager>();
    let target = Memo::new(move |_| {
        let stamp = session.stamp()?;
        manager.target.with(|target| {
            target
                .as_ref()
                .filter(|target| target.workbook == stamp.workbook)
                .cloned()
        })
    });
    move || target.get().map(|target| manager.dialog(session, target))
}

impl ScenarioManager {
    fn chosen(self) -> Result<(WorkbookId, ScenarioRef), String> {
        let target = self
            .target
            .get_untracked()
            .ok_or("Open Scenario Manager first.")?;
        let scenario = self
            .selected
            .get_untracked()
            .ok_or("Choose a worksheet scenario.")?;
        Ok((target.workbook, scenario))
    }

    fn submit(
        session: Session,
        workbook: WorkbookId,
        label: String,
        change: ModelChange,
        finished: impl FnOnce() + 'static,
    ) -> Result<(), String> {
        let stamp = session
            .stamp()
            .filter(|stamp| stamp.workbook == workbook)
            .ok_or("Reopen Scenario Manager for the current workbook.")?;
        if !matches!(session.seam(), Seam::Idle) {
            return Err("Finish reviewing the current edit first.".into());
        }
        session.submit_then(
            Batch {
                id: String::new(),
                workbook: Some(workbook),
                base: Some(stamp.model),
                expects: Vec::new(),
                selection_seq: None,
                label: Some(label),
                changes: vec![Change::Model(change)],
            },
            finished,
        );
        Ok(())
    }

    fn dialog(self, session: Session, target: Target) -> impl IntoView {
        let sheet = target.sheet;
        let addresses = target
            .areas
            .into_iter()
            .map(|Area(first, last)| {
                let start = address::address(first.2, first.3);
                if first == last {
                    start
                } else {
                    format!("{start}:{}", address::address(last.2, last.3))
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        crate::dialog::show_modal(self.element);
        let watch = session.watch(query::Inventory {
            kinds: vec![InventoryKind::Scenarios],
        });
        on_cleanup(move || session.unwatch(watch));

        let items = move || {
            let scenarios = match session.read(watch) {
                Live::Ready(answer) => answer
                    .body
                    .into_iter()
                    .filter_map(|item| match item {
                        InventoryItem::Scenario(scenario) if scenario.sheet == sheet => {
                            Some(scenario)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                Live::Refused(_) => {
                    return view! { <p>"The worksheet scenarios are unavailable."</p> }.into_any();
                }
                _ => return view! { <p>"Loading scenarios…"</p> }.into_any(),
            };
            if scenarios.is_empty() {
                return view! { <p>"This worksheet has no saved scenarios."</p> }.into_any();
            }
            view! {
                <ul>
                    {scenarios.into_iter().map(|scenario| {
                        let target = scenario.clone();
                        let label = scenario.name.clone();
                        let name = label.clone();
                        let chosen = move || self.selected.with(|item| item.as_ref() == Some(&target));
                        view! { <li>
                            <button type="button" data-scenario=name aria-pressed=move || chosen().to_string()
                                on:click=move |_| {
                                    self.name.set(scenario.name.clone());
                                    self.selected.set(Some(scenario.clone()));
                                }>
                                {label}
                            </button>
                        </li> }
                    }).collect_view()}
                </ul>
            }.into_any()
        };
        let unavailable = move || {
            !matches!(session.seam(), Seam::Idle)
                || session
                    .summary()
                    .is_some_and(|book| book.capabilities.read_only)
        };
        let no_record = move || unavailable() || self.selected.with(Option::is_none);
        let cannot_rename = move || {
            unavailable()
                || self.selected.with(|scenario| {
                    scenario.as_ref().is_none_or(|scenario| {
                        self.name
                            .with(|name| name.is_empty() || name == &scenario.name)
                    })
                })
        };
        view! {
            <dialog class="dna-workbook-dialog dna-scenario-manager" node_ref=self.element
                data-dnacalc="scenario-manager" data-dna-action-field aria-labelledby="dna-scenario-title"
                on:cancel=move |event: leptos::web_sys::Event| {
                    event.prevent_default();
                    let _ = session.command("edit.cancel");
                }>
                <h2 id="dna-scenario-title">"Scenario Manager"</h2>
                <div data-dnacalc="scenario-list">{items}</div>
                <button type="button" data-command=SHOW data-dnacalc="scenario-show"
                    disabled=no_record
                    on:click=move |_| { let _ = session.command(SHOW); }>"Show"</button>
                <button type="button" data-command=DELETE data-dnacalc="scenario-delete" disabled=no_record
                    on:click=move |_| { let _ = session.command(DELETE); }>"Delete"</button>
                <p>"Changing cells " <code>{addresses}</code></p>
                <form on:submit=move |event| { event.prevent_default(); let _ = session.command(SAVE); }>
                    <label>"Scenario name"
                        <input type="text" required data-dnacalc="scenario-name"
                            prop:value=move || self.name.get()
                            on:input=move |event| self.name.set(event_target_value(&event))/>
                    </label>
                    <button type="submit" data-command=SAVE data-dnacalc="scenario-save"
                        disabled=unavailable>"Save selected cells"</button>
                    <button type="button" data-command=RENAME data-dnacalc="scenario-rename" disabled=cannot_rename
                        on:click=move |_| { let _ = session.command(RENAME); }>"Rename selected scenario"</button>
                </form>
                <button type="button" on:click=move |_| { close(session); }>"Close"</button>
            </dialog>
        }
    }
}
