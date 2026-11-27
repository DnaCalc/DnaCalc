//! Shared renderer for code-free workbook pages. Every part owns its typed watches.

use crate::commands::{CommandId, Invocation};
use crate::{Live, Session};
use calc_skin::grid::Key;
use calc_skin::query;
use calc_skin::{Bound, Coverage, Page, PageNode, Provenance, RawValue, RowWindow, SourceRef};
use leptos::prelude::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub struct PageInput {
    pub command: &'static str,
    pub draft_command: &'static str,
    pub draft: fn(Session, &Bound) -> Option<String>,
    pub focus: fn(Session, Option<Bound>),
    pub error: fn(Session, &Bound, Key) -> Option<String>,
}

/// The same page can be previewed with read-only controls by omitting the input actions.
#[component]
pub fn PageRenderer(
    session: Session,
    document: Page,
    #[prop(optional)] input: Option<PageInput>,
) -> impl IntoView {
    view! {
        <article class="dna-page" data-dnacalc="page">
            <h1>{document.title}</h1>
            {node(session, document.root, input)}
        </article>
    }
}

fn children(session: Session, items: Vec<PageNode>, input: Option<PageInput>) -> Vec<AnyView> {
    items
        .into_iter()
        .map(|item| node(session, item, input))
        .collect()
}

fn node(session: Session, item: PageNode, input: Option<PageInput>) -> AnyView {
    match item {
        PageNode::Stack { children: items } => view! {
            <div class="dna-page-stack">{children(session, items, input)}</div>
        }.into_any(),
        PageNode::Row { children: items } => view! {
            <div class="dna-page-row">{children(session, items, input)}</div>
        }.into_any(),
        PageNode::Grid { columns, children: items } => view! {
            <div class="dna-page-grid" data-columns=columns.get()>{children(session, items, input)}</div>
        }.into_any(),
        PageNode::Section { title, folded, children: items } => view! {
            <details class="dna-page-section" open=!folded>
                <summary>{title}</summary>
                {children(session, items, input)}
            </details>
        }.into_any(),
        PageNode::Tabs { tabs } => {
            let selected = RwSignal::new(0);
            tabs.into_iter().enumerate().map(|(index, tab)| view! {
                <button type="button" aria-expanded=move || (selected.get() == index).to_string()
                    on:click=move |_| selected.set(index)>{tab.label}</button>
                <div hidden=move || selected.get() != index>{node(session, tab.body, input)}</div>
            }).collect_view().into_any()
        }
        PageNode::Part { id, component, mut bind, props } => {
            let label = props.get("label").and_then(|value| value.as_str()).unwrap_or(&id).to_owned();
            let slot = match component.as_str() {
                "value" | "input" => "value",
                "data-frame" => "rows",
                _ => return view! { <p class="dna-page-card">{label} " · Component unavailable"</p> }.into_any(),
            };
            let Some(bound) = bind.remove(slot) else { return missing() };
            part(session, id, label, component, bound, input)
        }
    }
}

fn missing() -> AnyView {
    view! { <p class="dna-page-missing">"Source missing"</p> }.into_any()
}

fn part(
    session: Session,
    id: String,
    label: String,
    component: String,
    bound: Bound,
    input: Option<PageInput>,
) -> AnyView {
    let source = session.following::<query::ResolveBound>();
    let query = query::ResolveBound {
        bound: bound.clone(),
    };
    let resolved = Memo::new(move |_| match source.at(Some(query.clone())) {
        Live::Ready(answer) if matches!(answer.coverage, Coverage::Complete) => {
            Live::Ready(answer.body)
        }
        Live::Ready(_) => Live::Refused(calc_skin::Refusal::Invalid {
            reason: "Source missing".into(),
        }),
        Live::Refused(reason) => Live::Refused(reason),
        Live::Loading | Live::Idle | Live::Outdated => Live::Loading,
    });
    let contents = move || match resolved.get() {
        Live::Ready(source) => {
            if component == "data-frame" {
                schedule(session, source)
            } else {
                scalar(
                    session,
                    source,
                    label.clone(),
                    bound.clone(),
                    if component == "input" { input } else { None },
                )
            }
        }
        Live::Refused(_) => missing(),
        _ => view! { <p aria-busy="true">"Reading source…"</p> }.into_any(),
    };
    view! { <section class="dna-page-card" data-page-part=id>{contents}</section> }.into_any()
}

fn scalar(
    session: Session,
    source: SourceRef,
    label: String,
    bound: Bound,
    input: Option<PageInput>,
) -> AnyView {
    let SourceRef::Cells { areas, .. } = &source else {
        return missing();
    };
    let [area] = areas.as_slice() else {
        return missing();
    };
    if area.0 != area.1 {
        return missing();
    }
    let at = area.0;
    let rows = session.following::<query::Rows>();
    let read = move || match rows.at(Some(query::Rows {
        source: source.clone(),
        filter: Vec::new(),
        sort: Vec::new(),
        window: RowWindow {
            offset: 0,
            limit: 1,
        },
    })) {
        Live::Ready(answer) if matches!(answer.coverage, Coverage::Complete) => answer
            .body
            .rows
            .into_iter()
            .flat_map(|row| row.cells)
            .next(),
        _ => None,
    };
    let read = Memo::new(move |_| read());
    let Some(input) = input else {
        return view! {
            <div class="dna-page-value">
                <strong>{label}</strong>
                {move || read.get().map(|cell| {
                    let error = matches!(cell.shown.value, RawValue::Error(_));
                    let status = match cell.shown.provenance {
                        Provenance::AsSaved => Some("Saved value"),
                        Provenance::Stale { .. } => Some("Previous calculation"),
                        Provenance::Refused { .. } => Some("Calculation refused"),
                        _ => None,
                    };
                    view! { <output class:dna-page-error=error>{cell.text}</output><small>{status}</small> }
                })}
            </div>
        }.into_any();
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

fn schedule(session: Session, source: SourceRef) -> AnyView {
    const PAGE: u32 = 12;
    let rows = session.following::<query::Rows>();
    let offset = RwSignal::new(0u32);
    view! {
        <div class="dna-page-schedule">
        {move || match rows.at(Some(query::Rows {
            source: source.clone(),
            filter: Vec::new(),
            sort: Vec::new(),
            window: RowWindow { offset: offset.get(), limit: PAGE },
        })) {
            Live::Ready(answer) if matches!(answer.coverage, Coverage::Complete) => {
                let page = answer.body;
                let end = page.offset + page.rows.len() as u32;
                let headings = page.headers.map(|labels| view! {
                    <thead><tr>{labels.into_iter().map(|label| view! { <th scope="col">{label}</th> }).collect_view()}</tr></thead>
                });
                view! {
                    <table>
                        <caption>{format!("Rows {}–{end} of {}", page.offset + 1, page.matched_rows)}</caption>
                        {headings}
                        <tbody>{page.rows.into_iter().map(|row| view! {
                            <tr>{row.cells.into_iter().map(|cell| view! { <td>{cell.text}</td> }).collect_view()}</tr>
                        }).collect_view()}</tbody>
                    </table>
                    <button type="button" disabled=page.offset == 0
                        on:click=move |_| offset.set(page.offset.saturating_sub(PAGE))>"Previous rows"</button>
                    <button type="button" disabled=end >= page.matched_rows
                        on:click=move |_| offset.set(end)>"Next rows"</button>
                }.into_any()
            }
            Live::Ready(_) | Live::Refused(_) => view! { <p>"Rows unavailable"</p> }.into_any(),
            _ => view! { <p aria-busy="true">"Reading rows…"</p> }.into_any(),
        }}
        </div>
    }.into_any()
}

/// Visit page parts in authored reading order.
pub fn visit_parts(node: &PageNode, visit: &mut impl FnMut(&str, &str, &BTreeMap<String, Bound>)) {
    match node {
        PageNode::Stack { children }
        | PageNode::Row { children }
        | PageNode::Grid { children, .. }
        | PageNode::Section { children, .. } => {
            children.iter().for_each(|item| visit_parts(item, visit))
        }
        PageNode::Tabs { tabs } => tabs.iter().for_each(|tab| visit_parts(&tab.body, visit)),
        PageNode::Part {
            id,
            component,
            bind,
            ..
        } => visit(id, component, bind),
    }
}
