use calc_skin::{
    Batch, Budget, Change, PresentationChange, Preview, PreviewDepth, ProposalId, ProposalReview,
    ProposalStatus, Refusal, TrailOutcome, UnitId, WorkbookId, query,
};
use dna_kit::{Following, Invocation, Live, Seam, Session, ViewCommand, commands::CommandId};
use leptos::prelude::*;
use std::collections::BTreeSet;

const OPEN: &str = "proposal.open";
const CLOSE: &str = "proposal.close";
const TOGGLE: &str = "proposal.toggle-unit";
const PREVIEW: &str = "proposal.preview";
const WRITES: &str = "proposal.preview-writes";
const ACCEPT: &str = "proposal.accept";
const REJECT: &str = "proposal.reject";
const TRAIL: &str = "proposal.trail";
const EFFECT_BUDGET: Budget = Budget {
    cells: 65_536,
    nodes: 65_536,
    edges: 65_536,
    bytes: 4 * 1024 * 1024,
    millis: 1000,
};

#[derive(Clone, Copy)]
pub(super) struct Proposals {
    list: Following<query::Proposals>,
    workbook: RwSignal<Option<WorkbookId>>,
    active: RwSignal<Option<ProposalId>>,
    selected: RwSignal<BTreeSet<UnitId>>,
    result: RwSignal<Option<Result<Preview, Refusal>>>,
    pending: RwSignal<bool>,
    ticket: RwSignal<u64>,
}

impl Proposals {
    pub(super) fn install(session: Session) -> Self {
        let panel = Self {
            list: session.following(),
            workbook: RwSignal::new(None),
            active: RwSignal::new(None),
            selected: RwSignal::new(BTreeSet::new()),
            result: RwSignal::new(None),
            pending: RwSignal::new(false),
            ticket: RwSignal::new(0),
        };
        session.install(vec![
            ViewCommand::global(OPEN, "Review proposals", &[], move |invocation| {
                let id: Option<ProposalId> = serde_json::from_value(invocation.args.clone())
                    .map_err(|_| "Choose a workbook proposal".to_owned())?;
                panel.workbook.set(Some(
                    session.stamp().ok_or("Open a workbook first")?.workbook,
                ));
                panel.active.set(id);
                panel.selected.set(BTreeSet::new());
                panel.reset();
                panel.list.at(Some(query::Proposals));
                Ok(())
            })
            .when("!editing"),
            ViewCommand::global(CLOSE, "Close proposals", &[], move |_| {
                panel.close();
                Ok(())
            }),
            ViewCommand::global(TOGGLE, "Select a proposal unit", &[], move |invocation| {
                let unit: UnitId = serde_json::from_value(invocation.args.clone())
                    .map_err(|_| "Choose a proposal unit".to_owned())?;
                let active = panel.review(session).ok_or("Choose a proposal first")?;
                if panel.pending.get_untracked()
                    || !active.units.iter().any(|candidate| {
                        candidate.id == unit && candidate.status == ProposalStatus::Pending
                    })
                {
                    return Err("This unit is unavailable for selection".into());
                }
                panel.selected.update(|selected| {
                    if !selected.remove(&unit) {
                        selected.insert(unit);
                    }
                });
                panel.reset();
                Ok(())
            }),
            ViewCommand::global(PREVIEW, "Preview selected effects", &[], move |_| {
                panel.preview(
                    session,
                    PreviewDepth::WithEffects {
                        budget: EFFECT_BUDGET,
                    },
                )
            })
            .when("!editing"),
            ViewCommand::global(WRITES, "Preview selected writes", &[], move |_| {
                panel.preview(session, PreviewDepth::WritesOnly)
            })
            .when("!editing"),
            ViewCommand::global(ACCEPT, "Accept selected units", &[], move |_| {
                panel.decide(session, true)
            })
            .when("!editing && writable"),
            ViewCommand::global(REJECT, "Reject selected units", &[], move |_| {
                panel.decide(session, false)
            })
            .when("!editing && writable"),
            ViewCommand::global(TRAIL, "Record the agent trail", &[], move |invocation| {
                let enabled = invocation.args.as_bool().ok_or("Choose whether to record the trail")?;
                let stamp = session.stamp().ok_or("Open a workbook first")?;
                session.submit(Batch {
                    id: String::new(),
                    workbook: Some(stamp.workbook),
                    base: Some(stamp.model),
                    expects: Vec::new(),
                    selection_seq: None,
                    label: Some(if enabled { "Enabled the agent trail" } else { "Paused the agent trail" }.into()),
                    changes: vec![Change::Presentation(PresentationChange::Trail { enabled })],
                });
                Ok(())
            })
            .when("!editing && writable"),
        ]);
        panel
    }

    pub(super) fn close(self) -> bool {
        let open = self.workbook.get_untracked().is_some();
        if open {
            self.workbook.set(None);
            self.active.set(None);
            self.selected.set(BTreeSet::new());
            self.reset();
            self.list.at(None);
        }
        open
    }

    fn reset(self) {
        self.ticket
            .update(|ticket| *ticket = ticket.wrapping_add(1));
        self.result.set(None);
        self.pending.set(false);
    }

    fn active_workbook(self, session: Session) -> bool {
        self.workbook
            .get()
            .is_some_and(|open| session.stamp().is_some_and(|stamp| stamp.workbook == open))
    }

    fn review(self, session: Session) -> Option<ProposalReview> {
        if !self.active_workbook(session) {
            return None;
        }
        let active = self.active.get()?;
        match self.list.at(Some(query::Proposals)) {
            Live::Ready(answer) => answer
                .body
                .into_iter()
                .find(|proposal| proposal.id == active),
            _ => None,
        }
    }

    fn selection(self, session: Session) -> Result<(ProposalId, Vec<UnitId>), String> {
        if self.pending.get_untracked() || !matches!(session.seam(), Seam::Idle) {
            return Err("Finish the current request first".into());
        }
        let review = self.review(session).ok_or("Choose a proposal first")?;
        let units: Vec<_> = self.selected.get_untracked().into_iter().collect();
        if units.is_empty() {
            return Err("Select at least one unit".into());
        }
        Ok((review.id, units))
    }

    fn preview(self, session: Session, depth: PreviewDepth) -> Result<(), String> {
        let (proposal, units) = self.selection(session)?;
        self.reset();
        self.pending.set(true);
        let ticket = self.ticket.get_untracked();
        session.preview_proposal(proposal, units, depth, move |result| {
            if self.ticket.get_untracked() == ticket {
                self.pending.set(false);
                self.result.set(Some(result));
            }
        });
        Ok(())
    }

    fn decide(self, session: Session, accept: bool) -> Result<(), String> {
        let (proposal, units) = self.selection(session)?;
        if accept
            && !self.result.with_untracked(
                |result| matches!(result, Some(Ok(preview)) if preview.refusal.is_none()),
            )
        {
            return Err("Preview the selected units before accepting".into());
        }
        self.pending.set(true);
        let ticket = self.ticket.get_untracked();
        let (accept, reject) = if accept {
            (units, Vec::new())
        } else {
            (Vec::new(), units)
        };
        session.decide_proposal(proposal, accept, reject, move |result| {
            if self.ticket.get_untracked() == ticket {
                self.pending.set(false);
                match result {
                    Ok(_) => {
                        self.selected.set(BTreeSet::new());
                        self.result.set(None);
                    }
                    Err(reason) => self.result.set(Some(Err(reason))),
                }
            }
        });
        Ok(())
    }
}

impl Proposals {
    pub(super) fn opener(self, session: Session) -> impl IntoView {
        view! {
            <Show when=move || session.summary().is_some_and(|book| book.pending_proposals > 0)
                fallback=move || view! {
                    <Show when=move || matches!(session.view_part(), Live::Ready(view) if !view.body.trail.events.is_empty())>
                        {super::command_button(session, OPEN, "Agent trail")}
                    </Show>
                }>
                {super::command_button(session, OPEN, "Review proposals")}
            </Show>
        }
    }

    pub(super) fn panel(self, session: Session) -> impl IntoView {
        let open = Memo::new(move |_| self.active_workbook(session));
        Effect::new(move |_| {
            if let Some(workbook) = self.workbook.get()
                && session.stamp().is_some_and(|stamp| stamp.workbook != workbook)
            {
                self.close();
            }
        });
        view! {
            <Show when=move || open.get()>
                <aside class="dna-book-details dna-proposal-review" data-dnacalc="proposal-review" aria-label="Agent proposals">
                    <header>
                        <h2>"Agent proposals"</h2>
                        {super::command_button(session, CLOSE, "Close")}
                    </header>
                    {move || {
                        if let Some(review) = self.review(session) {
                            return self.units(session, review).into_any();
                        }
                        match self.list.at(Some(query::Proposals)) {
                            Live::Ready(answer) if answer.body.is_empty() => view! {
                                <p>"No proposals yet"</p>
                            }.into_any(),
                            Live::Ready(answer) => view! {
                                <ul class="dna-proposal-list">
                                    {answer.body.into_iter().map(|review| {
                                        view! {
                                            <li>
                                                <button type="button" on:click=move |_| {
                                                    let _ = session.run(&Invocation {
                                                        command: CommandId::new(OPEN),
                                                        args: serde_json::to_value(&review.id).expect("proposal ID serializes"),
                                                    });
                                                }>{review.intent}</button>
                                                <small>{review.agent}</small>
                                            </li>
                                        }
                                    }).collect_view()}
                                </ul>
                            }.into_any(),
                            Live::Refused(reason) => view! { <p role="alert">{dna_kit::refusal_text(reason)}</p> }.into_any(),
                            _ => view! { <p role="status">"Loading proposals"</p> }.into_any(),
                        }
                    }}
                    {trail(session)}
                </aside>
            </Show>
        }
    }

    fn units(self, session: Session, review: ProposalReview) -> impl IntoView {
        let empty = move || self.selected.with(BTreeSet::is_empty);
        let awaiting = move || self.pending.get();
        let relevant = move || {
            self.result
                .with(|result| matches!(result, Some(Ok(preview)) if preview.refusal.is_none()))
        };
        view! {
            <button type="button" on:click=move |_| {
                let _ = session.command(OPEN);
            }>"All proposals"</button>
            <h3>{review.intent}</h3>
            <p>{review.agent}</p>
            <fieldset disabled=awaiting>
                <legend>"Units to review"</legend>
                {review.units.into_iter().map(|unit| {
                    let selected_id = unit.id.clone();
                    let action_id = unit.id;
                    let status = unit.status;
                    let needs = (!unit.needs.is_empty()).then(|| {
                        let names = unit.needs.into_iter().map(|id| id.0).collect::<Vec<_>>().join(", ");
                        format!("Needs {names}")
                    });
                    view! {
                        <label class="dna-proposal-unit">
                            <input type="checkbox" disabled=status != ProposalStatus::Pending
                                prop:checked=move || self.selected.with(|ids| ids.contains(&selected_id))
                                on:change=move |_| {
                                    let _ = session.run(&Invocation {
                                        command: CommandId::new(TOGGLE),
                                        args: serde_json::to_value(&action_id).expect("unit ID serializes"),
                                    });
                                }/>
                            <span>{unit.label}</span>
                            {needs.map(|needs| view! { <small>{needs}</small> })}
                            {(status != ProposalStatus::Pending).then(|| view! {
                                <small>{match status {
                                    ProposalStatus::Accepted => "Accepted",
                                    ProposalStatus::Rejected => "Rejected",
                                    ProposalStatus::Pending => "",
                                }}</small>
                            })}
                        </label>
                    }
                }).collect_view()}
            </fieldset>
            <div class="dna-proposal-actions">
                {action(session, PREVIEW, "Preview effects", move || awaiting() || empty())}
                {action(session, WRITES, "Preview writes", move || awaiting() || empty())}
                {action(session, ACCEPT, "Accept selected", move || awaiting() || empty() || !relevant())}
                {action(session, REJECT, "Reject selected", move || awaiting() || empty())}
            </div>
            <Show when=awaiting><p role="status">"Reviewing selected units"</p></Show>
            {move || self.result.get().map(|result| reviewed(session, result))}
        }
    }
}

fn trail(session: Session) -> impl IntoView {
    let current = Memo::new(move |_| match session.view_part() {
        Live::Ready(view) => Some(view.body.trail),
        _ => None,
    });
    view! {
        <details data-dnacalc="agent-trail">
            <summary>"Agent trail"</summary>
            <label class="dna-proposal-unit">
                <input type="checkbox" data-command=TRAIL
                    prop:checked=move || current.with(|trail| trail.as_ref().is_some_and(|trail| trail.enabled))
                    disabled=move || current.with(Option::is_none) || !matches!(session.seam(), Seam::Idle)
                    on:change=move |event| {
                        let _ = session.run(&Invocation {
                            command: CommandId::new(TRAIL),
                            args: serde_json::Value::Bool(event_target_checked(&event)),
                        });
                    }/>
                <span>"Record decisions in this workbook"</span>
            </label>
            {move || current.get().map(|trail| view! {
                {(trail.dropped > 0).then(|| view! {
                    <p>{format!("Earlier decisions removed from the trail ({})", trail.dropped)}</p>
                })}
                <ol>
                    {trail.events.into_iter().rev().map(|event| {
                        let outcome = match event.outcome {
                            TrailOutcome::Accepted => "Accepted",
                            TrailOutcome::Rejected => "Rejected",
                            TrailOutcome::Undone => "Undone",
                        };
                        view! {
                            <li>
                                <strong>{outcome}</strong> " · " {event.agent}
                                <p>{event.intent} " · " {event.unit.0}</p>
                                {(!event.targets.is_empty()).then(|| view! {
                                    <small>{event.targets.join(", ")}</small>
                                })}
                            </li>
                        }
                    }).collect_view()}
                </ol>
            })}
        </details>
    }
}

fn action(
    session: Session,
    command: &'static str,
    label: &'static str,
    disabled: impl Fn() -> bool + Send + 'static,
) -> impl IntoView {
    view! {
        <button type="button" data-command=command disabled=disabled
            on:click=move |_| { let _ = session.command(command); }>{label}</button>
    }
}

fn write_line(session: Session, write: calc_skin::ModelWriteDiff) -> impl IntoView {
    let at = write.at;
    let address = session
        .summary()
        .and_then(|summary| {
            summary
                .sheets
                .into_iter()
                .find(|sheet| sheet.sheet == calc_skin::grid::Sheet(at.0, at.1))
        })
        .map_or_else(
            || super::a1(at),
            |sheet| {
                format!(
                    "{}!{}",
                    calc_skin::address::quoted_sheet(&sheet.name),
                    super::a1(at)
                )
            },
        );
    view! {
        <li>
            <span class="dna-book-review-write">
                <strong>{address}</strong>
                <code>{write.before.unwrap_or_else(|| "Blank".into())}</code>" → "
                <code>{write.after.unwrap_or_else(|| "Blank".into())}</code>
            </span>
        </li>
    }
}

fn reviewed(session: Session, result: Result<Preview, Refusal>) -> AnyView {
    let preview = match result {
        Ok(preview) => preview,
        Err(Refusal::Conflict { diffs, .. }) => return view! {
            <section role="alert" data-dnacalc="proposal-conflict">
                <p>"The reviewed sources changed"</p>
                <ul>{diffs.into_iter().map(|diff| write_line(session, diff)).collect_view()}</ul>
            </section>
        }
        .into_any(),
        Err(reason) => {
            return view! { <p role="alert">{dna_kit::refusal_text(reason)}</p> }.into_any();
        }
    };
    view! {
        <section data-dnacalc="proposal-effects" aria-label="Selected changes">
            <h4>"Writes"</h4>
            <ul>
                {preview.writes.into_iter().map(|write| write_line(session, write)).collect_view()}
                {preview.properties.into_iter().map(|diff| view! {
                    <li>{super::model_property_text(session, diff)}</li>
                }).collect_view()}
            </ul>
            {(!preview.effects.is_empty()).then(|| view! {
                <h4>"Calculated effects"</h4>
                <ul>{preview.effects.into_iter().map(|effect| {
                    write_line(session, calc_skin::ModelWriteDiff {
                        at: effect.at,
                        before: Some(effect.before.value.display()),
                        after: Some(effect.after.value.display()),
                    })
                }).collect_view()}</ul>
            })}
            {preview.refusal.map(|reason| view! {
                <p role="alert">{dna_kit::refusal_text(reason)}</p>
            })}
            {preview.warnings.into_iter().map(|warning| view! { <p>{warning}</p> }).collect_view()}
        </section>
    }
    .into_any()
}
