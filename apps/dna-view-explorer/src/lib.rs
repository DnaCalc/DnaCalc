//! Skin zero, the formula workbench. It receives only the formula port.

mod providers;
mod tools;

use calc_formula_kernel::assist;
use calc_skin::{
    ArrayPosition, CalcGeneration, ConditionalRule, FormulaAnswer, FormulaLink, FormulaQuery,
    Provenance, ProviderCatalog, ProviderRequirement, Refusal, Shown, TestInput, Trial, TrialValue,
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
        let formula = RwSignal::new(context.draft.formula);
        let culture = RwSignal::new(context.draft.culture);
        let format = RwSignal::new(context.draft.number_format);
        let result = RwSignal::new(RequestState::<FormulaAnswer>::default());
        let steps = RwSignal::new(RequestState::<FormulaAnswer>::default());
        let status = context.status;
        let clock = context.clock;
        let port = StoredValue::new_local(context.port);
        let caller = RwSignal::new(context.draft.caller);
        let pinned = RwSignal::new(None::<(CalcGeneration, ArrayPosition)>);
        let provider_catalogue = RwSignal::new(ProviderCatalog::default());
        let conditional = RwSignal::new(None::<ConditionalRule>);
        let rule_type = RwSignal::new("greater".to_owned());
        let rule_operand = RwSignal::new(String::new());
        let empty_input = || TestInput {
            name: String::new(),
            constant: String::new(),
        };
        let rows = move |inputs: Vec<TestInput>| {
            if inputs.is_empty() {
                vec![empty_input()]
            } else {
                inputs
            }
        };
        let inputs = RwSignal::new(rows(context.draft.inputs));
        let edited = move || {
            pinned.set(None);
            conditional.set(None);
            result.update(RequestState::edited);
            steps.update(RequestState::edited);
            status.set("Ready");
        };
        let text_binding = TextBinding::new(formula.into(), move |text| {
            formula.set(text);
            edited();
        });
        let assist = FormulaAssist::new(text_binding);
        let snapshot = move || {
            let caller = caller.get_untracked();
            let now = clock().map(|time| {
                if caller.as_ref().is_some_and(|caller| caller.date_1904) {
                    time - 1462.0
                } else {
                    time
                }
            });
            Trial {
                formula: formula.get_untracked(),
                culture: culture.get_untracked(),
                number_format: format.get_untracked(),
                now,
                inputs: inputs.get_untracked(),
                caller,
            }
        };
        if let Some(save) = context.persist {
            Effect::new(move |_| {
                save(Trial {
                    formula: formula.get(),
                    culture: culture.get(),
                    number_format: format.get(),
                    inputs: inputs.get(),
                    now: None,
                    caller: None,
                })
            });
        }
        let draft = Callback::new(move |()| snapshot());
        let open = Callback::new(move |trial: Trial| {
            formula.set(trial.formula);
            culture.set(trial.culture);
            format.set(trial.number_format);
            inputs.set(rows(trial.inputs));
            caller.set(trial.caller);
            edited();
        });
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
                        let coordinates = answer.as_ref().ok().and_then(current_result);
                        let current = state.try_update(|state| state.finish(id, answer));
                        if current == Some(true) {
                            if let Some(coordinates) = coordinates {
                                let _ = pinned.try_set(Some(coordinates));
                            }
                            let _ = status.try_set(outcome);
                        }
                    }),
                )
            });
        };
        let page_query = move |calc, position| {
            let query = match conditional.get_untracked() {
                Some(rule) => FormulaQuery::ConditionalFormat {
                    calc,
                    position,
                    rule,
                },
                None => FormulaQuery::ArrayWindow { calc, position },
            };
            request(query, result);
        };
        let submit = move |trace: bool| {
            if trace {
                request(FormulaQuery::Trace(snapshot()), steps);
            } else {
                pinned.set(None);
                conditional.set(None);
                request(FormulaQuery::Evaluate(snapshot()), result);
            }
        };
        let page = Callback::new(move |position| {
            if let Some((calc, _)) = pinned.get_untracked() {
                page_query(calc, position);
            }
        });
        let apply_rule = move |_| {
            if let Some((calc, position)) = pinned.get_untracked() {
                let operand = rule_operand.get_untracked();
                let rule = match rule_type.get_untracked().as_str() {
                    "less" => ConditionalRule::LessThan(operand),
                    "equal" => ConditionalRule::EqualTo(operand),
                    "formula" => ConditionalRule::Expression(operand),
                    _ => ConditionalRule::GreaterThan(operand),
                };
                conditional.set(Some(rule));
                page_query(calc, position);
            }
        };
        let undo_rule = move |_| {
            conditional.set(None);
            if let Some((calc, position)) = pinned.get_untracked() {
                page_query(calc, position);
            }
        };
        let provider_requirements = Signal::derive(move || {
            provider_catalogue.with(|info| {
                info.libraries
                    .iter()
                    .map(|library| ProviderRequirement {
                        name: library
                            .path
                            .rsplit(['/', '\\'])
                            .next()
                            .unwrap_or(&library.path)
                            .to_owned(),
                        sha256: library.sha256.clone(),
                    })
                    .collect()
            })
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
                functions: provider_catalogue.with(|info| {
                    info.functions
                        .iter()
                        .map(|function| function.name.clone())
                        .collect()
                }),
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
        if context.auto_preview && !formula.get_untracked().is_empty() {
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
        let registered = move || {
            let needle = catalogue_filter.get().to_ascii_lowercase();
            provider_catalogue.get().functions.into_iter().filter(|function| function.name.to_ascii_lowercase().contains(&needle)).map(|function| {
                view! { <li><code>{function.signature}</code> <small>{function.origin}</small><p>{function.description}</p></li> }
            }).collect_view()
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
                        <fieldset class="dna-fe-rule">
                            <legend>"Conditional formatting"</legend>
                            <div class="dna-fe-input-row">
                                <label>"Rule"
                                    <select data-skin0="rule" prop:value=rule_type on:change=move |event| rule_type.set(event_target_value(&event))>
                                        <option value="greater">"Greater than"</option>
                                        <option value="less">"Less than"</option>
                                        <option value="equal">"Equal to"</option>
                                        <option value="formula">"Formula"</option>
                                    </select>
                                </label>
                                <label>{move || if rule_type.get() == "formula" { "Formula for the first result cell (A1)" } else { "Value or formula" }}
                                    <input data-skin0="rule-operand" prop:value=rule_operand on:input=move |event| rule_operand.set(event_target_value(&event))/>
                                </label>
                            </div>
                            <div class="dna-fe-actions">
                                <button type="button" data-skin0="apply-rule" disabled=move || pinned.with(Option::is_none) || rule_operand.with(|text| text.trim().is_empty()) on:click=apply_rule>"Apply rule"</button>
                                <button type="button" data-skin0="undo-rule" disabled=move || conditional.with(Option::is_none) on:click=undo_rule>"Undo rule"</button>
                            </div>
                        </fieldset>
                    </section>
                    <tools::Files port draft open/>
                    <tools::Suite port draft open provider_requirements/>
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
                        <ul class="dna-fe-catalogue">{registered}{catalogue}</ul>
                        {assist.help()}
                    </section>
                    <providers::Providers port catalogue=provider_catalogue changed=Callback::new(move |()| edited())/>
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
        Live::Ready(FormulaAnswer::Conditional { result, notes }) => view! {
            <ValueView shown=result.clone() page/>
            <p>{notes.join(" · ")}</p>
        }
        .into_any(),
        Live::Ready(_) => ().into_any(),
    }
}

fn current_result(answer: &FormulaAnswer) -> Option<(CalcGeneration, ArrayPosition)> {
    let shown: &Shown<TrialValue> = match answer {
        FormulaAnswer::Evaluated(shown) | FormulaAnswer::Conditional { result: shown, .. } => shown,
        _ => return None,
    };
    let Provenance::Calculated { calc } = shown.provenance else {
        return None;
    };
    let position = match shown.value {
        TrialValue::Array { position, .. } => position,
        TrialValue::Scalar(_) => ArrayPosition::default(),
    };
    Some((calc, position))
}

fn refusal_reason(refusal: &Refusal) -> String {
    match refusal {
        Refusal::Invalid { reason }
        | Refusal::Transport(reason)
        | Refusal::RequiresWorkbook { reason, .. } => reason.clone(),
        Refusal::NotAvailableHere {
            capability: calc_skin::Capability::NativeAddins,
            ..
        } => "Native add-ins require the Windows desktop app".into(),
        Refusal::NotModelled { owner, what } => format!("{what} ({owner})"),
        _ => format!("{refusal:?}"),
    }
}
