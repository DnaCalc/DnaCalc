use calc_skin::{MacroReview, MacroRun, MacroSecurity, WorkbookId};
use dna_kit::{Invocation, Session};
use dna_skin_runtime::client::refusal_text;
use leptos::prelude::*;

pub(super) use dna_kit::page::REVIEW_MACRO as COMMAND;

#[derive(Clone, Copy)]
pub(super) struct MacroCard {
    workbook: RwSignal<Option<WorkbookId>>,
    procedure: RwSignal<String>,
    review: RwSignal<Option<MacroReview>>,
    report: RwSignal<Option<MacroRun>>,
    busy: RwSignal<bool>,
    error: RwSignal<Option<String>>,
}

impl MacroCard {
    pub(super) fn install(session: Session) -> Self {
        let card = Self {
            workbook: RwSignal::new(None),
            procedure: RwSignal::new(String::new()),
            review: RwSignal::new(None),
            report: RwSignal::new(None),
            busy: RwSignal::new(false),
            error: RwSignal::new(None),
        };
        session.register(
            super::spec(COMMAND, "Run macro…", "!editing"),
            move |invocation| {
                let procedure: Option<String> = serde_json::from_value(invocation.args.clone())
                    .map_err(|_| "Enter a procedure as Module.Procedure.")?;
                let workbook = session.stamp().ok_or("Open a workbook first.")?.workbook;
                if !card.busy.get_untracked() {
                    card.workbook.set(Some(workbook));
                    card.procedure.set(procedure.clone().unwrap_or_default());
                    card.ask(session, procedure);
                }
                Ok(())
            },
        );
        card
    }

    fn choose_level(self, session: Session, mode: MacroSecurity) {
        if !self.is_open(session) || self.busy.get_untracked() || session.mode().editing() {
            return;
        }
        self.busy.set(true);
        let procedure = self
            .review
            .get_untracked()
            .and_then(|review| review.procedure);
        session.set_macro_security(mode, move |result| {
            self.busy.set(false);
            match result {
                Ok(_) if self.is_open(session) => self.ask(session, procedure),
                Err(error) => self.error.set(Some(refusal_text(error))),
                _ => {}
            }
        });
    }

    fn confirm(self, session: Session) {
        if !self.is_open(session) || self.busy.get_untracked() || session.mode().editing() {
            return;
        }
        let Some(mut review) = self.review.get_untracked() else {
            return;
        };
        if review.procedure.as_deref() != Some(self.procedure.get_untracked().trim()) {
            self.error
                .set(Some("Review the edited procedure first.".into()));
            return;
        }
        let Some(approval) = review.approval.take() else {
            return;
        };
        self.review.set(Some(review));
        self.busy.set(true);
        session.run_macro(approval, move |result| {
            self.busy.set(false);
            match result {
                Ok(run) => self.report.set(Some(run)),
                Err(error) => self.error.set(Some(refusal_text(error))),
            }
        });
    }

    fn is_open(self, session: Session) -> bool {
        self.workbook.get_untracked().is_some_and(|workbook| {
            session
                .stamp()
                .is_some_and(|stamp| stamp.workbook == workbook)
        })
    }

    pub(super) fn close(self) -> bool {
        self.workbook.try_update(Option::take).flatten().is_some()
    }

    fn ask(self, session: Session, procedure: Option<String>) {
        self.review.set(None);
        self.report.set(None);
        self.error.set(None);
        self.busy.set(true);
        session.review_macro(procedure, move |result| {
            self.busy.set(false);
            match result {
                Ok(review) => self.review.set(Some(review)),
                Err(error) => self.error.set(Some(refusal_text(error))),
            }
        });
    }

    pub(super) fn mount(self, session: Session) -> impl IntoView {
        let active = move || {
            self.workbook.get().is_some_and(|workbook| {
                session
                    .stamp()
                    .is_some_and(|stamp| stamp.workbook == workbook)
            })
        };
        view! {
            <Show when=active>
                <section class="dna-book-review" aria-label="Macro review" data-dnacalc="macro-card">
                    <form on:submit=move |event: leptos::ev::SubmitEvent| {
                        event.prevent_default();
                        let procedure = self.procedure.get_untracked().trim().to_owned();
                        if !procedure.is_empty() {
                            let _ = session.run(&Invocation {
                                command: COMMAND.into(),
                                args: procedure.into(),
                            });
                        }
                    }>
                        <label>
                            "Procedure"
                            <input type="text" placeholder="Module.Procedure" data-dna-action-field=""
                                prop:value=move || self.procedure.get()
                                on:input=move |event| self.procedure.set(event_target_value(&event))/>
                        </label>
                        <button type="submit" disabled=move || self.busy.get()>"Review"</button>
                        <button type="button" on:click=move |_| { self.close(); }>"Close"</button>
                    </form>
                    {move || self.review.get().map(|review| {
                        let levels = MacroSecurity::ALL.into_iter().map(|mode| {
                            let available = review.available_modes.contains(&mode);
                            view! {
                                <button type="button" aria-pressed=(mode == review.mode).to_string()
                                    disabled=move || !available || self.busy.get() || session.mode().editing()
                                    on:click=move |_| self.choose_level(session, mode)>{mode.label()}</button>
                            }
                        }).collect_view();
                        let reach = review.reach.into_iter().map(|line| view! { <li>{line}</li> }).collect_view();
                        view! {
                            <div role="group" aria-label="Macro security">{levels}</div>
                            {review.procedure.map(|procedure| view! {
                                <h2>{format!("{} · {procedure}", review.mode.label())}</h2>
                                {review.blocked.map(|reason| view! { <p role="status">{reason}</p> })}
                                <h3>"Reach"</h3>
                                <ul>{reach}</ul>
                                <p>{if review.grants.is_empty() {
                                    "Scoped grants · None".to_owned()
                                } else {
                                    format!("Scoped grants · {}", review.grants.join(", "))
                                }}</p>
                                <p>"Portability verdict unavailable"</p>
                                {review.module_source.map(|source| view! {
                                    <details open><summary>"VBA module source"</summary><pre>{source}</pre></details>
                                })}
                                {review.approval.map(|_| view! {
                                    <button type="button" data-dnacalc="macro-confirm"
                                        disabled=move || self.busy.get() || session.mode().editing()
                                        on:click=move |_| self.confirm(session)>"Run once"</button>
                                })}
                            })}
                        }
                    })}
                    {move || self.error.get().map(|text| view! { <p role="alert">{text}</p> })}
                    {move || self.report.get().map(|report| {
                        let denials = report.denials.into_iter().map(|denial| view! {
                            <li>{format!("Blocked {} · {}", denial.capability, denial.subject)}</li>
                        }).collect_view();
                        view! {
                            <p role="status">{report.error.map_or_else(
                                || format!("Finished {}", report.procedure), refusal_text,
                            )}</p>
                            <ul>{denials}</ul>
                        }
                    })}
                </section>
            </Show>
        }
    }
}
