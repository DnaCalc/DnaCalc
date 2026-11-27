//! Read a rectangular source at the current workbook stamp, in view order.

use crate::query::{QueryWork, show_cell, shown_value, source_areas};
use crate::{State, invalid};
use calc_doc_model::Table;
use calc_func::{EvalServices, FunctionCallScratch, PreparedArguments, catalogue, invoke};
use calc_host::value::{ArrayShape, CalcArray, CalcValue, CellKey, GridArea, fold_workbook_name};
use calc_host::{HostState, ProjectedCell, ProjectionRequest};
use calc_skin::grid::Area;
use calc_skin::{
    Budget, Coverage, PartialWhy, Provenance, RawValue, Refusal, RowFilter, RowPage, RowSort,
    RowWindow, SourceRef, TablePart, WorkbookCell, WorksheetRow,
};
use std::collections::BTreeSet;

pub(crate) const MAX_WINDOW_ROWS: u32 = 200;
const MAX_KEYS: usize = 32;
const ALTERNATIVE: &str =
    "Choose fewer sort or filter columns, a smaller source, or a smaller row window";

struct Source<'a> {
    bounds: GridArea,
    first_row: u32,
    length: u32,
    headers: Option<&'a [String]>,
    totals: Option<GridArea>,
}

pub(crate) fn footprint(state: &State, source: &SourceRef) -> Result<GridArea, Refusal> {
    Ok(resolve(state, source)?.bounds)
}

pub(crate) fn table_area(state: &State, source: &SourceRef) -> Result<Option<GridArea>, Refusal> {
    let selected = resolve(state, source)?;
    Ok((selected.length != 0).then(|| {
        area(
            selected.bounds,
            selected.first_row,
            selected.bounds.first().column(),
            selected.first_row + selected.length - 1,
            selected.bounds.last().column(),
        )
    }))
}

fn resolve<'a>(state: &'a State, source: &SourceRef) -> Result<Source<'a>, Refusal> {
    let bounds = match source {
        SourceRef::Cells { .. } | SourceRef::Spill { .. } => {
            let areas = source_areas(state, source)?;
            let [area] = areas.as_slice() else {
                return Err(invalid("Rows requires one rectangular worksheet range"));
            };
            *area
        }
        SourceRef::Table { name, part } => {
            let name = fold_workbook_name(name);
            let table = state
                .session
                .host()
                .model()
                .definitions()
                .tables()
                .iter()
                .find(|table| fold_workbook_name(&table.display_name) == name)
                .ok_or_else(|| invalid("The source table is unavailable"))?;
            return table_source(table, part);
        }
        _ => {
            return Err(invalid(
                "Choose a worksheet range, spill, or table as the row source",
            ));
        }
    };
    Ok(Source {
        bounds,
        first_row: bounds.first().row(),
        length: bounds.row_count(),
        headers: None,
        totals: None,
    })
}

fn table_source<'a>(table: &'a Table, part: &TablePart) -> Result<Source<'a>, Refusal> {
    let bounds = table.area;
    let first = bounds.first();
    let last = bounds.last();
    let length = bounds.row_count();
    if table.header_rows.saturating_add(table.totals_rows) > length {
        return Err(invalid(
            "The table's header and totals lie outside its bounds",
        ));
    }
    let data_first = first.row() + table.header_rows;
    let data_length = length - table.header_rows - table.totals_rows;
    let totals_first = data_first + data_length;
    let totals = (table.totals_rows != 0).then(|| {
        area(
            bounds,
            totals_first,
            first.column(),
            last.row(),
            last.column(),
        )
    });

    let (first_row, length, headers, totals, bounds) = match part {
        TablePart::Data => (
            data_first,
            data_length,
            Some(table.columns.as_slice()),
            totals,
            bounds,
        ),
        TablePart::All => (first.row(), length, None, None, bounds),
        TablePart::Headers => (first.row(), table.header_rows, None, None, bounds),
        TablePart::Totals => (totals_first, table.totals_rows, None, None, bounds),
        TablePart::Columns {
            first: left,
            last: right,
        } => {
            let index = |label: &str| {
                let label = fold_workbook_name(label);
                table
                    .columns
                    .iter()
                    .position(|name| fold_workbook_name(name) == label)
                    .and_then(|index| u32::try_from(index).ok())
                    .filter(|&index| index < bounds.column_count())
                    .ok_or_else(|| invalid("A source table column is unavailable"))
            };
            let left = first.column() + index(left)?;
            let right = first.column() + index(right)?;
            if left > right {
                return Err(invalid("The table column bounds are reversed"));
            }
            let bounds = area(bounds, first.row(), left, last.row(), right);
            let totals = totals.map(|totals| area(totals, totals_first, left, last.row(), right));
            let headers = &table.columns
                [(left - first.column()) as usize..=(right - first.column()) as usize];
            (data_first, data_length, Some(headers), totals, bounds)
        }
    };
    Ok(Source {
        bounds,
        first_row,
        length,
        headers,
        totals,
    })
}

fn at(bounds: GridArea, row: u32, column: u32) -> CellKey {
    CellKey::new(bounds.sheet(), row, column).expect("within checked row source")
}

fn area(bounds: GridArea, top: u32, left: u32, bottom: u32, right: u32) -> GridArea {
    GridArea::new(at(bounds, top, left), at(bounds, bottom, right))
        .expect("within checked row source")
}

pub(crate) fn read(
    state: &State,
    source: &SourceRef,
    filter: &[RowFilter],
    sort: &[RowSort],
    window: RowWindow,
    budget: Budget,
) -> Result<(RowPage, Coverage), Refusal> {
    let mut work = QueryWork::new(budget, ALTERNATIVE);
    work.check()?;
    let source = resolve(state, source)?;
    let bounds = source.bounds;
    let columns = bounds.column_count();
    if window.limit > MAX_WINDOW_ROWS {
        return Err(invalid(format!(
            "A row window holds at most {MAX_WINDOW_ROWS} rows"
        )));
    }
    if filter.len() > MAX_KEYS || sort.len() > MAX_KEYS {
        return Err(invalid(
            "Choose at most 32 filter criteria and 32 sort keys",
        ));
    }
    let sort_columns: BTreeSet<_> = sort.iter().map(|key| key.column).collect();
    if sort_columns.len() != sort.len() {
        return Err(invalid("Choose each sort column only once"));
    }
    let keys: Vec<_> = filter
        .iter()
        .map(|key| key.column)
        .chain(sort_columns)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if keys.iter().any(|&column| column >= columns) {
        return Err(invalid(
            "Every row filter and sort key must be inside the source's columns",
        ));
    }

    let mut matched = source.length;
    let original_rows = source.first_row..source.first_row + source.length;
    let rows: Vec<_> = if keys.is_empty() {
        original_rows
            .skip(window.offset as usize)
            .take(window.limit as usize)
            .collect()
    } else {
        work.counts.cells = u64::from(source.length) * keys.len() as u64;
        work.counts.nodes = u64::from(source.length);
        work.check()?;
        let mut rows = selected_rows(state, &source, filter, sort, &keys, &mut work)?;
        matched = rows.len() as u32;
        let start = window.offset.min(matched) as usize;
        let end = window.offset.saturating_add(window.limit).min(matched) as usize;
        rows.drain(start..end).collect()
    };

    work.counts.cells +=
        rows.len() as u64 * u64::from(columns) + source.totals.map_or(0, GridArea::cell_count);
    work.check()?;
    if let Some(headers) = source.headers {
        work.charge_bytes(&headers)?;
    }
    let first_column = bounds.first().column();
    let last_column = bounds.last().column();
    let host = state.session.host();
    let page = calc_host::project(
        host,
        &ProjectionRequest {
            cells: rows
                .iter()
                .flat_map(|&row| {
                    (first_column..=last_column).map(move |column| at(bounds, row, column))
                })
                .chain(source.totals.into_iter().flat_map(|totals| {
                    (totals.first().row()..=totals.last().row()).flat_map(move |row| {
                        (first_column..=last_column).map(move |column| at(bounds, row, column))
                    })
                }))
                .collect(),
        },
    )
    .map_err(|error| invalid(error.to_string()))?;
    let mut cells = page.cells().iter();
    let rows = rows
        .into_iter()
        .map(|row| {
            Ok(WorksheetRow {
                row,
                cells: show(state, cells.by_ref().take(columns as usize), &mut work)?,
            })
        })
        .collect::<Result<Vec<_>, Refusal>>()?;
    let totals = source
        .totals
        .map(|_| show(state, cells, &mut work))
        .transpose()?;
    let coverage = if rows.len() == matched as usize {
        Coverage::Complete
    } else {
        Coverage::Partial {
            done: rows.len() as u64,
            total: Some(u64::from(matched)),
            why: PartialWhy::Window,
        }
    };
    Ok((
        RowPage {
            bounds: Area::of(bounds),
            columns,
            headers: source.headers.map(<[String]>::to_vec),
            totals,
            source_rows: source.length,
            matched_rows: matched,
            offset: window.offset.min(matched),
            rows,
        },
        coverage,
    ))
}

fn show<'a>(
    state: &State,
    cells: impl Iterator<Item = &'a ProjectedCell>,
    work: &mut QueryWork,
) -> Result<Vec<WorkbookCell>, Refusal> {
    cells
        .map(|projected| {
            let cell = show_cell(state, projected);
            work.charge_bytes(&cell)?;
            Ok(cell)
        })
        .collect()
}

fn selected_rows(
    state: &State,
    source: &Source<'_>,
    filter: &[RowFilter],
    sort: &[RowSort],
    keys: &[u32],
    work: &mut QueryWork,
) -> Result<Vec<u32>, Refusal> {
    let host = state.session.host();
    let bounds = source.bounds;
    let first_column = bounds.first().column();
    let page = calc_host::project(
        host,
        &ProjectionRequest {
            cells: (source.first_row..source.first_row + source.length)
                .flat_map(|row| {
                    keys.iter()
                        .map(move |column| at(bounds, row, first_column + column))
                })
                .collect(),
        },
    )
    .map_err(|error| invalid(error.to_string()))?;
    work.check()?;

    let index = |column| keys.binary_search(&column).expect("a requested key");
    let filter: Vec<_> = filter
        .iter()
        .map(|key| (index(key.column), &key.equals))
        .collect();
    let mut sort_values: Vec<_> = sort.iter().map(|_| Vec::new()).collect();
    let sort_indices: Vec<_> = sort.iter().map(|key| index(key.column)).collect();
    let mut rows = Vec::new();
    let mut values = Vec::with_capacity(keys.len());

    for entry in page.cells().chunks_exact(keys.len()) {
        work.check()?;
        values.clear();
        for cell in entry {
            let (value, provenance) = shown_value(state, cell.value());
            match provenance {
                Provenance::Refused { owner, reason } => {
                    return Err(Refusal::NotModelled {
                        owner,
                        what: reason,
                    });
                }
                Provenance::AsSaved | Provenance::Stale { .. } => {
                    return Err(invalid(
                        "Calculate the source before sorting or filtering its rows",
                    ));
                }
                _ => {}
            }
            let value = value.unwrap_or_else(CalcValue::empty);
            let scalar = value
                .as_array()
                .and_then(|array| array.get(0, 0))
                .unwrap_or(&value);
            if let Some(text) = scalar.as_text() {
                work.counts.bytes += 2 * text.utf16_code_units().len() as u64;
            }
            values.push(scalar.clone());
        }
        work.check()?;
        if filter.iter().any(|&(index, expected)| {
            let actual = crate::query::raw(&values[index]);
            !filter_matches(&actual, expected)
        }) {
            continue;
        }
        rows.push(entry[0].at().row());
        for (values_for_key, &index) in sort_values.iter_mut().zip(&sort_indices) {
            values_for_key.push(values[index].clone());
        }
    }
    if sort.is_empty() || rows.len() < 2 {
        return Ok(rows);
    }
    sort_rows(host, rows, sort, sort_values, work)
}

fn filter_matches(actual: &RawValue, expected: &RawValue) -> bool {
    match (actual, expected) {
        (RawValue::Number { bits: left }, RawValue::Number { bits: right }) => {
            f64::from_bits(*left) == f64::from_bits(*right)
        }
        _ => actual == expected,
    }
}

struct Arguments(Vec<CalcValue>);

impl PreparedArguments for Arguments {
    fn count(&self) -> usize {
        self.0.len()
    }

    fn argument(&mut self, index: usize) -> CalcValue {
        self.0[index].clone()
    }
}

fn sort_rows(
    host: &HostState,
    rows: Vec<u32>,
    sort: &[RowSort],
    keys: Vec<Vec<CalcValue>>,
    work: &mut QueryWork,
) -> Result<Vec<u32>, Refusal> {
    let function = catalogue()
        .resolve_surface(calc_func::Notation::Call, "SORTBY")
        .expect("SORTBY is catalogued");
    let shape = ArrayShape {
        rows: rows.len(),
        cols: 1,
    };
    let column =
        |values| CalcValue::array(CalcArray::new(shape, values).expect("one value per row"));
    let mut arguments = vec![column(
        rows.iter()
            .map(|&row| CalcValue::number(f64::from(row)))
            .collect(),
    )];
    for (key, values) in sort.iter().zip(keys) {
        arguments.push(column(values));
        arguments.push(CalcValue::number(if key.descending { -1.0 } else { 1.0 }));
    }
    let value = invoke(
        function,
        &mut Arguments(arguments),
        &mut EvalServices::new(&host.options().numeric),
        &mut FunctionCallScratch::new(),
    )
    .map_err(|error| crate::unavailable("dc-aje2v", &format!("View sort: {error:?}")))?;
    work.check()?;
    let order = value.as_array().ok_or_else(|| Refusal::Excel {
        reason: format!("View sort returned {}", crate::query::raw(&value).display()),
    })?;
    order
        .iter_row_major()
        .map(|row| {
            row.as_number()
                .map(|row| row as u32)
                .ok_or_else(|| invalid("View sort did not return worksheet row positions"))
        })
        .collect()
}
