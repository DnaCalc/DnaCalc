use crate::{
    ID, INSTRUMENT_BUDGET, Instrument, State, area_label, button, cell_label, run, status,
};
use calc_skin::grid::{Area, Key};
use calc_skin::{
    Answer, DependencyDirection, DependencyGrouping, Evidence, Interest, SourceRef, Stamp, query,
};
use dna_kit::commands::{Layer, When};
use dna_kit::{Live, Session, ViewCommand};
use leptos::prelude::*;
use std::fmt::Write;

#[derive(Clone, Copy, Default)]
struct Export {
    request: RwSignal<Option<Request>>,
    clipboard: RwSignal<Option<String>>,
    serial: RwSignal<u64>,
    copied: RwSignal<u64>,
}

#[derive(Clone, PartialEq)]
struct Request {
    serial: u64,
    workbook: calc_skin::WorkbookId,
    scope: SourceRef,
    active: Option<Key>,
    pivot: Option<Key>,
    instrument: Instrument,
    dossier: bool,
    trace: bool,
    copy: bool,
}

impl Export {
    fn open(self, session: Session, copy: bool) -> Result<(), String> {
        let state = session.local::<State>();
        if session.context().view.0 != ID && state.dock.get_untracked().is_none() {
            return Err("Open the Inspector to export its evidence.".into());
        }
        let workbook = session.stamp().ok_or("Open a workbook first.")?.workbook;
        let selection = session
            .selection()
            .ok_or("Select a worksheet source first.")?;
        self.clipboard.set(None);
        let serial = self.serial.get_untracked() + 1;
        self.serial.set(serial);
        self.request.set(Some(Request {
            serial,
            workbook,
            scope: selection.target,
            active: selection.active,
            pivot: state.pivot.get_untracked().filter(|(owner, _)| *owner == workbook)
                .map(|(_, at)| at),
            instrument: state.instrument.get_untracked(),
            dossier: false,
            trace: state.tracing.get_untracked(),
            copy,
        }));
        Ok(())
    }
}

pub(super) fn commands(session: Session) -> Vec<ViewCommand> {
    let export = session.local::<Export>();
    let open = |id, label, chord, copy| {
        let mut command = ViewCommand::global(id, label, &[], move |_| export.open(session, copy));
        command.chords.push((Layer::View, When::view(ID), chord));
        command.when("!editing")
    };
    vec![
        open(
            "inspector.copy-evidence",
            "Copy evidence",
            "Ctrl+Shift+C",
            true,
        ),
        open("inspector.audit-print", "Print audit", "Ctrl+P", false),
        ViewCommand::global(
            "inspector.full-dossier",
            "Full Workbook Audit Dossier",
            &[],
            move |_| {
                export.request.update(|request| {
                    if let Some(request) = request {
                        request.dossier = true;
                        request.copy = false;
                    }
                });
                Ok(())
            },
        ),
        ViewCommand::global("inspector.close-export", "Close export", &[], move |_| {
            export.request.set(None);
            Ok(())
        }),
        ViewCommand::global(
            "inspector.copy-report",
            "Copy report",
            &[],
            move |invocation| {
                let text = invocation
                    .args
                    .as_str()
                    .ok_or("Wait for the evidence to arrive.")?;
                copy(text, export)
            },
        ),
        ViewCommand::global(
            "inspector.print-report",
            "Print report",
            &[],
            move |invocation| {
                let text = invocation
                    .args
                    .as_str()
                    .ok_or("Wait for the evidence to arrive.")?;
                print(text)
            },
        ),
    ]
}

pub(super) fn cancel(session: Session) -> bool {
    let export = session.local::<Export>();
    let state = session.local::<State>();
    if (session.context().view.0 == ID || state.dock.get_untracked().is_some())
        && export.request.with_untracked(|request| {
            request.as_ref().is_some_and(|request| {
                session
                    .stamp()
                    .is_some_and(|stamp| stamp.workbook == request.workbook)
            })
        })
    {
        let _ = session.command("inspector.close-export");
        true
    } else {
        false
    }
}

pub(super) fn mount(session: Session) -> impl IntoView {
    let export = session.local::<Export>();
    let request = Memo::new(move |_| {
        export.request.get().filter(|request| {
            session
                .stamp()
                .is_some_and(|stamp| stamp.workbook == request.workbook)
        })
    });
    view! {
        <Show when=move || request.get().is_some()>
            <section class="dna-inspector-card" aria-label="Audit export">
                <div class="dna-inspector-actions">
                    {button(session, "inspector.full-dossier", "Full Workbook Audit Dossier")}
                    {button(session, "inspector.close-export", "Close export")}
                </div>
                {move || request.get().map(|request| capture(session, export, request))}
            </section>
        </Show>
    }
}

fn capture(session: Session, export: Export, request: Request) -> impl IntoView {
    let formula = session.following::<query::Formula>();
    let cells = session.following::<query::Cells>();
    let issues = session.following_with_budget::<query::IssuesIn>(INSTRUMENT_BUDGET);
    let deps = session.following::<query::Deps>();
    let changes = session.following::<query::CalcChanges>();
    let iteration = session.following::<query::CalcIteration>();
    let execution = session.following::<query::CalcExecution>();
    let sources = session.following::<query::SourceInventory>();
    let notes = session.following::<query::NotesOf>();
    let pivot = session.following_with_budget::<query::PivotFreshnessAt>(INSTRUMENT_BUDGET);
    let contributions = session.following_with_budget::<query::ContributionsAt>(INSTRUMENT_BUDGET);
    let workbook_scope = request.dossier || request.instrument == Instrument::Overview;
    let scope = if workbook_scope {
        SourceRef::Workbook
    } else {
        request.scope.clone()
    };
    let direction = if request.instrument == Instrument::Dependents {
        DependencyDirection::Dependents
    } else {
        DependencyDirection::Precedents
    };
    let trace = request.instrument == Instrument::Precedents && request.trace;
    let dependencies = request.dossier
        || (!trace
            && matches!(
                request.instrument,
                Instrument::Precedents | Instrument::Dependents
            ));
    let calculation = request.dossier || request.instrument == Instrument::Calculation;
    let live = workbook_scope || request.instrument == Instrument::Live;
    let copy_on_arrival = request.copy;
    let snapshot: RwSignal<Option<String>> = RwSignal::new(None);

    Effect::new(move |_| {
        if snapshot.get_untracked().is_some() {
            return;
        }
        let Some(stamp) = session.stamp() else {
            return;
        };
        let formula = observe(
            formula.at(request.active.map(|at| query::Formula { at })),
            stamp,
        );
        let cells = observe(
            cells.at(request.active.map(|at| query::Cells {
                interest: Interest::Area(Area(at, at)),
            })),
            stamp,
        );
        let issues = observe(
            issues.at(Some(query::IssuesIn {
                scope: scope.clone(),
            })),
            stamp,
        );
        let deps = observe(
            deps.at(dependencies.then(|| query::Deps {
                of: request.scope.clone(),
                dir: direction,
                depth: 5,
                group: DependencyGrouping::Sheet,
            })),
            stamp,
        );
        let changes = observe(changes.at(calculation.then_some(query::CalcChanges)), stamp);
        let iteration = observe(iteration.at(calculation.then_some(query::CalcIteration)), stamp);
        let execution = observe(execution.at(calculation.then_some(query::CalcExecution)), stamp);
        let sources = observe(sources.at(live.then_some(query::SourceInventory)), stamp);
        let pivot = observe(
            pivot.at(request.pivot.filter(|_| workbook_scope)
                .map(|at| query::PivotFreshnessAt { at })),
            stamp,
        );
        let notes = observe(
            notes.at(request.dossier.then_some(query::NotesOf { anchor: None })),
            stamp,
        );
        let contributions = observe(
            contributions.at(request
                .active
                .filter(|_| trace)
                .map(|at| query::ContributionsAt { at })),
            stamp,
        );
        let (
            Some(formula),
            Some(cells),
            Some(issues),
            Some(deps),
            Some(changes),
            Some(iteration),
            Some(execution),
            Some(sources),
            Some(pivot),
            Some(notes),
            Some(contributions),
        ) = (
            formula,
            cells,
            issues,
            deps,
            changes,
            iteration,
            execution,
            sources,
            pivot,
            notes,
            contributions,
        )
        else {
            return;
        };

        let title = if request.dossier {
            "Full Workbook Audit Dossier"
        } else {
            request.instrument.label()
        };
        let mut report = format!(
            "{title}\nWorkbook: {}\nScope: {}\nModel revision {} · calculation publication {}\n",
            session
                .summary()
                .map_or_else(|| "Workbook".into(), |summary| summary.name),
            source_label(session, &scope),
            stamp.model.0,
            stamp.calc.0,
        );
        if let Some(at) = request.active {
            let _ = writeln!(report, "\nCell: {}", cell_label(session, at));
            if let Some(formula) = section(&mut report, "Formula", formula) {
                let _ = writeln!(report, "{}", formula.authored.unwrap_or(formula.input));
                if let Some(saved) = formula.as_saved {
                    let _ = writeln!(
                        report,
                        "As saved: {} → {}",
                        saved.entry,
                        saved.value.display()
                    );
                }
            }
            if let Some(window) = section(&mut report, "Worksheet value", cells)
                && let Some(cell) = window.cells.first()
            {
                let _ = writeln!(
                    report,
                    "{} ({:?})",
                    cell.shown.value.display(),
                    cell.shown.provenance
                );
            }
        }
        if let Some(issues) = section(&mut report, "Error roots", issues)
            .and_then(|answer| derived(&mut report, answer))
        {
            let _ = writeln!(report, "{} current errors in scope", issues.errors.len());
            for root in issues.roots {
                if let Ok(idx) = issues.errors.binary_search_by_key(&root.at, |item| item.at) {
                    let error = &issues.errors[idx];
                    let origin = match error.origin {
                        calc_skin::ErrorOrigin::Literal => "Literal",
                        _ if error.exact_root == Some(root.at) => "Evaluated root",
                        _ => "Root candidate",
                    };
                    let _ = writeln!(
                        report,
                        "{} {} · {origin} · {} potentially affected",
                        cell_label(session, root.at),
                        error.code,
                        root.affected
                    );
                    for name in &error.named_errors {
                        let source = SourceRef::Name { scope: name.scope.clone(), name: name.name.clone() };
                        let _ = writeln!(report, "{} returned {} · {}", source_label(session, &source), error.code, name.formula);
                    }
                }
            }
        }
        if let Some(deps) = section(&mut report, "Dependency explanation", deps)
            .and_then(|answer| derived(&mut report, answer))
        {
            let _ = writeln!(report, "{direction:?} · five levels");
            for edge in &deps.edges {
                let _ = writeln!(
                    report,
                    "{} → {} ({:?})",
                    crate::audit::target_label(session, &deps.nodes[edge.from]),
                    crate::audit::target_label(session, &deps.nodes[edge.to]),
                    edge.kind
                );
            }
            for reference in &deps.references {
                let _ = writeln!(report, "{} · {}", reference.spelling, source_label(session, &reference.source));
                if let Some(area) = reference.published_extent {
                    let _ = writeln!(report, "Published spill {}", crate::area_label(session, area));
                }
            }
        }
        if let Some(evidence) = section(&mut report, "Formula value changes", changes) {
            match evidence {
                Evidence::Observed { at, body, .. } => {
                    let _ = writeln!(report, "Publication {}", at.calc.0);
                    for change in body {
                        let _ = writeln!(
                            report,
                            "{} · {} → {}",
                            cell_label(session, change.at),
                            observation(change.before),
                            observation(change.after)
                        );
                        if let Some(anchor) = change.spill_anchor {
                            let _ = writeln!(report, "Spill from {}", cell_label(session, anchor));
                        }
                        for alias in change.aliases {
                            let source = SourceRef::Name { scope: alias.scope, name: alias.name };
                            let _ = writeln!(report, "Defined name {}", source_label(session, &source));
                        }
                    }
                }
                Evidence::Unavailable { reason } => {
                    let _ = writeln!(report, "{reason}");
                }
                Evidence::Derived { .. } => {}
            }
        }
        if let Some(Evidence::Observed { body, .. }) = section(&mut report, "Iteration", iteration) {
            let stop = crate::audit::iteration_stop(body.stop);
            let _ = writeln!(report, "{} passes · {stop}", body.passes);
            if let Some(residual) = body.residual_bits.map(f64::from_bits) {
                let max_change = f64::from_bits(body.max_change_bits);
                let _ = writeln!(report, "Residual {residual} · MaxChange {max_change}");
            }
            for read in body.reads {
                let pass = if read.current_pass { "Current pass" } else { "Prior pass" };
                let _ = writeln!(report, "{} read {} · {pass}", cell_label(session, read.reader), cell_label(session, read.source));
            }
        }
        if let Some(evidence) = section(&mut report, "DNA Calc's calculation order", execution) {
            match evidence {
                Evidence::Observed { body, .. } => {
                    let _ = writeln!(report, "{}", crate::audit::execution_count(body.total_entries));
                    if !body.finished {
                        let _ = writeln!(report, "Calculation still running.");
                    }
                    for band in body.bands {
                        let label = if band.workers { "Worker band" } else { "Evaluator entry" };
                        let cells = band.cells.into_iter().map(|cell| cell_label(session, cell)).collect::<Vec<_>>();
                        let _ = writeln!(report, "Pass {} · {label} · {}", band.pass, cells.join(", "));
                    }
                }
                Evidence::Unavailable { reason } => { let _ = writeln!(report, "{reason}"); }
                Evidence::Derived { .. } => {}
            }
        }
        if let Some(sources) = section(&mut report, "Sources", sources) {
            for link in sources.links {
                let _ = writeln!(
                    report,
                    "{} · {} cached worksheets · {} refresh errors",
                    link.file_name
                        .unwrap_or_else(|| format!("Link {}", link.index)),
                    link.cached_sheets,
                    link.refresh_error_sheets
                );
            }
            for topic in sources.topics {
                let _ = writeln!(
                    report,
                    "{} · lease {} · sequence {} · {} ({:?})",
                    topic.progid,
                    topic.lease,
                    topic.sequence,
                    topic.last.value.display(),
                    topic.last.provenance
                );
                if let Some(receipt) = topic.receipt {
                    let _ = writeln!(report, "{} accepted receipts", receipt.received);
                    if let Some(time) = receipt.last_unix_ms.and_then(dna_kit::format_utc_millis) {
                        let _ = writeln!(report, "Last received {time}");
                    }
                }
            }
        }
        if let Some(pivot) = section(&mut report, "PivotTable cache", pivot) {
            let _ = writeln!(report, "{} · {}", pivot.name, crate::audit::pivot_freshness_label(pivot.freshness));
            if let Some(source) = pivot.source {
                let _ = writeln!(report, "Source {}", area_label(session, source));
            }
            if let Some(time) = pivot.observed_refresh_unix_ms.and_then(dna_kit::format_utc_millis) {
                let _ = writeln!(report, "Refresh read {time}");
            }
            if let Some(time) = pivot.saved_refresh {
                let _ = writeln!(report, "Saved refresh {time}");
            }
            if let Some(reason) = pivot.unavailable {
                let _ = writeln!(report, "{reason}");
            }
        }
        if let Some(notes) = section(&mut report, "Findings", notes) {
            for note in notes {
                let source = note
                    .source
                    .as_ref()
                    .map_or("Orphaned anchor".into(), |source| {
                        source_label(session, source)
                    });
                let _ = writeln!(report, "{source}\n{}", note.text);
            }
        }
        if let Some(trace) = section(&mut report, "Contributions", contributions)
            .and_then(|answer| derived(&mut report, answer))
        {
            let _ = writeln!(
                report,
                "{} {} · {} inspected · {} matching · {} undetermined",
                trace.function,
                area_label(session, trace.source),
                trace.inspected,
                trace.matching_count,
                trace.undetermined
            );
        }
        snapshot.set(Some(report.clone()));
        if copy_on_arrival && export.copied.get_untracked() != request.serial {
            export.copied.set(request.serial);
            run(
                session,
                "inspector.copy-report",
                serde_json::Value::String(report),
            );
        }
    });

    view! {
        {move || match snapshot.get() {
            Some(report) => view! {
                <pre class="dna-inspector-evidence" data-inspector="audit-evidence">{report.clone()}</pre>
                <div class="dna-inspector-actions">
                    {crate::invoke(session, "inspector.copy-report", serde_json::Value::String(report.clone()), "Copy report".into())}
                    {crate::invoke(session, "inspector.print-report", serde_json::Value::String(report), "Print report".into())}
                </div>
            }.into_any(),
            None => status("Collecting workbook evidence…".into()),
        }}
        {move || export.clipboard.get().map(status)}
    }
}

fn observe<T>(live: Live<Answer<T>>, stamp: Stamp) -> Option<Result<Option<Answer<T>>, String>> {
    match live {
        Live::Idle => Some(Ok(None)),
        Live::Ready(answer) if answer.stamp == stamp => Some(Ok(Some(answer))),
        Live::Ready(answer) if answer.stamp.workbook == stamp.workbook => Some(Err(
            "The workbook changed while collecting this section. Open the report again to refresh it.".into(),
        )),
        Live::Refused(reason) => Some(Err(format!("{reason:?}"))),
        _ => None,
    }
}

fn section<T>(
    report: &mut String,
    label: &str,
    observation: Result<Option<Answer<T>>, String>,
) -> Option<T> {
    let answer = match observation {
        Ok(None) => return None,
        Ok(Some(answer)) => answer,
        Err(reason) => {
            let _ = writeln!(report, "\n{label}\nUnavailable · {reason}");
            return None;
        }
    };
    let _ = writeln!(
        report,
        "\n{label} · revision {} · publication {}",
        answer.stamp.model.0, answer.stamp.calc.0
    );
    if let Some(limit) = crate::coverage(answer.coverage) {
        let _ = writeln!(report, "{limit}");
    }
    Some(answer.body)
}

fn derived<T>(report: &mut String, answer: Evidence<T>) -> Option<T> {
    match answer {
        Evidence::Derived { body, .. } => {
            report.push_str("Derived from workbook evidence\n");
            Some(body)
        }
        Evidence::Unavailable { reason } => {
            let _ = writeln!(report, "{reason}");
            None
        }
        Evidence::Observed { body, .. } => Some(body),
    }
}

fn observation(value: query::FormulaObservation) -> String {
    format!(
        "{} ({:?})",
        value.value.map_or("—".into(), |value| value.display()),
        value.origin
    )
}

fn source_label(session: Session, source: &SourceRef) -> String {
    match source {
        SourceRef::Workbook => "Whole workbook".into(),
        SourceRef::Sheet(sheet) => crate::sheet_label(session, *sheet),
        SourceRef::Cells { areas, .. } => areas
            .iter()
            .map(|area| area_label(session, *area))
            .collect::<Vec<_>>()
            .join(", "),
        SourceRef::Spill { anchor } => format!("{}#", cell_label(session, *anchor)),
        SourceRef::Name {
            name,
            scope: calc_skin::NameScope::Sheet(sheet),
        } => {
            format!("{}!{name}", crate::sheet_label(session, *sheet))
        }
        source => format!("{source:?}"),
    }
}

#[cfg(target_arch = "wasm32")]
fn copy(text: &str, export: Export) -> Result<(), String> {
    let pending = window().navigator().clipboard().write_text(text);
    leptos::task::spawn_local(async move {
        let message = if wasm_bindgen_futures::JsFuture::from(pending).await.is_ok() {
            "Evidence copied."
        } else {
            "Clipboard unavailable. Select the report text to copy it."
        };
        export.clipboard.set(Some(message.into()));
    });
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn copy(_: &str, export: Export) -> Result<(), String> {
    export
        .clipboard
        .set(Some("Select the report text to copy it.".into()));
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn print(text: &str) -> Result<(), String> {
    let page = window()
        .open_with_url_and_target("about:blank", "_blank")
        .map_err(|_| "The browser could not open the print window.")?
        .ok_or("Allow pop-ups to print the report.")?;
    let document = page
        .document()
        .ok_or("The print document is unavailable.")?;
    document.set_title("DNA Calc audit");
    let stylesheet = leptos::prelude::document()
        .query_selector("link[rel='stylesheet']")
        .map_err(|_| "The report stylesheet is unavailable.")?;
    if let Some(stylesheet) = stylesheet {
        let stylesheet = stylesheet
            .clone_node_with_deep(false)
            .map_err(|_| "The report stylesheet could not be copied.")?;
        document
            .query_selector("head")
            .map_err(|_| "The print page is unavailable.")?
            .ok_or("The print page is unavailable.")?
            .append_child(&stylesheet)
            .map_err(|_| "The report could not be styled.")?;
    }
    let report = document
        .create_element("pre")
        .map_err(|_| "The report could not be rendered.")?;
    report
        .set_attribute("class", "dna-inspector-evidence")
        .map_err(|_| "The report could not be styled.")?;
    report.set_text_content(Some(text));
    document
        .body()
        .ok_or("The print page is unavailable.")?
        .append_child(&report)
        .map_err(|_| "The report could not be rendered.")?;
    page.print()
        .map_err(|_| "The browser could not print the report.".into())
}

#[cfg(not(target_arch = "wasm32"))]
fn print(_: &str) -> Result<(), String> {
    Err("The browser print carrier is unavailable.".into())
}
