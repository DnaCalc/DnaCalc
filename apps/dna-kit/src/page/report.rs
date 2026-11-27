//! Print layout for the shared workbook page renderer.

use super::{resolve_bound, rows_table};
use crate::{Live, Session};
use calc_skin::query;
use calc_skin::{Bound, Coverage, Page, PartialWhy, RowPage, RowWindow, SourceRef};
use leptos::prelude::*;

const ROWS_PER_SCHEDULE_PAGE: u32 = 36;
const ROWS_PER_READ: u32 = ROWS_PER_SCHEDULE_PAGE * 5;

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
    let watch = session.following::<query::Rows>();
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

pub fn costs(session: Session, page: &Page) -> Memo<ReportCosts> {
    let sources = |choice| {
        bindings(page, choice)
            .into_iter()
            .map(|bound| {
                let source = resolve_bound(session, bound);
                let window = session.following::<query::Rows>();
                Memo::new(move |_| {
                    let Live::Ready(source) = source.get() else {
                        return None;
                    };
                    let query = query::Rows {
                        source,
                        filter: Vec::new(),
                        sort: Vec::new(),
                        window: RowWindow {
                            offset: 0,
                            limit: 1,
                        },
                    };
                    match window.at(Some(query)) {
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
                            Some((
                                answer.body.matched_rows.div_ceil(ROWS_PER_SCHEDULE_PAGE),
                                u64::from(answer.body.matched_rows)
                                    * u64::from(answer.body.columns),
                            ))
                        }
                        _ => None,
                    }
                })
            })
            .collect::<Vec<_>>()
    };
    let annual = sources(PageReport::AnnualSchedule);
    let full = sources(PageReport::FullSchedule);
    Memo::new(move |_| {
        let total = |parts: &[Memo<Option<(u32, u64)>>]| {
            if parts.is_empty() {
                return None;
            }
            let (pages, cells) = parts
                .iter()
                .try_fold((0u32, 0u64), |(pages, cells), part| {
                    let (count, size) = part.get()?;
                    Some((pages.checked_add(count)?, cells.checked_add(size)?))
                })?;
            (cells <= crate::PAGE_BUDGET.cells && pages > 0).then_some(pages)
        };
        ReportCosts {
            annual: total(&annual),
            full: total(&full),
        }
    })
}

pub(super) fn schedules(session: Session, page: &Page, report: PageReport) -> Vec<AnyView> {
    let label = match report {
        PageReport::AnnualSchedule => "Annual schedule",
        _ => "Schedule",
    };
    bindings(page, report)
        .into_iter()
        .map(|bound| {
            let resolved = resolve_bound(session, bound);
            (move || match resolved.get() {
                Live::Ready(source) => schedule_pages(session, source, label),
                Live::Refused(_) => {
                    view! { <p data-print-unavailable>"Schedule source missing"</p> }.into_any()
                }
                _ => loading(),
            })
            .into_any()
        })
        .collect()
}

fn schedule_pages(session: Session, source: SourceRef, title: &'static str) -> AnyView {
    let first = rows(session, source.clone(), 0, ROWS_PER_READ);
    (move || match first.get() {
        Live::Ready(first) if first.matched_rows > 0 => {
            if u64::from(first.matched_rows) * u64::from(first.columns) > crate::PAGE_BUDGET.cells {
                return view! { <p data-print-unavailable>"Choose a smaller schedule to print"</p> }.into_any();
            }
            let count = first.matched_rows.div_ceil(ROWS_PER_READ);
            let source = source.clone();
            let tail = (1..count)
                .map(move |page| {
                    let read = rows(
                        session,
                        source.clone(),
                        page * ROWS_PER_READ,
                        ROWS_PER_READ,
                    );
                    move || match read.get() {
                        Live::Ready(page) => schedule_read(title, page).into_any(),
                        Live::Refused(_) => {
                            view! { <p data-print-unavailable>"Schedule rows unavailable"</p> }
                                .into_any()
                        }
                        _ => loading(),
                    }
                })
                .collect_view();
            view! { {schedule_read(title, first)} {tail} }.into_any()
        }
        Live::Ready(_) => ().into_any(),
        Live::Refused(_) => {
            view! { <p data-print-unavailable>"Schedule rows unavailable"</p> }.into_any()
        }
        _ => loading(),
    })
    .into_any()
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
    let end = page.offset + page.rows.len() as u32;
    let caption = format!("Rows {}–{end} of {}", page.offset + 1, page.matched_rows);
    view! {
        <section class="dna-page-print-schedule" data-dnacalc="app-print-schedule-page">
            <h2>{title}</h2>
            {rows_table(page, caption)}
        </section>
    }
    .into_any()
}

fn loading() -> AnyView {
    view! { <p aria-busy="true">"Reading schedule…"</p> }.into_any()
}
