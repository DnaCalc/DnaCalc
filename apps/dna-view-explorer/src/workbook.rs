//! The Formula Explorer tool in a workbook. The caller is pinned when the pane opens.

use super::{Explorer, refusal_reason};
use calc_skin::query::TrialFor;
use calc_skin::{Batch, Change, ModelChange, Refusal, Trial, WorkbookId, address};
use dna_formula_ui::web::serial_clock;
use dna_kit::commands::CommandId;
use dna_kit::{
    DockSide, DockSpec, FormulaContext, FormulaView, Invocation, Live, Registry, Session, ToolSpec,
    ViewCommand, Watch,
};
use leptos::prelude::*;
use std::{rc::Rc, sync::Arc};

const OPEN: &str = "explorer.dock";
const CLOSE: &str = "explorer.close";
const APPLY: &str = "explorer.apply";

#[derive(Clone, Default)]
enum Panel {
    #[default]
    Closed,
    Reading(WorkbookId, Watch<TrialFor>),
    Ready(WorkbookId, Arc<Trial>),
    Refused(WorkbookId, Refusal),
}

impl Panel {
    fn workbook(&self) -> Option<WorkbookId> {
        match self {
            Self::Closed => None,
            Self::Reading(workbook, _) | Self::Ready(workbook, _) | Self::Refused(workbook, _) => {
                Some(*workbook)
            }
        }
    }
}

#[derive(Clone, Copy)]
struct ExplorerDock(RwSignal<Panel>);

impl Default for ExplorerDock {
    fn default() -> Self {
        Self(RwSignal::new(Panel::Closed))
    }
}

pub fn install_docked(registry: &mut Registry) {
    registry.tool(ToolSpec {
        dock: DockSpec {
            command: OPEN,
            label: "Formula Explorer",
            mount,
            panel: None,
        },
        commands,
    });
}

fn close(session: Session, dock: ExplorerDock) {
    if let Panel::Reading(_, watch) = dock.0.get_untracked() {
        session.unwatch(watch);
    }
    dock.0.set(Panel::Closed);
}

fn commands(session: Session) -> Vec<ViewCommand> {
    let dock: ExplorerDock = session.local();
    vec![
        ViewCommand::global(
            OPEN,
            "Open Formula Explorer",
            &["Shift+F3", "Alt+Shift+E"],
            move |_| {
                let at = session.active().ok_or("Select a workbook cell.")?;
                let stamp = session.stamp().ok_or("Open a workbook first.")?;
                close(session, dock);
                dock.0.set(Panel::Reading(
                    stamp.workbook,
                    session.watch(TrialFor { at }),
                ));
                Ok(())
            },
        )
        .when("!editing"),
        ViewCommand::global(CLOSE, "Close Formula Explorer", &[], move |_| {
            close(session, dock);
            Ok(())
        }),
        ViewCommand::global(
            APPLY,
            "Apply Explorer formula to the caller",
            &[],
            move |invocation| {
                let Panel::Ready(_, trial) = dock.0.get_untracked() else {
                    return Err("Open a workbook cell in Formula Explorer.".into());
                };
                let caller = trial
                    .caller
                    .as_ref()
                    .ok_or("The trial has no workbook caller.")?;
                let text = invocation
                    .args
                    .as_str()
                    .filter(|text| text.trim_start().starts_with('='))
                    .ok_or("Enter an Excel formula beginning with =.")?;
                if text == trial.formula {
                    return Err("Change the trial formula before applying it.".into());
                }
                let at = caller.at;
                session.submit_for_review(Batch {
                    id: String::new(),
                    workbook: Some(caller.stamp.workbook),
                    base: Some(caller.stamp.model),
                    expects: vec![dna_skin_runtime::client::cell_source(at)],
                    selection_seq: None,
                    label: Some("Apply Explorer formula".into()),
                    changes: vec![Change::Model(ModelChange::Enter {
                        at,
                        text: text.into(),
                    })],
                });
                Ok(())
            },
        )
        .when("writable && !editing"),
    ]
}

fn mount(session: Session, side: DockSide) -> AnyView {
    if side != DockSide::Right {
        return ().into_any();
    }
    let dock: ExplorerDock = session.local();
    Effect::new(move |_| {
        let panel = dock.0.get();
        let workbook = session.stamp().map(|stamp| stamp.workbook);
        if panel.workbook().is_some() && panel.workbook() != workbook {
            close(session, dock);
            return;
        }
        if let Panel::Reading(workbook, watch) = panel {
            let next = match session.read(watch) {
                Live::Ready(answer) if answer.stamp.workbook == workbook => {
                    Panel::Ready(workbook, Arc::new(answer.body))
                }
                Live::Refused(reason) => Panel::Refused(workbook, reason),
                _ => return,
            };
            session.unwatch(watch);
            dock.0.set(next);
        }
    });
    view! {
        {move || {
            let panel = dock.0.get();
            if matches!(panel, Panel::Closed) {
                return ().into_any();
            }
            let status = RwSignal::new("Ready");
            let mut caller = String::new();
            let body = match panel {
                Panel::Ready(_, trial) => {
                    if let Some(snapshot) = &trial.caller {
                        let at = snapshot.at;
                        let sheet = snapshot.sheets.iter()
                            .find(|sheet| sheet.sheet.0 == at.0 && sheet.sheet.1 == at.1)
                            .map_or("", |sheet| sheet.name.as_str());
                        caller = format!("{sheet}!{}{}", address::column_letters(at.3), at.2);
                    }
                    Explorer::mount(FormulaContext {
                        port: Rc::new(session),
                        draft: (*trial).clone(),
                        auto_preview: true,
                        persist: None,
                        clock: serial_clock,
                        status,
                        apply: Some(Callback::new(move |text: String| {
                            let _ = session.run(&Invocation {
                                command: CommandId::new(APPLY),
                                args: text.into(),
                            });
                        })),
                    })
                }
                Panel::Refused(_, reason) => view! { <p role="alert">{refusal_reason(&reason)}</p> }.into_any(),
                Panel::Reading(..) => view! { <p role="status">"Reading caller…"</p> }.into_any(),
                Panel::Closed => ().into_any(),
            };
            view! {
                <aside class="dna-explorer-dock" data-dnacalc="explorer-dock" aria-label="Workbook Formula Explorer">
                    <header>
                        <strong>"Formula Explorer"</strong>
                        <span data-dnacalc="explorer-caller">{caller}</span>
                        <button type="button" data-command=CLOSE
                            on:click=move |_| { let _ = session.command(CLOSE); }>"Close"</button>
                    </header>
                    {body}
                    <span role="status">{move || status.get()}</span>
                </aside>
            }.into_any()
        }}
    }.into_any()
}
