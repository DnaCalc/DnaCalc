//! Audit instruments over the workbook session. The bench and dock use the same components.

mod audit;
mod live;
mod notes;
mod trace;

use calc_skin::{
    Batch, Budget, Change, Coverage, Evidence, ModelChange, Refusal, SourceRef, Stamp, ViewId,
    address,
    grid::{Area, Key, Sheet},
};
use dna_kit::{
    DockSide, DockSpec, Invocation, Live, Located, MountedView, Registry, Session, View,
    ViewCommand, ViewSpec, commands::CommandId,
};
use leptos::prelude::*;

pub const ID: &str = "inspector";

const INSTRUMENT_BUDGET: Budget = Budget {
    cells: 800_000,
    nodes: 800_000,
    edges: 200_000,
    bytes: 4 * 1024 * 1024,
    millis: 8_000,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Instrument {
    #[default]
    Overview,
    Precedents,
    Dependents,
    Calculation,
    Live,
}

const TABS: [Instrument; 5] = [
    Instrument::Overview,
    Instrument::Precedents,
    Instrument::Dependents,
    Instrument::Calculation,
    Instrument::Live,
];

impl Instrument {
    fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Precedents => "Precedents",
            Self::Dependents => "Dependents",
            Self::Calculation => "Calculation",
            Self::Live => "Live",
        }
    }

    fn command(self) -> &'static str {
        match self {
            Self::Overview => "inspector.overview",
            Self::Precedents => "inspector.precedents",
            Self::Dependents => "inspector.dependents",
            Self::Calculation => "inspector.calculation",
            Self::Live => "inspector.live",
        }
    }

    fn mount(self, session: Session) -> AnyView {
        match self {
            Self::Overview => audit::overview(session).into_any(),
            Self::Precedents => precedents(session).into_any(),
            Self::Dependents => {
                audit::dependencies(session, calc_skin::DependencyDirection::Dependents).into_any()
            }
            Self::Calculation => audit::calculation(session).into_any(),
            Self::Live => audit::sources(session).into_any(),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct State {
    instrument: RwSignal<Instrument>,
    dock: RwSignal<Option<DockSide>>,
    back: RwSignal<Option<(calc_skin::WorkbookId, Key)>>,
    tracing: RwSignal<bool>,
    trace_window: RwSignal<Option<trace::TraceWindow>>,
    details: RwSignal<bool>,
    notes: RwSignal<bool>,
    recording: RwSignal<live::Recording>,
}

pub fn install(registry: &mut Registry) {
    registry.view(Inspector);
}

struct Inspector;

impl View for Inspector {
    fn spec(&self) -> ViewSpec {
        ViewSpec {
            id: ViewId(ID.into()),
            title: "Inspector",
            profile: "excel-365-win-en-us",
        }
    }

    fn commands(&self, session: Session) -> Vec<ViewCommand> {
        let state = session.local::<State>();
        let mut commands = TABS
            .into_iter()
            .map(|instrument| {
                ViewCommand::global(instrument.command(), instrument.label(), &[], move |_| {
                    state.instrument.set(instrument);
                    if instrument == Instrument::Precedents {
                        state.tracing.set(false);
                    }
                    if session.context().view.0 != ID && state.dock.get_untracked().is_none() {
                        state.dock.set(Some(DockSide::Right));
                    }
                    Ok(())
                })
            })
            .collect::<Vec<_>>();
        commands.extend([
            ViewCommand::global("inspector.dock", "Inspector panel", &[], move |_| {
                state
                    .dock
                    .update(|side| *side = side.is_none().then_some(DockSide::Right));
                Ok(())
            }),
            ViewCommand::global(
                "inspector.dock-side",
                "Move Inspector panel",
                &[],
                move |_| {
                    state.dock.update(|side| {
                        *side = Some(match side {
                            Some(DockSide::Right) => DockSide::Bottom,
                            _ => DockSide::Right,
                        });
                    });
                    Ok(())
                },
            ),
            ViewCommand::global("inspector.enlarge", "Enlarge Inspector", &[], move |_| {
                session.show(ViewId(ID.into()));
                Ok(())
            }),
            ViewCommand::global(
                "inspector.reveal",
                "Reveal source",
                &[],
                move |invocation| {
                    let at: Key = serde_json::from_value(invocation.args.clone())
                        .map_err(|_| "Select a worksheet cell to reveal.")?;
                    state.back.set(
                        session
                            .stamp()
                            .map(|stamp| stamp.workbook)
                            .zip(session.active()),
                    );
                    session.select_cell(at);
                    Ok(())
                },
            ),
            ViewCommand::global(
                "inspector.back",
                "Previous inspected cell",
                &["Alt+ArrowLeft"],
                move |_| {
                    if let Some((workbook, at)) = state.back.get_untracked()
                        && session
                            .stamp()
                            .is_some_and(|stamp| stamp.workbook == workbook)
                    {
                        session.select_cell(at);
                    }
                    state.back.set(None);
                    Ok(())
                },
            )
            .when("!editing"),
        ]);
        commands.extend(trace_commands(session));
        commands.extend(notes::commands(session));
        commands.extend(live::commands(session));
        commands
    }

    fn cancel_interaction(&self, session: Session) -> bool {
        let state = session.local::<State>();
        if (session.context().view.0 == ID || state.dock.get_untracked().is_some())
            && state.tracing.get_untracked()
            && state.instrument.get_untracked() == Instrument::Precedents
            && state.trace_window.get_untracked().is_some()
        {
            let _ = session.command("inspector.clear-filter");
            return true;
        }
        false
    }

    fn dock(&self) -> Option<DockSpec> {
        Some(DockSpec {
            command: "inspector.dock",
            label: "Inspector panel",
            mount: dock,
        })
    }

    fn mount(&self, session: Session) -> MountedView {
        MountedView {
            body: bench(session, false).into_any(),
            locate: Box::new(|_| Located::Within {
                label: "Inspector".into(),
            }),
            reveal: Box::new(|_| ()),
        }
    }
}

fn dock(session: Session, side: DockSide) -> AnyView {
    let state = session.local::<State>();
    view! {
        <Show when=move || session.view().0 != ID && state.dock.get() == Some(side)>
            <aside class="dna-inspector-dock" aria-label="Inspector panel" data-inspector-dock=side.label()>
                {bench(session, true)}
            </aside>
        </Show>
    }.into_any()
}

fn bench(session: Session, docked: bool) -> impl IntoView {
    let state = session.local::<State>();
    let selectors = TABS.into_iter().map(|instrument| {
        let command = instrument.command();
        view! {
            <button type="button" data-command=command
                aria-pressed=move || (state.instrument.get() == instrument).to_string()
                on:click=move |_| { let _ = session.command(command); }>{instrument.label()}</button>
        }
    }).collect_view();
    view! {
        <section class="dna-inspector" aria-label="Inspector">
            <header class="dna-inspector-head">
                <div><h1>"Inspector"</h1>
                    <span class="dna-inspector-address">{move || session.active().map(|at| cell_label(session, at))}</span>
                </div>
                <div class="dna-inspector-actions">
                    {button(session, "inspector.details", "Details")}
                    {button(session, "inspector.notes", "Notes")}
                    {button(session, "inspector.pin-signal", "Pin cell")}
                    {docked.then(|| view! {
                        {button(session, "inspector.enlarge", "Enlarge")}
                        {button(session, "inspector.dock-side", "Move panel")}
                        {button(session, "inspector.dock", "Close panel")}
                    })}
                </div>
            </header>
            <nav class="dna-inspector-tabs" aria-label="Inspector instruments">{selectors}</nav>
            <section class="dna-inspector-instrument" data-inspector-tab=move || state.instrument.get().label()>
                {live::pinned(session)}
                <Show when=move || state.details.get()>{notes::details(session)}</Show>
                <Show when=move || state.notes.get()>{notes::mount(session)}</Show>
                {move || state.instrument.get().mount(session)}
            </section>
        </section>
    }
}

fn precedents(session: Session) -> impl IntoView {
    let state = session.local::<State>();
    view! {
        <div class="dna-inspector-actions">
            {button(session, "inspector.precedents", "Dependencies")}
            {button(session, "inspector.trace", "Contributions")}
        </div>
        {move || if state.tracing.get() {
            trace::mount(session).into_any()
        } else {
            audit::dependencies(session, calc_skin::DependencyDirection::Precedents).into_any()
        }}
    }
}

fn trace_commands(session: Session) -> Vec<ViewCommand> {
    let state = session.local::<State>();
    vec![
        ViewCommand::global(
            "inspector.trace",
            "Trace contributions",
            &["Alt+Shift+T"],
            move |_| {
                state.tracing.set(true);
                state.instrument.set(Instrument::Precedents);
                if session.context().view.0 != ID && state.dock.get_untracked().is_none() {
                    state.dock.set(Some(DockSide::Right));
                }
                Ok(())
            },
        )
        .when("!editing"),
        ViewCommand::global(
            "inspector.filter",
            "Show rows from Trace",
            &[],
            move |invocation| {
                let (at, criterion) = serde_json::from_value(invocation.args.clone())
                    .map_err(|_| "Choose a row in the contribution ladder.")?;
                if session.active() != Some(at) {
                    return Err("The inspected cell changed.".into());
                }
                let workbook = session.stamp().ok_or("Open a workbook first.")?.workbook;
                state.trace_window.set(Some(trace::TraceWindow {
                    at,
                    workbook,
                    criterion,
                    offset: 0,
                }));
                Ok(())
            },
        ),
        ViewCommand::global(
            "inspector.filter-page",
            "Change Trace rows",
            &[],
            move |invocation| {
                let offset = serde_json::from_value(invocation.args.clone())
                    .map_err(|_| "Choose a row offset.")?;
                state.trace_window.update(|window| {
                    if let Some(window) = window {
                        window.offset = offset;
                    }
                });
                Ok(())
            },
        ),
        ViewCommand::global(
            "inspector.clear-filter",
            "Clear Trace filter",
            &[],
            move |_| {
                state.trace_window.set(None);
                Ok(())
            },
        ),
        ViewCommand::global(
            "inspector.repair",
            "Preview single-cell repair",
            &[],
            move |invocation| {
                let (at, expected_old, new_text, stamp): (Key, String, String, Stamp) =
                    serde_json::from_value(invocation.args.clone())
                        .map_err(|_| "Choose a formula to replace.")?;
                if session.active() != Some(at) {
                    return Err("The inspected cell changed.".into());
                }
                if !new_text.starts_with('=') || new_text == expected_old {
                    return Err("Enter a changed formula beginning with =.".into());
                }
                session.submit_for_review(Batch {
                    id: String::new(),
                    workbook: Some(stamp.workbook),
                    base: Some(stamp.model),
                    expects: vec![cell_source(at)],
                    selection_seq: None,
                    label: Some("Repaired one formula in Inspector".into()),
                    changes: vec![Change::Model(ModelChange::RewriteFormula {
                        at,
                        expected_old,
                        new_text,
                    })],
                });
                Ok(())
            },
        )
        .when("!editing && writable"),
    ]
}

fn run(session: Session, command: &'static str, args: serde_json::Value) {
    let _ = session.run(&Invocation {
        command: CommandId::new(command),
        args,
    });
}

fn cell_source(at: Key) -> SourceRef {
    SourceRef::Cells {
        sheet: Sheet(at.0, at.1),
        areas: vec![Area(at, at)],
    }
}

fn button(session: Session, command: &'static str, label: &'static str) -> impl IntoView {
    invoke(session, command, serde_json::Value::Null, label.to_owned())
}

fn invoke(
    session: Session,
    command: &'static str,
    args: serde_json::Value,
    label: String,
) -> impl IntoView {
    let invocation = Invocation {
        command: CommandId::new(command),
        args,
    };
    view! {
        <button type="button" data-command=command
            on:click=move |_| { let _ = session.run(&invocation); }>{label}</button>
    }
}

fn reveal(session: Session, at: Key) -> impl IntoView {
    invoke(
        session,
        "inspector.reveal",
        serde_json::json!(at),
        cell_label(session, at),
    )
}

fn sheet_label(session: Session, sheet: Sheet) -> String {
    session
        .summary()
        .and_then(|summary| {
            summary
                .sheets
                .into_iter()
                .find(|entry| entry.sheet == sheet)
        })
        .map_or_else(|| format!("Sheet {}", sheet.0), |entry| entry.name)
}

fn a1(at: Key) -> String {
    format!("{}{}", address::column_letters(at.3), at.2)
}

fn cell_label(session: Session, at: Key) -> String {
    format!("{}!{}", sheet_label(session, Sheet(at.0, at.1)), a1(at))
}

fn area_label(session: Session, Area(first, last): Area) -> String {
    let first_label = cell_label(session, first);
    if first == last {
        first_label
    } else {
        format!("{first_label}:{}", a1(last))
    }
}

fn coverage(coverage: Coverage) -> Option<String> {
    match coverage {
        Coverage::Complete => None,
        Coverage::Partial {
            done,
            total: Some(total),
            ..
        } => Some(format!("Partial · {done} of {total}")),
        Coverage::Partial {
            done, total: None, ..
        } => Some(format!("Partial · {done} inspected")),
    }
}

fn evidence<T>(answer: Evidence<T>) -> Result<T, String> {
    match answer {
        Evidence::Derived { body, .. } | Evidence::Observed { body, .. } => Ok(body),
        Evidence::Unavailable { reason } => Err(reason),
    }
}

fn result<T>(live: Live<T>, prompt: &str) -> Result<T, String> {
    match live {
        Live::Ready(answer) => Ok(answer),
        Live::Idle => Err(prompt.into()),
        Live::Loading => Err("Loading…".into()),
        Live::Outdated => Err("Updating…".into()),
        Live::Refused(Refusal::NotModelled { what, .. }) => Err(what),
        Live::Refused(Refusal::Invalid { reason } | Refusal::Transport(reason)) => Err(reason),
        Live::Refused(Refusal::OverBudget { alternatives, .. }) => Err(format!(
            "Inspection exceeds its budget. {}",
            alternatives.join(" · ")
        )),
        Live::Refused(reason) => Err(format!("Unavailable · {reason:?}")),
    }
}

fn status(message: String) -> AnyView {
    view! { <p class="dna-inspector-status" role="status">{message}</p> }.into_any()
}
