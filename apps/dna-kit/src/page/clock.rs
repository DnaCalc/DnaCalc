//! Workbook-bound controls for the shared RTD operating panel.

use super::{first_cell, missing, resolve_bound, shown_value, single_cell};
use crate::commands::CommandId;
use crate::{Invocation, Live, Seam, Session, ViewCommand};
use calc_skin::{Batch, Bound, Change, Coverage, DocId, ModelChange, Page, WorkbookCell};
use leptos::prelude::*;
use std::collections::BTreeMap;

const COMMAND: &str = "clock.control";

/// Registers the workbook clock commands once for the entire shell.
pub fn command(session: Session) -> ViewCommand {
    ViewCommand::global(
        COMMAND,
        "Operate the workbook clock",
        &[],
        move |invocation| {
            let (document, part, action, value): (DocId, String, String, Option<String>) =
                serde_json::from_value(invocation.args.clone())
                    .map_err(|_| "Choose a workbook clock action".to_owned())?;
            operate(session, document, &part, &action, value)
        },
    )
    .when("writable")
    .when("!editing")
}

fn operate(
    session: Session,
    document: DocId,
    part: &str,
    action: &str,
    value: Option<String>,
) -> Result<(), String> {
    if !matches!(session.seam(), Seam::Idle) {
        return Err("Finish the change in progress first".into());
    }
    let stamp = session.stamp().ok_or("Open a workbook first")?;
    let Live::Ready(view) = session.view_part() else {
        return Err("Read the workbook's clock bindings first".into());
    };
    if view.coverage != Coverage::Complete
        || view.body.opaque
        || view.stamp.workbook != stamp.workbook
        || view.stamp.view != stamp.view
    {
        return Err("The workbook's clock bindings are unavailable".into());
    }
    let document = view
        .body
        .documents
        .into_iter()
        .find(|entry| entry.id == document && entry.schema == 1)
        .ok_or("The workbook clock page is unavailable")?;
    let page: Page = serde_json::from_value(document.body).map_err(|error| error.to_string())?;
    let mut bindings = None;
    page.root.for_each_part(&mut |id, component, slots| {
        if id == part && component == "clock" {
            bindings = Some(slots.clone());
        }
    });
    let bindings = bindings.ok_or("The workbook clock is unavailable")?;
    let bound = |slot: &str| {
        bindings
            .get(slot)
            .cloned()
            .ok_or_else(|| format!("The workbook clock needs its {slot} binding"))
    };
    let enter = |slot, text| {
        Ok::<_, String>(Change::Model(ModelChange::EnterBound {
            bound: bound(slot)?,
            text,
        }))
    };
    let count = |slot| {
        Ok::<_, String>(Change::Model(ModelChange::IncrementBound {
            bound: bound(slot)?,
        }))
    };
    let (label, changes) = match action {
        "Run" => ("Run workbook clock", vec![enter("running", "TRUE".into())?]),
        "Pause" => (
            "Pause workbook clock",
            vec![enter("running", "FALSE".into())?],
        ),
        "Step" => (
            "Step workbook clock",
            vec![enter("running", "FALSE".into())?, count("step")?],
        ),
        "Reset" => (
            "Reset workbook clock",
            vec![enter("running", "FALSE".into())?, count("reset")?],
        ),
        "Rate" => {
            let value = value
                .filter(|value| !value.trim().is_empty())
                .ok_or("Enter a clock rate")?;
            ("Set workbook clock rate", vec![enter("rate", value)?])
        }
        _ => return Err("Choose a workbook clock action".into()),
    };
    session.submit(Batch {
        id: String::new(),
        workbook: Some(stamp.workbook),
        base: Some(stamp.model),
        expects: Vec::new(),
        selection_seq: None,
        label: Some(label.into()),
        changes,
    });
    Ok(())
}

/// Show the clock controls authored in workbook pages alongside source observations.
pub fn authored_controls(session: Session) -> impl IntoView {
    let declarations = Memo::new(move |_| {
        let Live::Ready(answer) = session.view_part() else {
            return Vec::new();
        };
        if answer.coverage != Coverage::Complete || answer.body.opaque {
            return Vec::new();
        }
        let mut controls = Vec::new();
        for document in answer
            .body
            .documents
            .into_iter()
            .filter(|doc| doc.schema == 1)
        {
            let Ok(page) = serde_json::from_value::<Page>(document.body) else {
                continue;
            };
            page.root.for_each_part(&mut |id, component, bindings| {
                if component == "clock" {
                    controls.push((
                        document.id,
                        id.to_owned(),
                        page.title.clone(),
                        bindings.clone(),
                    ));
                }
            });
        }
        controls
    });
    move || {
        declarations
            .get()
            .into_iter()
            .map(|(document, id, title, bindings)| {
                clock_controls(session, id, title, &bindings, Some(document))
            })
            .collect_view()
    }
}

/// Render a workbook clock for operators, or show the readings alone in a page preview.
pub fn clock_controls(
    session: Session,
    id: String,
    label: String,
    bindings: &BTreeMap<String, Bound>,
    document: Option<DocId>,
) -> impl IntoView + use<> {
    let tick = bindings
        .get("tick")
        .map(|bound| display(session, bound.clone(), "Tick").into_any());
    let stop = bindings
        .get("stopped")
        .map(|bound| display(session, bound.clone(), "Stop condition").into_any());
    let rate = bindings.get("rate").cloned();
    let part = StoredValue::new(id.clone());
    view! {
        <section class="dna-page-card" data-page-part=id data-dnacalc="clock-controls">
            <h2>{label}</h2>
            <div class="dna-page-tools">{tick}{stop}</div>
            {document.zip(rate).map(|(document, rate)| {
                let input = NodeRef::<leptos::html::Input>::new();
                let source = resolve_bound(session, rate);
                let run = move |action: &'static str, value: Option<String>| {
                    let _ = session.run(&Invocation {
                        command: CommandId::new(COMMAND),
                        args: serde_json::to_value((document, part.get_value(), action, value))
                            .expect("a clock action serializes"),
                    });
                };
                let action = move |name: &'static str| view! {
                    <button type="button" data-clock-action=name data-command=COMMAND
                        on:click=move |_| run(name, None)>{name}</button>
                };
                view! {
                    <div class="dna-page-tools">
                        {action("Run")}{action("Pause")}{action("Step")}{action("Reset")}
                        <form on:submit=move |event: leptos::ev::SubmitEvent| {
                            event.prevent_default();
                            if let Some(input) = input.get() {
                                run("Rate", Some(input.value()));
                            }
                        }>
                            {move || match source.get() {
                                Live::Ready(source) if single_cell(&source).is_some() => {
                                    let read = first_cell(session, source);
                                    view! {
                                        <label>"Rate (ticks per second)"
                                            <input node_ref=input data-dna-action-field=""
                                                disabled=move || read.get().is_none()
                                                prop:value=move || read.get().map(|cell| cell.input).unwrap_or_default() />
                                        </label>
                                        <button type="submit">"Set rate"</button>
                                    }.into_any()
                                }
                                Live::Ready(_) | Live::Refused(_) => missing(),
                                _ => view! { <span aria-busy="true">"Reading rate…"</span> }.into_any(),
                            }}
                        </form>
                    </div>
                }
            })}
        </section>
    }
}

fn display(session: Session, bound: Bound, label: &'static str) -> impl IntoView {
    let source = resolve_bound(session, bound);
    view! {
        <div class="dna-page-value">
            <strong>{label}</strong>
            {move || match source.get() {
                Live::Ready(source) if single_cell(&source).is_some() => {
                    let read = first_cell(session, source);
                    (move || read.get().map(|cell: WorkbookCell| shown_value(cell, Some(label.into()))))
                        .into_any()
                }
                Live::Ready(_) | Live::Refused(_) => missing(),
                _ => view! { <span aria-busy="true">"Reading clock…"</span> }.into_any(),
            }}
        </div>
    }
}
