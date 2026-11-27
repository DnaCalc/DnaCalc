use super::{State, button};
use calc_skin::{Page, ViewId, WorkbookId};
use dna_kit::page::{PageRenderer, PageReport, capture_report, report_costs};
use dna_kit::{Live, Session, ViewCommand};
use leptos::prelude::*;
use std::sync::Arc;

const OPEN: &str = "app.print-options";
const CLOSE: &str = "app.close-print";
const CONFIRM: &str = "app.print-report";
const REFRESH: &str = "app.refresh-print";

#[derive(Clone, PartialEq)]
struct Request {
    workbook: WorkbookId,
    report: PageReport,
    capture: u64,
    document: Arc<Page>,
}

#[derive(Clone, Copy, Default)]
struct Preview(RwSignal<Option<Request>>);

fn preview(session: Session) -> RwSignal<Option<Request>> {
    session.local::<Preview>().0
}

fn current(session: Session) -> Option<Request> {
    let workbook = session.stamp()?.workbook;
    preview(session)
        .get()
        .filter(|request| request.workbook == workbook)
}

pub(super) fn commands(state: State, view: &ViewId) -> Vec<ViewCommand> {
    let session = state.session;
    let _ = preview(session);
    let open = ViewCommand::new(
        view,
        OPEN,
        "Print executive summary",
        &["Ctrl+P"],
        move |_| {
            let (document, _) = state.document()?;
            let workbook = session.stamp().ok_or("Open a workbook first")?.workbook;
            let capture = current(session).map_or(0, |request| request.capture.wrapping_add(1));
            preview(session).set(Some(Request {
                workbook,
                report: PageReport::ExecutiveSummary,
                capture,
                document: Arc::new(document),
            }));
            Ok(())
        },
    )
    .when("!editing");
    let close = ViewCommand::new(view, CLOSE, "Close print options", &[], move |_| {
        cancel(session);
        Ok(())
    });
    let print = ViewCommand::new(view, CONFIRM, "Print report", &[], move |_| {
        current(session).ok_or("Open the executive summary first")?;
        print_window()
    });
    let refresh = ViewCommand::new(view, REFRESH, "Refresh print preview", &[], move |_| {
        let mut request = current(session).ok_or("Open the executive summary first")?;
        request.document = Arc::new(state.document()?.0);
        request.capture = request.capture.wrapping_add(1);
        preview(session).set(Some(request));
        Ok(())
    });
    let mut commands = vec![open, close, print, refresh];
    for (command, label, report) in [
        (
            "app.report-summary",
            "Executive summary only",
            PageReport::ExecutiveSummary,
        ),
        (
            "app.report-annual",
            "Include annual summary",
            PageReport::AnnualSchedule,
        ),
        (
            "app.report-full",
            "Include full schedule",
            PageReport::FullSchedule,
        ),
    ] {
        commands.push(ViewCommand::new(view, command, label, &[], move |_| {
            let mut request = current(session).ok_or("Open the executive summary first")?;
            request.report = report;
            preview(session).set(Some(request));
            Ok(())
        }));
    }
    commands
}

pub(super) fn cancel(session: Session) -> bool {
    if current(session).is_some() {
        preview(session).set(None);
        true
    } else {
        false
    }
}

pub(super) fn open_button(session: Session) -> impl IntoView {
    button(session, OPEN, "Print summary…")
}

pub(super) fn showing(session: Session) -> Memo<bool> {
    Memo::new(move |_| current(session).is_some())
}

pub(super) fn stage(session: Session) -> impl IntoView {
    let capture = Memo::new(move |_| {
        current(session).map(|request| (request.workbook, request.capture, request.document))
    });
    move || {
        capture
            .get()
            .map(|(_, _, document)| snapshot_stage(session, document))
    }
}

fn snapshot_stage(session: Session, document: Arc<Page>) -> AnyView {
    let snapshot = capture_report(session, &document);
    let request = Memo::new(move |_| current(session));
    (move || {
        let Some(request) = request.get() else { return ().into_any(); };
        let answer = match snapshot.get() {
            Live::Ready(answer) => answer,
            Live::Refused(_) => return view! {
                <aside class="dna-page-card dna-page-print-options" aria-label="Print options">
                    <h2>"Executive summary"</h2>
                    <p role="alert">"The workbook report is unavailable."</p>
                    {button(session, REFRESH, "Refresh preview")}
                    {button(session, CLOSE, "Close")}
                </aside>
            }.into_any(),
            _ => return view! {
                <aside class="dna-page-card dna-page-print-options" aria-label="Print options">
                    <h2>"Executive summary"</h2>
                    <p aria-busy="true">"Reading a completed calculation…"</p>
                    {button(session, CLOSE, "Close")}
                </aside>
            }.into_any(),
        };
        let document = document.as_ref().clone();
        let costs = report_costs(&document, &answer.body);
        let report_footer = capture_caption(answer.stamp.calc.0, answer.body.captured_unix_ms);
        let snapshot = StoredValue::new(answer.body);
        let stamp = answer.stamp;
        view! {
            <aside class="dna-page-card dna-page-print-options" data-dnacalc="app-print-options" aria-label="Print options">
                <h2>"Executive summary"</h2>
                <div class="dna-page-tools">
                    {report_choice(session, "app.report-summary", "Summary only · 1 page".into(),
                        request.report == PageReport::ExecutiveSummary)}
                    {costs.annual.map(|pages| report_choice(
                        session, "app.report-annual", cost_label("Annual summary", pages),
                        request.report == PageReport::AnnualSchedule
                    ))}
                    {costs.full.map(|pages| report_choice(
                        session, "app.report-full", cost_label("Full schedule", pages),
                        request.report == PageReport::FullSchedule
                    ))}
                </div>
                <div class="dna-page-tools">
                    {button(session, CONFIRM, "Print")}
                    {button(session, CLOSE, "Close")}
                    <Show when=move || session.stamp().is_some_and(|current| current != stamp)>
                        <span role="status">"Workbook changed"</span>
                        {button(session, REFRESH, "Refresh preview")}
                    </Show>
                </div>
            </aside>
            <div class="dna-page-print-report" data-dnacalc="app-print-report" data-calc-generation=stamp.calc.0>
                <PageRenderer session document report=request.report snapshot report_footer compare=true />
            </div>
        }.into_any()
    }).into_any()
}

fn capture_caption(generation: u64, captured_unix_ms: Option<u64>) -> String {
    let caption = format!("Calculation {generation}");
    captured_unix_ms.and_then(capture_time).map_or_else(
        || caption.clone(),
        |time| format!("{caption} · Captured {time}"),
    )
}

#[cfg(target_arch = "wasm32")]
fn capture_time(unix_ms: u64) -> Option<String> {
    use leptos::web_sys::js_sys::Date;
    Date::new(&leptos::wasm_bindgen::JsValue::from_f64(unix_ms as f64))
        .to_iso_string()
        .as_string()
}

#[cfg(not(target_arch = "wasm32"))]
fn capture_time(_: u64) -> Option<String> {
    None
}

fn cost_label(label: &str, pages: u32) -> String {
    format!(
        "{label} · +{pages} {}",
        if pages == 1 { "page" } else { "pages" }
    )
}

fn report_choice(
    session: Session,
    command: &'static str,
    label: String,
    selected: bool,
) -> impl IntoView {
    view! {
        <button type="button" data-command=command aria-pressed=selected.to_string()
            on:click=move |_| { let _ = session.command(command); }>{label}</button>
    }
}

#[cfg(target_arch = "wasm32")]
fn print_window() -> Result<(), String> {
    let window = leptos::prelude::window();
    if window
        .document()
        .and_then(|document| {
            document
                .query_selector("[data-dnacalc=app-print-report]")
                .ok()
                .flatten()
        })
        .is_none()
    {
        return Err("Wait for the workbook report to finish reading.".into());
    }
    window
        .print()
        .map_err(|_| "The browser could not print the report.".into())
}

#[cfg(not(target_arch = "wasm32"))]
fn print_window() -> Result<(), String> {
    Err("The browser print carrier is unavailable.".into())
}
