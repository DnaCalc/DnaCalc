use crate::{State, batch, invalid, inventory::table_info, query::QueryWork, query::source_areas};
use calc_doc_model::{CellInput, FormulaForm, SheetKind, SheetVisibility};
use calc_host::value::{CellKey, GridArea};
use calc_skin::grid::{Area, Key};
use calc_skin::{FormulaRun, InputCandidate, InputSignal, NamedPort, Refusal, SourceRef, Structure};
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
    let mut formulas = 0;
    for &area in &areas {
        meter.work(0, 0, 0)?;
        for (at, input) in model.cells(area).into_iter().flatten() {
            let added = seen.insert(at);
            meter.work(u64::from(added), 0, 0)?;
            if !added {
                continue;
            }
            let formula = matches!(input, CellInput::Formula(_));
            formulas += u64::from(formula);
            let template = match input {
                CellInput::Formula(formula) => {
                    host.formula_r1c1(at).map(|text| (text, formula.form()))
                }
                _ => None,
            };
            columns
                .entry((at.sheet(), at.column()))
                .or_default()
                .push(RunCell { at, template, formula });
        }
    }
    let mut runs = Vec::new();
    let mut standalone = BTreeSet::new();
    for cells in columns.values_mut() {
        cells.sort_unstable_by_key(|cell| cell.at);
        column_runs(cells, &mut runs, &mut standalone, meter)?;
    }
    runs.sort_unstable_by_key(|run| run.extent.0);

    let mut ports = Vec::new();
    let mut port_count = 0;
    let names = model.definitions().name_table();
    for definition in host.defined_names() {
        meter.work(0, 0, 0)?;
        if definition.hidden {
            continue;
        }
        let Some((target, _)) = names.lookup(definition.scope, &definition.name)
            .and_then(|node| host.stable_name_source(node.id()))
        else { continue };
        if areas.iter().any(|area| area.intersection(target).is_some()) {
            port_count += 1;
            if ports.len() < 200 {
                meter.work(0, 1, 0)?;
                ports.push(NamedPort {
                    name: definition.name.clone(),
                    scope: crate::inventory::name_scope(definition.scope),
                    area: Area::of(target),
                });
            }
        }
    }
    meter.work(0, standalone.len() as u64, 0)?;
    Ok(Structure { tables, runs, formulas, standalone: standalone.into_iter().collect(), ports, port_count })
}

pub(crate) fn inputs(
    state: &State,
    scope: &SourceRef,
    meter: &mut QueryWork,
) -> Result<Vec<InputCandidate>, Refusal> {
    let host = state.session.host();
    let model = host.model();
    let visible = |sheet: &calc_doc_model::SheetEntry| {
        sheet.kind() == SheetKind::Worksheet && sheet.visibility() == SheetVisibility::Visible
    };
    let areas: Vec<_> = source_areas(state, scope)?
        .into_iter()
        .filter(|area| model.sheet_by_id(area.sheet()).is_some_and(visible))
        .collect();
    let mut choices = BTreeMap::new();
    let mut offer = |at, signal, label| {
        if !matches!(model.cell(at), Some(CellInput::Formula(_)))
            && batch::rules(&state.session, at).is_ok()
        {
            choices.insert(at, InputCandidate { at: Key::of(at), label, signal });
        }
    };
    for &area in &areas {
        if host.sheet_contents_protected(area.sheet()) {
            for region in calc_host::format_regions(host, area).map_err(invalid)? {
                meter.check()?;
                if !host.style_is_locked(region.format) {
                    meter.work(region.area.cell_count(), 0, 0)?;
                    for at in cells_in(region.area) {
                        offer(at, InputSignal::UnlockedOnProtectedSheet, None);
                    }
                }
            }
        }
    }
    for sheet in model.sheets().iter().filter(|sheet| visible(sheet)) {
        for (control, at) in host.form_control_links(sheet.id()) {
            meter.work(1, 0, 0)?;
            if control.macro_name.is_none() && areas.iter().any(|area| area.contains(at)) {
                offer(
                    at,
                    InputSignal::LinkedFormControl,
                    control.caption.as_ref().filter(|label| !label.is_empty()).cloned()
                        .or_else(|| Some(control.name.clone())),
                );
            }
        }
    }
    for area in areas {
        for rule in model.definitions().validations(area.sheet()) {
            meter.check()?;
            if rule.kind != "list" {
                continue;
            }
            for &(top, left, bottom, right) in &rule.areas {
                let cell = |row, column| CellKey::new(area.sheet(), row, column).ok();
                let overlap = cell(top, left)
                    .zip(cell(bottom, right))
                    .and_then(|(first, last)| GridArea::new(first, last).ok())
                    .and_then(|rule_area| area.intersection(rule_area));
                let Some(overlap) = overlap else { continue };
                meter.work(overlap.cell_count(), 0, 0)?;
                for at in cells_in(overlap) {
                    if let Some(rule) = host.validation(at).filter(|rule| rule.kind == "list") {
                        offer(at, InputSignal::ValidationList, rule.prompt.map(|(title, _)| title));
                    }
                }
            }
        }
    }
    meter.work(0, choices.len() as u64, 0)?;
    Ok(choices.into_values().collect())
}

fn cells_in(area: GridArea) -> impl Iterator<Item = CellKey> {
    let (first, last) = (area.first(), area.last());
    (first.row()..=last.row()).flat_map(move |row| {
        (first.column()..=last.column()).map(move |column| {
            CellKey::new(area.sheet(), row, column).expect("valid worksheet area")
        })
    })
}

pub(crate) fn outputs(
    state: &State,
    scope: &SourceRef,
    meter: &mut QueryWork,
) -> Result<Vec<Key>, Refusal> {
    let host = state.session.host();
    let mut outputs = BTreeSet::new();
    for area in source_areas(state, scope)? {
        for at in host.dependency_anchors(area) {
            meter.work(1, 0, 0)?;
            let result = host.spill_area(at).unwrap_or_else(|| GridArea::cell(at));
            if host.dependency_inputs(at).is_some() && host.dependency_readers(result).next().is_none()
                && outputs.insert(Key::of(at))
            {
                meter.work(0, 1, 0)?;
            }
        }
    }
    Ok(outputs.into_iter().collect())
}

struct RunCell {
    at: CellKey,
    template: Option<(String, FormulaForm)>,
    formula: bool,
}

fn remember_standalone(cell: &RunCell, standalone: &mut BTreeSet<Key>) {
    if cell.formula {
        standalone.insert(Key::of(cell.at));
        if standalone.len() > 200 {
            standalone.pop_last();
        }
    }
}

fn column_runs(
    cells: &[RunCell],
    runs: &mut Vec<FormulaRun>,
    standalone: &mut BTreeSet<Key>,
    meter: &mut QueryWork,
) -> Result<(), Refusal> {
    let mut first = 0;
    while let Some(start) = cells.get(first) {
        meter.work(0, 0, 0)?;
        let Some(template) = &start.template else {
            remember_standalone(start, standalone);
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
                remember_standalone(cell, standalone);
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
        } else {
            remember_standalone(start, standalone);
        }
        first = next;
    }
    Ok(())
}

fn area(first: CellKey, last: CellKey) -> Area {
    Area::of(GridArea::new(first, last).expect("run on one worksheet"))
}
