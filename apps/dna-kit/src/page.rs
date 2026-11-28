//! Shared renderer for code-free workbook pages. Every part owns its typed watches.

use crate::commands::{CommandId, Invocation};
use crate::{Following, Live, Session};
use calc_skin::grid::Key;
use calc_skin::query;
use calc_skin::{
    BindingKind, BindingSnapshot, BindingValue, Bound, Budget, ChartImage, Coverage, DocId,
    ErrorOrigin, Evidence, Page, PageNode,
    PartialWhy, Provenance, RawValue, RowWindow, SourceRef, InventoryItem, InventoryKind, RowPage,
    WorkbookCell,
};
use leptos::prelude::*;

pub mod clock;
pub use dna_skin_runtime::skin::validate_page;
/// Preview a typed page in the installed App view.
pub const TRY_APP_PAGE: &str = "app.try-page";
mod reader;
mod report;
pub use reader::live_values;
pub use report::{PageReport, ReportCosts, capture as capture_report, costs as report_costs};

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
    clock_document: Option<DocId>,
    snapshot: Option<StoredValue<BindingSnapshot>>,
    errors: Option<PageErrors>,
}

#[derive(Clone, Copy)]
struct PageErrors {
    issues: Following<query::IssuesIn>,
    visible: RwSignal<u32>,
}

impl PageErrors {
    fn new(session: Session) -> Self {
        Self {
            issues: session.following_with_budget(Budget {
                cells: 32_768,
                nodes: 65_536,
                edges: 65_536,
                bytes: 4 * 1024 * 1024,
                millis: 2_000,
            }),
            visible: RwSignal::new(0),
        }
    }

    fn mounted(self) {
        self.visible.update(|visible| *visible += 1);
        on_cleanup(move || {
            let remaining = self.visible.try_update(|visible| {
                *visible -= 1;
                *visible
            });
            if remaining == Some(0) {
                self.issues.at(None);
            }
        });
    }
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
    #[prop(optional)] clock_document: Option<DocId>,
    #[prop(optional)] snapshot: Option<StoredValue<BindingSnapshot>>,
    #[prop(optional)] report_footer: Option<String>,
) -> impl IntoView {
    let mode = PageMode {
        input, author, scenario, compare, report, clock_document, snapshot,
        errors: input.is_some().then(|| PageErrors::new(session)),
    };
    let schedules = report.zip(snapshot).map(|(report, snapshot)| {
        snapshot.with_value(|snapshot| report::schedules(&document, report, snapshot))
    });
    view! {
        <article class="dna-page" data-dnacalc="page">
            <h1>{document.title}</h1>
            {node(session, document.root, mode)}
            {report_footer.map(|footer| view! { <footer><small>{footer}</small></footer> })}
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
            if mode.report.is_some() && matches!(component.as_str(), "scenario-picker" | "data-frame" | "clock") {
                return ().into_any();
            }
            let label = props.get("label").and_then(|value| value.as_str()).unwrap_or(&id).to_owned();
            let body = match component.as_str() {
                "scenario-picker" => scenarios(session, label.clone(), mode.scenario),
                "clock" => clock::clock_controls(session, id.clone(), label.clone(), &bind, mode.clock_document).into_any(),
                "rich-prose" => {
                    let text = props.get("text").and_then(|value| value.as_str()).unwrap_or_default();
                    view! {
                        <section class="dna-page-card" data-page-part=id.clone()>
                            {reader::prose(session, text, &bind, mode.snapshot)}
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
    let contents = if let Some(snapshot) = mode.snapshot {
        snapshot.with_value(|snapshot| frozen_part(snapshot, &component, label, &bound, mode.compare))
    } else {
        let resolved = resolve_bound(session, bound.clone());
        (move || match resolved.get() {
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
                        mode.errors.filter(|_| component == "value"),
                    )
                }
            }
            Live::Refused(_) => missing(),
            _ => view! { <p aria-busy="true">"Reading source…"</p> }.into_any(),
        }).into_any()
    };
    view! { <section class="dna-page-card" data-page-part=id>{contents}</section> }.into_any()
}

fn frozen_part(
    snapshot: &BindingSnapshot,
    component: &str,
    label: String,
    bound: &Bound,
    compare: bool,
) -> AnyView {
    let kind = match component {
        "chart" => BindingKind::Chart { comparison: compare },
        "notebook-excerpt" => BindingKind::Rows { limit: Some(12) },
        _ => BindingKind::Value,
    };
    match snapshot.get(bound, kind) {
        Some(BindingValue::Cell(cell)) => {
            let delta = match snapshot.get(bound, BindingKind::Comparison) {
                Some(BindingValue::Comparison(Some(reading))) if component == "value" && compare => {
                    render_comparison(reading)
                }
                _ => None,
            };
            view! {
                <div class="dna-page-value">
                    <strong>{label}</strong>
                    {shown_value(cell.clone(), None)}
                    {delta}
                </div>
            }.into_any()
        }
        Some(BindingValue::Chart(chart)) => render_chart(chart.clone()),
        Some(BindingValue::Rows(page)) => rows_table(page.clone(), report::print_caption(page)).into_any(),
        _ => missing(),
    }
}

fn chart_image(session: Session, object: SourceRef, comparison: Option<Bound>) -> AnyView {
    let watch = session.following::<query::ChartImageOf>();
    let query = query::ChartImageOf { object, comparison };
    (move || match watch.at(Some(query.clone())) {
        Live::Ready(answer) if answer.coverage == Coverage::Complete => render_chart(answer.body),
        Live::Ready(_) | Live::Refused(_) => missing(),
        _ => view! { <p aria-busy="true">"Reading chart…"</p> }.into_any(),
    }).into_any()
}

/// Render the workbook chart that both identities of a saved binding designate.
pub fn bound_chart(session: Session, bound: Bound) -> impl IntoView {
    let resolved = resolve_bound(session, bound);
    move || match resolved.get() {
        Live::Ready(source) => chart_image(session, source, None),
        Live::Refused(_) => missing(),
        _ => view! { <p aria-busy="true">"Reading chart…"</p> }.into_any(),
    }
}

/// The current entry and write permission for a view's control of one bound cell.
pub fn bound_input(
    session: Session,
    bound: Bound,
    label: String,
    control: impl Fn(Memo<String>, Signal<bool>) -> AnyView + 'static,
) -> impl IntoView {
    let watch = session.following::<query::BoundInput>();
    let query = query::BoundInput { bound };
    let facts = Memo::new(move |_| match watch.at(Some(query.clone())) {
        Live::Ready(answer) if answer.coverage == Coverage::Complete => Live::Ready(answer.body),
        Live::Ready(_) => Live::Refused(calc_skin::Refusal::Invalid { reason: "Source missing".into() }),
        Live::Refused(reason) => Live::Refused(reason),
        _ => Live::Loading,
    });
    let available = Memo::new(move |_| facts.with(|facts| match facts {
        Live::Ready(_) => Some(true),
        Live::Refused(_) => Some(false),
        _ => None,
    }));
    let control = StoredValue::new_local(control);
    move || match available.get() {
        Some(true) => {
            let value = Memo::new(move |_| facts.with(|facts| match facts {
                Live::Ready(facts) => facts.input.clone(),
                _ => String::new(),
            }));
            let can_write = Signal::derive(move || facts.with(|facts| matches!(facts, Live::Ready(facts) if facts.writable)));
            let input = control.with_value(|control| control(value, can_write));
            let label = label.clone();
            let control_name = label.clone();
            view! {
                <label class="dna-page-card" data-page-control=control_name><strong>{label}</strong>{input}</label>
            }.into_any()
        }
        Some(false) => missing(),
        None => view! { <p aria-busy="true">"Reading input…"</p> }.into_any(),
    }
}

fn writable_literal(session: Session, at: Key, read: Memo<Option<WorkbookCell>>) -> impl Fn() -> bool + Copy + 'static {
    let rules = session.following::<query::Rules>();
    move || {
        matches!(rules.at(Some(query::Rules { at })),
            Live::Ready(answer) if answer.coverage == Coverage::Complete && answer.body.refusal.is_none())
            && read.get().is_some_and(|cell| !cell.input.starts_with('='))
    }
}

fn render_chart(chart: ChartImage) -> AnyView {
    view! {
        <figure class="dna-page-chart-frame">
            <img class="dna-page-chart" src=chart.data_url alt=chart.label/>
            {chart.uses_saved_values.then(|| view! { <figcaption>"Uses saved chart data"</figcaption> })}
            {chart.compares_baseline.then(|| view! { <figcaption>"Dotted baseline · Solid current"</figcaption> })}
        </figure>
    }.into_any()
}

fn comparison(session: Session, bound: Bound) -> impl IntoView {
    let values = session.following::<query::NumericComparisonOf>();
    let query = query::NumericComparisonOf { bound };
    move || {
        let Live::Ready(answer) = values.at(Some(query.clone())) else { return None; };
        render_comparison(&answer.body?)
    }
}

fn render_comparison(reading: &query::NumericComparison) -> Option<AnyView> {
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
    }.into_any())
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

fn error_explanation(session: Session, at: Key, errors: PageErrors) -> impl IntoView {
    const UNAVAILABLE: &str = "The source of this error is unavailable";
    errors.mounted();
    let label = move |at: Key| {
        let a1 = format!("{}{}", calc_skin::address::column_letters(at.3), at.2);
        let sheet = session
            .summary()?
            .sheets
            .into_iter()
            .find(|sheet| sheet.sheet == calc_skin::grid::Sheet(at.0, at.1))?;
        Some(format!("{}!{a1}", calc_skin::address::quoted_sheet(&sheet.name)))
    };
    view! {
        <div class="dna-page-error-detail">
            <p data-dnacalc="app-error-explanation">{move || {
                let answer = match errors.issues.at(Some(query::IssuesIn { scope: SourceRef::Workbook })) {
                    Live::Ready(answer) if answer.coverage == Coverage::Complete => answer,
                    Live::Ready(_) | Live::Refused(_) => return UNAVAILABLE.into(),
                    _ => return "Looking for the source of this error".into(),
                };
                if !session.stamp().is_some_and(|stamp| {
                    stamp.workbook == answer.stamp.workbook
                        && stamp.model == answer.stamp.model
                        && stamp.calc == answer.stamp.calc
                }) {
                    return "Looking for the source of this error".into();
                }
                let (Evidence::Observed { body, .. } | Evidence::Derived { body, .. }) = answer.body else {
                    return UNAVAILABLE.into();
                };
                let Some(issue) = body.errors.into_iter().find(|issue| issue.at == at) else {
                    return UNAVAILABLE.into();
                };
                if let Some(source) = issue.exact_root.and_then(label) {
                    return if issue.exact_root == Some(at) {
                        format!("{source} returned {}", issue.code)
                    } else {
                        format!("The error came from {source}")
                    };
                }
                match issue.origin {
                    ErrorOrigin::CandidatePropagation { roots } => {
                        let sources = roots.into_iter().filter_map(label).collect::<Vec<_>>().join(", ");
                        if sources.is_empty() {
                            UNAVAILABLE.into()
                        } else {
                            format!("The trace points to a possible source at {sources}")
                        }
                    }
                    ErrorOrigin::RootCandidate => label(at)
                        .map_or_else(|| UNAVAILABLE.into(),
                            |source| format!("{source} returned {}", issue.code)),
                    _ => UNAVAILABLE.into(),
                }
            }}</p>
            <button type="button" data-command="edit.undo"
                disabled=move || session.context().read_only || !matches!(session.seam(), crate::Seam::Idle)
                on:click=move |_| { let _ = session.command("edit.undo"); }>
                "Undo my last change"
            </button>
        </div>
    }
}

fn scalar(
    session: Session,
    source: SourceRef,
    label: String,
    bound: Bound,
    input: Option<PageInput>,
    compare: bool,
    errors: Option<PageErrors>,
) -> AnyView {
    let Some(at) = single_cell(&source) else {
        return missing();
    };
    let read = first_cell(session, source);
    let Some(input) = input else {
        let has_error = Memo::new(move |_| read.with(|cell| cell.as_ref().is_some_and(|cell| {
            matches!(cell.shown.value, RawValue::Error(_))
                && matches!(cell.shown.provenance, Provenance::Calculated { .. } | Provenance::Literal)
        })));
        return view! {
            <div class="dna-page-value">
                <strong>{label}</strong>
                {move || read.get().map(|cell| shown_value(cell, None))}
                {errors.map(|errors| {
                    view! {
                        <Show when=move || has_error.get()>
                            {error_explanation(session, at, errors)}
                        </Show>
                    }
                })}
                {compare.then(|| comparison(session, bound))}
            </div>
        }
        .into_any();
    };
    let can_write = writable_literal(session, at, read);
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
    let expanded = RwSignal::new(false);
    let summary = summary.map(|bound| resolve_bound(session, bound));
    let source = Memo::new(move |_| {
        if grouped.get() {
            let Some(Live::Ready(summary)) = summary.map(|summary| summary.get()) else {
                return None;
            };
            Some(summary)
        } else {
            Some(source.clone())
        }
    });
    let reading = Memo::new(move |_| {
        let read = source.get().map(|source| query::Rows {
            source,
            filter: Vec::new(),
            sort: Vec::new(),
            window: RowWindow { offset: offset.get(), limit: PAGE },
        });
        match rows.at(read) {
            Live::Ready(answer) if matches!(answer.coverage, Coverage::Complete
                | Coverage::Partial { why: PartialWhy::Window, .. }) => Live::Ready(answer.body),
            Live::Ready(_) => Live::Outdated,
            Live::Refused(reason) => Live::Refused(reason),
            _ => Live::Loading,
        }
    });
    let size = Memo::new(move |_| match reading.get() {
        Live::Ready(page) => Some((page.matched_rows, page.columns)),
        _ => None,
    });
    view! {
        <div class="dna-page-schedule">
        {summary.map(|summary| view! {
            <button type="button" aria-pressed=move || grouped.get().to_string()
                disabled=move || !matches!(summary.get(), Live::Ready(_))
                on:click=move |_| {
                    expanded.set(false);
                    offset.set(0);
                    grouped.update(|grouped| *grouped = !*grouped);
                }>"Group by Year (display rollup)"</button>
        })}
        {move || {
            if grouped.get() && summary.is_some_and(|summary| matches!(summary.get(), Live::Refused(_))) {
                return missing();
            }
            if expanded.get()
                && let (Some(source), Some((total, columns))) = (source.get(), size.get())
                && u64::from(total) * u64::from(columns) <= crate::PAGE_BUDGET.cells
            {
                let parts = report::schedule_windows(session, source, total)
                    .map(move |read| move || match read.get() {
                        Live::Ready(page) => {
                            let caption = row_caption(&page);
                            rows_table(page, caption).into_any()
                        }
                        Live::Refused(_) => view! { <p>"Rows unavailable"</p> }.into_any(),
                        _ => view! { <p aria-busy="true">"Reading rows…"</p> }.into_any(),
                    })
                    .collect_view();
                return view! {
                    <button type="button" data-page-action="collapse-rows"
                        on:click=move |_| expanded.set(false)>"Collapse"</button>
                    {parts}
                }.into_any();
            }
            match reading.get() {
                Live::Ready(page) => {
                    let start = page.offset;
                    let end = start + page.rows.len() as u32;
                    let total = page.matched_rows;
                    let can_expand = total > PAGE
                        && u64::from(total) * u64::from(page.columns) <= crate::PAGE_BUDGET.cells;
                    let caption = row_caption(&page);
                    view! {
                        {rows_table(page, caption)}
                        <button type="button" data-page-action="previous-rows" disabled={start == 0}
                            on:click=move |_| offset.set(start.saturating_sub(PAGE))>"Previous rows"</button>
                        <button type="button" data-page-action="next-rows" disabled={end >= total}
                            on:click=move |_| offset.set(end)>"Next rows"</button>
                        {can_expand.then(|| view! {
                            <button type="button" data-page-action="expand-rows"
                                on:click=move |_| expanded.set(true)>"Expand all"</button>
                        })}
                    }.into_any()
                }
                Live::Outdated | Live::Refused(_) => view! { <p>"Rows unavailable"</p> }.into_any(),
                _ => view! { <p aria-busy="true">"Reading rows…"</p> }.into_any(),
            }
        }}
        </div>
    }.into_any()
}

fn row_caption(page: &RowPage) -> String {
    if page.rows.is_empty() {
        return "No rows".into();
    }
    let start = page.offset + 1;
    let end = page.offset + page.rows.len() as u32;
    format!("Rows {start}–{end} of {}", page.matched_rows)
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
