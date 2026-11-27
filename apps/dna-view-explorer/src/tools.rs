//! The Formula Explorer's file and test-suite instruments.

use calc_skin::{
    FormulaAnswer, FormulaPort, FormulaQuery, ProviderRequirement, TestCase, TestOutcome,
    TestSuite, TestVerdict, Trial,
};
use dna_formula_ui::web::{download_bytes, read_chosen};
use leptos::prelude::*;
use std::rc::Rc;

pub(crate) type Port = StoredValue<Rc<dyn FormulaPort>, LocalStorage>;

const MAX_WORKBOOK_BYTES: usize = 20 * 1024 * 1024;
const MAX_SUITE_BYTES: usize = 1024 * 1024;

#[component]
pub(crate) fn Files(
    port: Port,
    draft: Callback<(), Trial>,
    open: Callback<Trial>,
) -> impl IntoView {
    let message = RwSignal::new(String::new());
    let export = move |_| {
        message.set("Preparing workbook…".into());
        port.with_value(|port| {
            port.ask(
                FormulaQuery::ExportWorkbook(draft.run(())),
                Box::new(move |answer| {
                    let outcome = match answer {
                        Ok(FormulaAnswer::File(bytes)) => download_bytes(
                            "formula-explorer.xlsx",
                            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                            &bytes,
                        )
                        .map(|()| "Workbook exported".to_owned()),
                        Ok(_) => Err("Unexpected workbook reply".into()),
                        Err(refusal) => Err(super::refusal_reason(&refusal)),
                    };
                    show_message(message, outcome);
                }),
            )
        });
    };
    let import = move |event: leptos::ev::Event| {
        read_chosen(
            event_target::<web_sys::HtmlInputElement>(&event),
            Some(MAX_WORKBOOK_BYTES),
            move |file| match file {
                Ok((name, bytes)) => port.with_value(|port| {
                    port.ask(
                        FormulaQuery::OpenWorkbook(bytes),
                        Box::new(move |answer| match answer {
                            Ok(FormulaAnswer::Opened(trial)) => {
                                open.run(trial);
                                let _ = message.try_set(format!("Opened {name}"));
                            }
                            Ok(_) => {
                                let _ = message.try_set("Unexpected workbook reply".into());
                            }
                            Err(refusal) => {
                                let _ = message.try_set(super::refusal_reason(&refusal));
                            }
                        }),
                    )
                }),
                Err(error) => {
                    let _ = message.try_set(error);
                }
            },
        );
        event_target::<web_sys::HtmlInputElement>(&event).set_value("");
    };
    view! {
        <section class="dna-fe-section" aria-labelledby="dna-fe-files-heading">
            <h2 id="dna-fe-files-heading">"Workbook file"</h2>
            <div class="dna-fe-actions">
                <button type="button" data-skin0="export-workbook" on:click=export>"Export as .xlsx"</button>
                <button type="button" data-skin0="new-draft" on:click=move |_| {
                    open.run(Trial::default());
                    message.set("New scratchpad".into());
                }>"New scratchpad"</button>
            </div>
            <label>"Open Explorer .xlsx"<input type="file" data-skin0="open-workbook" accept=".xlsx" on:change=import/></label>
            <p aria-live="polite">{move || message.get()}</p>
        </section>
    }
}

#[component]
pub(crate) fn Suite(
    port: Port,
    draft: Callback<(), Trial>,
    open: Callback<Trial>,
    provider_requirements: Signal<Vec<ProviderRequirement>>,
) -> impl IntoView {
    let cases = RwSignal::new(Vec::<TestCase>::new());
    let requirements = RwSignal::new(Vec::<ProviderRequirement>::new());
    let message = RwSignal::new(String::new());
    let outcomes = RwSignal::new(Vec::<TestOutcome>::new());
    let edit_version = RwSignal::new(0_u64);
    let edited = move || {
        edit_version.update(|version| *version += 1);
        outcomes.set(Vec::new());
        message.set(String::new());
    };
    let snapshot = move || TestSuite {
        schema: TestSuite::SCHEMA.into(),
        cases: cases.get_untracked(),
        providers: requirements.get_untracked(),
    };
    let save = move |_| {
        let outcome = serde_json::to_vec_pretty(&snapshot())
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                download_bytes("formula-explorer.dnafe.json", "application/json", &bytes)
            })
            .map(|()| "Test suite saved".to_owned());
        show_message(message, outcome);
    };
    let load = move |event: leptos::ev::Event| {
        let input = event_target::<web_sys::HtmlInputElement>(&event);
        read_chosen(input.clone(), Some(MAX_SUITE_BYTES), move |file| {
            let suite = file.and_then(|(name, bytes)| {
                let suite: TestSuite =
                    serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
                if suite.schema != TestSuite::SCHEMA {
                    return Err("Unsupported test suite schema".into());
                }
                Ok((name, suite))
            });
            match suite {
                Ok((name, suite)) => {
                    edited();
                    let _ = cases.try_set(suite.cases);
                    let _ = requirements.try_set(suite.providers);
                    let _ = message.try_set(format!("Opened {name}"));
                }
                Err(error) => {
                    let _ = message.try_set(error);
                }
            }
        });
        input.set_value("");
    };
    let run = move |_| {
        edited();
        let version = edit_version.get_untracked();
        message.set("Running test suite…".into());
        port.with_value(|port| {
            port.ask(
                FormulaQuery::RunSuite(snapshot()),
                Box::new(move |answer| {
                    if edit_version.try_get_untracked() != Some(version) {
                        return;
                    }
                    match answer {
                        Ok(FormulaAnswer::TestResults(results)) => {
                            let matched = results
                                .iter()
                                .filter(|row| row.verdict == TestVerdict::Match)
                                .count();
                            let _ =
                                message.try_set(format!("{matched} of {} matched", results.len()));
                            let _ = outcomes.try_set(results);
                        }
                        Err(refusal) => {
                            let _ = message.try_set(super::refusal_reason(&refusal));
                        }
                        _ => {
                            let _ = message.try_set("Unexpected test suite reply".into());
                        }
                    }
                }),
            )
        });
    };
    let row = move |index: usize| {
        view! {
                <li class="dna-fe-test-case">
                    <code>{move || cases.with(|cases| cases.get(index).map(|case| case.trial.formula.clone()))}</code>
                    <div class="dna-fe-input-row">
                        <label>"Expected Excel constant"
                            <input data-skin0="expected" prop:value=move || cases.with(|cases| cases.get(index).map_or_else(String::new, |case| case.expected.clone()))
                                on:input=move |event| { cases.update(|cases| cases[index].expected = event_target_value(&event)); edited(); }/>
                        </label>
                        <label>"Expected formatted text"
                            <input placeholder="Optional" prop:value=move || cases.with(|cases| cases.get(index).and_then(|case| case.formatted.clone()).unwrap_or_default())
                                on:input=move |event| {
                                    let text = event_target_value(&event);
                                    cases.update(|cases| cases[index].formatted = (!text.is_empty()).then_some(text)); edited();
                                }/>
                        </label>
                    </div>
                    <div class="dna-fe-actions">
                        <button type="button" on:click=move |_| open.run(cases.with_untracked(|cases| cases[index].trial.clone()))>"Use formula"</button>
                        <button type="button" on:click=move |_| { cases.update(|cases| { cases.remove(index); }); edited(); }>"Remove case"</button>
                        {move || outcomes.with(|outcomes| outcomes.get(index).map(describe_case))}
                    </div>
                </li>
        }
    };
    view! {
        <section class="dna-fe-section" aria-labelledby="dna-fe-suite-heading">
            <h2 id="dna-fe-suite-heading">"Test suite"</h2>
            <div class="dna-fe-actions">
                <button type="button" data-skin0="add-case" on:click=move |_| {
                    cases.update(|cases| cases.push(TestCase { trial: draft.run(()), expected: String::new(), formatted: None }));
                    edited();
                }>"Add current formula"</button>
                <button type="button" disabled=move || cases.with(Vec::is_empty) data-skin0="run-suite" on:click=run>"Run suite"</button>
                <button type="button" disabled=move || cases.with(Vec::is_empty) data-skin0="save-suite" on:click=save>"Save test suite"</button>
                <button type="button" disabled=move || provider_requirements.with(Vec::is_empty) && requirements.with(Vec::is_empty)
                    on:click=move |_| { requirements.set(provider_requirements.get_untracked()); edited(); }>"Use loaded provider versions"</button>
            </div>
            <label>"Open .dnafe.json"<input type="file" data-skin0="open-suite" accept=".json" on:change=load/></label>
            <p aria-live="polite" data-skin0="suite-status">{move || message.get()}</p>
            <ol><For each=move || 0..cases.with(Vec::len) key=|index| *index children=row/></ol>
        </section>
    }
}

fn describe_case(outcome: &TestOutcome) -> String {
    let verdict = match &outcome.verdict {
        TestVerdict::Match => "Match",
        TestVerdict::Mismatch => "Mismatch",
        TestVerdict::Refused(refusal) => {
            return format!("Refused · {}", super::refusal_reason(refusal));
        }
    };
    format!(
        "{verdict} · {}",
        outcome.actual.as_deref().unwrap_or("No scalar display")
    )
}

fn show_message(message: RwSignal<String>, outcome: Result<String, String>) {
    let _ = message.try_set(outcome.unwrap_or_else(|error| error));
}
