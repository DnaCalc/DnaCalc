//! Pivot refresh and saved definitions over the worksheet's existing grid presentation.
use crate::{controller::Controller, wire};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

pub fn controls(controller: Controller) -> AnyView {
    let open = RwSignal::new(false);
    let saved = RwSignal::new(Vec::<wire::SavedPivot>::new());
    let issues = RwSignal::new(Vec::<String>::new());
    let ready = RwSignal::new(false);
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        let generation = controller.generation.get();
        let revision = controller.summary.get().revision;
        let sheet = controller.sheet.get();
        ready.set(false);
        saved.set(Vec::new());
        issues.set(Vec::new());
        let Some(sheet) = sheet else {
            return;
        };
        controller.ask(
            wire::Request::PivotTables(wire::Sheet::of(sheet)),
            move |reply| {
                if open.try_get_untracked() != Some(true)
                    || controller.generation.get_untracked() != generation
                    || controller.summary.get_untracked().revision != revision
                    || controller.sheet.get_untracked() != Some(sheet)
                {
                    return;
                }
                if let wire::Reply::PivotTables {
                    tables,
                    issues: messages,
                } = reply
                {
                    saved.set(tables);
                    issues.set(messages);
                    ready.set(true);
                }
            },
        );
    });
    let source = RwSignal::new(None::<(u64, u64, wire::Area)>);
    let source_fields = RwSignal::new(Vec::<String>::new());
    let row_field = RwSignal::new("0".to_owned());
    let value_field = RwSignal::new("1".to_owned());
    let destination = RwSignal::new("D3".to_owned());
    let subtotal = RwSignal::new("sum".to_owned());
    Effect::new(move |_| {
        let generation = controller.generation.get();
        let revision = controller.summary.get().revision;
        if source
            .get_untracked()
            .is_some_and(|(g, r, _)| g != generation || r != revision)
        {
            source.set(None);
            source_fields.set(Vec::new());
        }
    });
    let choose_source = move |_| {
        controller.after_input(move || {
            let areas = controller.areas.get_untracked();
            if areas.len() != 1 {
                controller.say(
                    "Select one rectangular source range with headers and data",
                    true,
                );
                return;
            }
            let area = wire::Area::of(areas[0]);
            let generation = controller.generation.get_untracked();
            let revision = controller.summary.get_untracked().revision;
            source.set(Some((generation, revision, area)));
            source_fields.set(Vec::new());
            controller.ask(
                wire::Request::PivotSourceFields { area, revision },
                move |reply| {
                    if source.get_untracked() != Some((generation, revision, area))
                        || controller.generation.get_untracked() != generation
                        || controller.summary.get_untracked().revision != revision
                    {
                        return;
                    }
                    if let wire::Reply::PivotSourceFields(result) = reply {
                        match result {
                            Ok(fields) => {
                                value_field.set(if fields.len() > 1 { "1" } else { "0" }.into());
                                row_field.set("0".into());
                                source_fields.set(fields);
                            }
                            Err(reason) => {
                                source.set(None);
                                controller.say(&reason, true);
                            }
                        }
                    }
                },
            );
        });
    };
    let create = move |_| {
        let Some((generation, revision, area)) = source.get_untracked() else {
            controller.say("Choose the source selection first", true);
            return;
        };
        let (Ok(row), Ok(value)) = (
            row_field.get_untracked().parse::<u32>(),
            value_field.get_untracked().parse::<u32>(),
        ) else {
            controller.say("Choose the row and value fields", true);
            return;
        };
        let destination = destination.get_untracked();
        let subtotal = subtotal.get_untracked();
        controller.after_input(move || {
            if controller.generation.get_untracked() != generation
                || controller.summary.get_untracked().revision != revision
            {
                controller.say("The source changed; choose its fields again", true);
                return;
            }
            controller.dispatch(
                wire::Intent::PivotCreate {
                    revision,
                    source: area,
                    destination,
                    row,
                    value,
                    subtotal,
                },
                move |outcome| {
                    if outcome.accepted {
                        controller.say("Pivot table created from source data", false);
                    }
                },
            );
        });
    };
    view! {
        <details
            class="dc-clipboard-tools dc-pivot-tools"
            name="worksheet-tools"
            data-dnacalc="pivot-tools"
            on:toggle=move |ev: web_sys::Event| {
                if let Some(node) = ev
                    .target()
                    .and_then(|n| n.dyn_into::<web_sys::HtmlDetailsElement>().ok())
                {
                    open.set(node.open());
                }
            }
            on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                if ev.key() == "Escape" {
                    ev.prevent_default();
                    ev.stop_propagation();
                    if let Some(details) = document()
                        .query_selector("[data-dnacalc=pivot-tools]")
                        .ok()
                        .flatten()
                        .and_then(|e| e.dyn_into::<web_sys::HtmlDetailsElement>().ok())
                    {
                        details.set_open(false);
                    }
                    if let Some(summary) = document()
                        .query_selector("[data-dnacalc=pivot-tools-toggle]")
                        .ok()
                        .flatten()
                        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
                    {
                        let _ = summary.focus();
                    }
                }
            }
        >
            <summary data-dnacalc="pivot-tools-toggle">"Pivot tables"</summary>
            <Show when=move || open.get()>
                <button
                    type="button"
                    data-dnacalc="refresh-all"
                    on:click=move |_| {
                        controller
                            .after_input(move || {
                                let revision = controller.summary.get_untracked().revision;
                                controller
                                    .dispatch(
                                        wire::Intent::RefreshAll {
                                            revision,
                                        },
                                        move |outcome| {
                                            if outcome.accepted {
                                                controller.say("Data model refreshed", false);
                                            }
                                        },
                                    );
                            });
                    }
                >
                    "Refresh All"
                </button>
                <p class="dc-note" data-dnacalc="pivot-state">
                    {move || {
                        if ready.get() {
                            if saved.get().iter().any(|p| p.refreshed_revision.is_some()) {
                                "Pivot refreshed from source data. Later source edits need another refresh."
                            } else {
                                "Saved pivot definitions. No pivot refresh has run; displayed cells use the worksheet's current contents."
                            }
                        } else {
                            "Reading saved pivot definitions…"
                        }
                    }}
                </p>
                <Show when=move || ready.get() && saved.get().is_empty()>
                    <p class="dc-note">"No saved pivot tables on this worksheet."</p>
                </Show>
                <fieldset class="dc-command-fieldset" data-dnacalc="pivot-create">
                    <legend>"Create from selection"</legend>
                    <p class="dc-note" data-dnacalc="pivot-create-range">
                        {move || {
                            source
                                .get()
                                .and_then(|(_, _, a)| a.area())
                                .map(|a| controller.area_label(a))
                                .unwrap_or_else(|| {
                                    "Select headers and data, then choose this selection".into()
                                })
                        }}
                    </p>
                    <button
                        type="button"
                        data-dnacalc="pivot-source-selection"
                        on:click=choose_source
                    >
                        "Use selection as source"
                    </button>
                    <Show when=move || !source_fields.get().is_empty()>
                        <div class="dc-command-row">
                            <label>
                                "Rows "
                                <select
                                    data-dnacalc="pivot-create-row"
                                    prop:value=move || row_field.get()
                                    on:change=move |ev| row_field.set(event_target_value(&ev))
                                >
                                    <For
                                        each={move || {
                                            source_fields
                                                .get()
                                                .into_iter()
                                                .enumerate()
                                                .collect::<Vec<_>>()
                                        }}
                                        key=|(i, _)| *i
                                        children={move |(i, name)| {
                                            view! {
                                                <option
                                                    value=i.to_string()
                                                    prop:selected=move || row_field.get() == i.to_string()
                                                >
                                                    {name}
                                                </option>
                                            }
                                        }}
                                    />
                                </select>
                            </label>
                            <label>
                                "Values "
                                <select
                                    data-dnacalc="pivot-create-value"
                                    prop:value=move || value_field.get()
                                    on:change=move |ev| value_field.set(event_target_value(&ev))
                                >
                                    <For
                                        each={move || {
                                            source_fields
                                                .get()
                                                .into_iter()
                                                .enumerate()
                                                .collect::<Vec<_>>()
                                        }}
                                        key=|(i, _)| *i
                                        children={move |(i, name)| {
                                            view! {
                                                <option
                                                    value=i.to_string()
                                                    prop:selected=move || value_field.get() == i.to_string()
                                                >
                                                    {name}
                                                </option>
                                            }
                                        }}
                                    />
                                </select>
                            </label>
                        </div>
                        <div class="dc-command-row">
                            <label>
                                "Summarize values "
                                <select
                                    data-dnacalc="pivot-create-subtotal"
                                    prop:value=move || subtotal.get()
                                    on:change=move |ev| subtotal.set(event_target_value(&ev))
                                >
                                    <option value="sum">"Sum"</option>
                                    <option value="count">"Count"</option>
                                    <option value="countNums">"Count numbers"</option>
                                    <option value="average">"Average"</option>
                                    <option value="min">"Minimum"</option>
                                    <option value="max">"Maximum"</option>
                                </select>
                            </label>
                        </div>
                        <div class="dc-command-row">
                            <label>
                                "Destination cell "
                                <input
                                    data-dnacalc="pivot-create-destination"
                                    type="text"
                                    prop:value=move || destination.get()
                                    on:input=move |ev| destination.set(event_target_value(&ev))
                                />
                            </label>
                        </div>
                        <p class="dc-note">
                            "Creates one row field and one value field. The destination must have space "
                            "for labels, values and the grand total. Excel layout and formatting remain unmeasured."
                        </p>
                        <button type="button" data-dnacalc="pivot-create-apply" on:click=create>
                            "Create pivot table"
                        </button>
                    </Show>
                </fieldset>
                <For
                    each={move || saved.get()}
                    key=|p| p.part.clone()
                    children={move |pivot| {
                        let area = pivot.area;
                        let generation = controller.generation.get_untracked();
                        let revision = controller.summary.get_untracked().revision;
                        let sheet = controller.sheet.get_untracked();
                        let label = pivot.location.clone();
                        let name = pivot.name.clone();
                        let refresh_part = pivot.part.clone();
                        view! {
                            <section
                                class="dc-pivot-summary"
                                data-dnacalc="saved-pivot"
                                data-pivot=name
                            >
                                <h3>{pivot.name}" · "{label}</h3>
                                <p data-dnacalc="pivot-source">{pivot.source}</p>
                                <p data-dnacalc="pivot-fields">{pivot.fields}</p>
                                <p class="dc-note">{pivot.cache}</p>
                                <p class="dc-note">
                                    "Refresh supports one row field and one value field. Layout and formatting have not "
                                    "been measured against Excel; multi-field, column and page layouts are not implemented."
                                </p>
                                <button
                                    type="button"
                                    data-dnacalc="pivot-refresh"
                                    on:click=move |_| {
                                        let refresh_part = refresh_part.clone();
                                        controller
                                            .after_input(move || {
                                                if controller.generation.get_untracked() != generation
                                                    || controller.summary.get_untracked().revision != revision
                                                    || controller.sheet.get_untracked() != sheet
                                                {
                                                    return;
                                                }
                                                if let Some(sheet) = sheet {
                                                    controller
                                                        .dispatch(
                                                            wire::Intent::PivotRefresh {
                                                                sheet: wire::Sheet::of(sheet),
                                                                revision,
                                                                part: refresh_part.clone(),
                                                            },
                                                            move |outcome| {
                                                                if outcome.accepted {
                                                                    controller.say("Pivot refreshed from source data", false);
                                                                }
                                                            },
                                                        );
                                                }
                                            });
                                    }
                                >
                                    "Refresh from source"
                                </button>
                                <button
                                    type="button"
                                    data-dnacalc="pivot-reveal"
                                    on:click=move |_| {
                                        if controller.generation.get_untracked() != generation
                                            || controller.summary.get_untracked().revision != revision
                                            || controller.sheet.get_untracked() != sheet
                                        {
                                            return;
                                        }
                                        if let Some(area) = area.area() {
                                            controller
                                                .navigate_to(
                                                    area.first(),
                                                    move |arrived| {
                                                        if arrived
                                                            && controller.generation.get_untracked() == generation
                                                            && controller.sheet.get_untracked() == sheet
                                                        {
                                                            controller.select_areas(area.first(), vec![area]);
                                                        }
                                                    },
                                                );
                                        }
                                    }
                                >
                                    "Show saved range"
                                </button>
                            </section>
                        }
                    }}
                />
                <For
                    each={move || issues.get()}
                    key=|s| s.clone()
                    children={move |message| {
                        view! {
                            <p class="dc-note" data-dnacalc="pivot-reading-issue">
                                {message}
                            </p>
                        }
                    }}
                />
            </Show>
        </details>
    }.into_any()
}
