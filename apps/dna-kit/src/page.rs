//! Shared renderer for code-free workbook pages. Every part owns its typed watches.

use crate::commands::{CommandId, Invocation};
use crate::{Live, Session};
use calc_skin::grid::Key;
use calc_skin::query;
use calc_skin::{
    Bound, Coverage, DocId, Page, PageNode, PartialWhy, Provenance, RawValue, RowWindow, SourceRef,
    InventoryItem, InventoryKind, RowPage, WorkbookCell,
};
use leptos::prelude::*;

mod reader;
mod report;
pub use report::{PageReport, ReportCosts, costs as report_costs};

#[derive(Clone, Copy)]
pub struct PageInput {
    pub command: &'static str,
    pub draft_command: &'static str,
    pub draft: fn(Session, &Bound) -> Option<String>,
    pub focus: fn(Session, Option<Bound>),
    pub error: fn(Session, &Bound, Key) -> Option<String>,
}

/// Selection actions for controls in the authoring surface.
#[derive(Clone, Copy)]
pub struct PageAuthor {
    pub document: DocId,
    pub select_command: &'static str,
    pub selected: fn(Session, DocId, &str) -> bool,
}

#[derive(Clone, Copy)]
pub struct PageScenario {
    pub review: &'static str,
    pub save: &'static str,
}

#[derive(Clone, Copy)]
struct PageMode {
    input: Option<PageInput>,
    author: Option<PageAuthor>,
    scenario: Option<PageScenario>,
    compare: bool,
    report: Option<PageReport>,
}

/// The same page can be previewed with read-only controls by omitting the input actions.
#[component]
pub fn PageRenderer(
    session: Session,
    document: Page,
    #[prop(optional)] input: Option<PageInput>,
    #[prop(optional)] author: Option<PageAuthor>,
    #[prop(optional)] scenario: Option<PageScenario>,
    #[prop(optional)] compare: bool,
    #[prop(optional)] report: Option<PageReport>,
) -> impl IntoView {
    let mode = PageMode { input, author, scenario, compare, report };
    let schedules = report.map(|report| report::schedules(session, &document, report));
    view! {
        <article class="dna-page" data-dnacalc="page">
            <h1>{document.title}</h1>
            {node(session, document.root, mode)}
            {schedules}
        </article>
    }
}

fn children(session: Session, items: Vec<PageNode>, mode: PageMode) -> Vec<AnyView> {
    items
        .into_iter()
        .map(|item| node(session, item, mode))
        .collect()
}

fn node(session: Session, item: PageNode, mode: PageMode) -> AnyView {
    match item {
        PageNode::Stack { children: items } => view! {
            <div class="dna-page-stack">{children(session, items, mode)}</div>
        }.into_any(),
        PageNode::Row { children: items } => view! {
            <div class="dna-page-row">{children(session, items, mode)}</div>
        }.into_any(),
        PageNode::Grid { columns, children: items } => view! {
            <div class="dna-page-grid" data-columns=columns.get()>{children(session, items, mode)}</div>
        }.into_any(),
        PageNode::Section { title, folded, children: items } => view! {
            <details class="dna-page-section" open={mode.report.is_some() || !folded}>
                <summary>{title}</summary>
                {children(session, items, mode)}
            </details>
        }.into_any(),
        PageNode::Tabs { tabs } => {
            let selected = RwSignal::new(0);
            tabs.into_iter().enumerate().map(|(index, tab)| view! {
                <button type="button" aria-expanded=move || (selected.get() == index).to_string()
                    on:click=move |_| selected.set(index)>{tab.label}</button>
                <div hidden=move || selected.get() != index>{node(session, tab.body, mode)}</div>
            }).collect_view().into_any()
        }
        PageNode::Part { id, component, mut bind, props } => {
            if mode.report.is_some() && matches!(component.as_str(), "scenario-picker" | "data-frame") {
                return ().into_any();
            }
            let label = props.get("label").and_then(|value| value.as_str()).unwrap_or(&id).to_owned();
            let body = match component.as_str() {
                "scenario-picker" => scenarios(session, label.clone(), mode.scenario),
                "rich-prose" => {
                    let text = props.get("text").and_then(|value| value.as_str()).unwrap_or_default();
                    view! {
                        <section class="dna-page-card" data-page-part=id.clone()>
                            {reader::prose(session, text, &bind)}
                        </section>
                    }.into_any()
                }
                "value" | "input" | "data-frame" | "notebook-excerpt" | "chart" => {
                    let slot = match component.as_str() {
                        "chart" => "object",
                        "value" | "input" => "value",
                        _ => "rows",
                    };
                    match bind.remove(slot) {
                        Some(bound) => {
                            let summary = if component == "data-frame" { bind.remove("summary") } else { None };
                            part(session, id.clone(), label.clone(), component, bound, summary, mode)
                        }
                        None => missing(),
                    }
                }
                _ => view! { <p class="dna-page-card">{label.clone()} " · Component unavailable"</p> }.into_any(),
            };
            if let Some(author) = mode.author {
                author_control(session, author, id, label, body)
            } else {
                body
            }
        }
    }
}

fn scenarios(session: Session, label: String, actions: Option<PageScenario>) -> AnyView {
    let records = session.watch(query::Inventory { kinds: vec![InventoryKind::Scenarios] });
    view! {
        <section class="dna-page-card">
            <h2>{label}</h2>
            {actions.map(|actions| {
                let name = NodeRef::<leptos::html::Input>::new();
                view! {
                    <form class="dna-page-tools" on:submit=move |event: leptos::ev::SubmitEvent| {
                        event.prevent_default();
                        if let Some(name) = name.get() {
                            let _ = session.run(&Invocation {
                                command: CommandId::new(actions.save),
                                args: serde_json::Value::String(name.value()),
                            });
                        }
                    }>
                        <label>"Scenario name " <input node_ref=name required/></label>
                        <button type="submit" data-command=actions.save>"Review and save…"</button>
                    </form>
                }
            })}
            {move || match session.read(records) {
                Live::Ready(answer) if answer.coverage == Coverage::Complete => {
                    let items = answer.body.into_iter().filter_map(|item| {
                        if let InventoryItem::Scenario(record) = item { Some(record) } else { None }
                    }).collect::<Vec<_>>();
                    if items.is_empty() {
                        return view! { <p>"No saved Excel scenarios."</p> }.into_any();
                    }
                    let workbook = session.summary();
                    view! {
                        <ul>
                            {items.into_iter().map(|record| {
                                let sheet = workbook.as_ref().and_then(|book| {
                                    book.sheets.iter().find(|sheet| sheet.sheet == record.sheet)
                                });
                                let label = sheet.map_or_else(
                                    || record.name.clone(),
                                    |sheet| format!("{} · {}", record.name, sheet.name),
                                );
                                let args = serde_json::to_value(&record).expect("scenario identity");
                                view! {
                                    <li>
                                        {actions.map(|actions| actions.review).map_or_else(
                                            || label.clone().into_any(),
                                            |command| view! {
                                                <button type="button" data-command=command on:click=move |_| {
                                                    let _ = session.run(&Invocation {
                                                        command: CommandId::new(command), args: args.clone(),
                                                    });
                                                }>{label.clone()}</button>
                                            }.into_any(),
                                        )}
                                    </li>
                                }
                            }).collect_view()}
                        </ul>
                    }.into_any()
                }
                Live::Ready(_) | Live::Refused(_) => view! { <p>"Excel scenarios are unavailable."</p> }.into_any(),
                Live::Loading => view! { <p aria-busy="true">"Reading Excel scenarios…"</p> }.into_any(),
                _ => ().into_any(),
            }}
        </section>
    }.into_any()
}

fn author_control(
    session: Session,
    author: PageAuthor,
    id: String,
    label: String,
    body: AnyView,
) -> AnyView {
    let args = id.clone();
    let selected = Memo::new(move |_| (author.selected)(session, author.document, &id));
    view! {
        <div class="dna-page-control" class:dna-page-control-selected=move || selected.get()>
            <button type="button" data-command=author.select_command
                aria-pressed=move || selected.get().to_string()
                on:click=move |_| {
                    let _ = session.run(&Invocation {
                        command: CommandId::new(author.select_command),
                        args: serde_json::Value::String(args.clone()),
                    });
                }>{format!("Select {label}")}</button>
            {body}
        </div>
    }
    .into_any()
}

fn missing() -> AnyView {
    view! { <span class="dna-page-missing">"Source missing"</span> }.into_any()
}

fn resolve_bound(session: Session, bound: Bound) -> Memo<Live<SourceRef>> {
    let source = session.following::<query::ResolveBound>();
    let query = query::ResolveBound { bound };
    Memo::new(move |_| match source.at(Some(query.clone())) {
        Live::Ready(answer) if answer.coverage == Coverage::Complete => Live::Ready(answer.body),
        Live::Ready(_) => Live::Refused(calc_skin::Refusal::Invalid {
            reason: "Source missing".into(),
        }),
        Live::Refused(reason) => Live::Refused(reason),
        Live::Loading | Live::Idle | Live::Outdated => Live::Loading,
    })
}

fn part(
    session: Session,
    id: String,
    label: String,
    component: String,
    bound: Bound,
    summary: Option<Bound>,
    mode: PageMode,
) -> AnyView {
    let resolved = resolve_bound(session, bound.clone());
    let contents = move || match resolved.get() {
        Live::Ready(source) => {
            if component == "chart" {
                chart_image(session, source, mode.compare.then(|| bound.clone()))
            } else if matches!(component.as_str(), "data-frame" | "notebook-excerpt") {
                schedule(session, source, summary.clone())
            } else {
                scalar(
                    session,
                    source,
                    label.clone(),
                    bound.clone(),
                    if component == "input" { mode.input } else { None },
                    mode.compare && component == "value",
                )
            }
        }
        Live::Refused(_) => missing(),
        _ => view! { <p aria-busy="true">"Reading source…"</p> }.into_any(),
    };
    view! { <section class="dna-page-card" data-page-part=id>{contents}</section> }.into_any()
}

fn chart_image(session: Session, object: SourceRef, comparison: Option<Bound>) -> AnyView {
    let watch = session.following::<query::ChartImageOf>();
    let query = query::ChartImageOf { object, comparison };
    (move || match watch.at(Some(query.clone())) {
        Live::Ready(answer) if answer.coverage == Coverage::Complete => view! {
            <figure class="dna-page-chart-frame">
                <img class="dna-page-chart" src=answer.body.data_url alt=answer.body.label/>
                {answer.body.uses_saved_values.then(|| view! { <figcaption>"Uses saved chart data"</figcaption> })}
                {answer.body.compares_baseline.then(|| view! { <figcaption>"Dotted baseline · Solid current"</figcaption> })}
            </figure>
        }.into_any(),
        Live::Ready(_) | Live::Refused(_) => missing(),
        _ => view! { <p aria-busy="true">"Reading chart…"</p> }.into_any(),
    }).into_any()
}

fn comparison(session: Session, bound: Bound) -> impl IntoView {
    let values = session.following::<query::NumericComparisonOf>();
    let query = query::NumericComparisonOf { bound };
    move || {
        let Live::Ready(answer) = values.at(Some(query.clone())) else { return None; };
        let reading = answer.body?;
        let baseline = f64::from_bits(reading.baseline);
        let delta = f64::from_bits(reading.current) - baseline;
        if !delta.is_finite() {
            return None;
        }
        let relative = (baseline != 0.0).then(|| delta / baseline.abs() * 100.0);
        let relative = relative.filter(|value| value.is_finite())
            .map(|percent| format!(" ({percent:+.2}%)")).unwrap_or_default();
        Some(view! {
            <small data-dnacalc="app-baseline-delta">{format!("{delta:+}{relative} vs baseline")}</small>
        })
    }
}

fn single_cell(source: &SourceRef) -> Option<Key> {
    if let SourceRef::Cells { areas, .. } = source
        && let [area] = areas.as_slice()
        && area.0 == area.1
    {
        return Some(area.0);
    }
    None
}

fn first_cell(session: Session, source: SourceRef) -> Memo<Option<WorkbookCell>> {
    let rows = session.following::<query::Rows>();
    Memo::new(move |_| {
        match rows.at(Some(query::Rows {
            source: source.clone(),
            filter: Vec::new(),
            sort: Vec::new(),
            window: RowWindow {
                offset: 0,
                limit: 1,
            },
        })) {
            Live::Ready(answer) if answer.coverage == Coverage::Complete => answer
                .body
                .rows
                .into_iter()
                .flat_map(|row| row.cells)
                .next(),
            _ => None,
        }
    })
}

fn shown_value(cell: WorkbookCell, label: Option<String>) -> impl IntoView {
    let error = matches!(cell.shown.value, RawValue::Error(_));
    let status = match cell.shown.provenance {
        Provenance::AsSaved => Some("Saved value"),
        Provenance::Stale { .. } => Some("Previous calculation"),
        Provenance::Refused { .. } => Some("Calculation refused"),
        _ => None,
    };
    view! {
        <output class:dna-page-error=error aria-label=label>{cell.text}</output>
        {status.map(|status| view! { <small>{status}</small> })}
    }
}

fn scalar(
    session: Session,
    source: SourceRef,
    label: String,
    bound: Bound,
    input: Option<PageInput>,
    compare: bool,
) -> AnyView {
    let Some(at) = single_cell(&source) else {
        return missing();
    };
    let read = first_cell(session, source);
    let Some(input) = input else {
        return view! {
            <div class="dna-page-value">
                <strong>{label}</strong>
                {move || read.get().map(|cell| shown_value(cell, None))}
                {compare.then(|| comparison(session, bound))}
            </div>
        }
        .into_any();
    };
    let rules = session.following::<query::Rules>();
    let can_write = move || {
        matches!(rules.at(Some(query::Rules { at })),
            Live::Ready(answer) if matches!(answer.coverage, Coverage::Complete) && answer.body.refusal.is_none())
            && read.get().is_some_and(|cell| !cell.input.starts_with('='))
    };
    let bound = StoredValue::new(bound);
    let focused = RwSignal::new(false);
    view! {
        <form on:submit=move |event: leptos::ev::SubmitEvent| {
            event.prevent_default();
            let _ = session.run(&Invocation {
                command: CommandId::new(input.command),
                args: serde_json::to_value(bound.get_value()).expect("a binding serializes"),
            });
        }>
            <label>
                <strong>{label}</strong>
                <input data-dna-action-field="" disabled=move || !can_write()
                    prop:value=move || (input.draft)(session, &bound.get_value())
                        .or_else(|| read.get().map(|cell| cell.input)).unwrap_or_default()
                    on:focus=move |_| {
                        focused.set(true);
                        (input.focus)(session, Some(bound.get_value()));
                    }
                    on:blur=move |_| {
                        focused.set(false);
                        (input.focus)(session, None);
                    }
                    on:input=move |event| {
                        let _ = session.run(&Invocation {
                            command: CommandId::new(input.draft_command),
                            args: serde_json::to_value((bound.get_value(), event_target_value(&event)))
                                .expect("the input draft serializes"),
                        });
                    }/>
            </label>
            <Show when=move || focused.get()>
                <small>{move || format!("Model change · {}", bound.get_value().address)}</small>
            </Show>
            {move || (input.error)(session, &bound.get_value(), at).map(|error| view! {
                <p class="dna-page-error" role="alert">{error}</p>
            })}
        </form>
    }.into_any()
}

fn schedule(session: Session, source: SourceRef, summary: Option<Bound>) -> AnyView {
    const PAGE: u32 = 12;
    let rows = session.following::<query::Rows>();
    let offset = RwSignal::new(0u32);
    let grouped = RwSignal::new(false);
    let summary = summary.map(|bound| resolve_bound(session, bound));
    view! {
        <div class="dna-page-schedule">
        {summary.map(|summary| view! {
            <button type="button" aria-pressed=move || grouped.get().to_string()
                disabled=move || !matches!(summary.get(), Live::Ready(_))
                on:click=move |_| {
                    offset.set(0);
                    grouped.update(|grouped| *grouped = !*grouped);
                }>"Group by Year (display rollup)"</button>
        })}
        {move || match rows.at(Some(query::Rows {
            source: if grouped.get() {
                let Some(Live::Ready(summary)) = summary.map(|summary| summary.get()) else {
                    return missing();
                };
                summary
            } else {
                source.clone()
            },
            filter: Vec::new(),
            sort: Vec::new(),
            window: RowWindow { offset: offset.get(), limit: PAGE },
        })) {
            Live::Ready(answer) if matches!(answer.coverage, Coverage::Complete
                | Coverage::Partial { why: PartialWhy::Window, .. }) => {
                let page = answer.body;
                let start = page.offset;
                let end = start + page.rows.len() as u32;
                let total = page.matched_rows;
                let caption = format!("Rows {}–{end} of {total}", start + 1);
                view! {
                    {rows_table(page, caption)}
                    <button type="button" disabled={start == 0}
                        on:click=move |_| offset.set(start.saturating_sub(PAGE))>"Previous rows"</button>
                    <button type="button" disabled={end >= total}
                        on:click=move |_| offset.set(end)>"Next rows"</button>
                }.into_any()
            }
            Live::Ready(_) | Live::Refused(_) => view! { <p>"Rows unavailable"</p> }.into_any(),
            _ => view! { <p aria-busy="true">"Reading rows…"</p> }.into_any(),
        }}
        </div>
    }.into_any()
}

fn rows_table(page: RowPage, caption: String) -> impl IntoView {
    let headings = page.headers.map(|labels| view! {
        <thead><tr>{labels.into_iter().map(|label| view! { <th scope="col">{label}</th> }).collect_view()}</tr></thead>
    });
    view! {
        <table>
            <caption>{caption}</caption>
            {headings}
            <tbody>{page.rows.into_iter().map(|row| view! {
                <tr>{row.cells.into_iter().map(|cell| view! { <td>{cell.text}</td> }).collect_view()}</tr>
            }).collect_view()}</tbody>
        </table>
    }
}
