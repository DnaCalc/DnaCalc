//! Find an Excel backing location without consuming it or changing the workbook.

use crate::query::{QueryWork, checked_sheet};
use crate::{State, batch, grid_area, invalid};
use calc_doc_model::SheetKind;
use calc_host::value::{CellKey, GridArea, MAX_COLUMNS, MAX_ROWS, SheetId};
use calc_skin::grid::{Area, Key, Sheet};
use calc_skin::{Budget, Placement, PlacementKind, PlacementShape, Refusal};

const SCALAR_FIRST: u32 = 2;
const ARRAY_FIRST: u32 = 10;
const ALTERNATIVE: &str = "Choose another backing worksheet";

pub(crate) fn footprint(sheet: Sheet, kind: PlacementKind) -> GridArea {
    let last = match kind {
        PlacementKind::Scalar => ARRAY_FIRST - 1,
        PlacementKind::Array => MAX_ROWS,
    };
    area(sheet.id(), 1, 1, last, MAX_COLUMNS)
}

fn at(sheet: SheetId, row: u32, column: u32) -> CellKey {
    CellKey::new(sheet, row, column).expect("placement is within worksheet bounds")
}

fn area(sheet: SheetId, top: u32, left: u32, bottom: u32, right: u32) -> GridArea {
    GridArea::new(at(sheet, top, left), at(sheet, bottom, right)).expect("ordered worksheet bounds")
}

/// One difference array per scalar row, or one for all rows in an array lane. Overlapping
/// obstacles add constant work per affected row regardless of their worksheet area.
struct Occupancy {
    first_row: u32,
    last_row: u32,
    lanes: bool,
    columns: Vec<Vec<i32>>,
}

impl Occupancy {
    fn new(kind: PlacementKind) -> Self {
        let lanes = matches!(kind, PlacementKind::Array);
        let (first_row, last_row, height) = if lanes {
            (ARRAY_FIRST, MAX_ROWS, 1)
        } else {
            (SCALAR_FIRST, ARRAY_FIRST - 1, ARRAY_FIRST - SCALAR_FIRST)
        };
        Self {
            first_row,
            last_row,
            lanes,
            columns: vec![vec![0; MAX_COLUMNS as usize + 2]; height as usize],
        }
    }

    fn add(&mut self, area: GridArea, work: &mut QueryWork) -> Result<(), Refusal> {
        work.counts.cells += 1;
        work.check()?;
        let top = area.first().row().max(self.first_row);
        let bottom = area.last().row().min(self.last_row);
        if top <= bottom {
            let rows = if self.lanes {
                0..=0
            } else {
                (top - self.first_row) as usize..=(bottom - self.first_row) as usize
            };
            for row in rows {
                let columns = &mut self.columns[row];
                columns[area.first().column() as usize] += 1;
                columns[area.last().column() as usize + 1] -= 1;
            }
        }
        Ok(())
    }

    fn finish(mut self) -> Vec<Vec<i32>> {
        for row in &mut self.columns {
            let (mut overlaps, mut occupied) = (0, 0);
            for count in row.iter_mut() {
                overlaps += *count;
                occupied += i32::from(overlaps > 0);
                *count = occupied;
            }
        }
        self.columns
    }
}

pub(crate) fn find(
    state: &State,
    sheet: Sheet,
    kind: PlacementKind,
    shape: Option<PlacementShape>,
    reserved: &[Area],
    budget: Budget,
) -> Result<Placement, Refusal> {
    let mut work = QueryWork::new(budget, ALTERNATIVE);
    work.check()?;
    let host = state.session.host();
    let id = checked_sheet(host, sheet)?;
    if host
        .model()
        .sheet_by_id(id)
        .is_none_or(|sheet| sheet.kind() != SheetKind::Worksheet)
    {
        return Err(invalid("Placement requires a worksheet"));
    }
    let shape = shape.unwrap_or(PlacementShape {
        rows: 1,
        columns: 1,
    });
    match kind {
        PlacementKind::Scalar
            if shape
                != (PlacementShape {
                    rows: 1,
                    columns: 1,
                }) =>
        {
            return Err(invalid("A scalar placement holds one cell"));
        }
        PlacementKind::Array
            if shape.rows == 0
                || shape.rows > MAX_ROWS - ARRAY_FIRST + 1
                || shape.columns == 0
                || shape.columns >= MAX_COLUMNS =>
        {
            return Err(invalid(
                "The array and its spacer must fit on the backing worksheet",
            ));
        }
        _ => {}
    }

    let mut occupied = Occupancy::new(kind);
    for &lane in reserved {
        let lane = grid_area(lane)?;
        if lane.sheet() != id || lane.first().row() != ARRAY_FIRST || lane.last().row() != MAX_ROWS
        {
            return Err(invalid(
                "A lane reservation must run from row 10 to the bottom of this worksheet",
            ));
        }
        occupied.add(lane, &mut work)?;
    }
    let scan = footprint(sheet, kind);
    let model = host.model();
    let definitions = model.definitions();
    for (cell, _) in model.cells(scan).into_iter().flatten() {
        occupied.add(
            host.spill_area(cell)
                .unwrap_or_else(|| GridArea::cell(cell)),
            &mut work,
        )?;
    }
    for area in definitions
        .arrays_meeting(scan)
        .map(|array| array.area())
        .chain(
            definitions
                .unloaded_in(scan)
                .map(|(at, _)| GridArea::cell(at)),
        )
        .chain(
            definitions
                .tables()
                .iter()
                .filter_map(|table| table.area.intersection(scan)),
        )
        .chain(
            definitions
                .data_tables()
                .iter()
                .filter_map(|table| table.area().intersection(scan)),
        )
        .chain(
            definitions
                .pivots()
                .tables()
                .filter_map(|table| table.area.intersection(scan)),
        )
        .chain(
            host.merged_areas(id)
                .into_iter()
                .filter_map(|merge| merge.intersection(scan)),
        )
    {
        occupied.add(area, &mut work)?;
    }
    let occupied = occupied.finish();
    work.check()?;

    let (at, label, lane, footprint) = match kind {
        PlacementKind::Scalar => {
            let mut positions = (1..MAX_COLUMNS).step_by(2).flat_map(|column| {
                occupied
                    .iter()
                    .enumerate()
                    .map(move |(index, row)| (column, SCALAR_FIRST + index as u32, row))
            });
            let (column, row, _) = positions
                .find(|&(column, _, occupied)| {
                    occupied[column as usize + 1] == occupied[column as usize - 1]
                })
                .ok_or_else(|| invalid("The scalar zone is full"))?;
            let label = at(id, row, column);
            let value = at(id, row, column + 1);
            (
                value,
                Some(label),
                None,
                area(id, row, column, row, column + 1),
            )
        }
        PlacementKind::Array => {
            let occupied = &occupied[0];
            let column = (1..=MAX_COLUMNS - shape.columns)
                .find(|&column| {
                    let left = column.saturating_sub(2) as usize;
                    let right = (column + shape.columns) as usize;
                    occupied[right] == occupied[left]
                })
                .ok_or_else(|| invalid("No free array lane is wide enough"))?;
            let first = at(id, ARRAY_FIRST, column);
            let left = column.saturating_sub(1).max(1);
            (
                first,
                None,
                Some(area(
                    id,
                    ARRAY_FIRST,
                    column,
                    MAX_ROWS,
                    column + shape.columns - 1,
                )),
                area(id, ARRAY_FIRST, left, MAX_ROWS, column + shape.columns),
            )
        }
    };
    work.check()?;
    batch::rules(&state.session, at)?;
    if let Some(label) = label {
        batch::rules(&state.session, label)?;
    }
    Ok(Placement {
        at: Key::of(at),
        label: label.map(Key::of),
        lane: lane.map(Area::of),
        footprint: Area::of(footprint),
    })
}
