//! Skin zero, the formula workbench. It receives only the formula port.

use calc_formula_kernel::assist;
use calc_skin::{
    ArrayPosition, FormulaAnswer, FormulaLink, FormulaQuery, Provenance, Refusal, TestInput, Trial,
};
use dna_formula_ui::formula_field::{FormulaAssist, TextBinding};
use dna_kit::{FormulaContext, FormulaView, Registry, ValueView};
use dna_skin_runtime::{Live, RequestState};
use leptos::prelude::*;

pub const ID: &str = "skin-0";

pub fn install(registry: &mut Registry) {
    registry.formula::<Explorer>();
}

struct Explorer;

impl FormulaView for Explorer {
    fn mount(context: FormulaContext) -> AnyView {
        let formula = RwSignal::new(context.initial_formula);
        let culture = RwSignal::new("en-US".to_owned());
        let format = RwSignal::new("General".to_owned());
        let result = RwSignal::new(RequestState::<FormulaAnswer>::default());
        let steps = RwSignal::new(RequestState::<FormulaAnswer>::default());
        let status = context.status;
        let clock = context.clock;
        let port = StoredValue::new_local(context.port);
        let caller = StoredValue::new(context.caller);
        let empty_input = || TestInput {
            name: String::new(),
            constant: String::new(),
        };
        let inputs = RwSignal::new(vec![empty_input()]);
        let edited = move || {
            result.update(RequestState::edited);
            steps.update(RequestState::edited);
            status.set("Ready");
        };
        let text_binding = TextBinding::new(formula.into(), move |text| {
            formula.set(text);
            edited();
        });
        let assist = FormulaAssist::new(text_binding);
        let snapshot = move || Trial {
            formula: formula.get_untracked(),
            culture: culture.get_untracked(),
            number_format: format.get_untracked(),
            now: clock(),
            inputs: inputs.get_untracked(),
            caller: caller.get_value(),
        };
        let request = move |query: FormulaQuery, state: RwSignal<RequestState<FormulaAnswer>>| {
            let id = state
                .try_update(RequestState::begin)
                .expect("mounted request state");
            status.set("Calculating");
            port.with_value(|port| {
                port.ask(
                    query,
                    Box::new(move |answer| {
                        let outcome = if answer.is_ok() {
                            "Ready"
                        } else {
                            "Needs attention"
                        };
                        let current = state.try_update(|state| state.finish(id, answer));
                        if current == Some(true) {
                            let _ = status.try_set(outcome);
                        }
                    }),
                )
            });
        };
        let submit = move |trace: bool| {
            if trace {
                request(FormulaQuery::Trace(snapshot()), steps);
            } else {
                request(FormulaQuery::Evaluate(snapshot()), result);
            }
        };
        let page = Callback::new(move |position| {
            let calc = result.with_untracked(|result| match result.live() {
                Live::Ready(FormulaAnswer::Evaluated(shown)) => match shown.provenance {
                    Provenance::Calculated { calc } => Some(calc),
                    _ => None,
                },
                _ => None,
            });
            if let Some(calc) = calc {
                request(FormulaQuery::ArrayWindow { calc, position }, result);
            }
        });
        Effect::new(move |_| {
            let names = inputs.with(|rows| {
                rows.iter()
                    .map(|row| row.name.trim().to_owned())
                    .filter(|name| !name.is_empty())
                    .collect()
            });
            assist.scope().set(assist::Scope {
                names,
                ..Default::default()
            });
        });
        assist.enable_part_evaluation(move |start, len, done| {
            let trial = snapshot();
            port.with_value(|port| {
                port.ask(
                    FormulaQuery::EvaluatePart { trial, start, len },
                    Box::new(move |response| {
                        done(match response {
                            Ok(FormulaAnswer::Part(part)) => Ok(part.literal),
                            Err(refusal) => Err(refusal_reason(&refusal)),
                            _ => Err("Unexpected formula reply".into()),
                        });
                    }),
                )
            });
        });
        if !formula.get_untracked().is_empty() {
            request(FormulaQuery::Preview(snapshot()), result);
        }
        let keys = Callback::new(move |event: leptos::ev::KeyboardEvent| {
            if event.ctrl_key() && event.key() == "Enter" {
                event.prevent_default();
                submit(false);
            }
        });
        let catalogue_filter = RwSignal::new(String::new());
        let catalogue = move || {
            dna_formula_ui::editor::browse(&catalogue_filter.get())
                .into_iter()
                .take(40)
                .map(|entry| {
                    let name = entry.spelling();
                    view! {
                        <li><button type="button" on:click=move |_| assist.insert_function(name)>{name}</button></li>
                    }
                })
                .collect_view()
        };
        view! {
            <div class="dna-fe-workbench">
                <section class="dna-fe-main">
                    <h1>"Formula workbench"</h1>
                    <div class="dna-fe-editor">
                        {assist.field(Signal::derive(Vec::new), Some(keys), 5, "=SUM(1,2,3)")}
                    </div>
                    <div class="dna-fe-actions">
                        <button type="button" class="dna-fe-primary" data-skin0="evaluate"
                                on:click=move |_| submit(false)>"Evaluate"</button>
                        <kbd>"Ctrl+Enter"</kbd>
                        <a href=move || FormulaLink(formula.get()).fragment()>"Formula link"</a>
                    </div>
                    <p aria-live="polite">{move || assist.part_status().get()}</p>
                    <section class="dna-fe-section" aria-labelledby="dna-fe-inputs-heading">
                        <h2 id="dna-fe-inputs-heading">"Local test inputs"</h2>
                        <For each=move || 0..inputs.with(Vec::len) key=|index| *index children=move |index| {
                            view! {
                                <div class="dna-fe-input-row">
                                    <label>"Name"
                                        <input placeholder="TaxRate" prop:value=move || inputs.with(|rows| rows[index].name.clone())
                                            on:input=move |event| {
                                                inputs.update(|rows| rows[index].name = event_target_value(&event));
                                                edited();
                                            } />
                                    </label>
                                    <label>"Excel constant"
                                        <input placeholder="0.065" prop:value=move || inputs.with(|rows| rows[index].constant.clone())
                                            on:input=move |event| {
                                                inputs.update(|rows| rows[index].constant = event_target_value(&event));
                                                edited();
                                            } />
                                    </label>
                                </div>
                            }
                        }/>
                        <button type="button" on:click=move |_| inputs.update(|rows| rows.push(empty_input()))>"Add input"</button>
                    </section>
                    <section class="dna-fe-section" aria-labelledby="dna-fe-result-heading">
                        <h2 id="dna-fe-result-heading">"Result"</h2>
                        <div class="dna-fe-format">
                            <label>"Locale"
                                <select prop:value=move || culture.get()
                                        on:change=move |event| { culture.set(event_target_value(&event)); edited(); }>
                                    <option value="en-US">"English (US)"</option>
                                    <option value="en-ZA">"English (South Africa)"</option>
                                    <option value="fr-FR">"French (France)"</option>
                                    <option value="de-DE">"German (Germany)"</option>
                                </select>
                            </label>
                            <label>"Number format"
                                <input prop:value=move || format.get()
                                       on:input=move |event| { format.set(event_target_value(&event)); edited(); } />
                            </label>
                        </div>
                        <div aria-live="polite">{move || answer_view(result.get().live(), assist, page)}</div>
                    </section>
                </section>
                <aside class="dna-fe-side">
                    <section class="dna-fe-section" aria-labelledby="dna-fe-steps-heading">
                        <h2 id="dna-fe-steps-heading">"Steps"</h2>
                        <button type="button" on:click=move |_| submit(true)>"Evaluate steps"</button>
                        <div aria-live="polite">{move || answer_view(steps.get().live(), assist, page)}</div>
                    </section>
                    <section class="dna-fe-section" aria-labelledby="dna-fe-functions-heading">
                        <h2 id="dna-fe-functions-heading">"Functions"</h2>
                        <input type="search" aria-label="Search Excel functions" placeholder="Search functions"
                               on:input=move |event| catalogue_filter.set(event_target_value(&event)) />
                        <ul class="dna-fe-catalogue">{catalogue}</ul>
                        {assist.help()}
                    </section>
                </aside>
            </div>
        }.into_any()
    }
}

fn answer_view(
    answer: &Live<FormulaAnswer>,
    assist: FormulaAssist,
    page: Callback<ArrayPosition>,
) -> AnyView {
    match answer {
        Live::Idle => ().into_any(),
        Live::Loading => view! { <p>"Calculating…"</p> }.into_any(),
        Live::Outdated => view! { <p>"Evaluate to refresh."</p> }.into_any(),
        Live::Refused(Refusal::RequiresWorkbook { reason, route }) => {
            let href = format!("https://dnacalc.com/app/{}", route.fragment());
            view! {
                <div class="dna-fe-error" data-skin0="workbook-required">
                    <p>{reason.clone()}</p>
                    <a href=href>"Open in DNA Calc"</a>
                </div>
            }
            .into_any()
        }
        Live::Refused(refusal) => {
            let reason = refusal_reason(refusal);
            view! { <p class="dna-fe-error" data-skin0="refused">{reason}</p> }.into_any()
        }
        Live::Ready(FormulaAnswer::Evaluated(shown)) => view! {
            <ValueView shown=shown.clone() page/>
        }
        .into_any(),
        Live::Ready(FormulaAnswer::Steps(trace)) => {
            let note = trace.note.clone();
            let steps = trace
                .steps
                .iter()
                .map(|step| {
                    let text = step.text.clone();
                    let value = step.value.clone();
                    let span = calc_formula_kernel::TextSpan {
                        start: step.start,
                        len: step.len,
                    };
                    view! {
                        <li><button type="button" on:click=move |_| assist.focus().set(Some(span))>
                            <code>{text}</code>
                            <span>{value}</span>
                        </button></li>
                    }
                })
                .collect_view();
            view! { <ol class="dna-fe-steps">{steps}</ol><p>{note}</p> }.into_any()
        }
        Live::Ready(FormulaAnswer::Part(_)) => ().into_any(),
    }
}

fn refusal_reason(refusal: &Refusal) -> String {
    match refusal {
        Refusal::Invalid { reason }
        | Refusal::Transport(reason)
        | Refusal::RequiresWorkbook { reason, .. } => reason.clone(),
        Refusal::NotModelled { owner, what } => format!("{what} ({owner})"),
        _ => format!("{refusal:?}"),
    }
}
