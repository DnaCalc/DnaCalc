//! Print layout for the shared workbook page renderer.

use super::{row_caption, rows_table};
use crate::{Live, Session};
use calc_skin::query;
use calc_skin::{
    Answer, BindingKind, BindingRead, BindingSnapshot, BindingValue, Bound, Budget, Coverage, Page,
    PartialWhy, Provenance, RowPage, RowWindow, SourceRef,
};
use leptos::prelude::*;

const ROWS_PER_SCHEDULE_PAGE: u32 = 36;
const CELLS_PER_SCHEDULE_READ: u32 = 512;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageReport {
    #[default]
    ExecutiveSummary,
    AnnualSchedule,
    FullSchedule,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReportCosts {
    pub annual: Option<u32>,
    pub full: Option<u32>,
}

fn bindings(page: &Page, report: PageReport) -> Vec<Bound> {
    let slot = match report {
        PageReport::ExecutiveSummary => return Vec::new(),
        PageReport::AnnualSchedule => "summary",
        PageReport::FullSchedule => "rows",
    };
    let mut sources = Vec::new();
    page.root.for_each_part(&mut |_, component, bound| {
        if component == "data-frame"
            && let Some(source) = bound.get(slot)
        {
            sources.push(source.clone());
        }
    });
    sources
}

fn rows(session: Session, source: SourceRef, offset: u32, limit: u32) -> Memo<Live<RowPage>> {
    // Expansion can overlap the calculation and publication from an applied input.
    let watch = session.following_with_budget::<query::Rows>(Budget {
        millis: 500,
        ..crate::PAGE_BUDGET
    });
    let query = query::Rows {
        source,
        filter: Vec::new(),
        sort: Vec::new(),
        window: RowWindow { offset, limit },
    };
    Memo::new(move |_| match watch.at(Some(query.clone())) {
        Live::Ready(answer)
            if matches!(
                answer.coverage,
                Coverage::Complete
                    | Coverage::Partial {
                        why: PartialWhy::Window,
                        ..
                    }
            ) =>
        {
            Live::Ready(answer.body)
        }
        Live::Ready(_) => Live::Refused(calc_skin::Refusal::Invalid {
            reason: "The schedule is unavailable.".into(),
        }),
        Live::Refused(reason) => Live::Refused(reason),
        _ => Live::Loading,
    })
}

/// Read all report parts into one completed calculation cut.
pub fn capture(session: Session, page: &Page) -> Memo<Live<Answer<BindingSnapshot>>> {
    let mut reads = Vec::new();
    let mut push = |bound: &Bound, kind| {
        let read = BindingRead {
            bound: bound.clone(),
            kind,
        };
        if !reads.contains(&read) {
            reads.push(read);
        }
    };
    page.root.for_each_part(&mut |_, component, slots| {
        let mut read = |slot, kind| {
            if let Some(bound) = slots.get(slot) {
                push(bound, kind);
            }
        };
        match component {
            "input" => read("value", BindingKind::Value),
            "value" => {
                read("value", BindingKind::Value);
                read("value", BindingKind::Comparison);
            }
            "chart" => read("object", BindingKind::Chart { comparison: true }),
            "notebook-excerpt" => read("rows", BindingKind::Rows { limit: Some(12) }),
            "rich-prose" => {
                for bound in slots.values() {
                    push(bound, BindingKind::Value);
                }
            }
            _ => {}
        }
    });
    for report in [PageReport::AnnualSchedule, PageReport::FullSchedule] {
        for bound in bindings(page, report) {
            push(&bound, BindingKind::Rows { limit: None });
        }
    }
    session.consistent(
        query::ConsistentBindings { reads },
        Budget {
            millis: 500,
            ..crate::PAGE_BUDGET
        },
    )
}

pub fn costs(page: &Page, snapshot: &BindingSnapshot) -> ReportCosts {
    let total = |report| {
        let pages = bindings(page, report)
            .iter()
            .try_fold(0u32, |pages, bound| {
                let rows = schedule(snapshot, bound)?;
                pages.checked_add(rows.matched_rows.div_ceil(ROWS_PER_SCHEDULE_PAGE))
            })?;
        (pages > 0).then_some(pages)
    };
    ReportCosts {
        annual: total(PageReport::AnnualSchedule),
        full: total(PageReport::FullSchedule),
    }
}

fn schedule<'a>(snapshot: &'a BindingSnapshot, bound: &Bound) -> Option<&'a RowPage> {
    match snapshot.get(bound, BindingKind::Rows { limit: None })? {
        BindingValue::Rows(rows) if rows.rows.len() as u32 == rows.matched_rows => Some(rows),
        _ => None,
    }
}

pub(super) fn schedules(
    page: &Page,
    report: PageReport,
    snapshot: &BindingSnapshot,
) -> Vec<AnyView> {
    let label = if report == PageReport::AnnualSchedule {
        "Annual schedule"
    } else {
        "Schedule"
    };
    bindings(page, report)
        .iter()
        .flat_map(|bound| {
            schedule(snapshot, bound).map_or_else(
                || {
                    vec![
                        view! { <p data-print-unavailable>"Schedule rows unavailable"</p> }
                            .into_any(),
                    ]
                },
                |rows| schedule_read(label, rows.clone()),
            )
        })
        .collect()
}

pub(super) fn schedule_windows(
    session: Session,
    source: SourceRef,
    matched_rows: u32,
    columns: u32,
) -> impl Iterator<Item = Memo<Live<RowPage>>> {
    // Keep each projection small as the schedule gets wider.
    let window = (CELLS_PER_SCHEDULE_READ / columns.max(1)).max(1);
    (0..matched_rows.div_ceil(window))
        .map(move |index| rows(session, source.clone(), index * window, window))
}

fn schedule_read(title: &'static str, mut read: RowPage) -> Vec<AnyView> {
    let rows = std::mem::take(&mut read.rows);
    rows.chunks(ROWS_PER_SCHEDULE_PAGE as usize)
        .enumerate()
        .map(|(page, rows)| {
            schedule_page(
                title,
                RowPage {
                    offset: read.offset + page as u32 * ROWS_PER_SCHEDULE_PAGE,
                    rows: rows.to_vec(),
                    ..read.clone()
                },
            )
        })
        .collect()
}

fn schedule_page(title: &'static str, page: RowPage) -> AnyView {
    let caption = print_caption(&page);
    view! {
        <section class="dna-page-print-schedule" data-dnacalc="app-print-schedule-page">
            <h2>{title}</h2>
            {rows_table(page, caption)}
        </section>
    }
    .into_any()
}

pub(super) fn print_caption(page: &RowPage) -> String {
    let mut caption = row_caption(page);
    let (mut saved, mut previous, mut refused) = (false, false, false);
    for cell in page.rows.iter().flat_map(|row| &row.cells) {
        match &cell.shown.provenance {
            Provenance::AsSaved => saved = true,
            Provenance::Stale { .. } => previous = true,
            Provenance::Refused { .. } => refused = true,
            _ => {}
        }
    }
    for (present, text) in [
        (saved, " · Includes saved values"),
        (previous, " · Includes previous calculations"),
        (refused, " · Includes refused calculations"),
    ] {
        if present {
            caption.push_str(text);
        }
    }
    caption
}
