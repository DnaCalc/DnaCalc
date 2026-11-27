use crate::{State, inventory::table_info, query::QueryWork, query::source_areas};
use calc_doc_model::{CellInput, FormulaForm};
use calc_host::value::{CellKey, GridArea};
use calc_skin::grid::{Area, Key};
use calc_skin::{FormulaRun, Refusal, SourceRef, Structure};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn structure(
    state: &State,
    scope: &SourceRef,
    meter: &mut QueryWork,
) -> Result<Structure, Refusal> {
    let host = state.session.host();
    let model = host.model();
    let areas = source_areas(state, scope)?;
    let mut tables = Vec::new();
    for table in model.definitions().tables() {
        meter.work(0, 0, 0)?;
        if areas
            .iter()
            .any(|area| area.intersection(table.area).is_some())
        {
            meter.work(0, 1, 0)?;
            tables.push(table_info(table));
        }
    }

    let mut columns: BTreeMap<_, Vec<RunCell>> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for area in areas {
        meter.work(0, 0, 0)?;
        for (at, input) in model.cells(area).into_iter().flatten() {
            let added = seen.insert(at);
            meter.work(u64::from(added), 0, 0)?;
            if !added {
                continue;
            }
            let template = match input {
                CellInput::Formula(formula) => {
                    host.formula_r1c1(at).map(|text| (text, formula.form()))
                }
                _ => None,
            };
            columns
                .entry((at.sheet(), at.column()))
                .or_default()
                .push(RunCell { at, template });
        }
    }
    let mut runs = Vec::new();
    for cells in columns.values_mut() {
        cells.sort_unstable_by_key(|cell| cell.at);
        column_runs(cells, &mut runs, meter)?;
    }
    runs.sort_unstable_by_key(|run| run.extent.0);
    Ok(Structure { tables, runs })
}

struct RunCell {
    at: CellKey,
    template: Option<(String, FormulaForm)>,
}

fn column_runs(
    cells: &[RunCell],
    runs: &mut Vec<FormulaRun>,
    meter: &mut QueryWork,
) -> Result<(), Refusal> {
    let mut first = 0;
    while let Some(start) = cells.get(first) {
        meter.work(0, 0, 0)?;
        let Some(template) = &start.template else {
            first += 1;
            continue;
        };
        let matches = |cell: &RunCell| cell.template.as_ref() == Some(template);
        let mut next = first + 1;
        let mut segment = start.at;
        let mut members = Vec::new();
        let mut exceptions = Vec::new();
        while let Some(cell) = cells.get(next) {
            meter.work(0, 0, 0)?;
            let prev = cells[next - 1].at;
            if prev.row() + 1 != cell.at.row() {
                break;
            }
            if !matches(cell) {
                // Call an occupied cell an exception only when two matching rows on
                // each side establish the surrounding pattern.
                let pair_before = prev.row() > segment.row();
                let pair_after = cells.get(next + 1..next + 3).is_some_and(|pair| {
                    pair[0].at.row() == cell.at.row() + 1
                        && pair[1].at.row() == cell.at.row() + 2
                        && pair.iter().all(matches)
                });
                if !pair_before || !pair_after {
                    break;
                }
                members.push(area(segment, prev));
                exceptions.push(Key::of(cell.at));
                segment = cells[next + 1].at;
            }
            next += 1;
        }
        if next > first + 1 {
            meter.work(0, 1, 0)?;
            let end = cells[next - 1].at;
            members.push(area(segment, end));
            runs.push(FormulaRun {
                extent: area(start.at, end),
                r1c1: template.0.clone(),
                members,
                exceptions,
            });
        }
        first = next;
    }
    Ok(())
}

fn area(first: CellKey, last: CellKey) -> Area {
    Area::of(GridArea::new(first, last).expect("run on one worksheet"))
}
