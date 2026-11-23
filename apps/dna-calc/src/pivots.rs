//! Pivot refresh and saved definitions over the worksheet's existing grid presentation.
use crate::workbook_command::{WorkbookCommand, projected_area};
use crate::{controller::Controller, wire};
use dna_calc_model::Target;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

fn source_field_select(
    name: &'static str,
    selector: &'static str,
    fields: RwSignal<Vec<String>>,
    selected: RwSignal<String>,
    optional: bool,
) -> impl IntoView {
    view! {
        <label>
            {name}
            <select
                data-dnacalc=selector
                prop:value=move || selected.get()
                on:change=move |ev| selected.set(event_target_value(&ev))
            >
                {optional.then(|| view! {
                    <option value="" prop:selected=move || selected.get().is_empty()>"None"</option>
                })}
                <For
                    each={move || fields.get().into_iter().enumerate().collect::<Vec<_>>()}
                    key=|(index, name)| (*index, name.clone())
                    children={move |(index, name)| view! {
                        <option value=index.to_string()
                            prop:selected=move || selected.get() == index.to_string()>
                            {name}
                        </option>
                    }}
                />
            </select>
        </label>
    }
}

fn move_field(axis: RwSignal<Vec<i32>>, field: i32, offset: isize) {
    axis.update(|fields| {
        if let Some(index) = fields.iter().position(|&item| item == field)
            && let Some(other) = index.checked_add_signed(offset)
            && other < fields.len()
        {
            fields.swap(index, other);
        }
    });
}

fn pivot_axis(
    name: &'static str,
    add_selector: &'static str,
    axis: RwSignal<Vec<i32>>,
    other: RwSignal<Vec<i32>>,
    fields: StoredValue<Vec<(i32, String)>>,
) -> impl IntoView {
    view! {
        <fieldset>
            <legend>{name}</legend>
            <ol>
                <For
                    each=move || axis.get()
                    key=|field| *field
                    children=move |field| {
                        let label = fields.with_value(|fields| {
                            fields.iter().find(|(id, _)| *id == field)
                                .map(|(_, name)| name.clone()).unwrap_or_else(|| format!("Field {field}"))
                        });
                        let position = move || axis.with(|fields| fields.iter().position(|&id| id == field));
                        view! {
                            <li>
                                {label}
                                <button type="button"
                                    disabled=move || position() == Some(0)
                                    on:click=move |_| move_field(axis, field, -1)>
                                    "Move up"
                                </button>
                                <button type="button"
                                    disabled=move || position().is_some_and(|index| index + 1 == axis.with(Vec::len))
                                    on:click=move |_| move_field(axis, field, 1)>
                                    "Move down"
                                </button>
                                <button type="button" on:click=move |_| axis.update(|fields| {
                                    fields.retain(|&id| id != field);
                                })>
                                    "Remove"
                                </button>
                            </li>
                        }
                    }
                />
            </ol>
            <label>
                "Add field "
                <select data-dnacalc=add_selector on:change=move |ev| {
                    let Ok(field) = event_target_value(&ev).parse::<i32>() else { return };
                    if !axis.with_untracked(|ids| ids.contains(&field))
                        && !other.with_untracked(|ids| ids.contains(&field))
                    {
                        axis.update(|ids| ids.push(field));
                    }
                }>
                    <option value="">"Choose a field"</option>
                    <For
                        each=move || {
                            let occupied = [axis.get(), other.get()];
                            fields.with_value(|fields| {
                                fields.iter()
                                    .filter(|(field, _)| !occupied.iter().any(|ids| ids.contains(field)))
                                    .cloned().collect::<Vec<_>>()
                            })
                        }
                        key=|(field, _)| *field
                        children={move |(field, label)| view! { <option value=field.to_string()>{label}</option> }}
                    />
                </select>
            </label>
        </fieldset>
    }
}

fn pivot_layout_editor(
    controller: Controller,
    sheet: calc_value::SheetId,
    revision: u64,
    generation: u64,
    part: String,
    layout: wire::SavedPivotLayout,
) -> AnyView {
    let mut fields: Vec<_> = layout
        .source_fields
        .into_iter()
        .enumerate()
        .map(|(index, name)| (index as i32, name))
        .collect();
    let values = wire::SavedPivotLayout::VALUES;
    let has_values = layout.rows.contains(&values) || layout.columns.contains(&values);
    if has_values {
        fields.push((values, "Values".into()));
    }
    let fields = StoredValue::new(fields);
    let original_rows = StoredValue::new(layout.rows.clone());
    let original_columns = StoredValue::new(layout.columns.clone());
    let rows = RwSignal::new(layout.rows);
    let columns = RwSignal::new(layout.columns);
    let apply = move |_| {
        let rows = rows.get_untracked();
        let columns = columns.get_untracked();
        let part = part.clone();
        controller.after_input(move || {
            if controller.generation.get_untracked() != generation
                || controller.summary.get_untracked().revision != revision
                || controller.sheet.get_untracked() != Some(sheet)
            {
                controller.say("The workbook changed; review the pivot layout again", true);
                return;
            }
            controller.dispatch(
                WorkbookCommand::PivotSetLayout {
                    sheet: Target::Sheet(sheet),
                    revision,
                    part,
                    rows,
                    columns,
                },
                move |outcome| {
                    if outcome.accepted {
                        controller.say("Pivot layout updated", false);
                    }
                },
            );
        });
    };
    view! {
        <details data-dnacalc="pivot-layout">
            <summary>"Edit field layout"</summary>
            {pivot_axis("Rows", "pivot-row-add", rows, columns, fields)}
            {pivot_axis("Columns", "pivot-column-add", columns, rows, fields)}
            <button
                type="button"
                data-dnacalc="pivot-layout-apply"
                disabled=move || {
                    let rows = rows.get();
                    let columns = columns.get();
                    (has_values && !rows.contains(&values) && !columns.contains(&values))
                        || original_rows.with_value(|original| *original == rows)
                            && original_columns.with_value(|original| *original == columns)
                }
                on:click=apply
            >
                "Apply layout"
            </button>
        </details>
    }
    .into_any()
}

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
    let column_field = RwSignal::new(String::new());
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
                                column_field.set(String::new());
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
        let columns = column_field
            .get_untracked()
            .parse::<u32>()
            .ok()
            .into_iter()
            .collect();
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
                WorkbookCommand::PivotCreateLayout {
                    revision,
                    source: projected_area(area),
                    destination,
                    rows: vec![row],
                    columns,
                    values: vec![(value, subtotal)],
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
                                        WorkbookCommand::RefreshAll {
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
                            {source_field_select("Rows ", "pivot-create-row", source_fields, row_field, false)}
                            {source_field_select("Columns ", "pivot-create-column", source_fields, column_field, true)}
                            {source_field_select("Values ", "pivot-create-value", source_fields, value_field, false)}
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
                        <button type="button" data-dnacalc="pivot-create-apply" on:click=create>
                            "Create pivot table"
                        </button>
                    </Show>
                </fieldset>
                <For
                    each={move || saved.get()}
                    key=move |p| (
                        p.part.clone(),
                        controller.generation.get_untracked(),
                        controller.summary.get_untracked().revision,
                    )
                    children={move |pivot| {
                        let area = pivot.area;
                        let generation = controller.generation.get_untracked();
                        let revision = controller.summary.get_untracked().revision;
                        let sheet = controller.sheet.get_untracked();
                        let label = pivot.location.clone();
                        let name = pivot.name.clone();
                        let refresh_part = pivot.part.clone();
                        let layout = sheet.zip(pivot.layout).map(|(sheet, layout)| {
                            pivot_layout_editor(controller, sheet, revision, generation, pivot.part, layout)
                        });
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
                                {layout}
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
                                                            WorkbookCommand::PivotRefresh {
                                                                sheet: Target::Sheet(sheet),
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
