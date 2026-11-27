//! Worksheet arrows from the workbook's typed dependency graph.

use calc_skin::grid::{Area, Key, Sheet};
use calc_skin::{
    DependencyDirection, DependencyGrouping, DependencyTarget, Evidence, SourceRef, ViewId,
    WorkbookId, address, query,
};
use dna_kit::{Live, Session, ViewCommand};
use leptos::{leptos_dom::helpers::request_animation_frame, prelude::*, wasm_bindgen::JsCast};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Default)]
struct Traces(RwSignal<Vec<Trace>>);

#[derive(Clone, Copy)]
struct Trace {
    workbook: WorkbookId,
    root: Key,
    direction: DependencyDirection,
    depth: u16,
}

fn sheet_of(at: Key) -> Sheet {
    Sheet(at.0, at.1)
}

pub(super) fn commands(view: &ViewId, session: Session) -> [ViewCommand; 3] {
    let traces = session.local::<Traces>().0;
    let trace = |id, title, direction| {
        ViewCommand::new(view, id, title, &[], move |_| {
            let root = session
                .active()
                .ok_or("Choose a worksheet cell to trace.")?;
            let workbook = session
                .stamp()
                .ok_or("The workbook is unavailable.")?
                .workbook;
            traces.update(|traces| {
                traces.retain(|trace| trace.workbook == workbook);
                if let Some(trace) = traces
                    .iter_mut()
                    .find(|trace| trace.root == root && trace.direction == direction)
                {
                    trace.depth = trace.depth.saturating_add(1);
                } else {
                    traces.push(Trace {
                        workbook,
                        root,
                        direction,
                        depth: 1,
                    });
                }
            });
            Ok(())
        })
        .when("mode:ready")
    };
    [
        trace(
            "grid.trace-precedents",
            "Trace Precedents",
            DependencyDirection::Precedents,
        ),
        trace(
            "grid.trace-dependents",
            "Trace Dependents",
            DependencyDirection::Dependents,
        ),
        ViewCommand::new(
            view,
            "grid.clear-traces",
            "Remove All Arrows",
            &[],
            move |_| {
                let Some(at) = session.active() else {
                    return Ok(());
                };
                traces.update(|traces| traces.retain(|trace| sheet_of(trace.root) != sheet_of(at)));
                Ok(())
            },
        )
        .when("mode:ready"),
    ]
}

pub(super) fn layer(
    session: Session,
    content: NodeRef<leptos::html::Div>,
    page: Memo<Option<Area>>,
) -> impl IntoView {
    let traces = session.local::<Traces>().0;
    view! {
        <div class="dna-sheet-traces">
            <For
                each=move || {
                    let (Some(at), Some(stamp)) = (session.active(), session.stamp()) else {
                        return Vec::new();
                    };
                    traces.with(|traces| traces.iter().copied().filter(|trace| {
                        trace.workbook == stamp.workbook && sheet_of(trace.root) == sheet_of(at)
                    }).collect::<Vec<_>>())
                }
                key=|trace| (trace.workbook, trace.root, trace.direction == DependencyDirection::Precedents, trace.depth)
                children=move |trace| arrows(session, content, page, trace)
            />
        </div>
    }
}

fn cell_address(at: Key) -> String {
    format!("{}{}", address::column_letters(at.3), at.2)
}

fn endpoint(target: &DependencyTarget) -> Option<Key> {
    match target {
        DependencyTarget::Source(SourceRef::Cells { areas, .. }) => {
            areas.first().map(|area| area.0)
        }
        DependencyTarget::Source(SourceRef::Spill { anchor }) => Some(*anchor),
        _ => None,
    }
}

#[derive(Clone)]
struct Line {
    from: Key,
    to: Key,
    x: f64,
    y: f64,
    length: f64,
    angle: f64,
}

fn arrows(
    session: Session,
    content: NodeRef<leptos::html::Div>,
    page: Memo<Option<Area>>,
    trace: Trace,
) -> impl IntoView {
    let deps = session.following::<query::Deps>();
    let lines = RwSignal::new(Vec::<Line>::new());
    Effect::new(move |_| {
        let page_at_read = page.get();
        let stamp_at_read = session.stamp();
        let sheet = sheet_of(trace.root);
        let graph = match deps.at(Some(query::Deps {
            of: SourceRef::Cells {
                sheet,
                areas: vec![Area(trace.root, trace.root)],
            },
            dir: trace.direction,
            depth: trace.depth,
            group: DependencyGrouping::Reference,
        })) {
            Live::Ready(answer) => match answer.body {
                Evidence::Observed { body, .. } | Evidence::Derived { body, .. } => body,
                Evidence::Unavailable { .. } => {
                    lines.set(Vec::new());
                    return;
                }
            },
            _ => {
                lines.set(Vec::new());
                return;
            }
        };
        let edges = graph
            .edges
            .iter()
            .filter_map(|edge| {
                let left = graph.nodes.get(edge.from).and_then(endpoint)?;
                let right = graph.nodes.get(edge.to).and_then(endpoint)?;
                if left == right || sheet_of(left) != sheet || sheet_of(right) != sheet {
                    return None;
                }
                Some(match trace.direction {
                    DependencyDirection::Precedents => (right, left),
                    DependencyDirection::Dependents => (left, right),
                })
            })
            .collect::<BTreeSet<_>>();
        request_animation_frame(move || {
            if page.get_untracked() != page_at_read || session.stamp() != stamp_at_read {
                return;
            }
            let Some(content) = content.get_untracked() else {
                return;
            };
            let centre = |at| {
                let selector = format!("[data-cell='{}']", cell_address(at));
                let mut element = content
                    .query_selector(&selector)
                    .ok()
                    .flatten()?
                    .dyn_into::<leptos::web_sys::HtmlElement>()
                    .ok()?;
                let mut x = f64::from(element.offset_width()) / 2.0;
                let mut y = f64::from(element.offset_height()) / 2.0;
                loop {
                    x += f64::from(element.offset_left());
                    y += f64::from(element.offset_top());
                    let parent = element.offset_parent()?;
                    if parent.is_same_node(Some(content.as_ref())) {
                        return Some((x, y));
                    }
                    element = parent.dyn_into().ok()?;
                }
            };
            let measured = edges
                .into_iter()
                .filter_map(|(from, to)| {
                    let (x, y) = centre(from)?;
                    let (end_x, end_y) = centre(to)?;
                    let (dx, dy) = (end_x - x, end_y - y);
                    Some(Line {
                        from,
                        to,
                        x,
                        y,
                        length: dx.hypot(dy),
                        angle: dy.atan2(dx).to_degrees(),
                    })
                })
                .collect();
            let _ = lines.try_set(measured);
        });
    });
    move || {
        lines
            .get()
            .into_iter()
            .map(|line| {
                let from = cell_address(line.from);
                let to = cell_address(line.to);
                let other = if trace.root == line.to {
                    line.from
                } else {
                    line.to
                };
                view! {
                    <div class="dna-sheet-trace-arrow" data-dnacalc="trace-arrow"
                        role="img" aria-label=format!("{from} to {to}")
                        data-from=from data-to=to
                        style:left=format!("{}px", line.x) style:top=format!("{}px", line.y)
                        style:width=format!("{}px", line.length)
                        style:transform=format!("rotate({}deg)", line.angle)
                        on:dblclick=move |_| session.select_cell(other)>
                    </div>
                }
            })
            .collect_view()
    }
}
