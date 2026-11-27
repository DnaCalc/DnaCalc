use crate::{
    INSTRUMENT_BUDGET, State, area_label, coverage, evidence, invoke, result, reveal, status,
};
use calc_skin::grid::{Area, Key, Sheet};
use calc_skin::{Contributions, InventoryItem, InventoryKind, RowWindow, SourceRef, query};
use dna_kit::Session;
use leptos::prelude::*;

const PAGE_ROWS: u32 = 25;

/// Identifies a set of host-inspected positions, plus its page in worksheet-row order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TraceWindow {
    pub at: Key,
    pub workbook: calc_skin::WorkbookId,
    pub criterion: Option<usize>,
    pub offset: u64,
}

pub(crate) fn mount(session: Session) -> impl IntoView {
    let contributions = session.following_with_budget::<query::ContributionsAt>(INSTRUMENT_BUDGET);
    let rows = session.following::<query::Rows>();
    let tables = session.following::<query::Inventory>();
    let formula = session.following::<query::Formula>();
    let state = session.local::<State>();
    view! {
        <div class="dna-inspector-card">
            <h2>"Contribution ladder"</h2>
            {move || {
                let at = session.active();
                let data = result(contributions.at(at.map(|at| query::ContributionsAt { at })), "Select a formula to trace.")
                    .and_then(|answer| Ok((evidence(answer.body)?, coverage(answer.coverage))));
                let (data, limit) = match data {
                    Ok(data) => data,
                    Err(message) => {
                        rows.at(None);
                        tables.at(None);
                        return status(message);
                    }
                };

                let table = result(tables.at(Some(query::Inventory { kinds: vec![InventoryKind::Tables] })), "")
                    .ok().and_then(|answer| answer.body.into_iter().find_map(|item| {
                        let InventoryItem::Table(table) = item else { return None; };
                        let Area(first, last) = table.area;
                        let Area(source, end) = data.source;
                        (Sheet(first.0, first.1) == Sheet(source.0, source.1)
                            && first.2.checked_add(table.headers) == Some(source.2)
                            && last.2.checked_sub(table.totals) == Some(end.2)
                            && first.3 <= source.3 && end.3 <= last.3)
                            .then_some(table)
                    }));

                let workbook = session.stamp().map(|stamp| stamp.workbook);
                let window = state.trace_window.get().filter(|window| at == Some(window.at) && workbook == Some(window.workbook));
                let preview_rows = window.and_then(|window| {
                    let positions = match window.criterion {
                        Some(criterion) => &data.criteria.get(criterion)?.failing_only_this,
                        None => &data.matching,
                    };
                    let total = row_count(positions);
                    let (first, last) = row_window(positions, window.offset)?;
                    let (left, right) = table.as_ref().map_or((data.source.0.3, data.source.1.3),
                        |table| (table.area.0.3, table.area.1.3));
                    let area = Area(Key(first.0, first.1, first.2, left), Key(last.0, last.1, last.2, right));
                    Some((window, total, area))
                });
                let page = rows.at(preview_rows.map(|(_, _, area)| query::Rows {
                    source: SourceRef::Cells { sheet: Sheet(area.0.0, area.0.1), areas: vec![area] },
                    filter: Vec::new(), sort: Vec::new(), window: RowWindow { offset: 0, limit: PAGE_ROWS },
                }));

                view! {
                    <p data-inspector="trace-result"><strong>{data.result.display()}</strong>" · " {data.function.clone()}</p>
                    <p data-inspector="trace-coverage">{format!("{} inspected · {} match · {} undetermined",
                        data.inspected, data.matching_count, data.undetermined)}</p>
                    <p>{area_label(session, data.source)}</p>
                    {limit.map(status)}
                    {ladder(session, &data, table.as_ref())}
                    {preview_rows.map(|(window, total, area)| {
                        let heading = table.as_ref().map(|table| table.name.clone())
                            .unwrap_or_else(|| area_label(session, data.source));
                        let headers = table.as_ref().map(|table| table.columns.clone()).unwrap_or_default();
                        let visible = u64::from(area.1.2 - area.0.2 + 1);
                        view! {
                            <div class="dna-inspector-filter" data-inspector="trace-filter">
                                <div class="dna-inspector-actions">
                                    <strong>"View filter from Trace · " {heading}</strong>
                                    {crate::button(session, "inspector.clear-filter", "Clear filter")}
                                </div>
                                <p>{format!("Showing {}–{} of {total} worksheet rows", window.offset + 1, window.offset + visible)}</p>
                                {match result(page, "") {
                                    Ok(answer) => row_table(session, answer.body, headers),
                                    Err(message) => status(message),
                                }}
                                {(window.offset > 0).then(|| invoke(session, "inspector.filter-page", serde_json::json!(0), "First rows".into()))}
                                {(window.offset + visible < total).then(|| invoke(session, "inspector.filter-page",
                                    serde_json::json!(window.offset + visible), "Next rows".into()))}
                            </div>
                        }
                    })}
                }.into_any()
            }}
        </div>
        {move || {
            let at = session.active();
            let fact = result(formula.at(at.map(|at| query::Formula { at })), "").ok()?;
            let at = at?;
            let authored = fact.body.authored?;
            if session.context().read_only {
                return None;
            }
            Some(repair(session, at, fact.body.input, authored, fact.stamp))
        }}
    }
}

fn ladder(
    session: Session,
    data: &Contributions,
    table: Option<&calc_skin::TableInfo>,
) -> impl IntoView {
    let at = data.formula;
    let criteria = data.criteria.iter().enumerate().map(|(index, criterion)| {
        let label = table.and_then(|table| {
            let col = criterion.range.0.3.checked_sub(table.area.0.3)?;
            (criterion.range.0.3 == criterion.range.1.3)
                .then(|| table.columns.get(col as usize).cloned()).flatten()
        }).unwrap_or_else(|| area_label(session, criterion.range));
        let count = position_count(&criterion.failing_only_this);
        view! {
            <tr data-criterion=label.clone()><th scope="row">{label.clone()}</th><td>{criterion.matching}</td><td>
                {if count == 0 { "0".into_any() } else {
                    invoke(session, "inspector.filter", serde_json::json!([at, index]), count.to_string()).into_any()
                }}
            </td></tr>
        }
    }).collect_view();
    view! {
        <table class="dna-inspector-ladder">
            <thead><tr><th scope="col">"Criterion source"</th><th scope="col">"Pass this test"</th><th scope="col">"Fail only this test"</th></tr></thead>
            <tbody>{criteria}</tbody>
        </table>
        {(data.matching_count > 0).then(|| invoke(session, "inspector.filter", serde_json::json!([at, null]), "Show matching rows".into()))}
    }
}

fn position_count(areas: &[Area]) -> u64 {
    areas
        .iter()
        .map(|Area(first, last)| u64::from(last.2 - first.2 + 1) * u64::from(last.3 - first.3 + 1))
        .sum()
}

/// Collapse only positions on the same or adjacent worksheet rows, keeping gaps.
fn row_runs(areas: &[Area]) -> Vec<(Key, Key)> {
    let mut runs: Vec<(Key, Key)> = Vec::new();
    for &Area(first, last) in areas {
        if let Some((_, end)) = runs.last_mut()
            && first.2 <= end.2 + 1
        {
            if last.2 > end.2 {
                *end = last;
            }
        } else {
            runs.push((first, last));
        }
    }
    runs
}

fn row_count(areas: &[Area]) -> u64 {
    row_runs(areas)
        .iter()
        .map(|(first, last)| u64::from(last.2 - first.2 + 1))
        .sum()
}

fn row_window(areas: &[Area], mut offset: u64) -> Option<(Key, Key)> {
    for (mut first, mut last) in row_runs(areas) {
        let length = u64::from(last.2 - first.2 + 1);
        if offset < length {
            first.2 += offset as u32;
            last.2 = last.2.min(first.2 + PAGE_ROWS - 1);
            return Some((first, last));
        }
        offset -= length;
    }
    None
}

fn row_table(session: Session, page: calc_skin::RowPage, names: Vec<String>) -> AnyView {
    let first = page.bounds.0.3;
    let headers = (0..page.columns)
        .map(|column| {
            let title = names
                .get(column as usize)
                .cloned()
                .unwrap_or_else(|| calc_skin::address::column_letters(first + column));
            view! { <th scope="col">{title}</th> }
        })
        .collect_view();
    let body = page.rows.into_iter().map(|row| {
        let at = Key(page.bounds.0.0, page.bounds.0.1, row.row, first);
        view! {
            <tr data-trace-row=row.row>
                <th scope="row">{reveal(session, at)}</th>
                {row.cells.into_iter().map(|cell| {
                    let raw = cell.shown.value.display();
                    let raw_label = (raw != cell.text).then(|| format!("Raw {raw}"));
                    view! { <td>{cell.text}{raw_label.map(|label| view! { <small>{label}</small> })}</td> }
                }).collect_view()}
            </tr>
        }
    }).collect_view();
    view! { <table><thead><tr><th scope="col">"Worksheet row"</th>{headers}</tr></thead><tbody>{body}</tbody></table> }.into_any()
}

fn repair(
    session: Session,
    at: Key,
    current: String,
    original: String,
    stamp: calc_skin::Stamp,
) -> impl IntoView {
    let input = NodeRef::<leptos::html::Input>::new();
    let include_run = NodeRef::<leptos::html::Input>::new();
    let structure = session.following_with_budget::<query::StructureOf>(INSTRUMENT_BUDGET);
    let siblings = Memo::new(move |_| {
        let Key(slot, generation, _, column) = at;
        let scope = SourceRef::Cells {
            sheet: Sheet(slot, generation),
            areas: vec![Area(
                Key(slot, generation, 1, column),
                Key(slot, generation, 1_048_576, column),
            )],
        };
        let answer = result(structure.at(Some(query::StructureOf { scope })), "").ok()?;
        let selected = at.cell()?;
        let run = evidence(answer.body).ok()?.runs.into_iter().find(|run| {
            run.members
                .iter()
                .any(|span| span.area().is_some_and(|area| area.contains(selected)))
        })?;
        let count = position_count(&run.members) - 1;
        (count > 0).then_some((answer.stamp, run.extent, count))
    });
    view! {
        <details class="dna-inspector-card">
            <summary>"Repair formula"</summary>
            <form class="dna-inspector-form" on:submit=move |event| {
                event.prevent_default();
                let Some(input) = input.get_untracked() else { return; };
                let replacement = input.value();
                match siblings.get_untracked().filter(|_| include_run.get_untracked().is_some_and(|control| control.checked())) {
                    Some((base, extent, _)) => crate::run(session, "inspector.repair-run",
                        serde_json::json!([at, original, replacement, base, extent])),
                    None => crate::run(session, "inspector.repair",
                        serde_json::json!([at, original, replacement, stamp])),
                }
            }>
                <label for="inspector-repair">"Replacement formula"</label>
                <input id="inspector-repair" type="text" node_ref=input value=current />
                {move || siblings.get().map(|(_, _, count)| view! {
                    <label><input type="checkbox" node_ref=include_run />
                        {format!("Also apply to {count} matching cells in run")}
                    </label>
                })}
                <button type="submit" data-command="inspector.repair">"Preview repair"</button>
            </form>
        </details>
    }
}
