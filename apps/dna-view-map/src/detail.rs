use super::*;
use crate::board::button;
use crate::commands::Rewire;
use calc_skin::query::{self, CalcState};
use calc_skin::{
    Coverage, Dependencies, DependencyKind, DependencyTarget, Interest, LiveTopic, Provenance,
    ReferenceOperands, StaleCause, Stamp, WorkbookCell,
};
use std::collections::BTreeMap;

const DETAIL_ROWS: usize = 30;

pub(super) fn selection(
    session: Session,
    state: MapState,
    dependencies: Memo<Live<calc_skin::Answer<Evidence<Dependencies>>>>,
) -> impl IntoView {
    let value = session.following::<query::Cells>();
    let operands = session.following::<query::ReferenceOperandsOf>();
    let iteration = session.following::<query::CalcIteration>();
    let calc = session.following::<CalcState>();
    let sources = session.following_with_budget::<query::SourceInventory>(SURVEY);
    let active = Memo::new(move |_| session.active());
    let editing_wire = Memo::new(move |_| {
        active
            .get()
            .is_some_and(|at| state.editing_wire(session, at))
    });
    let has_formula = Memo::new(move |_| session.active_entry().is_some_and(|entry| entry.formula));
    view! {
        <div class="dna-map-selection">
            {move || {
                let Some(at) = active.get() else {
                    return view! { <h2>"Select a cell or port"</h2> }.into_any();
                };
                let location = format!("{}!{}", sheet_name(session, Sheet(at.0, at.1)), a1(at));
                let reading = value.at(Some(query::Cells { interest: Interest::Area(Area(at, at)) }));
                let status = calc.at(Some(CalcState));
                let loops = iteration.at(Some(query::CalcIteration));
                view! {
                    <h2>{location}</h2>
                    {read_error(reading, "Reading selected cell…", |answer| {
                        answer.body.cells.into_iter().find(|cell| cell.at == at)
                            .map(|cell| {
                                let circular = matches!(cell.shown.provenance, Provenance::Stale { cause: StaleCause::CircularHold, .. });
                                view! {
                                    <p class="dna-map-reading">{cell_value(cell)}</p>
                                    {cycle(at, answer.stamp, loops, status, circular)}
                                }
                            })
                    })}
                    {move || {
                        let formula = session.active_entry().filter(|entry| entry.formula);
                        formula.map(|formula| view! {
                            <details class="dna-map-source">
                                <summary>"Source formula"</summary><code>{formula.input}</code>
                            </details>
                        })
                    }}
                    {move || has_formula.get().then(|| button(session, "map.rewire", "Rewire reference"))}
                }.into_any()
            }}
            {move || {
                let read = operands.at(active.get().filter(|_| editing_wire.get()).map(|at| query::ReferenceOperandsOf { at }));
                editing_wire.get().then(|| read_error(read, "Reading formula operands…", |answer| {
                    active.get().map(|at| rewire(session, at, answer.stamp, answer.body))
                }))
            }}
            {move || {
                let show = state.listing.get() && has_formula.get();
                show.then(|| read_error(dependencies.get(), "Reading connections…", |answer| {
                    active.get().and_then(|at| evidence(answer.body)
                        .map(|body| selected_connections(session, state, at, body, answer.coverage)))
                }))
            }}
            {move || {
                // Source evidence comes from the port's admitted RTD ledger.
                let read = sources.at(state.scope(session).sheet().map(|_| query::SourceInventory));
                match read {
                    Live::Ready(answer) => Some(clock_readings(answer.body.topics)),
                    _ => None,
                }
            }}
        </div>
    }
}

fn clock_readings(topics: Vec<LiveTopic>) -> impl IntoView {
    let mut clocks = BTreeMap::<String, LiveTopic>::new();
    for topic in topics {
        let [model, kind, ..] = topic.strings.as_slice() else {
            continue;
        };
        if !topic.progid.eq_ignore_ascii_case("DnaCalc.Clock")
            || !topic.server.is_empty()
            || kind != "tick"
        {
            continue;
        }
        // Changing the clock inputs replaces the local provider's lease. The ledger retains old topics.
        if clocks
            .get(model)
            .is_none_or(|prior| (prior.lease, prior.sequence) < (topic.lease, topic.sequence))
        {
            clocks.insert(model.clone(), topic);
        }
    }
    let total = clocks.len();
    let readings = clocks.into_iter().take(16).map(|(name, topic)| view! {
        <section class="dna-map-clock" data-map-clock>
            <h3>{format!("{name} · RTD clock")}</h3>
            <p data-map-tick>{topic.last.value.display()} " · " {provenance(&topic.last.provenance)}</p>
        </section>
    }).collect_view();
    view! {
        {readings}
        {(total > 16).then(|| view! { <p>{format!("16 of {total} workbook clocks shown")}</p> })}
    }
}

fn cycle(
    at: Key,
    stamp: Stamp,
    iterations: Live<calc_skin::Answer<Evidence<calc_skin::IterationFacts>>>,
    state: Live<calc_skin::Answer<query::CalcStatus>>,
    held: bool,
) -> impl IntoView {
    let observed = match iterations {
        Live::Ready(answer) => match answer.body {
            Evidence::Observed { at: when, body, .. }
                if when.workbook == stamp.workbook
                    && when.model == stamp.model
                    && when.calc == stamp.calc
                    && body.members.contains(&at) =>
            {
                Some(body)
            }
            _ => None,
        },
        _ => None,
    };
    if let Some(body) = observed {
        let tolerance = f64::from_bits(body.max_change_bits);
        return view! {
            <div class="dna-map-iteration" data-map-cycle="iteration">
                <strong>"Iteration enclosure"</strong>
                <span>{format!("{} observed passes · tolerance {tolerance}", body.passes)}</span>
            </div>
        }
        .into_any();
    }
    let reported_here = matches!(state, Live::Ready(answer) if answer.body.circular == Some(at));
    (held || reported_here)
        .then(|| {
            view! {
                <div class="dna-map-cycle" data-map-cycle="unknown">
                    <strong>"Circular reference · Unknown timing"</strong>
                </div>
            }
        })
        .into_any()
}

fn rewire(session: Session, at: Key, stamp: Stamp, operands: ReferenceOperands) -> impl IntoView {
    if operands.operands.is_empty() {
        return view! {
            <p data-map-rewire>"No scalar reference can be rewired here. Edit the formula in Classic."</p>
        }.into_any();
    }
    let choice = NodeRef::<leptos::html::Select>::new();
    let replacement = NodeRef::<leptos::html::Input>::new();
    let options = operands
        .operands
        .iter()
        .enumerate()
        .map(|(i, operand)| {
            let option = format!(
                "{} · {} at character {}",
                i + 1,
                operand.text,
                operand.span.start + 1
            );
            view! { <option value=i>{option}</option> }
        })
        .collect_view();
    view! {
        <form class="dna-map-rewire" data-map-rewire data-command="map.replace"
            on:submit=move |event| {
                event.prevent_default();
                let Some(index) = choice.get().and_then(|choice| choice.value().parse::<usize>().ok()) else { return; };
                let Some(operand) = operands.operands.get(index) else { return; };
                let Some(with) = replacement.get().map(|field| field.value()) else { return; };
                invoke(session, "map.replace", Rewire {
                    at, expected_old: operands.formula.clone(), occurrence: operand.span, stamp, with,
                });
            }>
            <label>"Operand"<select node_ref=choice data-dna-action-field="">{options}</select></label>
            <label>"Replacement cell or Defined Name"
                <input node_ref=replacement data-dna-action-field="" required/>
            </label>
            <button type="submit">"Preview rewire"</button>
        </form>
    }.into_any()
}

fn selected_connections(
    session: Session,
    state: MapState,
    at: Key,
    graph: Dependencies,
    coverage: Coverage,
) -> impl IntoView {
    let total = graph.edges.len();
    let pages = total.div_ceil(DETAIL_ROWS);
    let page = state.wire_page.get().min(pages.saturating_sub(1));
    let offset = page * DETAIL_ROWS;
    let entries = graph.edges.iter().skip(offset).take(DETAIL_ROWS).filter_map(|edge| {
        let target = graph.nodes.get(edge.to)?;
        let (label, area) = target_label(session, target);
        let source = area.filter(|area| area.0 == area.1).map(|area| area.0);
        let lag = source.is_some_and(|key| {
            (key.0, key.1) == (at.0, at.1) && key.2 + 1 == at.2
        });
        let kind = match edge.kind {
            DependencyKind::Dynamic => "Dynamic read",
            DependencyKind::Name => "Defined Name",
            _ => "Worksheet reference",
        };
        Some(view! {
            <li>
                {source.map_or_else(|| view! { <span>{label.clone()}</span> }.into_any(), |key| view! {
                    <button type="button" data-command="map.select"
                        on:click=move |_| invoke(session, "map.select", key)>{label.clone()}</button>
                }.into_any())}
                <small>{kind}</small>
                {lag.then(|| view! { <small data-map-lag="prior-row">"Previous row · [t−1]"</small> })}
            </li>
        })
    }).collect_view();
    view! {
        <section class="dna-map-list" aria-label="Selected formula connections">
            <h3>{format!("{total} direct connection records")}</h3>
            {(coverage != Coverage::Complete).then(|| view! {
                <p role="status">"Dependency binding is incomplete."</p>
            })}
            <ul>{entries}</ul>
            {(page > 0).then(|| view! {
                <button type="button" data-command="map.wire-page"
                    on:click=move |_| invoke(session, "map.wire-page", page - 1)>"Previous wires"</button>
            })}
            {(page + 1 < pages).then(|| view! {
                <button type="button" data-command="map.wire-page"
                    on:click=move |_| invoke(session, "map.wire-page", page + 1)>"Next wires"</button>
            })}
        </section>
    }
}

fn target_label(session: Session, target: &DependencyTarget) -> (String, Option<Area>) {
    match target {
        DependencyTarget::Source(SourceRef::Cells { sheet, areas }) => {
            let label = areas
                .iter()
                .copied()
                .map(extent)
                .collect::<Vec<_>>()
                .join(", ");
            (
                format!("{}!{label}", sheet_name(session, *sheet)),
                areas.first().copied(),
            )
        }
        DependencyTarget::Source(SourceRef::Name { name, .. }) => (name.clone(), None),
        DependencyTarget::Source(SourceRef::Spill { anchor }) => {
            (format!("{}#", a1(*anchor)), None)
        }
        DependencyTarget::SheetSpan { area, last } => {
            let first = Sheet(area.0.0, area.0.1);
            (
                format!(
                    "{}:{}!{}",
                    sheet_name(session, first),
                    sheet_name(session, *last),
                    extent(*area)
                ),
                None,
            )
        }
        _ => ("Workbook source".into(), None),
    }
}

pub(super) fn excerpt(session: Session, area: Area) -> impl IntoView {
    let first = area.0;
    let last = area.1;
    let rows = last.2 - first.2 + 1;
    let columns = last.3 - first.3 + 1;
    let cells = session.following::<query::Cells>();
    let geometry = session.following::<query::Geometry>();
    let sheet = Sheet(first.0, first.1);
    view! {
        <div class="dna-map-excerpt" data-map-excerpt=extent(area)>
            <h2>{format!("Sheet excerpt · {}!{}", sheet_name(session, sheet), extent(area))}</h2>
            <p>{format!("{rows} rows × {columns} columns · original worksheet addresses")}</p>
            {move || {
                use calc_skin::metrics::{SheetBands, sheet_metrics};
                let read = cells.at(Some(query::Cells { interest: Interest::Area(area) }));
                let geometry = geometry.at(Some(query::Geometry { sheet }));
                read_error(read, "Reading bounded worksheet excerpt…", move |answer| {
                    if answer.body.areas.as_slice() != [area] || answer.coverage != Coverage::Complete {
                        return view! { <p role="status">"The worksheet excerpt changed. Open it again from the map."</p> }.into_any();
                    }
                    let geometry = match geometry {
                        Live::Ready(geometry) if geometry.stamp.workbook == answer.stamp.workbook
                            && geometry.stamp.model == answer.stamp.model => geometry.body,
                        Live::Refused(reason) => return view! { <p role="status">{dna_kit::refusal_text(reason)}</p> }.into_any(),
                        _ => return view! { <p aria-busy="true">"Reading worksheet geometry…"</p> }.into_any(),
                    };
                    let layout = sheet_metrics(Some(SheetBands {
                        default_digit_px: Some(geometry.default_digit_px),
                        default_row_height: Some(geometry.default_row_height),
                        default_column_width: Some(geometry.default_column_width),
                        columns: &geometry.columns,
                        rows: &geometry.rows,
                        ..SheetBands::default()
                    })).scaled(f64::from(geometry.zoom) / 100.0);
                    let pixels = |width: f64| format!("{width}px");
                    let table_width = layout.header_w
                        + (first.3..=last.3).map(|column| layout.width(column)).sum::<f64>();
                    let widths = (first.3..=last.3).filter(|&column| layout.width(column) > 0.0).map(|column| {
                        view! { <col style:width=pixels(layout.width(column))/> }
                    }).collect_view();
                    let mut values: BTreeMap<_, _> = answer.body.cells.into_iter()
                        .map(|cell| (cell.at, cell)).collect();
                    let headers = (first.3..=last.3).map(|column| {
                        let label = calc_skin::address::column_letters(column);
                        view! {
                            <th scope="col" aria-colindex=column hidden=layout.width(column) == 0.0>{label}</th>
                        }
                    }).collect_view();
                    let body = (first.2..=last.2).map(|row| {
                        let height = layout.height(row);
                        let columns = (first.3..=last.3).map(|column| {
                            let at = Key(first.0, first.1, row, column);
                            let value = values.remove(&at).map(cell_value);
                            view! {
                                <td aria-colindex=column hidden=layout.width(column) == 0.0>
                                    <button type="button" data-command="map.select" aria-label=a1(at)
                                        aria-pressed=move || session.active() == Some(at)
                                        on:click=move |_| invoke(session, "map.select", at)>{value}</button>
                                </td>
                            }
                        }).collect_view();
                        view! { <tr aria-rowindex=row hidden=height == 0.0 style:height=pixels(height)>
                            <th scope="row">{row}</th>{columns}
                        </tr> }
                    }).collect_view();
                    view! {
                        <table aria-label="Worksheet excerpt" style:width=pixels(table_width)>
                            <colgroup><col style:width=pixels(layout.header_w)/>{widths}</colgroup>
                            <thead><tr><th aria-label="Worksheet row"></th>{headers}</tr></thead>
                            <tbody>{body}</tbody>
                        </table>
                    }.into_any()
                })
            }}
        </div>
    }
}

fn cell_value(cell: WorkbookCell) -> impl IntoView {
    let provenance = provenance(&cell.shown.provenance);
    let error = matches!(cell.shown.value, calc_skin::RawValue::Error(_));
    view! {
        <span class:dna-map-error=error data-map-reading=provenance>
            <strong>{cell.text}</strong>
            <small>{provenance}</small>
        </span>
    }
}

pub(super) fn provenance(provenance: &Provenance) -> &'static str {
    match provenance {
        Provenance::Empty => "Empty",
        Provenance::Literal => "Literal",
        Provenance::Calculated { .. } => "Calculated",
        Provenance::AsSaved => "As saved",
        Provenance::External { .. } => "External",
        Provenance::Stale { .. } => "Out of date",
        Provenance::Refused { .. } => "Refused",
    }
}
