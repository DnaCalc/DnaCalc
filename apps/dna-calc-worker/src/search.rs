//! Presentation of the host search and arithmetic over application projections.
use crate::{cells, wire, workbook::Workbook};
use calc_host::value::{CellKey, CoreRef, GridArea, MAX_COLUMNS, MAX_ROWS, SheetId};
use calc_host::{ProjectedValue, ProjectionRequest, Selection};
use std::collections::{BTreeMap, BTreeSet};

const MAX_SPECIAL_AREAS: usize = 1024;
const MAX_SPECIAL_CELL_VISITS: usize = 100_000;
const MAX_SPECIAL_INTERSECTIONS: usize = 5_000_000;
const SPECIAL_SCOPE_LIMIT: &str = "Select a smaller range for Go To Special";

pub fn host_options(
    options: &wire::FindOptions,
) -> Result<calc_host::search::SearchOptions, String> {
    Ok(calc_host::search::SearchOptions {
        sheet: options.sheet.id(),
        areas: options
            .areas
            .iter()
            .map(|a| a.area().ok_or_else(|| "Invalid search area".to_owned()))
            .collect::<Result<_, _>>()?,
        text: options.text.clone(),
        case_sensitive: options.case_sensitive,
        whole: options.whole,
        wildcards: options.wildcards,
        formulas: options.formulas,
        scope: match options.scope {
            wire::FindScope::Sheet => calc_host::search::SearchScope::Sheet,
            wire::FindScope::Selection => calc_host::search::SearchScope::Selection,
            wire::FindScope::Workbook => calc_host::search::SearchScope::Workbook,
        },
    })
}
pub fn find(book: &Workbook, options: &wire::FindOptions) -> Result<wire::Found, String> {
    let host = book.host();
    let found = calc_host::search::find(host, &host_options(options)?, |cell| {
        if options.formulas {
            cells::input_text(cell.input())
        } else {
            book.shown(cell).text
        }
    })?;
    let rows = found
        .rows
        .into_iter()
        .map(|cell| {
            let at = cell.at();
            let shown = book.shown(&cell);
            wire::FoundRow {
                at: wire::Key::of(at),
                label: dna_formula_ui::display::cell(&cells::WorkbookNames(host), at),
                source: cells::display_input(cell.input()),
                display: shown.text,
                provenance: shown.provenance,
                hidden: host
                    .model()
                    .sheet_by_id(at.sheet())
                    .is_some_and(|s| s.visibility() != calc_host::SheetVisibility::Visible),
            }
        })
        .collect();
    Ok(wire::Found {
        revision: host.revision().get(),
        rows,
        scanned: found.scanned,
        partial: found.partial,
    })
}

pub fn special_cells(
    book: &Workbook,
    sheet: wire::Sheet,
    areas: Vec<wire::Area>,
    kind: wire::SpecialCellKind,
) -> Result<Vec<wire::Area>, String> {
    use wire::SpecialCellKind;

    let sheet = sheet.id();
    let host = book.host();
    if host.model().sheet_by_id(sheet).is_none() {
        return Err("The worksheet is unavailable".into());
    }
    if areas.len() > MAX_SPECIAL_AREAS {
        return Err(SPECIAL_SCOPE_LIMIT.into());
    }
    let mut scope = areas
        .into_iter()
        .map(|area| {
            area.area()
                .filter(|area| area.sheet() == sheet)
                .ok_or("Invalid Go To Special selection")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if scope.is_empty() || matches!(scope.as_slice(), [area] if area.cell_count() == 1) {
        scope = if kind == SpecialCellKind::Blanks {
            book.used_area(sheet).into_iter().collect()
        } else {
            vec![GridArea::full_sheet(sheet)]
        };
    }
    let mut matches = match kind {
        SpecialCellKind::Blanks => subtract_areas(scope, host.occupied_areas(sheet))?,
        SpecialCellKind::Notes => {
            let notes = host.model().definitions().review_notes(sheet);
            cell_areas(
                notes
                    .iter()
                    .filter(|note| {
                        !note.compatibility
                            && note.source.as_ref().is_none_or(|source| !source.threaded())
                            && scope.iter().any(|area| area.contains(note.at))
                    })
                    .map(|note| note.at)
                    .collect(),
            )?
        }
        SpecialCellKind::Visible => {
            let geometry = host
                .sheet_geometry(sheet)
                .ok_or("The worksheet is unavailable")?;
            visible_areas(scope, sheet, &geometry)?
        }
        SpecialCellKind::Formulas | SpecialCellKind::Constants => {
            input_areas(host, scope, kind == SpecialCellKind::Formulas)?
        }
    };
    if matches.is_empty() {
        return Err("No matching cells".into());
    }
    matches.sort_by_key(|area| (area.first(), area.last()));
    matches.dedup();
    Ok(matches.into_iter().map(wire::Area::of).collect())
}

fn input_areas(
    host: &calc_host::HostState,
    scope: Vec<GridArea>,
    formulas: bool,
) -> Result<Vec<GridArea>, String> {
    let definitions = host.model().definitions();
    let mut derived = Vec::new();
    let mut cells = BTreeSet::new();
    let mut visited = 0;
    for area in scope {
        // Legacy array and data-table cells carry a formula across the whole range.
        // A dynamic spill has one authored formula at its anchor.
        let first_derived = derived.len();
        if formulas {
            let legacy = definitions
                .arrays_meeting(area)
                .filter(|array| array.kind() == calc_host::ArrayKind::Legacy)
                .map(|array| array.area());
            let tables = definitions.data_tables().iter().map(|table| table.area());
            derived.extend(
                legacy
                    .chain(tables)
                    .filter_map(|range| range.intersection(area)),
            );
            if derived.len() > MAX_SPECIAL_AREAS {
                return Err(SPECIAL_SCOPE_LIMIT.into());
            }
        }
        for (at, input) in host.model().cells(area).into_iter().flatten() {
            visited += 1;
            if visited > MAX_SPECIAL_CELL_VISITS {
                return Err(SPECIAL_SCOPE_LIMIT.into());
            }
            let matches = if formulas {
                matches!(input, calc_host::CellInput::Formula(_))
                    && !derived[first_derived..]
                        .iter()
                        .any(|range| range.contains(at))
            } else {
                !host.holds_formula(at)
            };
            if matches {
                cells.insert(at);
            }
        }
    }
    derived.extend(cell_areas(cells)?);
    if derived.len() > MAX_SPECIAL_AREAS {
        return Err(SPECIAL_SCOPE_LIMIT.into());
    }
    Ok(derived)
}

fn visible_areas(
    scope: Vec<GridArea>,
    sheet: SheetId,
    geometry: &calc_host::SheetGeometry,
) -> Result<Vec<GridArea>, String> {
    let hidden_rows = geometry
        .rows
        .iter()
        .filter(|(_, format)| format.hidden || format.height == Some(0.0))
        .map(|(&row, _)| {
            GridArea::new(
                CellKey::new(sheet, row, 1).unwrap(),
                CellKey::new(sheet, row, MAX_COLUMNS).unwrap(),
            )
            .unwrap()
        });
    let hidden_columns = geometry
        .columns
        .iter()
        .filter(|span| span.hidden || span.width == Some(0.0))
        .map(|span| {
            GridArea::new(
                CellKey::new(sheet, 1, span.first).unwrap(),
                CellKey::new(sheet, MAX_ROWS, span.last).unwrap(),
            )
            .unwrap()
        });
    subtract_areas(scope, hidden_rows.chain(hidden_columns))
}

/// Join adjacent matches so a dense row, column or block needs only one selection area.
fn cell_areas(cells: BTreeSet<CellKey>) -> Result<Vec<GridArea>, String> {
    let mut cells = cells.into_iter().peekable();
    let mut areas: Vec<GridArea> = Vec::new();
    let mut previous_runs = BTreeMap::new();
    while let Some(first) = cells.next() {
        let mut last = first;
        while let Some(next) =
            cells.next_if(|next| next.row() == last.row() && next.column() == last.column() + 1)
        {
            last = next;
        }
        let index = previous_runs
            .entry((first.column(), last.column()))
            .or_insert(areas.len());
        if let Some(previous) = areas.get_mut(*index)
            && previous.last().row() + 1 == first.row()
        {
            *previous = GridArea::new(previous.first(), last).unwrap();
        } else {
            *index = areas.len();
            if areas.len() == MAX_SPECIAL_AREAS {
                return Err(SPECIAL_SCOPE_LIMIT.into());
            }
            areas.push(GridArea::new(first, last).unwrap());
        }
    }
    Ok(areas)
}

/// Subtract rectangles for blank or visible cells without visiting each cell.
fn subtract_areas(
    mut areas: Vec<GridArea>,
    removed: impl IntoIterator<Item = GridArea>,
) -> Result<Vec<GridArea>, String> {
    let mut next = Vec::new();
    let mut intersections = 0;
    for removed in removed {
        if areas.is_empty() {
            break;
        }
        intersections += areas.len();
        if intersections > MAX_SPECIAL_INTERSECTIONS {
            return Err(SPECIAL_SCOPE_LIMIT.into());
        }
        for area in areas.drain(..) {
            if area.intersection(removed).is_some() {
                next.extend(area.subtract(removed));
            } else {
                next.push(area);
            }
        }
        if next.len() > MAX_SPECIAL_AREAS {
            return Err(SPECIAL_SCOPE_LIMIT.into());
        }
        std::mem::swap(&mut areas, &mut next);
    }
    Ok(areas)
}

pub fn selection_stats(
    book: &Workbook,
    selection: &Selection,
) -> Result<wire::SelectionStats, String> {
    if selection.areas.iter().map(|a| a.cell_count()).sum::<u64>() > 10_000 {
        return Err("Selection summary is limited to 10000 cells; select a smaller range".into());
    }
    let mut cells = BTreeSet::new();
    for area in &selection.areas {
        for row in area.first().row()..=area.last().row() {
            for col in area.first().column()..=area.last().column() {
                cells.insert(CellKey::new(area.sheet(), row, col).unwrap());
            }
        }
    }
    let page = calc_host::project(
        book.host(),
        &ProjectionRequest {
            cells: cells.into_iter().collect(),
        },
    )
    .map_err(|e| e.to_string())?;
    let mut out = wire::SelectionStats {
        selected: page.cells().len() as u64,
        ..Default::default()
    };
    let (mut sum, mut correction) = (0.0, 0.0);
    let mut min = None::<f64>;
    let mut max = None::<f64>;
    for cell in page.cells() {
        let value = match cell.value() {
            ProjectedValue::Literal(value) => {
                out.literal += 1;
                Some(value)
            }
            ProjectedValue::Calculated { value, .. } => {
                out.calculated += 1;
                Some(value)
            }
            ProjectedValue::FileCached(_) => {
                out.cached += 1;
                None
            }
            ProjectedValue::Stale(_) => {
                out.stale += 1;
                None
            }
            ProjectedValue::NotModeled { .. } => {
                out.refused += 1;
                None
            }
            ProjectedValue::Empty => {
                out.blank += 1;
                None
            }
        };
        let Some(value) = value else {
            continue;
        };
        match value.core_ref() {
            CoreRef::Empty => out.blank += 1,
            CoreRef::Text(t) if t.utf16_code_units().is_empty() => {
                out.count += 1;
                out.empty_text += 1;
            }
            CoreRef::Number(n) => {
                out.count += 1;
                out.numeric += 1;
                let y = n - correction;
                let next = sum + y;
                correction = (next - sum) - y;
                sum = next;
                min = Some(min.map_or(n, |m| m.min(n)));
                max = Some(max.map_or(n, |m| m.max(n)));
            }
            CoreRef::Error(_) => {
                out.count += 1;
                out.errors += 1;
            }
            _ => out.count += 1,
        }
    }
    let display = |n: f64| {
        if n.is_finite() {
            n.to_string()
        } else {
            "Overflow".into()
        }
    };
    out.sum = if out.numeric == 0 {
        "—".into()
    } else {
        display(sum)
    };
    out.average = if out.numeric == 0 {
        "—".into()
    } else {
        display(sum / out.numeric as f64)
    };
    out.min = min.map(display).unwrap_or_else(|| "—".into());
    out.max = max.map(display).unwrap_or_else(|| "—".into());
    Ok(out)
}
