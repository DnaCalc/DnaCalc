use crate::{Refusal, State, invalid, query::QueryWork, query::source_areas};
use calc_doc_model::ArrayKind;
use calc_host::value::{CellKey, CoreRef, GridArea, SheetId, WorksheetErrorCode};
use calc_host::{ProjectedValue, ProjectionRequest};
use calc_skin::SourceRef;
use calc_skin::grid::{Area, Sheet};
use calc_skin::query::{Census, SheetCensus};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn scoped_cells(
    state: &State,
    scope: &SourceRef,
    meter: &mut QueryWork,
) -> Result<BTreeMap<SheetId, Vec<CellKey>>, Refusal> {
    let host = state.session.host();
    let model = host.model();
    let definitions = model.definitions();
    let mut by_sheet: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for area in source_areas(state, scope)? {
        meter.work(0, 1, 0)?;
        by_sheet.entry(area.sheet()).or_default().push(area);
    }
    let mut cells_by_sheet = BTreeMap::new();
    for (sheet, areas) in by_sheet {
        if let [area] = areas.as_slice()
            && definitions
                .data_tables()
                .iter()
                .all(|table| table.area().intersection(*area).is_none())
            && !definitions
                .arrays_meeting(*area)
                .any(|array| array.kind() == ArrayKind::Legacy)
        {
            // Occupied cells are already unique and in worksheet order.
            let mut cells = Vec::new();
            for (at, _) in model.cells(*area).into_iter().flatten() {
                meter.work(1, 0, 0)?;
                cells.push(at);
            }
            cells_by_sheet.insert(sheet, cells);
            continue;
        }
        let mut cells = BTreeSet::new();
        for area in areas {
            add_occupied_and_legacy(model, area, &mut cells, meter)?;
        }
        cells_by_sheet.insert(sheet, cells.into_iter().collect());
    }
    Ok(cells_by_sheet)
}

fn add_occupied_and_legacy(
    model: &calc_doc_model::ModelRoot,
    area: GridArea,
    cells: &mut BTreeSet<CellKey>,
    meter: &mut QueryWork,
) -> Result<(), Refusal> {
    let definitions = model.definitions();
    let mut count = |at| meter.work(u64::from(cells.insert(at)), 0, 0);
    for (at, _) in model.cells(area).into_iter().flatten() {
        count(at)?;
    }
    let arrays = definitions
        .arrays_meeting(area)
        .filter(|array| array.kind() == ArrayKind::Legacy)
        .map(|array| array.area());
    let data_tables = definitions.data_tables().iter().map(|table| table.area());
    for overlap in arrays
        .chain(data_tables)
        .filter_map(|part| area.intersection(part))
    {
        for row in overlap.first().row()..=overlap.last().row() {
            for col in overlap.first().column()..=overlap.last().column() {
                count(CellKey::new(area.sheet(), row, col).expect("inside the worksheet"))?;
            }
        }
    }
    Ok(())
}

pub(crate) fn census(
    state: &State,
    scope: &SourceRef,
    meter: &mut QueryWork,
) -> Result<Census, Refusal> {
    let host = state.session.host();
    let cells_by_sheet = scoped_cells(state, scope, meter)?;
    let request = ProjectionRequest {
        cells: cells_by_sheet.values().flatten().copied().collect(),
    };
    let mut projected =
        calc_host::project_values(host, &request).map_err(|error| invalid(error.to_string()))?;
    let mut census = Census::default();
    for (id, cells) in cells_by_sheet {
        let mut sheet = SheetCensus {
            sheet: Sheet::of(id),
            formulas: 0,
            literals: 0,
            used: None,
            errors: BTreeMap::new(),
        };
        for (at, value) in cells.into_iter().zip(projected.by_ref()) {
            meter.work(0, 0, 0)?;
            if host.holds_formula(at) {
                sheet.formulas += 1;
            } else {
                sheet.literals += 1;
            }
            let extent = sheet.used.get_or_insert(Area::of(GridArea::cell(at)));
            extent.0.2 = extent.0.2.min(at.row());
            extent.0.3 = extent.0.3.min(at.column());
            extent.1.2 = extent.1.2.max(at.row());
            extent.1.3 = extent.1.3.max(at.column());
            if let Some(code) = current_error(&value) {
                *sheet
                    .errors
                    .entry(code.excel_spelling().into())
                    .or_default() += 1;
            }
        }
        census.formulas += sheet.formulas;
        census.literals += sheet.literals;
        census.sheets.push(sheet);
    }
    Ok(census)
}

pub(crate) fn current_error(projected: &ProjectedValue) -> Option<WorksheetErrorCode> {
    let value = match projected {
        ProjectedValue::Literal(value) => value,
        ProjectedValue::Calculated { value, calculation }
            if !calculation.held_by_circular_rule() && !calculation.reads_external_links() =>
        {
            value
        }
        _ => return None,
    };
    let result = match value.core_ref() {
        CoreRef::Array(array) => array.get(0, 0)?.core_ref(),
        value => value,
    };
    match result {
        CoreRef::Error(code) => Some(code),
        _ => None,
    }
}
