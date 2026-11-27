use super::{State, button};
use calc_skin::{Page, ViewId, WorkbookId};
use dna_kit::page::{PageRenderer, PageReport, report_costs};
use dna_kit::{Session, ViewCommand};
use leptos::prelude::*;

const OPEN: &str = "app.print-options";
const CLOSE: &str = "app.close-print";
const CONFIRM: &str = "app.print-report";

#[derive(Clone, Copy, PartialEq, Eq)]
struct Request {
    workbook: WorkbookId,
    report: PageReport,
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
            state.document()?;
            let workbook = session.stamp().ok_or("Open a workbook first")?.workbook;
            preview(session).set(Some(Request {
                workbook,
                report: PageReport::ExecutiveSummary,
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
    let mut commands = vec![open, close, print];
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

pub(super) fn stage(session: Session, document: Page) -> impl IntoView {
    let request = Memo::new(move |_| current(session));
    move || {
        request.get().map(|request| {
        let costs = report_costs(session, &document);
        let document = document.clone();
        view! {
            <aside class="dna-page-card dna-page-print-options" data-dnacalc="app-print-options" aria-label="Print options">
                <h2>"Executive summary"</h2>
                <div class="dna-page-tools">
                    {report_choice(session, "app.report-summary", "Summary only · 1 page".into(),
                        request.report == PageReport::ExecutiveSummary)}
                    {move || costs.get().annual.map(|pages| report_choice(
                        session, "app.report-annual", cost_label("Annual summary", pages),
                        request.report == PageReport::AnnualSchedule
                    ))}
                    {move || costs.get().full.map(|pages| report_choice(
                        session, "app.report-full", cost_label("Full schedule", pages),
                        request.report == PageReport::FullSchedule
                    ))}
                </div>
                <div class="dna-page-tools">
                    {button(session, CONFIRM, "Print")}
                    {button(session, CLOSE, "Close")}
                </div>
            </aside>
            <div class="dna-page-print-report" data-dnacalc="app-print-report">
                <PageRenderer session document report=request.report compare=true />
            </div>
        }
    })
    }
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
    leptos::prelude::window()
        .print()
        .map_err(|_| "The browser could not print the report.".into())
}

#[cfg(not(target_arch = "wasm32"))]
fn print_window() -> Result<(), String> {
    Err("The browser print carrier is unavailable.".into())
}
